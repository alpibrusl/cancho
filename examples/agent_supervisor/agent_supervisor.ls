// `agent_supervisor` -- the host side of `lex-os`'s guest/supervisor
// exchange, over plain HTTP/1.0 rather than `AF_VSOCK`.
//
//     agent_supervisor <port> <goal> <step>
//
// Binds the port named on the command line, accepts one connection,
// reads a `POST` request in full -- headers, then the body, by
// `Content-Length`, echoed to standard output the way `examples/collect/`
// already does -- and answers with an `AgentViewMsg` built from `<goal>`
// and `<step>`: `{"goal":"<goal>","step":<step>,"last_outcome":null,
// "completed":[],"reprovisions":0}`, checked byte for byte against real
// `serde_json` output for exactly that shape (a first step: no prior
// outcome, nothing completed yet, never reprovisioned).
//
// `examples/vsock/vsock.ls` plays the same exchange -- one `AgentViewMsg`
// out, one `AgentActionMsg` back -- over the real channel
// `lex-os-guest` uses, and says in its own comments that this sandbox
// has no `vhost_vsock`, so that round trip stays untested here. This
// program and `examples/agent_guest/` are not a second transport for
// `lex-os` -- `lex-os-proto` names no HTTP channel, and none is proposed
// here -- they exist to give the same view/action exchange a channel
// this sandbox *can* round-trip end to end, over a real socket, in real
// CI. The exchange is inverted from vsock's own shape only because HTTP
// is guest-initiated where a vsock stream lets the supervisor push
// first: `examples/agent_guest/` POSTs the action it would otherwise
// have sent last, and this program answers with the view it would
// otherwise have sent next.
//
// Like `examples/serve/` and `examples/collect/`, it is `extern fn`
// declarations against libc through `Ffi("libc")`, and its authority
// report says so and no more.

import std.bytes;
import std.io;
import net.sockets;
import http.request;

// ---------------------------------------------------------------------
// libc -- the eight `extern fn`s and the two byte helpers used to live
// here -- byte-for-byte the same as `examples/serve/serve.ls`'s and
// `examples/collect/collect.ls`'s own copies. All three now `import
// net.sockets` (`packages/net-sockets/`, `docs/package-system.md` §6)
// instead.
//
// `content_length_of`/`read_request` used to live here too, copied
// from `examples/collect/collect.ls`. Now `packages/http-request/
// request.ls`, this repository's fourth real package and the first
// that itself depends on a package (net.sockets, §4.6).
// ---------------------------------------------------------------------

// A base-ten value, as the command line spells it. Anything that is not
// a digit ends the number, the same rule `examples/serve/`'s `port_of`
// uses.
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
// The wire protocol -- `AgentViewMsg`, fixed to the one shape a first
// step ever has (`docs`-worth of design left at `examples/vsock/
// vsock.ls`'s own header). Not a JSON library: one escaper, borrowed
// from that same file, and one encoder for exactly this shape.
// ---------------------------------------------------------------------

// Append `src` into `dst` at `at`, escaping `"`, `\`, and the three
// common control bytes -- copied from `examples/vsock/vsock.ls`'s
// `append_json_escaped`, needed here because `goal` crosses from argv
// with no guarantee it is already safe JSON content, and there is
// nowhere to put a shared function between two examples
// (`docs/many-files.md` is about a program's own files, not the
// corpus -- `examples/report/report.ls`'s own header makes the same
// point about `octets_of`).
fn put_escaped[&s, &d](dst: &!d [byte], at: int, src: &s [byte]) -> [] int {
    var out = at;
    var i = 0;
    while i < len(src) {
        let c = int_of(src[i]);
        if c == 34 {
            out = sockets.put(dst, out, "\\\"");
        } else if c == 92 {
            out = sockets.put(dst, out, "\\\\");
        } else if c == 10 {
            out = sockets.put(dst, out, "\\n");
        } else if c == 13 {
            out = sockets.put(dst, out, "\\r");
        } else if c == 9 {
            out = sockets.put(dst, out, "\\t");
        } else {
            dst[out] = src[i];
            out = out + 1;
        }
        i = i + 1;
    }
    return out;
}

// `{"goal":"<goal>","step":<step>,"last_outcome":null,"completed":[],
// "reprovisions":0}` -- checked against real `serde_json` output for
// `AgentViewMsg { goal, step, last_outcome: None, completed: vec![],
// reprovisions: 0 }`.
fn encode_view[&g, &d](dst: &!d [byte], goal: &g [byte], step: int) -> [] int {
    var at = sockets.put(dst, 0, "{\"goal\":\"");
    at = put_escaped(dst, at, goal);
    at = sockets.put(dst, at, "\",\"step\":");
    at = sockets.put_nat(dst, at, step);
    return sockets.put(dst, at, ",\"last_outcome\":null,\"completed\":[],\"reprovisions\":0}");
}

// ---------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------

fn respond_with_view[&f, &g](libc: &f Ffi("libc"), conn: int, goal: &g [byte], step: int)
    -> [ffi("libc")] int {
    region scratch {
        let body = alloc_slice[scratch](len(goal) * 2 + 96, byte_of(0));
        let blen = encode_view(body, goal, step);
        let out = alloc_slice[scratch](blen + 96, byte_of(0));
        var at = sockets.put(out, 0, "HTTP/1.1 200 OK\r\nContent-Length: ");
        at = sockets.put_nat(out, at, blen);
        at = sockets.put(out, at, "\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n");
        at = sockets.put(out, at, body[0..blen]);
        sockets.write(libc, conn, out[0..at]);
    }
    return 0;
}

// ---------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------

fn run_supervisor[&f, &i, &g](libc: &f Ffi("libc"), io: &!i Io, port: int, goal: &g [byte],
    step: int) -> [ffi("libc"), io_write] int {
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
        respond_with_view(libc, conn, goal, step);
        sockets.close(libc, conn);
        sockets.close(libc, fd);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(heap);

    let libc = narrow(ffi, "libc");
    var status = 2;
    borrow mut io as &!i in {
        borrow libc as &f in {
            borrow args as &g in {
                if arg_count(g) != 4 {
                    io.error_all(i, "usage: agent_supervisor <port> <goal> <step>\n");
                } else {
                    let port = nat_of(arg(g, 1));
                    let step = nat_of(arg(g, 3));
                    status = run_supervisor(f, i, port, arg(g, 2), step);
                }
            }
        }
    }
    release(libc);
    release(args);
    release(io);
    return status;
}
