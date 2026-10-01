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
import http.server;

// The default input buffer a connection gets; the sixth argument sets it.
fn buffer_size() -> [] int {
    return 16384;
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

fn user[&h, &p, &s](heap: &!h Heap, out: buffer.Buffer, path: &s [byte], params: &p [int], keep: bool) -> [heap] buffer.Buffer {
    let id = route.param_nat(path, params, 0);
    if id < 0 {
        return server.failure(heap, out, 400, "id must be a number", keep);
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
        answer = server.reply(heap, answer, 200, buffer.bytes(bb), keep);
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
            answer = server.failure(heap, answer, 422, json.error_message(json.error_code(nodes)), keep);
        } else {
            let a = json.get(body, t, 0, "a");
            let b = json.get(body, t, 0, "b");
            if !json.is_int(t, a) || !json.is_int(t, b) || !json.fits_int(body, t, a) || !json.fits_int(body, t, b) {
                answer = server.failure(heap, answer, 422, "a and b must be integers", keep);
            } else {
                var w = json.writer(heap, 32);
                w = json.begin_object(heap, w);
                w = json.put_key(heap, w, "sum");
                w = json.put_int(heap, w, json.to_int(body, t, a) + json.to_int(body, t, b));
                w = json.end_object(heap, w);
                let reply_body = json.finish(w);
                borrow reply_body as &rb in {
                    answer = server.reply(heap, answer, 200, buffer.bytes(rb), keep);
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
        return server.failure(heap, out, 400, "n must be a number up to 32768", keep);
    }
    var body = buffer.empty(heap, n + 1);
    var i = 0;
    while i < n {
        body = buffer.push(heap, body, byte_of('a' + i % 26));
        i = i + 1;
    }
    var answer = out;
    borrow body as &bb in {
        answer = server.reply(heap, answer, 200, buffer.bytes(bb), keep);
    }
    buffer.drop(heap, body);
    return answer;
}

fn search[&h, &q](heap: &!h Heap, out: buffer.Buffer, query: &q [byte], keep: bool) -> [heap] buffer.Buffer {
    let (from, to) = http.query_value(query, "q");
    if from < 0 {
        return server.failure(heap, out, 400, "q is required", keep);
    }
    var answer = out;
    region scratch {
        let decoded = alloc_slice[scratch](512, byte_of(0));
        let n = http.percent_decode(query[from..to], decoded, true);
        if n < 0 {
            answer = server.failure(heap, answer, 400, "q is not valid percent-encoding, or is too long", keep);
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
                answer = server.reply(heap, answer, 200, buffer.bytes(bb), keep);
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
        return server.reply(heap, out, 200, "{\"ok\":true}", keep);
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
            answer = server.failure_with(heap, answer, 405, "method not allowed", keep, buffer.bytes(eb));
        }
        buffer.drop(heap, extra);
        return answer;
    }
    return server.failure(heap, out, 404, "not found", keep);
}

// ---------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------

// Serve until killed. `http.server` does the network -- `wait` accepts, reads,
// sends what was waiting and closes the quiet; `next` hands over one whole
// request at a time -- and this is what answers them. The request is a view of
// the server's own buffer, so it is read inside a `borrow` of the server and
// the answer is built in a buffer of this program's own; the server is borrowed
// again, mutably, to send it.
fn run[&h, &r, &k, &l](heap: &!h Heap, router: &r route.Router, clock: &k Clock, listener: &!l Listener, idle: int, chunk: int, size: int) -> [heap, conn_accept, conn_read, conn_write, poll, clock] int {
    match poller_new() {
        Polling::Ok(p) => {
            var srv = server.open(heap, p, listener, size, chunk, idle);
            var widest = 1;
            if route.most_params(router) > 1 {
                widest = route.most_params(router);
            }
            let params = box_slice(heap, 2 * widest, 0);
            var out = buffer.empty(heap, 4096);
            while true {
                srv = server.wait(heap, srv, clock, listener, 1000);
                var more = true;
                while more {
                    var slot = 0 - 1;
                    borrow mut srv as &!sw in {
                        slot = server.next(heap, sw);
                    }
                    if slot < 0 {
                        more = false;
                    } else {
                        borrow mut out as &!ob in {
                            buffer.clear(ob);
                        }
                        borrow srv as &sr in {
                            borrow mut params as &!pw in {
                                out = handle(heap, router, server.head(sr), server.parsed(sr), contents(pw), server.body(sr), out);
                            }
                        }
                        borrow mut srv as &!sw in {
                            borrow out as &ob in {
                                server.respond(sw, buffer.bytes(ob));
                            }
                        }
                    }
                }
            }
            server.close(heap, srv);
            buffer.drop(heap, out);
            unbox_slice(heap, params);
            return 0;
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
                                    status = run(h, r, c, lh, idle, chunk, size);
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
