// `socket` -- the raw TCP connect `tls_client.ls`'s TLS layer sits on
// top of, kept in its own module and on its own edition for one reason:
// this file declares its own `extern fn connect`, and edition 2 made
// `connect` a builtin (`Net`, `docs/net.md`'s own narrowed capability).
// `docs/editions.md` §7 made a file's edition a marker of that file, not
// of the whole program, exactly so a file written before a name became
// a builtin does not have to be rewritten around the collision --
// `tls_client.ls` needs edition 3 for `c_ptr`, and this file, needing
// none of edition 2 or 3's additions, stays on edition 1, where
// `connect` still resolves to this declaration rather than the builtin
// (`crates/lex-sys-ir/src/lower/expr.rs`'s own comment: "an edition-1
// file's own `extern fn connect` must reach its `Extern` arm, not
// [the builtin]").
//
// The builtin was not usable here regardless of the name collision:
// `narrow`'s bound must be a literal (`docs/net.md`), which cannot
// express a port a test picks freely at run time -- the same reason
// `examples/fetch/` and `examples/report/` never adopted it either.
// Copied from those two, unchanged, down to the comments.

module socket;

extern fn socket[&f](ffi: &f Ffi("libc"), domain: int, kind: int, proto: int)
    -> [ffi("libc")] c_int;
extern fn connect[&f, &a](ffi: &f Ffi("libc"), fd: int, addr: &a [byte])
    -> [ffi("libc")] c_int;
extern fn close[&f](ffi: &f Ffi("libc"), fd: int) -> [ffi("libc")] c_int;

// `struct sockaddr_in`, sixteen bytes, in the Linux layout
// (`docs/connect.md` §3 -- portable to macOS by BSD's own compatibility
// rule, not by being right).
fn address[&o, &a](out: &!a [byte], octets: &o [byte], port: int) -> [] int {
    out[0] = byte_of(2);
    out[1] = byte_of(0);
    out[2] = byte_of(port / 256);
    out[3] = byte_of(port % 256);
    var i = 0;
    while i < 4 {
        out[4 + i] = octets[i];
        i = i + 1;
    }
    return 0;
}

pub fn connect_to[&f, &o](libc: &f Ffi("libc"), octets: &o [byte], port: int)
    -> [ffi("libc")] int {
    region scratch {
        let addr = alloc_slice[scratch](16, byte_of(0));
        let fd = socket(libc, 2, 1, 0);
        if fd < 0 {
            return 0 - 1;
        }
        address(addr, octets, port);
        if connect(libc, fd, addr) == 0 {
            return fd;
        }
        close(libc, fd);
    }
    return 0 - 1;
}

pub fn close_fd[&f](libc: &f Ffi("libc"), fd: int) -> [ffi("libc")] int {
    return close(libc, fd);
}
