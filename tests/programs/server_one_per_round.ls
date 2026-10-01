edition 5;

// `http.server`'s contract (`docs/http-server.md` §4) says `next` should be
// called until it answers -1 before the next `wait`, and what happens if it
// is not: the connection it was part-way through is finished, and the ones it
// had not reached stay queued. This program breaks the rule on purpose -- one
// `next` per `wait` -- so a test can pipeline several requests down several
// connections and check that none is lost or answered out of order.
//
// Answers every request with its own path, so a misplaced answer shows -- except
// `/empty`, a `204` with no `Content-Length`, and `/typed`, a `text/plain` answer
// with an extra header, which are `server.reply_empty` and `server.reply_as`.

import std.buffer;
import std.bytes;
import std.http;
import http.server;

fn number_of[&t](text: &t [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - 48);
        i = i + 1;
    }
    return n;
}

fn run[&h, &k, &l](heap: &!h Heap, clock: &k Clock, listener: &!l Listener) -> [heap, conn_accept, conn_read, conn_write, poll, clock] int {
    match poller_new() {
        Polling::Ok(p) => {
            var srv = server.open(heap, p, listener, 16384, 0, 9);
            var out = buffer.empty(heap, 256);
            while true {
                srv = server.wait(heap, srv, clock, listener, 200);
                var slot = 0 - 1;
                borrow mut srv as &!sw in {
                    slot = server.next(heap, sw);
                }
                if slot >= 0 {
                    borrow mut out as &!ob in {
                        buffer.clear(ob);
                    }
                    borrow srv as &sr in {
                        let path = http.path(server.head(sr), server.parsed(sr));
                        if bytes.equal(path, "/empty") {
                            out = server.reply_empty(heap, out, 204, true, "");
                        } else if bytes.equal(path, "/typed") {
                            out = server.reply_as(heap, out, 200, "text/plain; charset=utf-8", "hi", true, "X-Test: 1\r\n");
                        } else {
                            out = server.reply(heap, out, 200, path, true);
                        }
                    }
                    borrow mut srv as &!sw in {
                        borrow out as &ob in {
                            server.respond(sw, buffer.bytes(ob));
                        }
                    }
                }
            }
            server.close(heap, srv);
            buffer.drop(heap, out);
            return 0;
        }
        Polling::Failed(e) => {
            return 4;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    release(ffi);
    var port = 0 - 1;
    borrow args as &g in {
        if arg_count(g) > 1 {
            port = number_of(arg(g, 1));
        }
    }
    var status = 2;
    if port > 0 {
        status = 3;
        borrow net as &nn in {
            match tcp_listen(nn, port, 1024, 0) {
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        listener_nonblocking(lh);
                        borrow mut heap as &!h in {
                            borrow clock as &c in {
                                status = run(h, c, lh);
                            }
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
