edition 5;

// `docs/tls-server.md` §7: the whole server from `serve`, fed a client's
// bytes from standard input, as `fuzz_client` feeds the client a server's.
// The input is a run of chunks, each a 2-byte big-endian length and that
// many bytes (a short last chunk is what is left), and each chunk is one
// `feed`: where the network cuts the stream is the fuzzer's choice too.
// After each, everything `take` and `recv` have is taken, as a caller
// would. At the end, data is sent if the connection got that far, then the
// peer's end of stream and `finish`. The identity, seed and time are
// `fuzz_server_fixture`'s; the server speaks `h2` and `http/1.1`. An input of
// odd length is served with no ALPN list, so both paths are reached.
//
// The corpus starts from real ClientHellos (`tests/vectors/fuzz/server/`,
// OpenSSL, curl, Go, wolfSSL, mosquitto and Chromium), so the fuzzer begins
// at the parser's far side rather than at its first length check.
import fuzz_server_fixture;
import std.io;
import tls;

fn read_all[&i, &o](io: &!i Io, into: &!o [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    while c >= 0 && n < len(into) {
        into[n] = byte_of(c);
        n = n + 1;
        c = getchar(io);
    }
    return n;
}

fn drain[&e, &o](engine: &!e tls.Engine, out: &!o [byte]) -> [] int {
    var n = tls.take(engine, 0, out);
    while n > 0 {
        n = tls.take(engine, 0, out);
    }
    n = tls.recv(engine, 0, out);
    while n > 0 {
        n = tls.recv(engine, 0, out);
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 262144, byte_of(0));
    var out = box_slice(heap, 65536, byte_of(0));
    var engine = tls.open_server(heap, 1);
    borrow mut input as &!d in {
        borrow mut out as &!o in {
            borrow mut engine as &!e in {
                let total = read_all(io, contents(d));
                tls.seed(e, fuzz_server_fixture.seed());
                tls.add_identity(e, fuzz_server_fixture.chain(), fuzz_server_fixture.key(), fuzz_server_fixture.names(), fuzz_server_fixture.now_ms());
                if total % 2 == 0 {
                    tls.set_alpn(e, "h2 http/1.1");
                }
                var code = tls.serve(e, 0, fuzz_server_fixture.now_ms());
                var at = 0;
                while code >= 0 && at < total {
                    var size = total - at;
                    var from = at;
                    if at + 2 <= total {
                        let named = int_of(contents(d)[at]) * 256 + int_of(contents(d)[at + 1]);
                        from = at + 2;
                        size = total - from;
                        if named < size {
                            size = named;
                        }
                    }
                    var used = 0;
                    var going = true;
                    while going {
                        let c = tls.feed(e, 0, contents(d)[from + used..from + size]);
                        drain(e, contents(o));
                        if c < 0 {
                            code = c;
                            going = false;
                        } else {
                            used = used + c;
                            going = used < size && c > 0;
                        }
                    }
                    at = from + size;
                    if size == 0 {
                        at = at + 1;
                    }
                }
                if tls.event(e, 0) == tls.event_established() {
                    tls.send(e, 0, "HTTP/1.0 200 OK\r\n\r\n");
                    drain(e, contents(o));
                }
                tls.eof(e, 0);
                drain(e, contents(o));
                tls.finish(e, 0);
                drain(e, contents(o));
                tls.drop(e, 0);
            }
        }
    }
    tls.close(heap, engine);
    unbox_slice(heap, input);
    unbox_slice(heap, out);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(args);
    release(ffi);
    release(fs);
    release(net);
    release(clock);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
