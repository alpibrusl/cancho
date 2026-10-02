edition 5;

// `http.server`'s `hold` and `answer`, and the handles an application registers with the
// server's own poller (`docs/http-server.md` §10).
//
//     server_hold <port> <peer port> [<buffer size>]
//
// It connects to a peer a test is listening on and watches that connection in the server's
// poller. A request for `/hold` is held, not answered; every byte the peer writes releases
// the oldest held request with `released` (a byte written when none is held waits for the next
// one). So a test can show that a held request does not
// stop the loop (other requests are answered meanwhile), that what a client pipelined behind
// it waits and is answered in order afterwards, and that a ticket for a connection that is
// gone does not reach whichever one took its slot, nor does an answer given twice (`/stale`
// says how many answers were refused; `/replay/<n>` answers the `n`-th ticket handed out
// once more with `replayed`, then says the same). `/instant` is held and answered at once,
// with `instant`. Every other path is answered with itself.

import std.buffer;
import std.bytes;
import std.conns;
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

// Let go of up to `count` held requests, oldest first. Answers how many answers were refused
// (a ticket whose connection was gone) and the new oldest of the queue.
fn let_go[&h, &s, &q](heap: &!h Heap, srv: &!s server.Server, held: &q [int], oldest: int, newest: int, count: int) -> [heap, conn_write, poll] (int, int) {
    var at = oldest;
    var refused = 0;
    var n = 0;
    while n < count && at < newest {
        let ticket = held[at % 64];
        var out = server.reply(heap, buffer.empty(heap, 128), 200, "released", true);
        borrow out as &ob in {
            if server.answer(srv, ticket, buffer.bytes(ob)) < 0 {
                refused = refused + 1;
            }
        }
        buffer.drop(heap, out);
        at = at + 1;
        n = n + 1;
    }
    return (at, refused);
}

fn run[&h, &k, &l](heap: &!h Heap, clock: &k Clock, listener: &!l Listener, peer: Conn, size: int) -> [heap, conn_accept, poll, clock] int {
    match poller_new() {
        Polling::Ok(p) => {
            var srv = server.open(heap, p, listener, size, 0, 9);
            let (opened, peer_slot) = conns.put(heap, conns.empty(heap, 4), peer);
            var table = opened;
            var watched = 0;
            borrow mut table as &!tw in {
                borrow mut srv as &!sw in {
                    watched = conns.nonblocking(tw, peer_slot);
                    watched = watched + conns.watch(tw, server.poller(sw), peer_slot, server.first_token(sw), 1);
                }
            }
            if watched != 0 {
                server.close(heap, srv);
                conns.drop(heap, table);
                return 5;
            }
            let held = box_slice(heap, 64, 0);
            let scratch = box_slice(heap, 64, byte_of(0));
            var oldest = 0;
            var newest = 0;
            var stale = 0;
            var credits = 0;
            var out = buffer.empty(heap, 256);
            while true {
                srv = server.wait(heap, srv, clock, listener, 200);
                // The peer wrote: one release per byte.
                var woken = false;
                borrow srv as &sr in {
                    var i = 0;
                    while i < server.foreign_count(sr) {
                        if server.foreign(sr)[2 * i] == server.first_token(sr) {
                            woken = true;
                        }
                        i = i + 1;
                    }
                }
                if woken {
                    borrow mut table as &!tw in {
                        borrow mut scratch as &!bw in {
                            match conns.read(tw, peer_slot, contents(bw)) {
                                Received::Data(n) => {
                                    credits = credits + n;
                                }
                                Received::End => {
                                }
                                Received::Again => {
                                }
                                Received::Failed(e) => {
                                }
                            }
                        }
                    }
                }
                // A byte written before there was a request to release is not lost: it waits.
                if credits > 0 && oldest < newest {
                    borrow mut srv as &!sw in {
                        borrow held as &hr in {
                            let (nh, refused) = let_go(heap, sw, contents(hr), oldest, newest, credits);
                            credits = credits - (nh - oldest);
                            oldest = nh;
                            stale = stale + refused;
                        }
                    }
                }
                var more = true;
                while more {
                    var slot = 0 - 1;
                    borrow mut srv as &!sw in {
                        slot = server.next(heap, sw);
                    }
                    if slot < 0 {
                        more = false;
                    } else {
                        var holding = false;
                        var replay = 0 - 1;
                        var instant = false;
                        borrow mut out as &!ob in {
                            buffer.clear(ob);
                        }
                        borrow srv as &sr in {
                            let path = http.path(server.head(sr), server.parsed(sr));
                            if bytes.equal(path, "/hold") {
                                holding = true;
                            } else if bytes.equal(path, "/instant") {
                                instant = true;
                            } else if bytes.starts_with(path, "/replay/") {
                                replay = number_of(path[8..len(path)]);
                            } else if bytes.equal(path, "/stale") {
                                var text = buffer.push_nat(heap, buffer.empty(heap, 16), stale);
                                borrow text as &tr in {
                                    out = server.reply(heap, out, 200, buffer.bytes(tr), true);
                                }
                                buffer.drop(heap, text);
                            } else {
                                out = server.reply(heap, out, 200, path, true);
                            }
                        }
                        if instant {
                            var ticket = 0 - 1;
                            borrow mut srv as &!sw in {
                                ticket = server.hold(sw);
                                var again = server.reply(heap, buffer.empty(heap, 64), 200, "instant", true);
                                borrow again as &ab in {
                                    if server.answer(sw, ticket, buffer.bytes(ab)) < 0 {
                                        stale = stale + 1;
                                    }
                                }
                                buffer.drop(heap, again);
                            }
                        } else if replay >= 0 {
                            if replay < newest {
                                var again = server.reply(heap, buffer.empty(heap, 64), 200, "replayed", true);
                                borrow again as &ab in {
                                    borrow mut srv as &!sw in {
                                        borrow held as &hr in {
                                            if server.answer(sw, contents(hr)[replay % 64], buffer.bytes(ab)) < 0 {
                                                stale = stale + 1;
                                            }
                                        }
                                    }
                                }
                                buffer.drop(heap, again);
                            }
                            var text = buffer.push_nat(heap, buffer.empty(heap, 16), stale);
                            borrow text as &tr in {
                                out = server.reply(heap, out, 200, buffer.bytes(tr), true);
                            }
                            buffer.drop(heap, text);
                            borrow mut srv as &!sw in {
                                borrow out as &ob in {
                                    server.respond(sw, buffer.bytes(ob));
                                }
                            }
                        } else if holding {
                            var ticket = 0 - 1;
                            borrow mut srv as &!sw in {
                                ticket = server.hold(sw);
                            }
                            borrow mut held as &!hw in {
                                contents(hw)[newest % 64] = ticket;
                            }
                            newest = newest + 1;
                        } else {
                            borrow mut srv as &!sw in {
                                borrow out as &ob in {
                                    server.respond(sw, buffer.bytes(ob));
                                }
                            }
                        }
                    }
                }
            }
            server.close(heap, srv);
            conns.drop(heap, table);
            buffer.drop(heap, out);
            unbox_slice(heap, held);
            unbox_slice(heap, scratch);
            return 0;
        }
        Polling::Failed(e) => {
            conn_close(peer);
            return 4;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    release(ffi);
    var port = 0 - 1;
    var peer_port = 0 - 1;
    var size = 16384;
    borrow args as &g in {
        if arg_count(g) > 3 {
            size = number_of(arg(g, 3));
        }
        if arg_count(g) > 2 {
            port = number_of(arg(g, 1));
            peer_port = number_of(arg(g, 2));
        }
    }
    var status = 2;
    if port > 0 && peer_port > 0 {
        status = 3;
        borrow net as &nn in {
            match tcp_connect(nn, "127.0.0.1", peer_port) {
                Dialed::Ok(peer) => {
                    match tcp_listen(nn, port, 1024, 0) {
                        Listening::Ok(l) => {
                            var listener = l;
                            borrow mut listener as &!lh in {
                                listener_nonblocking(lh);
                                borrow mut heap as &!h in {
                                    borrow clock as &c in {
                                        status = run(h, c, lh, peer, size);
                                    }
                                }
                            }
                            listener_close(listener);
                        }
                        Listening::Failed(e) => {
                            conn_close(peer);
                        }
                    }
                }
                Dialed::Failed(e) => {
                    status = 4;
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
