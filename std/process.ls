edition 7;
module std.process;

import std.buffer;
import std.option;

// `std.process` -- an argument list, and a child's output captured with a
// bound and a deadline (`docs/processes.md` §7.1).
//
// Nothing here starts a program. `exec_spawn` takes an `Exec` narrowed to
// one prefix, and a library cannot take a capability of whatever prefix
// its caller holds: a parameter names one, and a program holding
// `Exec("/usr/bin")` cannot pass it where `Exec("")` is asked for (§7.1,
// measured). So the caller spawns, under its own `Exec`, which is where the
// authority report should see it, and this module does everything either
// side of that: building the list, opening the channels, and `capture`.

// A `\0`-separated list, as `exec_spawn` takes its arguments and its
// environment (§4.2), built one entry at a time.
pub res struct Argv {
    held: buffer.Buffer,
    count: int,
}

pub fn argv[&h](heap: &!h Heap, capacity: int) -> [heap] Argv {
    return Argv { held: buffer.empty(heap, capacity), count: 0 };
}

// Add one entry: `0`, or `22` (`EINVAL`) for one containing a `\0`, which
// is left out. Such an entry would arrive as two, and the second could be
// anything -- `--root` included. An outcome and not a trap, unlike an
// unterminated list (§4.2): the list is the program's own, but an entry is
// data, and the MCP server's come from a model (§7.1).
pub fn add[&h, &s](heap: &!h Heap, list: Argv, one: &s [byte]) -> [heap] (Argv, int) {
    var i = 0;
    while i < len(one) {
        if one[i] == byte_of(0) {
            return (list, 22);
        }
        i = i + 1;
    }
    let Argv { held, count } = list;
    let added = buffer.append(heap, held, one);
    return (Argv { held: buffer.push(heap, added, byte_of(0)), count: count + 1 }, 0);
}

// The list, for `exec_spawn`: every entry ends in its `\0`.
pub fn list[&a](list: &a Argv) -> [] &a [byte] {
    return buffer.bytes(list.held);
}

// How many entries it holds.
pub fn count[&a](list: &a Argv) -> [] int {
    return list.count;
}

pub fn drop[&h](heap: &!h Heap, list: Argv) -> [heap] int {
    let Argv { held, count } = list;
    buffer.drop(heap, held);
    return count;
}

// Both channels a captured child needs: the parent's end and the child's
// end of its input, then of its output. One outcome for the two
// `pipe_open`s, so a caller handles one failure, not two nested ones.
pub enum Channels {
    Ok(Pipe, ChildEnd, Pipe, ChildEnd),
    Failed(int),
}

pub fn channels() -> [] Channels {
    match pipe_open() {
        Piped::Failed(e) => {
            return Channels::Failed(e);
        }
        Piped::Ok(to_child, child_in) => {
            match pipe_open() {
                Piped::Failed(e) => {
                    pipe_close(to_child);
                    child_end_close(child_in);
                    return Channels::Failed(e);
                }
                Piped::Ok(from_child, child_out) => {
                    return Channels::Ok(to_child, child_in, from_child, child_out);
                }
            }
        }
    }
}

// The three channels a child captured with its standard error needs
// (`capture_both`, §7.2): the parent's end and the child's end of its input,
// of its output, then of its errors.
pub enum ChannelsWithErrors {
    Ok(Pipe, ChildEnd, Pipe, ChildEnd, Pipe, ChildEnd),
    Failed(int),
}

pub fn channels_with_errors() -> [] ChannelsWithErrors {
    match channels() {
        Channels::Failed(e) => {
            return ChannelsWithErrors::Failed(e);
        }
        Channels::Ok(to_child, child_in, from_child, child_out) => {
            match pipe_open() {
                Piped::Failed(e) => {
                    pipe_close(to_child);
                    child_end_close(child_in);
                    pipe_close(from_child);
                    child_end_close(child_out);
                    return ChannelsWithErrors::Failed(e);
                }
                Piped::Ok(from_errors, child_errors) => {
                    return ChannelsWithErrors::Ok(to_child, child_in, from_child, child_out, from_errors, child_errors);
                }
            }
        }
    }
}

// The parent's two ends, when the spawn they were opened for failed (its
// child's ends went with the spawn either way, §4.1).
pub fn close(to_child: Pipe, from_child: Pipe) -> [] int {
    pipe_close(to_child);
    pipe_close(from_child);
    return 0;
}

// How a capture ended. `Code` and `Signaled` are the child's own ending
// (`Exited`'s, in `std.signals`' bits); `TimedOut` and `TooMuch` say why
// `capture` killed it, rather than a `Signaled(256)` that could have come
// from anywhere; `Failed` is an `errno`.
pub enum Ran {
    Code(int),
    Signaled(int),
    TimedOut,
    TooMuch,
    Failed(int),
}

// The poller's tokens.
fn output_token() -> [] int {
    return 1;
}

fn input_token() -> [] int {
    return 2;
}

fn child_token() -> [] int {
    return 3;
}

fn errors_token() -> [] int {
    return 4;
}

// What one pass over the output found.
fn read_again() -> [] int {
    return 0;
}

fn read_end() -> [] int {
    return 1;
}

fn read_too_much() -> [] int {
    return 2;
}

// Read until the channel has nothing more for now (`Again`), ends, or would
// take the buffer past `most`. Keeps at most `most` bytes; the byte beyond is
// what says `TooMuch`, and it is not kept.
fn drain[&h, &p, &s](heap: &!h Heap, from: &!p Pipe, scratch: &!s [byte], out: buffer.Buffer, most: int) -> [heap, pipe_read] (buffer.Buffer, int) {
    var kept = out;
    while true {
        match pipe_read(from, scratch) {
            Received::Data(n) => {
                var room = 0;
                borrow kept as &k in {
                    room = most - buffer.size(k);
                }
                if n > room {
                    kept = buffer.append(heap, kept, scratch[0..room]);
                    return (kept, read_too_much());
                }
                kept = buffer.append(heap, kept, scratch[0..n]);
            }
            Received::End => {
                return (kept, read_end());
            }
            Received::Again => {
                return (kept, read_again());
            }
            Received::Failed(e) => {
                return (kept, read_end());
            }
        }
    }
    return (kept, read_again());
}

// Close a channel the capture is done with, if it is still open; closing it
// is also what takes it off the poller (§4.8).
fn shut(held: option.Option[Pipe]) -> [] int {
    match held {
        option.Option::Some(p) => {
            pipe_close(p);
        }
        option.Option::None => {
        }
    }
    return 0;
}

// Why the loop stopped.
fn running() -> [] int {
    return 0;
}

fn exited() -> [] int {
    return 1;
}

fn timed_out() -> [] int {
    return 2;
}

fn too_much() -> [] int {
    return 3;
}

fn failed() -> [] int {
    return 4;
}

// Run a started child to its end, writing `input` to it while its output is
// read, keeping at most `most` bytes of the output, for at most `timeout`
// milliseconds by `clock` (§7.1). The child is reaped on every path: killed
// first when the deadline passes, when it writes more than `most`, or when
// it cannot be watched.
//
// The input and the output are on one poller from the start, never written
// all and then read: a channel holds 8,192 bytes on macOS and 180,224 on
// Linux, and a child blocked writing output nobody reads stops reading its
// input (§7.1). The child's exit ends the capture, not the end of the
// stream: what it wrote is in the channel when it exits, and the end can be
// held off for ever by a process it left behind.
pub fn capture[&h, &c, &i](heap: &!h Heap, clock: &c Clock, child: Child, to_child: Pipe, from_child: Pipe, input: &i [byte], most: int, timeout: int) -> [heap, clock, poll] (buffer.Buffer, Ran) {
    let (out, none, ran) = gather(heap, clock, child, to_child, from_child, option.Option::None, input, most, 0, timeout);
    buffer.drop(heap, none);
    return (out, ran);
}

// `capture`, and the child's standard error beside its standard output
// (§7.2): the third channel is read on the same poller as the other two, into
// a buffer of its own, and `most_errors` bounds that buffer as `most` bounds
// the output. Either one exceeded ends the child, `TooMuch`. Both buffers come
// back on every path, the one that overran holding exactly its bound.
pub fn capture_both[&h, &c, &i](heap: &!h Heap, clock: &c Clock, child: Child, to_child: Pipe, from_child: Pipe, from_errors: Pipe, input: &i [byte], most: int, most_errors: int, timeout: int) -> [heap, clock, poll] (buffer.Buffer, buffer.Buffer, Ran) {
    return gather(heap, clock, child, to_child, from_child, option.Option::Some(from_errors), input, most, most_errors, timeout);
}

// The loop both are: `errors_in` is `None` for `capture`, whose child's
// standard error is not ours to read.
fn gather[&h, &c, &i](heap: &!h Heap, clock: &c Clock, child: Child, to_child: Pipe, from_child: Pipe, errors_in: option.Option[Pipe], input: &i [byte], most: int, most_errors: int, timeout: int) -> [heap, clock, poll] (buffer.Buffer, buffer.Buffer, Ran) {
    let deadline = clock_ms(clock) + timeout;
    var out = buffer.empty(heap, 4096);
    var err = buffer.empty(heap, 256);
    var writer = option.Option::Some(to_child);
    var reader = option.Option::Some(from_child);
    var errors = errors_in;
    var state = running();
    var why = 0;

    match poller_new() {
        Polling::Failed(e) => {
            state = failed();
            why = e;
        }
        Polling::Ok(p) => {
            var poller = p;
            var scratch_box = box_slice(heap, 4096, byte_of(0));
            var events = box_slice(heap, 16, 0);
            var written = 0;
            borrow mut poller as &!ph in {
                borrow child as &ch in {
                    let added = poller_add_child(ph, ch, child_token());
                    if added != 0 {
                        state = failed();
                        why = added;
                    }
                }
                borrow mut reader as &!r in {
                    match r {
                        option.Option::Some(rp) => {
                            pipe_nonblocking(rp);
                            let added = poller_add_pipe(ph, rp, output_token(), 1);
                            if added != 0 && state == running() {
                                state = failed();
                                why = added;
                            }
                        }
                        option.Option::None => {
                        }
                    }
                }
                borrow mut errors as &!r in {
                    match r {
                        option.Option::Some(rp) => {
                            pipe_nonblocking(rp);
                            let added = poller_add_pipe(ph, rp, errors_token(), 1);
                            if added != 0 && state == running() {
                                state = failed();
                                why = added;
                            }
                        }
                        option.Option::None => {
                        }
                    }
                }
                if len(input) == 0 {
                    shut(writer);
                    writer = option.Option::None;
                } else {
                    borrow mut writer as &!w in {
                        match w {
                            option.Option::Some(wp) => {
                                pipe_nonblocking(wp);
                                let added = poller_add_pipe(ph, wp, input_token(), 2);
                                if added != 0 && state == running() {
                                    state = failed();
                                    why = added;
                                }
                            }
                            option.Option::None => {
                            }
                        }
                    }
                }

                while state == running() {
                    let left = deadline - clock_ms(clock);
                    if left <= 0 {
                        state = timed_out();
                    } else {
                        var n = 0;
                        borrow mut events as &!eb in {
                            n = poller_wait(ph, contents(eb), left);
                        }
                        if n < 0 {
                            state = failed();
                            why = 0 - n;
                        }
                        var k = 0;
                        var gone = false;
                        var input_done = false;
                        var output_done = false;
                        var errors_done = false;
                        while k < n && state == running() {
                            var token = 0;
                            borrow events as &e in {
                                token = contents(e)[2 * k];
                            }
                            if token == child_token() {
                                gone = true;
                            }
                            if token == output_token() {
                                borrow mut reader as &!r in {
                                    match r {
                                        option.Option::Some(rp) => {
                                            borrow mut scratch_box as &!sb in {
                                                let (kept, found) = drain(heap, rp, contents(sb), out, most);
                                                out = kept;
                                                if found == read_too_much() {
                                                    state = too_much();
                                                }
                                                if found == read_end() {
                                                    output_done = true;
                                                }
                                            }
                                        }
                                        option.Option::None => {
                                        }
                                    }
                                }
                            }
                            if token == errors_token() {
                                borrow mut errors as &!r in {
                                    match r {
                                        option.Option::Some(rp) => {
                                            borrow mut scratch_box as &!sb in {
                                                let (kept, found) = drain(heap, rp, contents(sb), err, most_errors);
                                                err = kept;
                                                if found == read_too_much() {
                                                    state = too_much();
                                                }
                                                if found == read_end() {
                                                    errors_done = true;
                                                }
                                            }
                                        }
                                        option.Option::None => {
                                        }
                                    }
                                }
                            }
                            if token == input_token() {
                                borrow mut writer as &!w in {
                                    match w {
                                        option.Option::Some(wp) => {
                                            match pipe_write(wp, input[written..len(input)]) {
                                                Sent::Wrote(m) => {
                                                    written = written + m;
                                                    if written == len(input) {
                                                        input_done = true;
                                                    }
                                                }
                                                Sent::Again => {
                                                }
                                                // A child that stopped reading is not an
                                                // error: its exit status says what it thought.
                                                Sent::Failed(e) => {
                                                    input_done = true;
                                                }
                                            }
                                        }
                                        option.Option::None => {
                                        }
                                    }
                                }
                            }
                            k = k + 1;
                        }
                        if input_done {
                            shut(writer);
                            writer = option.Option::None;
                        }
                        if output_done {
                            shut(reader);
                            reader = option.Option::None;
                        }
                        if errors_done {
                            shut(errors);
                            errors = option.Option::None;
                        }
                        if gone && state == running() {
                            // Everything the child wrote is in the channel now.
                            borrow mut reader as &!r in {
                                match r {
                                    option.Option::Some(rp) => {
                                        borrow mut scratch_box as &!sb in {
                                            let (kept, found) = drain(heap, rp, contents(sb), out, most);
                                            out = kept;
                                            if found == read_too_much() {
                                                state = too_much();
                                            }
                                        }
                                    }
                                    option.Option::None => {
                                    }
                                }
                            }
                            borrow mut errors as &!r in {
                                match r {
                                    option.Option::Some(rp) => {
                                        borrow mut scratch_box as &!sb in {
                                            let (kept, found) = drain(heap, rp, contents(sb), err, most_errors);
                                            err = kept;
                                            if found == read_too_much() {
                                                state = too_much();
                                            }
                                        }
                                    }
                                    option.Option::None => {
                                    }
                                }
                            }
                            if state == running() {
                                state = exited();
                            }
                        }
                    }
                }
            }
            unbox_slice(heap, scratch_box);
            unbox_slice(heap, events);
            poller_close(poller);
        }
    }

    shut(writer);
    shut(reader);
    shut(errors);
    if state != exited() {
        borrow child as &ch in {
            child_kill(ch, 256);
        }
    }
    var ran = Ran::Failed(why);
    match child_wait(child) {
        Exited::Code(n) => {
            ran = Ran::Code(n);
        }
        Exited::Signaled(s) => {
            ran = Ran::Signaled(s);
        }
        Exited::Failed(e) => {
            ran = Ran::Failed(e);
        }
    }
    if state == timed_out() {
        ran = Ran::TimedOut;
    }
    if state == too_much() {
        ran = Ran::TooMuch;
    }
    if state == failed() {
        ran = Ran::Failed(why);
    }
    return (out, err, ran);
}
