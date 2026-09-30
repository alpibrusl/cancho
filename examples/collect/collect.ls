// `collect` -- an agent that receives results, and the second program
// here that listens.
//
//     collect <port> <count>
//
// Binds the port named on the command line, accepts `<count>`
// connections one after another, and for each one reads a `POST`
// request in full -- headers, then the body, by `Content-Length` --
// writing the body to standard output before answering `200` and
// moving on. Exits 0 once `<count>` requests have been handled.
//
// `docs/listen.md` is the design. `examples/serve/` accepts exactly one
// connection and never reads a body, which is what makes it testable
// without a shutdown protocol; this program keeps that discipline with
// a number instead of the constant one, and adds the one thing `serve/`
// never needed: reading a body it does not already have the length of
// in one read. The body streams straight to standard output the way
// `examples/fetch/` and `examples/report/` stream a response, rather
// than being copied into one buffer first -- a region is a single
// 64 KiB arena chunk (`docs/defined-behaviour.md`), and `docs/connect.md`
// §8 already found what materialising a whole body costs.
//
// Like `serve/`, it is `extern fn` declarations against libc through
// `Ffi("libc")`, and its authority report says so and no more.
//
// The eight `extern fn`s and the two byte helpers used to live in this
// file -- byte-for-byte the same as `examples/serve/serve.ls`'s and
// `examples/agent_supervisor/agent_supervisor.ls`'s own copies. All
// three now `import net.sockets` (`packages/net-sockets/`,
// `docs/package-system.md` §6) instead.
//
// `content_length_of`/`read_request` used to live here too, byte-for-
// byte the same as `examples/agent_supervisor/agent_supervisor.ls`'s
// own copies. Now `packages/http-request/request.ls`, this
// repository's fourth real package and the first that itself depends
// on a package (`net.sockets`, §4.6).

import std.bytes;
import std.io;
import net.sockets;
import http.request;

// ---------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------

// A base-ten value, as the command line spells it. Anything that is not
// a digit ends the number, the same rule `serve.ls`'s `port_of` uses.
fn nat_of[&a](text: &a [byte]) -> [] int {
    var value = 0;
    var i = 0;
    while i < len(text) {
        let digit = bytes.digit_of(int_of(text[i]));
        if digit < 0 {
            return value;
        }
        value = value * 10 + digit;
        i = i + 1;
    }
    return value;
}

// ---------------------------------------------------------------------
// The connection
// ---------------------------------------------------------------------

fn respond[&f](libc: &f Ffi("libc"), conn: int) -> [ffi("libc")] int {
    region scratch {
        let body = "{\"ok\":true}";
        let out = alloc_slice[scratch](len(body) + 96, byte_of(0));
        var at = sockets.put(out, 0, "HTTP/1.1 200 OK\r\nContent-Length: ");
        at = sockets.put_nat(out, at, len(body));
        at = sockets.put(out, at, "\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n");
        at = sockets.put(out, at, body);
        sockets.write(libc, conn, out[0..at]);
    }
    return 0;
}

// ---------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------

fn collect[&f, &i](libc: &f Ffi("libc"), io: &!i Io, port: int, count: int)
    -> [ffi("libc"), io_write] int {
    region scratch {
        let fd = sockets.socket(libc, 2, 1, 0);
        if fd < 0 {
            return 1;
        }
        let enable = alloc_slice[scratch](4, byte_of(0));
        enable[0] = byte_of(1);
        sockets.setsockopt(libc, fd, 1, 2, enable);

        let addr = alloc_slice[scratch](16, byte_of(0));
        addr[0] = byte_of(2);
        addr[2] = byte_of(port / 256);
        addr[3] = byte_of(port - (port / 256) * 256);
        if sockets.bind(libc, fd, addr) < 0 {
            sockets.close(libc, fd);
            return 2;
        }
        sockets.listen(libc, fd, 16);

        var handled = 0;
        while handled < count {
            let conn = sockets.accept(libc, fd, 0, 0);
            if conn < 0 {
                sockets.close(libc, fd);
                return 3;
            }
            if !request.read_request(libc, io, conn) {
                sockets.close(libc, conn);
                sockets.close(libc, fd);
                return 4;
            }
            respond(libc, conn);
            sockets.close(libc, conn);
            handled = handled + 1;
        }
        sockets.close(libc, fd);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(heap);

    let libc = narrow(ffi, "libc");
    var status = 5;
    borrow mut io as &!i in {
        borrow libc as &f in {
            borrow args as &g in {
                if arg_count(g) > 2 {
                    let port = nat_of(arg(g, 1));
                    let count = nat_of(arg(g, 2));
                    status = collect(f, i, port, count);
                }
            }
        }
    }
    release(libc);
    release(args);
    release(io);
    return status;
}
