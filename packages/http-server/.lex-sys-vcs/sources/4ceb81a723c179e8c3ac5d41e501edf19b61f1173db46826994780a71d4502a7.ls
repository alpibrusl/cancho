edition 5;

// `http.server` -- the server loop, as a package (`docs/http-server.md`).
//
// What `examples/api/api.ls` carried alongside its routes: accepting, reading,
// parsing, framing (a length or chunked), pipelining, back-pressure on a client
// that does not read, an idle timeout. What it does *not* carry is anything
// about what a request means -- no routes, no JSON, no handler. The
// application's loop calls `wait`, then `next` until it answers `-1`, answering
// each request with `respond`:
//
//     var srv = server.open(heap, poller, listener, size, chunk, idle);
//     while true {
//         srv = server.wait(heap, srv, clock, listener, 1000);
//         while next(srv) >= 0 { ... server.head/parsed/body ...; server.respond(srv, answer); }
//     }
//
// A callback would have been the obvious shape and does not type-check: a
// function value's type may mention only regions already in scope, and a
// request is a view of a buffer this loop borrows itself (`http-server.md` §2).
// So the views are borrows of the `Server`, valid where the application reads
// them, and the application's own state stays its own variables.
//
// Imports `std`, which makes this the first package that does
// (`docs/package-system.md` §4.8): publish it with `--std`.

module http.server;

import std.buffer;
import std.conns;
import std.http;
import std.json;

// Connections at once, at most.
fn max_connections() -> [] int {
    return 1024;
}

// How much input buffer all the connections may hold together: 256 MiB, so a
// bigger per-connection buffer means fewer connections rather than more memory.
fn input_budget() -> [] int {
    return 268435456;
}

// What one connection may have waiting to be sent. One whole answer and a
// little more; a connection that cannot even do that is closed.
fn output_size() -> [] int {
    return 65536;
}

// How many connections fit: `max_connections`, or what the budget allows at
// this buffer size if that is fewer.
fn connection_limit(size: int) -> [] int {
    var limit = input_budget() / size;
    if limit > max_connections() {
        limit = max_connections();
    }
    return limit;
}

// Per connection `k`, `state[16k..16k+16]` is:
//
//      0  bytes of input buffered
//      1  the time of its last progress, in seconds
//      2  bytes of output waiting (-1: it could not be held)
//      3  1 if it is to close once that output has gone
//      4  1 if the slot is in use
//      5  what it is watched for (1 to read, 2 to write)
//      6  bytes of the buffer already answered, this visit
//      7  the head of the request in hand, in bytes
//      8  the bytes after the head that the request consumed
//      9  the body's length, as the application sees it
//     10  1 if that body was decoded into `decoded`, 0 if it is in the buffer
//     11  1 if the connection must be dropped now
//     12  1 if it is in `ready`
//     13  1 if the request in the front of the buffer is held: handed to the application
//         and not yet answered (`hold`)
//     14  how many connections have used this slot, so a held request's ticket can tell
//         the connection it was taken from from a later one in the same slot
fn stride() -> [] int {
    return 16;
}

// Everything but the connection table, which `wait` has to take out of the
// server to grow (`std.conns.put` consumes it).
res struct Core {
    poller: Poller,
    // `poller_wait` fills these with (token, readiness) pairs.
    events: Box[[int]],
    state: Box[[int]],
    // One input buffer per connection, one slab.
    bufs: Box[[byte]],
    // One output buffer per connection, one slab.
    pends: Box[[byte]],
    // The parse table of the request in hand: `std.http` fills it.
    parsed: Box[[int]],
    // Where a chunked body is decoded: one request at a time uses it.
    decoded: Box[[byte]],
    // Connections that hold input, in the order `next` visits them.
    ready: Box[[int]],
    limit: int,
    size: int,
    chunk: int,
    idle: int,
    nready: int,
    cursor: int,
    // The connection `next` is draining, or -1.
    cur: int,
    last_sweep: int,
    // What `poller_wait` reported for tokens the application registered itself (above
    // `limit`): (token, readiness) pairs, `nforeign` of them, from the last `wait`.
    foreign: Box[[int]],
    nforeign: int,
}

pub res struct Server {
    tab: conns.Table,
    core: Core,
}

// ---------------------------------------------------------------------
// Answers
// ---------------------------------------------------------------------

// A whole JSON response: the head, then `body`.
pub fn reply[&h, &b](heap: &!h Heap, out: buffer.Buffer, status: int, body: &b [byte], keep: bool) -> [heap] buffer.Buffer {
    return reply_with(heap, out, status, body, keep, "");
}

// `reply`, with extra header lines in the head.
pub fn reply_with[&h, &b, &x](heap: &!h Heap, out: buffer.Buffer, status: int, body: &b [byte], keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
    return reply_as(heap, out, status, "application/json", body, keep, extra);
}

// A whole response of any content type -- `application/problem+json`,
// `text/plain` -- with extra header lines (`Location: /users/7\r\n`) in the head.
pub fn reply_as[&h, &c, &b, &x](heap: &!h Heap, out: buffer.Buffer, status: int, content_type: &c [byte], body: &b [byte], keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
    let head = http.respond_head_with(heap, out, status, content_type, len(body), keep, extra);
    return buffer.append(heap, head, body);
}

// A response with no body: `204 No Content` or `304 Not Modified`, which carry
// no `Content-Length` and no `Content-Type` (`std.http.respond_no_content`).
pub fn reply_empty[&h, &x](heap: &!h Heap, out: buffer.Buffer, status: int, keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
    return http.respond_no_content(heap, out, status, keep, extra);
}

// `{"error": message}`.
pub fn failure[&h, &m](heap: &!h Heap, out: buffer.Buffer, status: int, message: &m [byte], keep: bool) -> [heap] buffer.Buffer {
    return failure_with(heap, out, status, message, keep, "");
}

// `failure`, with extra header lines (`Allow: GET\r\n`) in the head.
pub fn failure_with[&h, &m, &x](heap: &!h Heap, out: buffer.Buffer, status: int, message: &m [byte], keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
    var w = json.writer(heap, 64);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "error");
    w = json.put_string(heap, w, message);
    w = json.end_object(heap, w);
    let body = json.finish(w);
    var answer = out;
    borrow body as &bb in {
        answer = reply_with(heap, answer, status, buffer.bytes(bb), keep, extra);
    }
    buffer.drop(heap, body);
    return answer;
}

// ---------------------------------------------------------------------
// One connection's bytes
// ---------------------------------------------------------------------

// How many of `wanted` bytes one write is handed: all of them, or `chunk` of
// them if there is a limit (`chunk` of 0 means none).
fn quantum(wanted: int, chunk: int) -> [] int {
    if chunk > 0 && wanted > chunk {
        return chunk;
    }
    return wanted;
}

// Hand `data` to the kernel without waiting for room, and keep what it did not
// take.
//
// `pend[0..pending]` is what this connection already has waiting. If there is
// none, the answer is offered to the connection straight away -- the common
// case, and the whole of the fast path; if there is some, the new bytes must
// queue behind it or the answers would arrive out of order. Whatever was not
// sent is appended to `pend`. Answers the new `pending`, or -1 if the queue
// cannot hold it, which is a client that is not reading and has asked for more
// than the buffer -- or a connection that has failed: the caller closes it.
//
// A connection that takes nothing (`Again`) is the kernel being full, which is
// the one case this queue exists for.
fn emit[&c, &d, &e](table: &!c conns.Table, slot: int, chunk: int, data: &d [byte], pend: &!e [byte], pending: int) -> [conn_write] int {
    var at = 0;
    if pending == 0 {
        match conns.write(table, slot, data[0..quantum(len(data), chunk)]) {
            Sent::Wrote(n) => {
                at = n;
            }
            Sent::Again => {
            }
            Sent::Failed(e) => {
                return 0 - 1;
            }
        }
    }
    if at >= len(data) {
        return pending;
    }
    if pending + len(data) - at > len(pend) {
        return 0 - 1;
    }
    var i = at;
    while i < len(data) {
        pend[pending + i - at] = data[i];
        i = i + 1;
    }
    return pending + len(data) - at;
}

// Close connection `k` and give its slot back.
fn shut[&t, &c](tab: &!t conns.Table, core: &!c Core, k: int) -> [] int {
    let st = contents(core.state);
    conns.close(tab, k);
    st[stride() * k + 4] = 0;
    st[stride() * k + 12] = 0;
    st[stride() * k + 13] = 0;
    return 0;
}

// Watch `k` for what it now waits on: room to write if output is queued (and
// read no more), input otherwise.
fn settle[&t, &c](tab: &!t conns.Table, core: &!c Core, k: int) -> [poll] int {
    let st = contents(core.state);
    let p = stride() * k;
    var want = 1;
    if st[p + 2] > 0 {
        want = 2;
    } else if st[p + 13] == 1 && st[p] >= core.size {
        // A held request and a full buffer behind it: nothing to read into, so nothing
        // to be woken for -- except the peer going away, which the poller reports anyway.
        want = 0;
    }
    if want != st[p + 5] {
        conns.rewatch(tab, core.poller, k, k + 1, want);
        st[p + 5] = want;
    }
    return 0;
}

// One step of I/O on connection `k`, which the poller said was ready: send
// what is waiting if anything is, otherwise read. Answers 1 if there is input
// to answer now, 2 if the connection is to close, 0 if nothing more can be done
// until the next wakeup.
fn step[&t, &c](tab: &!t conns.Table, core: &!c Core, k: int, now: int) -> [conn_read, conn_write] int {
    let st = contents(core.state);
    let bf = contents(core.bufs);
    let pd = contents(core.pends);
    let size = core.size;
    let osize = output_size();
    let p = stride() * k;
    let base = k * size;
    let obase = k * osize;
    var code = 0;
    if st[p + 2] > 0 {
        // Waiting to send: the kernel can take more.
        match conns.write(tab, k, pd[obase..obase + quantum(st[p + 2], core.chunk)]) {
            Sent::Wrote(sent) => {
                st[p + 1] = now;
                var at = 0;
                while at < st[p + 2] - sent {
                    pd[obase + at] = pd[obase + sent + at];
                    at = at + 1;
                }
                st[p + 2] = st[p + 2] - sent;
                if st[p + 2] == 0 {
                    if st[p + 3] == 1 {
                        code = 2;
                    } else if st[p] > 0 {
                        // Held back while the last answer was being sent.
                        code = 1;
                    }
                }
            }
            Sent::Again => {
            }
            Sent::Failed(e) => {
                code = 2;
            }
        }
    } else if st[p] >= size {
        // No room to read into. Only a held request can leave the buffer full (anything
        // else is refused first), and `settle` then asks to be woken for nothing: what
        // wakes it is the peer hanging up.
        code = 2;
    } else {
        match conns.read(tab, k, bf[base + st[p]..base + size]) {
            Received::Data(got) => {
                st[p] = st[p] + got;
                st[p + 1] = now;
                code = 1;
            }
            Received::End => {
                code = 2;
            }
            Received::Again => {
            }
            Received::Failed(e) => {
                code = 2;
            }
        }
    }
    return code;
}

// Parse the next request at the front of connection `k`'s input, if there is
// a whole one, and leave it in hand for the application: `state[7..11]` says
// where its head ends and what its body is, `parsed` is its parse table.
//
// Answers 1 for a request in hand, 0 for none. A request that cannot be served
// -- one the parser refuses, or one that could never fit the buffer -- is
// refused here with the status that says why, and the connection closes once
// that has been sent: after a malformed request there is no saying where the
// next one starts.
//
// Nothing is taken from a connection that has output waiting (back-pressure:
// no more requests from a connection that is not taking its answers) or that is
// ending, and a body that has not all arrived is waited for.
fn produce[&h, &t, &c](heap: &!h Heap, tab: &!t conns.Table, core: &!c Core, k: int) -> [heap, conn_write] int {
    let st = contents(core.state);
    let p = stride() * k;
    if st[p + 2] != 0 || st[p + 3] != 0 || st[p + 13] == 1 {
        return 0;
    }
    let size = core.size;
    let base = k * size;
    let data = contents(core.bufs)[base..base + size];
    let scratch = contents(core.decoded);
    let table = contents(core.parsed);
    let filled = st[p];
    let used = st[p + 6];
    if used >= filled {
        return 0;
    }
    let view = data[used..filled];
    let n = http.parse(view, table);
    // What to refuse with, or 0 to carry on.
    var refuse = 0;
    var message = "bad request";
    var ready = 0;
    if n < 0 {
        if http.is_incomplete(n) {
            // Nothing wrong yet -- unless there is no room for the rest.
            if used == 0 && filled >= size {
                refuse = 431;
                message = "request head too large";
            }
        } else {
            refuse = 400;
            message = http.error_message(http.error_code(n));
        }
    } else if http.is_chunked(table) {
        let (took, decoded) = http.dechunk(view[n..filled], scratch);
        if http.dechunk_incomplete(took) {
            // The body is still arriving -- unless it could never fit.
            if used == 0 && filled >= size {
                refuse = 413;
                message = "request too large";
            }
        } else if took < 0 {
            refuse = 400;
            if took == 0 - 4 {
                refuse = 413;
            }
            message = http.dechunk_message(took);
        } else {
            st[p + 7] = n;
            st[p + 8] = took;
            st[p + 9] = decoded;
            st[p + 10] = 1;
            ready = 1;
        }
    } else {
        let length = http.content_length(table);
        var body_length = 0;
        if length > 0 {
            body_length = length;
        }
        if n + body_length > size {
            refuse = 413;
            message = "request too large";
        } else if used + n + body_length <= filled {
            st[p + 7] = n;
            st[p + 8] = body_length;
            st[p + 9] = body_length;
            st[p + 10] = 0;
            ready = 1;
        }
        // Otherwise the body is still arriving.
    }
    if refuse != 0 {
        let pd = contents(core.pends);
        var o = buffer.empty(heap, 256);
        o = failure(heap, o, refuse, message, false);
        borrow o as &ob in {
            st[p + 2] = emit(tab, k, core.chunk, buffer.bytes(ob), pd[k * output_size()..(k + 1) * output_size()], st[p + 2]);
        }
        buffer.drop(heap, o);
        if st[p + 2] < 0 {
            st[p + 11] = 1;
        }
        st[p + 3] = 1;
        return 0;
    }
    return ready;
}

// Connection `k` has no more requests to give this round: move what is left of
// its input (the start of a request still arriving, or requests held back) to
// the front, then close it or set what it is watched for.
fn finish[&t, &c](tab: &!t conns.Table, core: &!c Core, k: int) -> [poll] int {
    let st = contents(core.state);
    let bf = contents(core.bufs);
    let p = stride() * k;
    let base = k * core.size;
    st[p + 12] = 0;
    if st[p + 11] == 1 {
        shut(tab, core, k);
        return 0;
    }
    let used = st[p + 6];
    if used > 0 {
        var at = 0;
        while at < st[p] - used {
            bf[base + at] = bf[base + used + at];
            at = at + 1;
        }
        st[p] = st[p] - used;
        st[p + 6] = 0;
    }
    if st[p + 3] == 1 && st[p + 2] == 0 {
        shut(tab, core, k);
    } else {
        settle(tab, core, k);
    }
    return 0;
}

// The next connection with a request in hand, or -1.
fn advance[&h, &t, &c](heap: &!h Heap, tab: &!t conns.Table, core: &!c Core) -> [heap, conn_write, poll] int {
    let st = contents(core.state);
    let queue = contents(core.ready);
    var found = 0 - 1;
    var going = true;
    while going {
        if core.cur >= 0 {
            if produce(heap, tab, core, core.cur) == 1 {
                found = core.cur;
                going = false;
            } else {
                finish(tab, core, core.cur);
                core.cur = 0 - 1;
            }
        } else if core.cursor >= core.nready {
            going = false;
        } else {
            let k = queue[core.cursor];
            core.cursor = core.cursor + 1;
            // It may have been closed since it was queued.
            if st[stride() * k + 4] == 1 {
                core.cur = k;
                st[stride() * k + 6] = 0;
            }
        }
    }
    return found;
}

// Take every connection waiting on the listener, up to the limit: each goes
// in the table, is made non-blocking, and is watched for input under the token
// `slot + 1` (the listener is token 0).
fn accept_all[&h, &l, &c](heap: &!h Heap, conn: conns.Table, listener: &!l Listener, core: &!c Core, now: int) -> [heap, conn_accept, poll] conns.Table {
    let st = contents(core.state);
    var table = conn;
    var more = true;
    while more {
        match tcp_accept(listener) {
            Accepted::Ok(c) => {
                var held = 0;
                borrow table as &tt in {
                    held = conns.live(tt);
                }
                if held >= core.limit {
                    conn_close(c);
                } else {
                    let (grown, slot) = conns.put(heap, table, c);
                    table = grown;
                    if slot >= 0 {
                        let p = stride() * slot;
                        st[p] = 0;
                        st[p + 1] = now;
                        st[p + 2] = 0;
                        st[p + 3] = 0;
                        st[p + 4] = 1;
                        st[p + 5] = 1;
                        st[p + 6] = 0;
                        st[p + 11] = 0;
                        st[p + 12] = 0;
                        st[p + 13] = 0;
                        st[p + 14] = st[p + 14] + 1;
                        borrow mut table as &!ct in {
                            if conns.nonblocking(ct, slot) != 0 || conns.watch(ct, core.poller, slot, slot + 1, 1) != 0 {
                                shut(ct, core, slot);
                            }
                        }
                    }
                }
            }
            Accepted::Again => {
                more = false;
            }
            Accepted::Failed(e) => {
                more = false;
            }
        }
    }
    return table;
}

// What the poller woke for, and the idle sweep: step every connection that
// woke, queue the ones with input, and once a second close the ones that have
// gone quiet.
fn serve_events[&t, &c](tab: &!t conns.Table, core: &!c Core, ready: int, now: int) -> [conn_read, conn_write, poll] int {
    let st = contents(core.state);
    let ev = contents(core.events);
    let queue = contents(core.ready);
    let outside = contents(core.foreign);
    var j = 0;
    while j < ready {
        let token = ev[2 * j];
        let k = token - 1;
        if token > core.limit {
            // The application's own handle (`first_token`): not a connection of ours, and
            // `state` has no slot for it.
            if 2 * core.nforeign + 1 < len(outside) {
                outside[2 * core.nforeign] = token;
                outside[2 * core.nforeign + 1] = ev[2 * j + 1];
                core.nforeign = core.nforeign + 1;
            }
        } else if token > 0 && st[stride() * k + 4] == 1 {
            let code = step(tab, core, k, now);
            if code == 2 {
                shut(tab, core, k);
            } else if code == 1 {
                if st[stride() * k + 12] == 0 {
                    st[stride() * k + 12] = 1;
                    queue[core.nready] = k;
                    core.nready = core.nready + 1;
                }
            } else {
                settle(tab, core, k);
            }
        }
        j = j + 1;
    }
    // Once a second: close the connections that have gone quiet.
    if now != core.last_sweep {
        core.last_sweep = now;
        var s = 0;
        while s < conns.slots(tab) {
            if st[stride() * s + 4] == 1 && st[stride() * s + 13] == 0 && now - st[stride() * s + 1] > core.idle {
                shut(tab, core, s);
            }
            s = s + 1;
        }
    }
    return 0;
}

// ---------------------------------------------------------------------
// The interface
// ---------------------------------------------------------------------

// A server for connections on `listener` (already listening and non-blocking),
// waited on through `poller`, which it owns from now on. `size` is each
// connection's input buffer -- a request, head and body together, must fit in
// it -- and the number of connections that fit shrinks with it so the buffers
// together never pass 256 MiB. `chunk` is a limit on how many bytes one write
// is handed (0: none), and `idle` how many seconds a connection may go without
// progress -- a byte read, a byte sent -- before it is closed.
pub fn open[&h, &l](heap: &!h Heap, poller: Poller, listener: &!l Listener, size: int, chunk: int, idle: int) -> [heap] Server {
    let limit = connection_limit(size);
    var p = poller;
    borrow mut p as &!pw in {
        poller_add_listener(pw, listener, 0);
    }
    let core = Core { poller: p, events: box_slice(heap, 128, 0), state: box_slice(heap, stride() * limit, 0), bufs: box_slice(heap, limit * size, byte_of(0)), pends: box_slice(heap, limit * output_size(), byte_of(0)), parsed: box_slice(heap, http.slots(64), 0), decoded: box_slice(heap, size, byte_of(0)), ready: box_slice(heap, limit, 0), limit: limit, size: size, chunk: chunk, idle: idle, nready: 0, cursor: 0, cur: 0 - 1, last_sweep: 0, foreign: box_slice(heap, 128, 0), nforeign: 0 };
    return Server { tab: conns.empty(heap, 64), core: core };
}

// End the server: every connection still open is closed, the poller ended and
// the buffers freed.
pub fn close[&h](heap: &!h Heap, srv: Server) -> [heap] int {
    let Server { tab, core } = srv;
    conns.drop(heap, tab);
    let Core { poller, events, state, bufs, pends, parsed, decoded, ready, limit, size, chunk, idle, nready, cursor, cur, last_sweep, foreign, nforeign } = core;
    poller_close(poller);
    unbox_slice(heap, events);
    unbox_slice(heap, state);
    unbox_slice(heap, bufs);
    unbox_slice(heap, pends);
    unbox_slice(heap, parsed);
    unbox_slice(heap, decoded);
    unbox_slice(heap, ready);
    unbox_slice(heap, foreign);
    return 0;
}

// How many connections are open.
pub fn connections[&s](srv: &s Server) -> [] int {
    return conns.live(srv.tab);
}

// Wait for the network, at most `timeout_ms`, and do the I/O that is ready:
// take new connections, read, send what was waiting, close the ones that have
// gone quiet. Connections that now hold input are queued for `next`.
//
// By value, because accepting replaces the connection table.
//
// `next` must be called until it answers -1 before the next `wait`. If it was
// not, the connection it was part-way through is finished and the ones it had
// not reached stay queued, and the wait does not block.
pub fn wait[&h, &k, &l](heap: &!h Heap, srv: Server, clock: &k Clock, listener: &!l Listener, timeout_ms: int) -> [heap, conn_accept, conn_read, conn_write, poll, clock] Server {
    let Server { tab, core } = srv;
    var table = tab;
    var state = core;
    var timeout = timeout_ms;
    borrow mut table as &!tw in {
        borrow mut state as &!cw in {
            let st = contents(cw.state);
            let queue = contents(cw.ready);
            cw.nforeign = 0;
            // What `next` did not reach goes to the front.
            var left = 0;
            while cw.cursor + left < cw.nready {
                queue[left] = queue[cw.cursor + left];
                left = left + 1;
            }
            cw.nready = left;
            cw.cursor = 0;
            // The connection it was part-way through is finished -- and, if
            // requests are still buffered behind the one answered, queued again:
            // no input will arrive to wake it for them.
            let half = cw.cur;
            if half >= 0 {
                cw.cur = 0 - 1;
                finish(tw, cw, half);
                let p = stride() * half;
                if st[p + 4] == 1 && st[p] > 0 && st[p + 2] == 0 && st[p + 3] == 0 {
                    st[p + 12] = 1;
                    queue[cw.nready] = half;
                    cw.nready = cw.nready + 1;
                    left = left + 1;
                }
            }
            if left > 0 {
                timeout = 0;
            }
        }
    }
    var ready = 0 - 1;
    borrow mut state as &!cw in {
        ready = poller_wait(cw.poller, contents(cw.events), timeout);
    }
    let now = clock_ms(clock) / 1000;
    if ready >= 0 {
        // New connections first, while the table is ours to grow.
        var j = 0;
        while j < ready {
            var token = 0 - 1;
            borrow state as &cr in {
                token = contents(cr.events)[2 * j];
            }
            if token == 0 {
                borrow mut state as &!cw in {
                    table = accept_all(heap, table, listener, cw, now);
                }
            }
            j = j + 1;
        }
        borrow mut table as &!tw in {
            borrow mut state as &!cw in {
                serve_events(tw, cw, ready, now);
            }
        }
    }
    return Server { tab: table, core: state };
}

// The next request, in hand, or -1 when no connection has a whole one left.
// The answer is the connection's slot, which is only for telling requests
// apart: everything to do with it goes through `head`, `parsed`, `body` and
// `respond`, which act on the request in hand.
//
// Refusals the loop owns -- a request the parser rejects, one that cannot fit
// -- are answered here and never reach the application.
pub fn next[&h, &s](heap: &!h Heap, srv: &!s Server) -> [heap, conn_write, poll] int {
    return advance(heap, srv.tab, srv.core);
}

// The request in hand: its request line and headers, as `std.http` reads them
// with `parsed`.
pub fn head[&s](srv: &s Server) -> [] &s [byte] {
    let core = srv.core;
    let st = contents(core.state);
    let p = stride() * core.cur;
    let from = core.cur * core.size + st[p + 6];
    return contents(core.bufs)[from..from + st[p + 7]];
}

// The parse table of the request in hand.
pub fn parsed[&s](srv: &s Server) -> [] &s [int] {
    return contents(srv.core.parsed);
}

// The body of the request in hand, whole: a chunked body is already decoded,
// so the application cannot tell the two apart. Empty if there is none.
pub fn body[&s](srv: &s Server) -> [] &s [byte] {
    let core = srv.core;
    let st = contents(core.state);
    let p = stride() * core.cur;
    if st[p + 10] == 1 {
        return contents(core.decoded)[0..st[p + 9]];
    }
    let from = core.cur * core.size + st[p + 6] + st[p + 7];
    return contents(core.bufs)[from..from + st[p + 9]];
}

// Answer the request in hand. The bytes are offered to the connection at once
// if nothing is queued ahead of them and otherwise queued behind it, so answers
// leave in the order of their requests; what the kernel did not take waits in
// the connection's output buffer and the connection is watched for room instead
// of input. A connection that asked for `Connection: close`, or whose answer
// could not be held at all (a client that is not reading and has asked for more
// than the buffer), is closed once its output is gone, or at once.
//
// Answers the bytes now waiting, or -1 if the connection was abandoned.
pub fn respond[&s, &a](srv: &!s Server, answer: &a [byte]) -> [conn_write] int {
    return deliver(srv.tab, srv.core, answer);
}

fn deliver[&t, &c, &a](tab: &!t conns.Table, core: &!c Core, answer: &a [byte]) -> [conn_write] int {
    return deliver_to(tab, core, core.cur, answer);
}

// `deliver` for connection `k`, which is the one in hand or one whose request was held.
fn deliver_to[&t, &c, &a](tab: &!t conns.Table, core: &!c Core, k: int, answer: &a [byte]) -> [conn_write] int {
    if k < 0 {
        return 0 - 1;
    }
    let st = contents(core.state);
    let pd = contents(core.pends);
    let p = stride() * k;
    let osize = output_size();
    st[p + 2] = emit(tab, k, core.chunk, answer, pd[k * osize..(k + 1) * osize], st[p + 2]);
    if st[p + 2] < 0 {
        st[p + 11] = 1;
        return 0 - 1;
    }
    if !http.keeps_alive(contents(core.parsed)) {
        st[p + 3] = 1;
    }
    // The request is answered: the next starts after it.
    st[p + 6] = st[p + 6] + st[p + 7] + st[p + 8];
    return st[p + 2];
}

// ---------------------------------------------------------------------
// A request that is answered later
// ---------------------------------------------------------------------

// A held request is named by a ticket: the connection's slot and how many connections
// have used that slot, so that an answer for a connection that has since gone cannot
// reach whichever one took its place. The slot is `ticket % ticket_span()`.
fn ticket_span() -> [] int {
    return 2048;
}

// The slot a ticket names, to index the application's own per-connection records with.
pub fn ticket_slot(ticket: int) -> [] int {
    return ticket % ticket_span();
}

// Leave the request in hand unanswered and go on with the others. Answers its ticket, or -1
// if there is no request in hand.
//
// What stays with the application is the ticket and whatever it noted about the request
// (its route, the id in its path); what goes is the request's *views*: `head`, `parsed` and
// `body` are for the request in hand, and the next `next` replaces it. The request itself
// stays whole at the front of the connection's buffer. Nothing more is taken from the
// connection until `answer`; what the client sends meanwhile is buffered (a client that
// sends more than the buffer holds is read no more, and closed if it hangs up), and a
// held connection is not closed for being idle: the application's own timeout says how
// long it will wait. A held connection that is closed -- the client left -- makes `answer`
// answer -1.
pub fn hold[&s](srv: &!s Server) -> [poll] int {
    return hold_in(srv.tab, srv.core);
}

fn hold_in[&t, &c](tab: &!t conns.Table, core: &!c Core) -> [poll] int {
    let k = core.cur;
    if k < 0 {
        return 0 - 1;
    }
    let st = contents(core.state);
    let p = stride() * k;
    st[p + 13] = 1;
    core.cur = 0 - 1;
    // As if the visit were over: what was answered before it is dropped from the buffer and
    // the held request becomes its front.
    finish(tab, core, k);
    return st[p + 14] * ticket_span() + k;
}

// Answer a held request, as `respond` would have answered it in hand. Answers the bytes
// now waiting, or -1: the ticket is not one that was handed out and is still held (it was
// answered already, or its connection closed, or another has taken its slot), or the
// connection could not be written to and was abandoned.
//
// The connection goes back to being served: if the client had sent more requests behind
// this one they are given by `next` now.
pub fn answer[&s, &a](srv: &!s Server, ticket: int, bytes: &a [byte]) -> [conn_write, poll] int {
    return answer_in(srv.tab, srv.core, ticket, bytes);
}

fn answer_in[&t, &c, &a](tab: &!t conns.Table, core: &!c Core, ticket: int, bytes: &a [byte]) -> [conn_write, poll] int {
    if ticket < 0 {
        return 0 - 1;
    }
    let k = ticket % ticket_span();
    if k >= core.limit {
        return 0 - 1;
    }
    let st = contents(core.state);
    let p = stride() * k;
    if st[p + 4] != 1 || st[p + 13] != 1 || st[p + 14] != ticket / ticket_span() {
        return 0 - 1;
    }
    // The parse table holds whichever request came after; this one is still whole at the
    // front of the buffer, so it parses again (`deliver` reads whether to keep alive from it).
    let base = k * core.size;
    http.parse(contents(core.bufs)[base..base + st[p]], contents(core.parsed));
    st[p + 13] = 0;
    let sent = deliver_to(tab, core, k, bytes);
    finish(tab, core, k);
    if st[p + 4] == 1 && st[p] > 0 && st[p + 2] == 0 && st[p + 3] == 0 && st[p + 11] == 0 && st[p + 12] == 0 {
        // More requests were already buffered behind it, and no input will arrive to wake
        // the connection for them.
        let queue = contents(core.ready);
        if core.nready >= core.limit {
            var left = 0;
            while core.cursor + left < core.nready {
                queue[left] = queue[core.cursor + left];
                left = left + 1;
            }
            core.nready = left;
            core.cursor = 0;
        }
        st[p + 12] = 1;
        queue[core.nready] = k;
        core.nready = core.nready + 1;
    }
    return sent;
}

// ---------------------------------------------------------------------
// Handles of the application's own
// ---------------------------------------------------------------------

// The server's poller, to register handles with (`std.conns.watch`) that are not its own
// connections -- a database connection, say -- so that one `wait` serves both.
pub fn poller[&s](srv: &!s Server) -> [] &!s Poller {
    return srv.core.poller;
}

// The first token the application may register a handle under: the server uses 0 for the
// listener and 1 to `limit` for its connections. Token `first_token(srv) + i` is the
// application's `i`-th.
pub fn first_token[&s](srv: &s Server) -> [] int {
    return srv.core.limit + 1;
}

// What the last `wait` reported for those handles: `foreign_count` pairs, flat, of (token,
// readiness) -- 1 for readable (or hung up), 2 for writable. The server does nothing for
// them but tell the application.
pub fn foreign_count[&s](srv: &s Server) -> [] int {
    return srv.core.nforeign;
}

pub fn foreign[&s](srv: &s Server) -> [] &s [int] {
    return contents(srv.core.foreign)[0..2 * srv.core.nforeign];
}
