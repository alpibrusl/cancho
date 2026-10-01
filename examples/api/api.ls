// `api` -- a JSON API server: keep-alive, pipelining, routed, one thread.
//
//     api <port> [reuseport] [idle-seconds] [send-chunk-bytes]
//
// `docs/server.md` is the design. This is the program the last four
// library pieces were for: `std.http` parses each request, `std.route`
// picks the handler, `std.json` reads and writes the bodies, and the loop
// around them is `poll(2)` over every open connection, so a connection
// that is open and silent costs a slot and nothing else -- one slow client
// cannot make another wait.
//
// What it answers:
//
//     GET  /health        {"ok":true}
//     GET  /users/:id     {"id":42,"name":"user-42"}      400 if :id is not a number
//     POST /add           {"a":1,"b":2}  ->  {"sum":3}     422 if the body is not that
//     GET  /search?q=...  {"q":"a b","length":3}           q is percent-decoded
//     GET  /blob/:n       n bytes of a-z repeating (n <= 32768)   a large answer, to watch a slow reader
//
// and a JSON `{"error":...}` for everything else: 404 for a path no route
// has, 405 for a path with another method, 400 for a request the parser
// refuses (and the connection closes: after a malformed request there is no
// saying where the next one starts), 413 for a body that cannot fit, 431
// for a head that cannot.
//
// **Limits, all constants, all said aloud:** 1024 connections, 16 KiB of
// input buffer each (a request, head and body together, must fit in it),
// 64 KiB of output buffer each, nine seconds without progress before a
// connection is closed unless the third argument says otherwise.
//
// The fourth argument is a quantum: no `send` is handed more than that many
// bytes, so an answer larger than it takes several, which is what a loop that
// wants to be fair to its other connections might choose. It is also how the
// tests make partial sends certain: on Linux a loopback `send` is whole or
// refused, never partial, and the code that resumes a partial one would
// otherwise never run.
//
// **Writes never block.** Every `send` carries `MSG_DONTWAIT`, so one call
// returns at once with whatever the kernel took. What it did not take waits in
// the connection's own output buffer, the connection asks `poll` for
// `POLLOUT` instead of `POLLIN` -- it is not read from until its output is
// gone, which is what keeps a client that never reads from making the server
// buffer without bound -- and every other connection is served meanwhile.
// A client that makes no progress for `idle` seconds is closed.
//
// **What it does not do**, and `docs/server.md` §6 is the list: decode a
// chunked body (it answers 501), or use more than one core -- `reuseport`
// lets several copies of this program share a port and the kernel spreads the
// connections, which is how it scales.
//
// The sockets come from the `net.sockets` package; `poll`, `signal` and
// `time` are declared here because nothing else wants them yet.

import std.buffer;
import std.bytes;
import std.http;
import std.io;
import std.json;
import std.route;
import net.sockets;

// `poll(struct pollfd *fds, nfds_t nfds, int timeout)`. A foreign slice is
// passed as pointer and length, and only a `[byte]` slice may cross, so the
// array of 8-byte records is a byte array and the slice handed over is a
// *prefix of it whose length is the number of records*: C reads `nfds`
// records from the pointer and the allocation behind it is eight times as
// long as the slice says.
extern fn poll[&f, &p](ffi: &f Ffi("libc"), fds: &!p [byte], timeout: int) -> [ffi("libc")] c_int;

// `signal(SIGPIPE, SIG_IGN)`: writing to a connection the peer has closed
// would otherwise kill the process. 13 and 1 are the same on Linux and macOS.
extern fn signal[&f](ffi: &f Ffi("libc"), sig: int, handler: int) -> [ffi("libc")] int;

extern fn time[&f](ffi: &f Ffi("libc"), t: int) -> [ffi("libc")] int;

// `send(fd, buf, len, flags)`. Unlike `fcntl(F_SETFL, O_NONBLOCK)`, which is
// variadic -- and variadic arguments are not passed like fixed ones on Apple
// arm64, so declaring it here would be wrong on one of the two targets --
// `send` has a fixed signature, and `MSG_DONTWAIT` makes one call
// non-blocking without changing the socket.
extern fn send[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &b [byte], flags: int) -> [ffi("libc")] int;

fn max_connections() -> [] int {
    return 1024;
}

fn buffer_size() -> [] int {
    return 16384;
}

// What one connection may have waiting to be sent. The largest answer is a
// `/blob` of 32 KiB, so a connection holds one whole answer and a little
// more; one that cannot even do that is closed.
fn output_size() -> [] int {
    return 65536;
}

// A decimal number, or -1 for empty text, a non-digit, or more than 17
// digits (which cannot be an id a `{"id":N}` should echo).
fn number_of[&t](text: &t [byte]) -> [] int {
    if len(text) == 0 || len(text) > 17 {
        return 0 - 1;
    }
    var n = 0;
    var i = 0;
    while i < len(text) {
        let c = int_of(text[i]);
        if c < 48 || c > 57 {
            return 0 - 1;
        }
        n = n * 10 + (c - 48);
        i = i + 1;
    }
    return n;
}

// ---------------------------------------------------------------------
// Answers
// ---------------------------------------------------------------------

// A whole response: the head, then `body`.
fn reply[&h, &b](heap: &!h Heap, out: buffer.Buffer, status: int, body: &b [byte], keep: bool) -> [heap] buffer.Buffer {
    let head = http.respond_head(heap, out, status, "application/json", len(body), keep);
    return buffer.append(heap, head, body);
}

// `{"error": message}`.
fn failure[&h, &m](heap: &!h Heap, out: buffer.Buffer, status: int, message: &m [byte], keep: bool) -> [heap] buffer.Buffer {
    var w = json.writer(heap, 64);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "error");
    w = json.put_string(heap, w, message);
    w = json.end_object(heap, w);
    let body = json.finish(w);
    var answer = out;
    borrow body as &bb in {
        answer = reply(heap, answer, status, buffer.bytes(bb), keep);
    }
    buffer.drop(heap, body);
    return answer;
}

fn user[&h, &p, &s](heap: &!h Heap, out: buffer.Buffer, path: &s [byte], params: &p [int], keep: bool) -> [heap] buffer.Buffer {
    let id = number_of(path[params[0]..params[1]]);
    if id < 0 {
        return failure(heap, out, 400, "id must be a number", keep);
    }
    var w = json.writer(heap, 64);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "id");
    w = json.put_int(heap, w, id);
    w = json.put_key(heap, w, "name");
    var name = buffer.append(heap, buffer.empty(heap, 24), "user-");
    name = buffer.push_nat(heap, name, id);
    borrow name as &nb in {
        w = json.put_string(heap, w, buffer.bytes(nb));
    }
    buffer.drop(heap, name);
    w = json.end_object(heap, w);
    let body = json.finish(w);
    var answer = out;
    borrow body as &bb in {
        answer = reply(heap, answer, 200, buffer.bytes(bb), keep);
    }
    buffer.drop(heap, body);
    return answer;
}

// `{"a": int, "b": int}` in, `{"sum": a + b}` out. Both must be present and
// integers; anything else is a 422 saying so, and a body that is not JSON
// says what is wrong with it.
fn add[&h, &b](heap: &!h Heap, out: buffer.Buffer, body: &b [byte], keep: bool) -> [heap] buffer.Buffer {
    let tape = box_slice(heap, json.tape_len(body), 0);
    var answer = out;
    borrow mut tape as &!tw in {
        let t = contents(tw);
        let nodes = json.parse(body, t);
        if nodes < 0 {
            answer = failure(heap, answer, 422, json.error_message(json.error_code(nodes)), keep);
        } else {
            let a = json.get(body, t, 0, "a");
            let b = json.get(body, t, 0, "b");
            if !json.is_int(t, a) || !json.is_int(t, b) || !json.fits_int(body, t, a) || !json.fits_int(body, t, b) {
                answer = failure(heap, answer, 422, "a and b must be integers", keep);
            } else {
                var w = json.writer(heap, 32);
                w = json.begin_object(heap, w);
                w = json.put_key(heap, w, "sum");
                w = json.put_int(heap, w, json.to_int(body, t, a) + json.to_int(body, t, b));
                w = json.end_object(heap, w);
                let reply_body = json.finish(w);
                borrow reply_body as &rb in {
                    answer = reply(heap, answer, 200, buffer.bytes(rb), keep);
                }
                buffer.drop(heap, reply_body);
            }
        }
    }
    unbox_slice(heap, tape);
    return answer;
}

// `n` bytes of `abcdefghijklmnopqrstuvwxyzabc...`: an answer as large as a
// test wants, within the output buffer. The body is not one repeated byte, so a
// byte sent twice or out of place changes it.
fn blob[&h, &p, &s](heap: &!h Heap, out: buffer.Buffer, path: &s [byte], params: &p [int], keep: bool) -> [heap] buffer.Buffer {
    let n = number_of(path[params[0]..params[1]]);
    if n < 0 || n > 32768 {
        return failure(heap, out, 400, "n must be a number up to 32768", keep);
    }
    var body = buffer.empty(heap, n + 1);
    var i = 0;
    while i < n {
        body = buffer.push(heap, body, byte_of('a' + i % 26));
        i = i + 1;
    }
    var answer = out;
    borrow body as &bb in {
        answer = reply(heap, answer, 200, buffer.bytes(bb), keep);
    }
    buffer.drop(heap, body);
    return answer;
}

fn search[&h, &q](heap: &!h Heap, out: buffer.Buffer, query: &q [byte], keep: bool) -> [heap] buffer.Buffer {
    let (from, to) = http.query_value(query, "q");
    if from < 0 {
        return failure(heap, out, 400, "q is required", keep);
    }
    var answer = out;
    region scratch {
        let decoded = alloc_slice[scratch](512, byte_of(0));
        let n = http.percent_decode(query[from..to], decoded, true);
        if n < 0 {
            answer = failure(heap, answer, 400, "q is not valid percent-encoding, or is too long", keep);
        } else {
            var w = json.writer(heap, 64);
            w = json.begin_object(heap, w);
            w = json.put_key(heap, w, "q");
            w = json.put_string(heap, w, decoded[0..n]);
            w = json.put_key(heap, w, "length");
            w = json.put_int(heap, w, n);
            w = json.end_object(heap, w);
            let body = json.finish(w);
            borrow body as &bb in {
                answer = reply(heap, answer, 200, buffer.bytes(bb), keep);
            }
            buffer.drop(heap, body);
        }
    }
    return answer;
}

// The route table. The ids are this program's own: `handle` matches on them.
fn routes[&h](heap: &!h Heap) -> [heap] route.Router {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/health", 1);
    r = route.add(heap, r, "GET", "/users/:id", 2);
    r = route.add(heap, r, "POST", "/add", 3);
    r = route.add(heap, r, "GET", "/search", 4);
    r = route.add(heap, r, "GET", "/blob/:n", 5);
    return r;
}

// One parsed request in, one response appended to `out`.
fn handle[&h, &r, &q, &t, &p, &b](heap: &!h Heap, router: &r route.Router, request: &q [byte], table: &t [int], params: &!p [int], body: &b [byte], out: buffer.Buffer) -> [heap] buffer.Buffer {
    let keep = http.keeps_alive(table);
    let path = http.path(request, table);
    let id = route.find(router, http.method(request, table), path, params);
    if id == 1 {
        return reply(heap, out, 200, "{\"ok\":true}", keep);
    }
    if id == 2 {
        return user(heap, out, path, params, keep);
    }
    if id == 3 {
        return add(heap, out, body, keep);
    }
    if id == 4 {
        return search(heap, out, http.query(request, table), keep);
    }
    if id == 5 {
        return blob(heap, out, path, params, keep);
    }
    if id == 0 - 2 {
        return failure(heap, out, 405, "method not allowed", keep);
    }
    return failure(heap, out, 404, "not found", keep);
}

// ---------------------------------------------------------------------
// One connection's bytes
// ---------------------------------------------------------------------

// How many of `wanted` bytes one `send` is handed: all of them, or `chunk` of
// them if there is a limit (`chunk` of 0 means none).
fn quantum(wanted: int, chunk: int) -> [] int {
    if chunk > 0 && wanted > chunk {
        return chunk;
    }
    return wanted;
}

// Hand `data` to the kernel without waiting, and keep what it did not take.
//
// `pend[0..pending]` is what this connection already has waiting. If there is
// none, the answer is offered to `send` straight away -- the common case, and
// the whole of the fast path; if there is some, the new bytes must queue behind
// it or the answers would arrive out of order. Whatever was not sent is
// appended to `pend`. Answers the new `pending`, or -1 if the queue cannot hold
// it, which is a client that is not reading and has asked for more than the
// buffer: the caller closes it.
//
// A `send` that fails is treated as "the kernel is full": the real error, if it
// is one, arrives as `POLLERR` or `POLLHUP` and the connection is closed then.
fn emit[&f, &d, &e](libc: &f Ffi("libc"), fd: int, mflag: int, chunk: int, data: &d [byte], pend: &!e [byte], pending: int) -> [ffi("libc")] int {
    var at = 0;
    if pending == 0 {
        let n = send(libc, fd, data[0..quantum(len(data), chunk)], mflag);
        if n > 0 {
            at = n;
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

// Answer the complete requests at the front of `data[0..filled]`, until one of
// them leaves output the kernel would not take.
//
// The answer is the bytes consumed: the caller moves the rest (the start of a
// request still arriving, or requests held back for now) to the front. `-1`
// means the connection must close *now*; `st[4 * k + 3]` set means it closes
// once its output has gone, which is what a refusal or `Connection: close`
// asks for.
//
// Pipelined requests -- several in one read -- are answered in order. It stops
// when an answer could not be sent whole (backpressure: no more requests are
// taken from a connection that is not taking its answers), when a body has not
// all arrived, and when a request could never fit the buffer, which is refused
// now rather than waited on for ever.
fn drain[&f, &h, &r, &d, &t, &p, &e, &s](libc: &f Ffi("libc"), heap: &!h Heap, router: &r route.Router, data: &!d [byte], filled: int, fd: int, mflag: int, chunk: int, table: &!t [int], params: &!p [int], pend: &!e [byte], st: &!s [int], k: int, out: buffer.Buffer) -> [ffi("libc"), heap] (buffer.Buffer, int) {
    var o = out;
    var used = 0;
    var abandon = false;
    var going = true;
    while going && used < filled {
        let view = data[used..filled];
        let n = http.parse(view, table);
        // What to refuse with, or 0 to carry on.
        var refuse = 0;
        var message = "bad request";
        if n < 0 {
            going = false;
            if http.is_incomplete(n) {
                // Nothing wrong yet -- unless there is no room for the rest.
                if used == 0 && filled >= len(data) {
                    refuse = 431;
                    message = "request head too large";
                }
            } else {
                refuse = 400;
                message = http.error_message(http.error_code(n));
            }
        } else if http.is_chunked(table) {
            refuse = 501;
            message = "chunked request bodies are not supported";
        } else {
            let length = http.content_length(table);
            var body_length = 0;
            if length > 0 {
                body_length = length;
            }
            if n + body_length > len(data) {
                refuse = 413;
                message = "request too large";
            } else if used + n + body_length > filled {
                // The body is still arriving.
                going = false;
            } else {
                borrow mut o as &!ob in {
                    buffer.clear(ob);
                }
                o = handle(heap, router, view, table, params, view[n..n + body_length], o);
                borrow o as &ob in {
                    st[4 * k + 2] = emit(libc, fd, mflag, chunk, buffer.bytes(ob), pend, st[4 * k + 2]);
                }
                if st[4 * k + 2] < 0 {
                    abandon = true;
                }
                if !http.keeps_alive(table) {
                    st[4 * k + 3] = 1;
                }
                used = used + n + body_length;
                // Output left over, or the connection is ending: take no more.
                if st[4 * k + 2] != 0 || st[4 * k + 3] != 0 {
                    going = false;
                }
            }
        }
        if refuse != 0 {
            borrow mut o as &!ob in {
                buffer.clear(ob);
            }
            o = failure(heap, o, refuse, message, false);
            borrow o as &ob in {
                st[4 * k + 2] = emit(libc, fd, mflag, chunk, buffer.bytes(ob), pend, st[4 * k + 2]);
            }
            if st[4 * k + 2] < 0 {
                abandon = true;
            }
            st[4 * k + 3] = 1;
            going = false;
        }
    }
    if abandon {
        return (o, 0 - 1);
    }
    return (o, used);
}

// ---------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------

// A poll record's `fd` and `events` (POLLIN, 1), written into slot `k`:
// `struct pollfd` is a 4-byte int, then two 2-byte shorts, little-endian on
// both targets.
fn set_record[&p](polls: &!p [byte], k: int, fd: int) -> [] int {
    polls[8 * k] = byte_of(fd & 255);
    polls[8 * k + 1] = byte_of(fd >> 8 & 255);
    polls[8 * k + 2] = byte_of(fd >> 16 & 255);
    polls[8 * k + 3] = byte_of(fd >> 24 & 255);
    polls[8 * k + 4] = byte_of(1);
    polls[8 * k + 5] = byte_of(0);
    polls[8 * k + 6] = byte_of(0);
    polls[8 * k + 7] = byte_of(0);
    return k;
}

// What to wait for on slot `k`: 1 (POLLIN) to read, 4 (POLLOUT) to send.
fn set_events[&p](polls: &!p [byte], k: int, events: int) -> [] int {
    polls[8 * k + 4] = byte_of(events);
    return k;
}

// Slot `to` takes over slot `from`: the descriptor and what it waits for.
fn move_record[&p](polls: &!p [byte], from: int, to: int) -> [] int {
    var i = 0;
    while i < 6 {
        polls[8 * to + i] = polls[8 * from + i];
        i = i + 1;
    }
    polls[8 * to + 6] = byte_of(0);
    polls[8 * to + 7] = byte_of(0);
    return to;
}

fn record_fd[&p](polls: &p [byte], k: int) -> [] int {
    return int_of(polls[8 * k]) + int_of(polls[8 * k + 1]) * 256 + int_of(polls[8 * k + 2]) * 65536 + int_of(polls[8 * k + 3]) * 16777216;
}

// What poll found for slot `k`: nonzero if anything happened to it (data,
// hang-up, error).
fn record_events[&p](polls: &p [byte], k: int) -> [] int {
    return int_of(polls[8 * k + 6]) + int_of(polls[8 * k + 7]) * 256;
}

// The listening socket: reusable, optionally shared (`reuse`), bound to every
// address on `port`. `-1` if any step fails.
//
// Both operating systems' spellings of `SO_REUSEADDR` and `SO_REUSEPORT` are
// tried, because the numbers differ between Linux (level 1; options 2 and 15)
// and macOS (level 0xffff; options 4 and 0x200) and the wrong pair is refused
// harmlessly by the kernel that does not know it.
fn listener[&f](libc: &f Ffi("libc"), port: int, reuse: bool) -> [ffi("libc")] int {
    let fd = sockets.socket(libc, 2, 1, 0);
    if fd < 0 {
        return 0 - 1;
    }
    var bound = fd;
    region scratch {
        let on = alloc_slice[scratch](4, byte_of(0));
        on[0] = byte_of(1);
        sockets.setsockopt(libc, fd, 1, 2, on);
        sockets.setsockopt(libc, fd, 65535, 4, on);
        if reuse {
            sockets.setsockopt(libc, fd, 1, 15, on);
            sockets.setsockopt(libc, fd, 65535, 512, on);
        }
        // `struct sockaddr_in`: AF_INET, the port big-endian, INADDR_ANY.
        let addr = alloc_slice[scratch](16, byte_of(0));
        addr[0] = byte_of(2);
        addr[2] = byte_of(port / 256);
        addr[3] = byte_of(port - port / 256 * 256);
        if sockets.bind(libc, fd, addr) < 0 {
            bound = 0 - 1;
        } else if sockets.listen(libc, fd, 1024) < 0 {
            bound = 0 - 1;
        }
    }
    if bound < 0 {
        sockets.close(libc, fd);
    }
    return bound;
}

// Whether this is Linux rather than macOS, found by asking: `SO_REUSEADDR` is
// level 1, option 2 there and means something else (or nothing) here. It picks
// `MSG_DONTWAIT`, which is 0x40 on Linux and 0x80 on macOS.
fn is_linux[&f](libc: &f Ffi("libc")) -> [ffi("libc")] bool {
    let fd = sockets.socket(libc, 2, 1, 0);
    var linux = false;
    if fd >= 0 {
        region scratch {
            let on = alloc_slice[scratch](4, byte_of(0));
            on[0] = byte_of(1);
            linux = sockets.setsockopt(libc, fd, 1, 2, on) == 0;
        }
        sockets.close(libc, fd);
    }
    return linux;
}

// Serve until killed. `idle` is how many seconds a connection may go without
// progress -- a byte read, a byte sent -- before it is closed.
//
// Per connection `k`, `st[4k..4k+4]` is: bytes of input buffered, the time of
// its last progress, bytes of output waiting, and 1 if it is to close once that
// output has gone.
fn serve[&f, &h, &r](libc: &f Ffi("libc"), heap: &!h Heap, router: &r route.Router, lfd: int, idle: int, chunk: int, mflag: int) -> [ffi("libc"), heap] int {
    let limit = max_connections();
    let size = buffer_size();
    let osize = output_size();
    let polls = box_slice(heap, 8 * (limit + 1), byte_of(0));
    let state = box_slice(heap, 4 * limit, 0);
    let bufs = box_slice(heap, limit * size, byte_of(0));
    let pends = box_slice(heap, limit * osize, byte_of(0));
    let table = box_slice(heap, http.slots(64), 0);
    var widest = 1;
    if route.most_params(router) > 1 {
        widest = route.most_params(router);
    }
    let params = box_slice(heap, 2 * widest, 0);
    var out = buffer.empty(heap, 4096);

    borrow mut polls as &!pw in {
        borrow mut state as &!sw in {
            borrow mut bufs as &!bw in {
                borrow mut pends as &!ow in {
                    borrow mut table as &!tw in {
                        borrow mut params as &!qw in {
                            let pl = contents(pw);
                            let st = contents(sw);
                            let bf = contents(bw);
                            let pd = contents(ow);
                            let tb = contents(tw);
                            let pr = contents(qw);
                            set_record(pl, 0, lfd);
                            var n = 0;
                            while true {
                                let ready = poll(libc, pl[0..n + 1], 1000);
                                let now = time(libc, 0);
                                if ready >= 0 {
                                    // A new connection, if the listener woke.
                                    if record_events(pl, 0) != 0 {
                                        let c = sockets.accept(libc, lfd, 0, 0);
                                        if c >= 0 {
                                            if n >= limit {
                                                sockets.close(libc, c);
                                            } else {
                                                set_record(pl, n + 1, c);
                                                st[4 * n] = 0;
                                                st[4 * n + 1] = now;
                                                st[4 * n + 2] = 0;
                                                st[4 * n + 3] = 0;
                                                n = n + 1;
                                            }
                                        }
                                    }
                                    // The connections, last first, so that closing one
                                    // (which moves the last into its place) only ever
                                    // moves one already looked at.
                                    var k = n - 1;
                                    while k >= 0 {
                                        let cfd = record_fd(pl, k + 1);
                                        let base = k * size;
                                        let obase = k * osize;
                                        var drop = false;
                                        // Is there input to answer: just read, or held back
                                        // while the last answer was being sent?
                                        var answer = false;
                                        if record_events(pl, k + 1) != 0 {
                                            if st[4 * k + 2] > 0 {
                                                // Waiting to send: the kernel can take more.
                                                let sent = send(libc, cfd, pd[obase..obase + quantum(st[4 * k + 2], chunk)], mflag);
                                                if sent <= 0 {
                                                    drop = true;
                                                } else {
                                                    st[4 * k + 1] = now;
                                                    var at = 0;
                                                    while at < st[4 * k + 2] - sent {
                                                        pd[obase + at] = pd[obase + sent + at];
                                                        at = at + 1;
                                                    }
                                                    st[4 * k + 2] = st[4 * k + 2] - sent;
                                                    if st[4 * k + 2] == 0 {
                                                        if st[4 * k + 3] == 1 {
                                                            drop = true;
                                                        } else {
                                                            answer = st[4 * k] > 0;
                                                        }
                                                    }
                                                }
                                            } else {
                                                let got = sockets.read(libc, cfd, bf[base + st[4 * k]..base + size]);
                                                if got <= 0 {
                                                    drop = true;
                                                } else {
                                                    st[4 * k] = st[4 * k] + got;
                                                    st[4 * k + 1] = now;
                                                    answer = true;
                                                }
                                            }
                                        } else if now - st[4 * k + 1] > idle {
                                            drop = true;
                                        }
                                        if answer && !drop {
                                            let (grown, used) = drain(libc, heap, router, bf[base..base + size], st[4 * k], cfd, mflag, chunk, tb, pr, pd[obase..obase + osize], st, k, out);
                                            out = grown;
                                            if used < 0 {
                                                drop = true;
                                            } else {
                                                if used > 0 {
                                                    // Whatever is left is the start of the next
                                                    // request, or a request held back: move it to
                                                    // the front.
                                                    var at = 0;
                                                    while at < st[4 * k] - used {
                                                        bf[base + at] = bf[base + used + at];
                                                        at = at + 1;
                                                    }
                                                    st[4 * k] = st[4 * k] - used;
                                                }
                                                if st[4 * k + 3] == 1 && st[4 * k + 2] == 0 {
                                                    drop = true;
                                                }
                                            }
                                        }
                                        if drop {
                                            sockets.close(libc, cfd);
                                            let last = n - 1;
                                            if k != last {
                                                var at = 0;
                                                while at < st[4 * last] {
                                                    bf[base + at] = bf[last * size + at];
                                                    at = at + 1;
                                                }
                                                at = 0;
                                                while at < st[4 * last + 2] {
                                                    pd[obase + at] = pd[last * osize + at];
                                                    at = at + 1;
                                                }
                                                st[4 * k] = st[4 * last];
                                                st[4 * k + 1] = st[4 * last + 1];
                                                st[4 * k + 2] = st[4 * last + 2];
                                                st[4 * k + 3] = st[4 * last + 3];
                                                move_record(pl, last + 1, k + 1);
                                            }
                                            n = last;
                                        } else if st[4 * k + 2] > 0 {
                                            // Output is waiting: wait for room, and read no more.
                                            set_events(pl, k + 1, 4);
                                        } else {
                                            set_events(pl, k + 1, 1);
                                        }
                                        k = k - 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    buffer.drop(heap, out);
    unbox_slice(heap, params);
    unbox_slice(heap, table);
    unbox_slice(heap, pends);
    unbox_slice(heap, bufs);
    unbox_slice(heap, state);
    unbox_slice(heap, polls);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    // A server: no files. It keeps the console for one line and the heap for
    // its tables, and the one foreign library it needs.
    release(fs);
    let libc = narrow(ffi, "libc");

    var port = 0 - 1;
    var reuse = false;
    var idle = 9;
    var chunk = 0;
    borrow args as &g in {
        if arg_count(g) > 1 {
            port = number_of(arg(g, 1));
        }
        if arg_count(g) > 2 {
            reuse = len(arg(g, 2)) > 0 && (int_of(arg(g, 2)[0]) == '1' || int_of(arg(g, 2)[0]) == 'r');
        }
        if arg_count(g) > 3 {
            idle = number_of(arg(g, 3));
        }
        if arg_count(g) > 4 {
            chunk = number_of(arg(g, 4));
        }
    }

    var status = 2;
    if port > 0 && port < 65536 && idle > 0 && chunk >= 0 {
        status = 3;
        borrow libc as &f in {
            signal(f, 13, 1);
            // `MSG_DONTWAIT`: 0x40 on Linux, 0x80 on macOS, where 0x40 is
            // `MSG_WAITALL` and would make every send block.
            var mflag = 128;
            if is_linux(f) {
                mflag = 64;
            }
            let lfd = listener(f, port, reuse);
            if lfd >= 0 {
                borrow mut heap as &!h in {
                    let router = routes(h);
                    borrow mut io as &!i in {
                        // On the unbuffered stream: standard output, piped,
                        // is held until the process ends, and a server that
                        // announces itself only then has not announced itself.
                        var line = buffer.append(h, buffer.empty(h, 64), "listening on ");
                        line = buffer.push_nat(h, line, port);
                        line = buffer.append(h, line, " send-flag ");
                        line = buffer.push_nat(h, line, mflag);
                        line = buffer.push(h, line, byte_of(10));
                        borrow line as &lb in {
                            io.error_all(i, buffer.bytes(lb));
                        }
                        buffer.drop(h, line);
                    }
                    borrow router as &r in {
                        status = serve(f, h, r, lfd, idle, chunk, mflag);
                    }
                    route.drop(h, router);
                }
            }
        }
    }
    release(libc);
    release(args);
    release(io);
    release(heap);
    return status;
}
