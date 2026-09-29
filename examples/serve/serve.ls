// `serve` -- a REST endpoint, and the answer to "can this language do X?"
//
// It opens a TCP socket, binds the port named on the command line, accepts
// one connection, routes the request line, and answers with JSON. `GET
// /health` gets a 200; anything else gets a 404.
//
// Nothing here is a language feature added for the purpose -- this program
// predates the one that was. There is no socket type, no async runtime and
// no HTTP library, and at the time this was written there was no `Net`
// capability either: the whole thing is eight `extern fn` declarations
// against libc, reached through the same `Ffi("libc")` capability
// `examples/tour.ls` uses for `labs`, and the bytes it sends are the same
// `&r [byte]` slices every other program here builds.
//
// `Net` exists now (`docs/net.md`, `docs/listen.md`): `bind`, `listen` and
// `accept` are builtins checked against a capability's bound before
// `socket` ever runs. This program is deliberately not ported onto it --
// `read`, `write` and `close` on the accepted connection still need
// `extern fn`, so replacing only the three calls `Net` now covers would add
// a capability to the authority report without removing `Ffi("libc")` from
// it (`docs/listen.md` §6.3 has the one place that porting would collide).
//
// That is still the point of leaving it as it is, and `docs/reach.md` is
// the argument. What decides whether a program is writable is not a
// feature list -- it is whether the authority it needs has a name. Sockets
// are libc, libc has a name, so this program exists. The cost is that
// `ffi("libc")` is the *only* thing its row can say: a supervisor reading
// the effect row alone learns that this program calls a C library, not
// that it listens on a port. §5 of that document is about what covers the
// difference, and why it is a report rather than a row.
//
// Read from the bottom: `main` splits the world, keeps exactly two
// capabilities and destroys the other three, and never regains them.
//
// The eight `extern fn`s against libc and the `put`/`put_nat` byte
// helpers used to live in this file -- byte-for-byte the same as
// `examples/results_stub/results_stub.ls`'s own copies. They are now
// `packages/net-sockets/sockets.ls`, this repository's first real
// `lex-sys-vcs` package (`docs/package-system.md` §6): published once,
// locked by name in `net.lock`, and fetched into a real file this
// program imports rather than duplicates. Building with `--std` alone is
// not enough to run this file any more -- see `net.lock`'s own header.

import std.bytes;
import net.sockets;

// A base-ten port, as the command line spells it. Anything that is not a
// digit ends the number, so a trailing newline or a stray character
// truncates rather than lying.
fn port_of[&a](text: &a [byte]) -> [] int {
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
// HTTP
// ---------------------------------------------------------------------

// `HTTP/1.1 <status> <reason>` CRLF `Content-Length: <n>` CRLF
// `Connection: close` CRLF CRLF `<body>`.
//
// The header is assembled rather than templated, because there is no
// string type to template with -- and the assembling is where the length
// comes from, so the `Content-Length` this sends cannot disagree with the
// bytes that follow it. A server that gets that wrong hangs its client.
fn respond[&o, &r, &b](out: &!o [byte], status: int, reason: &r [byte],
    body: &b [byte]) -> [] int {
    var at = sockets.put(out, 0, "HTTP/1.1 ");
    at = sockets.put_nat(out, at, status);
    at = sockets.put(out, at, " ");
    at = sockets.put(out, at, reason);
    at = sockets.put(out, at, "\r\nContent-Length: ");
    at = sockets.put_nat(out, at, len(body));
    at = sockets.put(out, at, "\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n");
    return sockets.put(out, at, body);
}

// One route, and a default. `starts_with` on the request line is the whole
// router: a real one would take the method and the path apart, and it
// would take them apart out of exactly these bytes.
fn route[&q, &o](request: &q [byte], out: &!o [byte]) -> [] int {
    if bytes.starts_with(request, "GET /health ") {
        return respond(out, 200, "OK", "{\"ok\":true}");
    }
    return respond(out, 404, "Not Found", "{\"error\":\"not found\"}");
}

// Read until the request line is complete, which is what `read` does not
// promise.
//
// One `read` returns what has *arrived*, not what was sent, and a client's
// request can be split across segments — so a server that reads once and
// routes what it got will occasionally route half a request line. It is
// rare on loopback and not rare under load, which is how this was found:
// the conformance suite failed roughly one run in six on a busy machine,
// answering 404 to a request for `/health`.
//
// The loop ends at the first newline, because the request line is all the
// router reads. A server that parsed headers would look for CRLF CRLF and
// would be the same shape.
fn read_request[&f, &b](libc: &f Ffi("libc"), conn: int, buffer: &!b [byte])
    -> [ffi("libc")] int {
    var filled = 0;
    while filled < len(buffer) {
        let got = sockets.read(libc, conn, buffer[filled..len(buffer)]);
        if got < 0 {
            return 0 - 1;
        }
        // End of stream before a complete line: hand back what there is
        // and let the router refuse it.
        if got == 0 {
            return filled;
        }
        let end = filled + got;
        while filled < end {
            if int_of(buffer[filled]) == '\n' {
                return end;
            }
            filled = filled + 1;
        }
    }
    return filled;
}

// ---------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------

// Everything from `socket` to the last `close`, with the capability lent
// in rather than owned: this function can call libc and can do nothing
// else, and its row says exactly that.
fn serve[&f, &a](libc: &f Ffi("libc"), port: &a [byte]) -> [ffi("libc")] int {
    region scratch {
        let fd = sockets.socket(libc, 2, 1, 0);
        if fd < 0 {
            return 1;
        }

        // SOL_SOCKET = 1, SO_REUSEADDR = 2, and the value is a C `int`:
        // four bytes, least significant first. Assembling it by hand is
        // what a foreign boundary with no struct layout costs
        // (`docs/reach.md` §3.2), and it is checkable only by reading it.
        let enable = alloc_slice[scratch](4, byte_of(0));
        enable[0] = byte_of(1);
        sockets.setsockopt(libc, fd, 1, 2, enable);

        // `struct sockaddr_in`: AF_INET, the port in network byte order,
        // then `INADDR_ANY` and eight bytes of padding, all zero.
        let number = port_of(port);
        let addr = alloc_slice[scratch](16, byte_of(0));
        addr[0] = byte_of(2);
        addr[2] = byte_of(number / 256);
        addr[3] = byte_of(number - (number / 256) * 256);
        if sockets.bind(libc, fd, addr) < 0 {
            sockets.close(libc, fd);
            return 2;
        }
        sockets.listen(libc, fd, 16);

        let conn = sockets.accept(libc, fd, 0, 0);
        if conn < 0 {
            sockets.close(libc, fd);
            return 3;
        }

        // A request larger than the buffer is truncated, not overflowed:
        // `read` is told the slice's own length, so the bytes it may write
        // are the bytes that exist.
        let request = alloc_slice[scratch](4096, byte_of(0));
        let got = read_request(libc, conn, request);
        if got < 0 {
            sockets.close(libc, conn);
            sockets.close(libc, fd);
            return 4;
        }
        let reply = alloc_slice[scratch](512, byte_of(0));
        let end = route(request[0..got], reply);
        sockets.write(libc, conn, reply[0..end]);

        sockets.close(libc, conn);
        sockets.close(libc, fd);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    // A server, and it cannot touch a file, allocate on the heap, or print
    // to the console. Three capabilities destroyed on three lines, and
    // nothing downstream can conjure another.
    release(fs);
    release(heap);
    release(io);

    let libc = narrow(ffi, "libc");
    var status = 4;
    borrow libc as &f in {
        borrow args as &g in {
            if arg_count(g) > 1 {
                status = serve(f, arg(g, 1));
            }
        }
    }
    release(libc);
    release(args);
    return status;
}
