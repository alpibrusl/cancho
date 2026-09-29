// `results-stub`, ported: the lex-sys twin of `lex-os/crates/results-stub`
// (issue #10), the single allowed-egress target for the lex-os demo. It
// accepts HTTP requests on a configurable port, logs each one to
// standard output, and replies with a fixed 200 -- the same job the
// Rust binary does, checked line by line against it rather than
// reimagined. This is the first lex-os component ported to lex-sys
// (the epic issue's own "first production use").
//
// Same discipline `examples/serve/`/`examples/collect/` already
// established: hand-rolled HTTP/1.1 over raw libc sockets, `Ffi("libc")`
// and nothing else, no `Net` capability (`read`/`write`/`close` on the
// accepted connection still need `extern fn`, so `Net` would add a
// capability to the authority report without removing `Ffi("libc")`
// from it -- `docs/listen.md` §6.3).
//
// Two places this port is honestly narrower than the Rust original,
// both because of what this language's FFI can and cannot cross
// (`docs/reach.md` §3):
//   * no peer address. `getpeername`'s real signature wants a
//     `socklen_t *` the kernel both reads (buffer capacity) and writes
//     (bytes used) -- a pointer to a plain scalar, which is not the
//     `&[byte]` "pointer and length" shape any foreign call here can
//     express, and not the "value across, nothing back" shape a bare
//     `int` parameter is either. `accept`'s own two trailing `NULL`s
//     already made the identical choice for the identical reason
//     (`examples/serve/serve.ls`'s own comment). Logged as `peer=?`.
//   * a body larger than this stub's 200-byte preview buffer is still
//     drained from the socket (so the connection does not hang) but
//     only its first 200 bytes are kept -- the log line only ever
//     printed a preview anyway, and a whole unbounded body has nowhere
//     to live in a 64 KiB arena (`docs/defined-behaviour.md`,
//     `examples/cut/`'s own line-length cap for the same reason).
//
// The eight `extern fn`s against libc and the `put`/`put_nat` byte
// helpers used to live in this file -- byte-for-byte the same as
// `examples/serve/serve.ls`'s own copies. They are now
// `packages/net-sockets/sockets.ls`, this repository's first real
// `lex-sys-vcs` package (`docs/package-system.md` §6): published once,
// locked by name in `net.lock`, and fetched into a real file this
// program imports rather than duplicates. Building with `--std` alone is
// not enough to run this file any more -- see `net.lock`'s own header.

import std.bytes;
import std.io;
import net.sockets;

// ---------------------------------------------------------------------
// libc
// ---------------------------------------------------------------------

// `time(NULL)`: a plain scalar back, unlike `getpeername`'s pointer
// out-param above, so it crosses with no trouble at all.
extern fn time[&f](ffi: &f Ffi("libc"), tloc: int) -> [ffi("libc")] int;

// `fflush(NULL)`: standard output is fully buffered once it is not a
// terminal (`docs/standard-error.md` §1.2), same as any C program's,
// but Rust's own `println!` is internally line-buffered regardless --
// so the real `results-stub`'s log is visible per request and this
// port's would not be without an explicit flush here. `NULL` flushes
// every open output stream, well-defined by the C standard.
extern fn fflush[&f](ffi: &f Ffi("libc"), stream: int) -> [ffi("libc")] int;

// ---------------------------------------------------------------------
// Bytes
// ---------------------------------------------------------------------

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

// `HOST:PORT`, or a bare port: only the part after the first `:` (or
// the whole thing, if there is none) is ever a decimal number, matching
// every real invocation this stub has ever been given
// (`0.0.0.0:8443`, `127.0.0.1:$STUB_PORT`, `0.0.0.0:443`). The host is
// read and discarded -- this port always binds `INADDR_ANY`, the same
// choice `collect.ls`/`serve.ls` already make silently, which is a
// superset of every host any real invocation has ever named.
fn port_of_listen[&a](text: &a [byte]) -> [] int {
    let at = bytes.find(text, ":");
    if at < 0 {
        return nat_of(text);
    }
    return nat_of(text[at + 1..len(text)]);
}

fn content_length_of[&h](head: &h [byte]) -> [] int {
    let at = bytes.find(head, "Content-Length: ");
    if at < 0 {
        return 0;
    }
    var i = at + len("Content-Length: ");
    var value = 0;
    while i < len(head) {
        let digit = bytes.digit_of(int_of(head[i]));
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
        let body = "{\"ok\":true,\"stub\":true}";
        let out = alloc_slice[scratch](len(body) + 128, byte_of(0));
        var at = sockets.put(out, 0, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ");
        at = sockets.put_nat(out, at, len(body));
        at = sockets.put(out, at, "\r\nConnection: close\r\n\r\n");
        at = sockets.put(out, at, body);
        sockets.write(libc, conn, out[0..at]);
    }
    return 0;
}

// Reads one request in full -- headers, then the body by
// `Content-Length`, the same shape `collect.ls`'s own `read_request`
// already established -- logs it, and answers. The body is drained but
// only its first `len(preview)` bytes are kept, per this file's own
// header comment.
fn handle[&f, &i](libc: &f Ffi("libc"), io: &!i Io, conn: int) -> [ffi("libc"), io_write] int {
    region scratch {
        let head = alloc_slice[scratch](8192, byte_of(0));
        let chunk = alloc_slice[scratch](4096, byte_of(0));
        let preview = alloc_slice[scratch](200, byte_of(0));
        var held = 0;
        var boundary = 0 - 1;
        var content_length = 0;
        var body_seen = 0;
        var preview_len = 0;
        var going = true;
        while going {
            let got = sockets.read(libc, conn, chunk);
            if got <= 0 {
                going = false;
            } else if boundary >= 0 {
                var k = 0;
                while k < got && preview_len < len(preview) {
                    preview[preview_len] = chunk[k];
                    preview_len = preview_len + 1;
                    k = k + 1;
                }
                body_seen = body_seen + got;
                if body_seen >= content_length {
                    going = false;
                }
            } else {
                let before = held;
                var take = got;
                if take > len(head) - held {
                    take = len(head) - held;
                }
                sockets.put(head, held, chunk[0..take]);
                held = held + take;
                let end = bytes.find(head[0..held], "\r\n\r\n");
                if end >= 0 {
                    boundary = end;
                    content_length = content_length_of(head[0..held]);
                    let from = end + 4 - before;
                    if from < got {
                        var k = from;
                        while k < got && preview_len < len(preview) {
                            preview[preview_len] = chunk[k];
                            preview_len = preview_len + 1;
                            k = k + 1;
                        }
                        body_seen = got - from;
                    }
                    if content_length == 0 || body_seen >= content_length {
                        going = false;
                    }
                } else if held == len(head) {
                    boundary = held;
                    going = false;
                }
            }
        }

        var line_end = bytes.find(head[0..held], "\r\n");
        if line_end < 0 {
            line_end = held;
        }
        let request_line = head[0..line_end];

        var header_count = 0;
        if boundary >= 0 && boundary > line_end + 2 {
            header_count = bytes.count_byte(head[line_end + 2..boundary], 10);
        }

        let ts = time(libc, 0);
        io.write_all(io, "[");
        io.print_int(io, ts);
        io.write_all(io, "] peer=? req=\"");
        io.write_all(io, request_line);
        io.write_all(io, "\" headers=");
        io.print_int(io, header_count);
        io.write_all(io, " body_len=");
        io.print_int(io, content_length);
        io.write_all(io, " body_preview=\"");
        io.write_all(io, preview[0..preview_len]);
        io.write_all(io, "\"\n");
        fflush(libc, 0);

        respond(libc, conn);
    }
    return 0;
}

// ---------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------

fn listen_forever[&f, &i](libc: &f Ffi("libc"), io: &!i Io, port: int)
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
        sockets.listen(libc, fd, 128);

        io.write_all(io, "results-stub: listening (the lex-sys port of the lex-os demo's allowed-egress target)\n");

        while true {
            let conn = sockets.accept(libc, fd, 0, 0);
            if conn >= 0 {
                handle(libc, io, conn);
                sockets.close(libc, conn);
            }
        }
    }
    // Unreachable: the loop above never exits. Here only because every
    // path out of a function needs a value of its declared type, and
    // this language has no `!`/never type to say "this path does not
    // exist" instead.
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(heap);

    let libc = narrow(ffi, "libc");
    var port = 8443;
    var run = true;
    var status = 0;
    borrow args as &g in {
        if arg_count(g) > 1 {
            if bytes.equal(arg(g, 1), "-h") || bytes.equal(arg(g, 1), "--help") {
                borrow mut io as &!i in {
                    io.write_all(i, "results-stub: HTTP stub for the lex-os demo's results.demo.internal target.\n");
                    io.write_all(i, "usage: results-stub [--listen HOST:PORT]   (default: 0.0.0.0:8443)\n");
                }
                run = false;
            } else if bytes.equal(arg(g, 1), "--listen") && arg_count(g) > 2 {
                port = port_of_listen(arg(g, 2));
            } else {
                borrow mut io as &!i in {
                    io.error_all(i, "results-stub: bad arguments; usage: results-stub [--listen HOST:PORT]\n");
                }
                run = false;
                status = 2;
            }
        }
    }

    if run {
        borrow libc as &f in {
            borrow mut io as &!i in {
                status = listen_forever(f, i, port);
            }
        }
    }
    release(libc);
    release(args);
    release(io);
    return status;
}
