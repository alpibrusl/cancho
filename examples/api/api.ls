edition 5;

// `api` -- a JSON API server: keep-alive, pipelining, routed, one thread.
//
//     api <port> [reuseport] [idle-seconds] [send-chunk-bytes] [input-buffer-bytes]
//
// `docs/server.md` is the design. This is the program the last four
// library pieces were for: `std.http` parses each request, `std.route`
// picks the handler, `std.json` reads and writes the bodies, and the loop
// around them waits on a `Poller` (epoll on Linux, kqueue on macOS), so a
// connection that is open and silent costs a slot and nothing else -- one
// slow client cannot make another wait.
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
// **Limits, all said aloud:** 1024 connections, 16 KiB of input buffer each (a
// request, head and body together, must fit in it; the fifth argument sets it,
// from 4 KiB to 1 MiB, and the connections that fit shrink with it so that the
// input buffers together never pass 256 MiB), 64 KiB of output buffer each,
// nine seconds without progress before a connection is closed unless the third
// argument says otherwise.
//
// **Bodies.** A body with a `Content-Length`, or a chunked one, is read into that
// buffer and handed to the handler whole; a chunked body is decoded first
// (`std.http.dechunk`, which refuses chunk extensions and trailers) so the
// handler cannot tell the two apart. A request that cannot fit is refused at
// once with 413. Streaming a body larger than the buffer to a handler as it
// arrives is not done.
//
// The fourth argument is a quantum: no write is handed more than that many
// bytes, so an answer larger than it takes several, which is what a loop that
// wants to be fair to its other connections might choose. It is also how the
// tests make partial writes certain: on Linux a loopback send is whole or
// refused, never partial, and the code that resumes a partial one would
// otherwise never run.
//
// **Writes do not wait.** Every connection is non-blocking, so a write that
// cannot go whole answers at once with what the kernel took (or `Again`). What
// was not taken waits in the connection's own output buffer, the connection is
// watched for writability instead of readability -- it is not read from until
// its output is gone, which is what keeps a client that never reads from making
// the server buffer without bound -- and every other connection is served
// meanwhile. A client that makes no progress for `idle` seconds is closed.
//
// **What it does not do**, and `docs/server.md` §6 is the list: stream a body
// larger than its buffer, or use more than one core -- `reuseport`
// lets several copies of this program share a port and the kernel spreads the
// connections, which is how it scales.
//
// **Authority.** This program declares no `extern fn` and holds no `Ffi`:
// `lex-sys authority` reports `net_in` (with no port named, because the port is
// an argument), `conn_accept`, `conn_read`, `conn_write`, `poll`, `clock`,
// `heap`, `args` and the console, and "never touches" foreign code or the
// filesystem. Its predecessor reported `ffi("libc")`, which said nothing.
// `docs/native-sockets.md` is how it got here.

import std.buffer;
import std.bytes;
import std.http;
import std.io;
import std.json;
import std.route;
import std.conns;

fn max_connections() -> [] int {
    return 1024;
}

// The default input buffer a connection gets; the sixth argument sets it.
fn buffer_size() -> [] int {
    return 16384;
}

// How much input buffer all the connections may hold together: 256 MiB, so a
// bigger per-connection buffer means fewer connections rather than more memory.
fn input_budget() -> [] int {
    return 268435456;
}

// How many connections fit: `max_connections`, or what `budget` allows at this
// buffer size if that is fewer.
fn connection_limit(size: int) -> [] int {
    var limit = input_budget() / size;
    if limit > max_connections() {
        limit = max_connections();
    }
    return limit;
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
    return reply_with(heap, out, status, body, keep, "");
}

// `reply`, with extra header lines in the head.
fn reply_with[&h, &b, &x](heap: &!h Heap, out: buffer.Buffer, status: int, body: &b [byte], keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
    let head = http.respond_head_with(heap, out, status, "application/json", len(body), keep, extra);
    return buffer.append(heap, head, body);
}

// `{"error": message}`.
fn failure[&h, &m](heap: &!h Heap, out: buffer.Buffer, status: int, message: &m [byte], keep: bool) -> [heap] buffer.Buffer {
    return failure_with(heap, out, status, message, keep, "");
}

// `failure`, with extra header lines (`Allow: GET\r\n`) in the head.
fn failure_with[&h, &m, &x](heap: &!h Heap, out: buffer.Buffer, status: int, message: &m [byte], keep: bool, extra: &x [byte]) -> [heap] buffer.Buffer {
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

fn user[&h, &p, &s](heap: &!h Heap, out: buffer.Buffer, path: &s [byte], params: &p [int], keep: bool) -> [heap] buffer.Buffer {
    let id = route.param_nat(path, params, 0);
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
    let n = route.param_nat(path, params, 0);
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
        // A 405 says what would have been allowed (RFC 9110 §15.5.6).
        var extra = buffer.append(heap, buffer.empty(heap, 48), "Allow: ");
        extra = route.allowed(heap, router, path, params, extra);
        extra = buffer.append(heap, extra, "\r\n");
        var answer = out;
        borrow extra as &eb in {
            answer = failure_with(heap, answer, 405, "method not allowed", keep, buffer.bytes(eb));
        }
        buffer.drop(heap, extra);
        return answer;
    }
    return failure(heap, out, 404, "not found", keep);
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

// Answer one request whose head and body are in hand: route it, build the
// answer and hand it to the connection. Answers the output buffer back and
// whether the connection has to be abandoned (an answer it could not hold).
fn answer_one[&c, &h, &r, &q, &t, &p, &b, &e, &s](conn: &!c conns.Table, heap: &!h Heap, router: &r route.Router, request: &q [byte], table: &t [int], params: &!p [int], body: &b [byte], chunk: int, pend: &!e [byte], st: &!s [int], k: int, out: buffer.Buffer) -> [conn_write, heap] (buffer.Buffer, bool) {
    var o = out;
    borrow mut o as &!ob in {
        buffer.clear(ob);
    }
    o = handle(heap, router, request, table, params, body, o);
    borrow o as &ob in {
        st[6 * k + 2] = emit(conn, k, chunk, buffer.bytes(ob), pend, st[6 * k + 2]);
    }
    if !http.keeps_alive(table) {
        st[6 * k + 3] = 1;
    }
    return (o, st[6 * k + 2] < 0);
}

// Answer the complete requests at the front of `data[0..filled]`, until one of
// them leaves output the kernel would not take.
//
// The answer is the bytes consumed: the caller moves the rest (the start of a
// request still arriving, or requests held back for now) to the front. `-1`
// means the connection must close *now*; `st[6 * k + 3]` set means it closes
// once its output has gone, which is what a refusal or `Connection: close`
// asks for.
//
// A chunked body is decoded into `scratch` (the same size as `data`) and the
// handler is given the decoded bytes, so it cannot tell the two apart.
//
// Pipelined requests -- several in one read -- are answered in order. It stops
// when an answer could not be sent whole (backpressure: no more requests are
// taken from a connection that is not taking its answers), when a body has not
// all arrived, and when a request could never fit the buffer, which is refused
// now rather than waited on for ever.
fn drain[&c, &h, &r, &d, &t, &p, &e, &s, &z](conn: &!c conns.Table, heap: &!h Heap, router: &r route.Router, data: &!d [byte], filled: int, chunk: int, table: &!t [int], params: &!p [int], pend: &!e [byte], st: &!s [int], k: int, scratch: &!z [byte], out: buffer.Buffer) -> [conn_write, heap] (buffer.Buffer, int) {
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
        // How many bytes of the request, past its head, were its body.
        var taken = 0 - 1;
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
            let (took, decoded) = http.dechunk(view[n..filled], scratch);
            if http.dechunk_incomplete(took) {
                // The body is still arriving -- unless it could never fit.
                going = false;
                if used == 0 && filled >= len(data) {
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
                let (grown, ab) = answer_one(conn, heap, router, view, table, params, scratch[0..decoded], chunk, pend, st, k, o);
                o = grown;
                abandon = abandon || ab;
                taken = took;
            }
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
                let (grown, ab) = answer_one(conn, heap, router, view, table, params, view[n..n + body_length], chunk, pend, st, k, o);
                o = grown;
                abandon = abandon || ab;
                taken = body_length;
            }
        }
        if taken >= 0 {
            used = used + n + taken;
            // Output left over, or the connection is ending: take no more.
            if st[6 * k + 2] != 0 || st[6 * k + 3] != 0 {
                going = false;
            }
        }
        if refuse != 0 {
            borrow mut o as &!ob in {
                buffer.clear(ob);
            }
            o = failure(heap, o, refuse, message, false);
            borrow o as &ob in {
                st[6 * k + 2] = emit(conn, k, chunk, buffer.bytes(ob), pend, st[6 * k + 2]);
            }
            if st[6 * k + 2] < 0 {
                abandon = true;
            }
            st[6 * k + 3] = 1;
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

// Take every connection waiting on the listener, up to the limit: each goes
// in the table, is made non-blocking, and is watched for input under the token
// `slot + 1` (the listener is token 0).
fn accept_all[&h, &l, &p, &s](heap: &!h Heap, conn: conns.Table, listener: &!l Listener, poller: &!p Poller, st: &!s [int], now: int, limit: int) -> [heap, conn_accept, poll] conns.Table {
    var table = conn;
    var more = true;
    while more {
        match tcp_accept(listener) {
            Accepted::Ok(c) => {
                var held = 0;
                borrow table as &tt in {
                    held = conns.live(tt);
                }
                if held >= limit {
                    conn_close(c);
                } else {
                    let (grown, slot) = conns.put(heap, table, c);
                    table = grown;
                    if slot >= 0 {
                        st[6 * slot] = 0;
                        st[6 * slot + 1] = now;
                        st[6 * slot + 2] = 0;
                        st[6 * slot + 3] = 0;
                        st[6 * slot + 4] = 1;
                        st[6 * slot + 5] = 1;
                        borrow mut table as &!ct in {
                            if conns.nonblocking(ct, slot) != 0 || conns.watch(ct, poller, slot, slot + 1, 1) != 0 {
                                conns.close(ct, slot);
                                st[6 * slot + 4] = 0;
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

// Serve until killed. `idle` is how many seconds a connection may go without
// progress -- a byte read, a byte sent -- before it is closed.
//
// Per connection `k` (a slot in the connection table), `st[6k..6k+6]` is:
// bytes of input buffered, the time of its last progress, bytes of output
// waiting, 1 if it is to close once that output has gone, 1 if the slot is in
// use, and what it is watched for (1 to read, 2 to write).
fn serve_on[&h, &r, &k, &l, &p](heap: &!h Heap, router: &r route.Router, clock: &k Clock, listener: &!l Listener, poller: &!p Poller, idle: int, chunk: int, size: int) -> [heap, conn_accept, conn_read, conn_write, poll, clock] int {
    let limit = connection_limit(size);
    let osize = output_size();
    let events = box_slice(heap, 128, 0);
    let state = box_slice(heap, 6 * limit, 0);
    let bufs = box_slice(heap, limit * size, byte_of(0));
    let pends = box_slice(heap, limit * osize, byte_of(0));
    let table = box_slice(heap, http.slots(64), 0);
    // Where a chunked body is decoded: one connection's worth, used by one
    // request at a time.
    let decoded = box_slice(heap, size, byte_of(0));
    var widest = 1;
    if route.most_params(router) > 1 {
        widest = route.most_params(router);
    }
    let params = box_slice(heap, 2 * widest, 0);
    var out = buffer.empty(heap, 4096);
    var tab = conns.empty(heap, 64);
    poller_add_listener(poller, listener, 0);

    borrow mut events as &!ew in {
        borrow mut state as &!sw in {
            borrow mut bufs as &!bw in {
                borrow mut pends as &!ow in {
                    borrow mut table as &!tw in {
                        borrow mut decoded as &!dw in {
                            borrow mut params as &!qw in {
                                let dc = contents(dw);
                                let ev = contents(ew);
                                let st = contents(sw);
                                let bf = contents(bw);
                                let pd = contents(ow);
                                let tb = contents(tw);
                                let pr = contents(qw);
                                var last_sweep = 0;
                                while true {
                                    let ready = poller_wait(poller, ev, 1000);
                                    let now = clock_ms(clock) / 1000;
                                    if ready >= 0 {
                                        // New connections first, while the table is
                                        // ours to grow.
                                        var j = 0;
                                        while j < ready {
                                            if ev[2 * j] == 0 {
                                                tab = accept_all(heap, tab, listener, poller, st, now, limit);
                                            }
                                            j = j + 1;
                                        }
                                        borrow mut tab as &!ct in {
                                            // The connections that woke.
                                            j = 0;
                                            while j < ready {
                                                let token = ev[2 * j];
                                                let k = token - 1;
                                                if token > 0 && st[6 * k + 4] == 1 {
                                                    let base = k * size;
                                                    let obase = k * osize;
                                                    var drop = false;
                                                    // Is there input to answer: just read, or held back
                                                    // while the last answer was being sent?
                                                    var answer = false;
                                                    if st[6 * k + 2] > 0 {
                                                        // Waiting to send: the kernel can take more.
                                                        match conns.write(ct, k, pd[obase..obase + quantum(st[6 * k + 2], chunk)]) {
                                                            Sent::Wrote(sent) => {
                                                                st[6 * k + 1] = now;
                                                                var at = 0;
                                                                while at < st[6 * k + 2] - sent {
                                                                    pd[obase + at] = pd[obase + sent + at];
                                                                    at = at + 1;
                                                                }
                                                                st[6 * k + 2] = st[6 * k + 2] - sent;
                                                                if st[6 * k + 2] == 0 {
                                                                    if st[6 * k + 3] == 1 {
                                                                        drop = true;
                                                                    } else {
                                                                        answer = st[6 * k] > 0;
                                                                    }
                                                                }
                                                            }
                                                            Sent::Again => {
                                                            }
                                                            Sent::Failed(e) => {
                                                                drop = true;
                                                            }
                                                        }
                                                    } else {
                                                        match conns.read(ct, k, bf[base + st[6 * k]..base + size]) {
                                                            Received::Data(got) => {
                                                                st[6 * k] = st[6 * k] + got;
                                                                st[6 * k + 1] = now;
                                                                answer = true;
                                                            }
                                                            Received::End => {
                                                                drop = true;
                                                            }
                                                            Received::Again => {
                                                            }
                                                            Received::Failed(e) => {
                                                                drop = true;
                                                            }
                                                        }
                                                    }
                                                    if answer && !drop {
                                                        let (grown, used) = drain(ct, heap, router, bf[base..base + size], st[6 * k], chunk, tb, pr, pd[obase..obase + osize], st, k, dc, out);
                                                        out = grown;
                                                        if used < 0 {
                                                            drop = true;
                                                        } else {
                                                            if used > 0 {
                                                                // Whatever is left is the start of the next
                                                                // request, or a request held back: move it to
                                                                // the front.
                                                                var at = 0;
                                                                while at < st[6 * k] - used {
                                                                    bf[base + at] = bf[base + used + at];
                                                                    at = at + 1;
                                                                }
                                                                st[6 * k] = st[6 * k] - used;
                                                            }
                                                            if st[6 * k + 3] == 1 && st[6 * k + 2] == 0 {
                                                                drop = true;
                                                            }
                                                        }
                                                    }
                                                    if drop {
                                                        conns.close(ct, k);
                                                        st[6 * k + 4] = 0;
                                                    } else {
                                                        // Output waiting: wait for room, and read no
                                                        // more. Otherwise wait for input.
                                                        var want = 1;
                                                        if st[6 * k + 2] > 0 {
                                                            want = 2;
                                                        }
                                                        if want != st[6 * k + 5] {
                                                            conns.rewatch(ct, poller, k, token, want);
                                                            st[6 * k + 5] = want;
                                                        }
                                                    }
                                                }
                                                j = j + 1;
                                            }
                                            // Once a second: close the connections that have
                                            // gone quiet.
                                            if now != last_sweep {
                                                last_sweep = now;
                                                var s = 0;
                                                while s < conns.slots(ct) {
                                                    if st[6 * s + 4] == 1 && now - st[6 * s + 1] > idle {
                                                        conns.close(ct, s);
                                                        st[6 * s + 4] = 0;
                                                    }
                                                    s = s + 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    conns.drop(heap, tab);
    buffer.drop(heap, out);
    unbox_slice(heap, params);
    unbox_slice(heap, decoded);
    unbox_slice(heap, table);
    unbox_slice(heap, pends);
    unbox_slice(heap, bufs);
    unbox_slice(heap, state);
    unbox_slice(heap, events);
    return 0;
}

// The `Poller` the loop waits on, for as long as it runs.
fn serve[&h, &r, &k, &l](heap: &!h Heap, router: &r route.Router, clock: &k Clock, listener: &!l Listener, idle: int, chunk: int, size: int) -> [heap, conn_accept, conn_read, conn_write, poll, clock] int {
    match poller_new() {
        Polling::Ok(p) => {
            var poller = p;
            var status = 1;
            borrow mut poller as &!pw in {
                status = serve_on(heap, router, clock, listener, pw, idle, chunk, size);
            }
            poller_close(poller);
            return status;
        }
        Polling::Failed(e) => {
            return 4;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    // A server: no files and no foreign code. It keeps the console for one
    // line, the heap for its tables, the network for its port, and the clock
    // for its idle timeout.
    release(fs);
    release(ffi);

    var port = 0 - 1;
    var reuse = 0;
    var idle = 9;
    var chunk = 0;
    var size = buffer_size();
    borrow args as &g in {
        if arg_count(g) > 1 {
            port = number_of(arg(g, 1));
        }
        if arg_count(g) > 2 {
            if len(arg(g, 2)) > 0 && (int_of(arg(g, 2)[0]) == '1' || int_of(arg(g, 2)[0]) == 'r') {
                reuse = 1;
            }
        }
        if arg_count(g) > 3 {
            idle = number_of(arg(g, 3));
        }
        if arg_count(g) > 4 {
            chunk = number_of(arg(g, 4));
        }
        if arg_count(g) > 5 {
            size = number_of(arg(g, 5));
        }
    }

    var status = 2;
    if port > 0 && port < 65536 && idle > 0 && chunk >= 0 && size >= 4096 && size <= 1048576 {
        status = 3;
        // `Net` is not narrowed: the port is an argument, so which one is not
        // known until the program runs, and the authority report says so
        // (`net_in` with no port named) rather than pretending otherwise.
        borrow net as &nn in {
            match tcp_listen(nn, port, 1024, reuse) {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        listener_nonblocking(lh);
                        borrow mut heap as &!h in {
                            let router = routes(h);
                            borrow mut io as &!i in {
                                // On the unbuffered stream: standard output, piped,
                                // is held until the process ends, and a server that
                                // announces itself only then has not announced itself.
                                var line = buffer.append(h, buffer.empty(h, 64), "listening on ");
                                line = buffer.push_nat(h, line, port);
                                line = buffer.push(h, line, byte_of(10));
                                borrow line as &lb in {
                                    io.error_all(i, buffer.bytes(lb));
                                }
                                buffer.drop(h, line);
                            }
                            borrow router as &r in {
                                borrow clock as &c in {
                                    status = serve(h, r, c, lh, idle, chunk, size);
                                }
                            }
                            route.drop(h, router);
                        }
                    }
                    listener_close(listener);
                }
                Listening::Failed(e) => {
                }
            }
        }
    }
    release(net);
    release(clock);
    release(args);
    release(io);
    release(heap);
    return status;
}
