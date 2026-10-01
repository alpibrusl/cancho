// `net.sockets` -- the first real `lex-sys-vcs` package.
//
// `examples/serve/serve.ls` and `examples/results_stub/results_stub.ls`
// each declared these same eight `extern fn`s against libc and the same
// two byte-writing helpers, independently -- the same shape `modules.md`
// §2's own "the functions here are byte-for-byte the ones 25 other files
// in this repository each define for themselves" already named for a
// different pair of files. Unlike that case, these do not belong in
// `std`: a raw-socket binding and a decimal-digit writer are not
// language-general the way `std.io`/`std.buffer` are, they are what two
// *particular* programs both happen to want. That is what a package is
// for (`docs/package-system.md` §4.1): a `lex-sys-vcs` store someone
// else's program locks and fetches, not something every program gets by
// default the way `--std` does.
//
// `docs/reach.md` §3 is why the eight externs are shaped the way they
// are: `c_int` rather than `int` where the real C function returns a
// 32-bit `int` and a caller's `< 0` check needs the sign bit this
// backend actually put there; `read`/`write` stay plain `int` because
// their real return is `ssize_t`, genuinely 64 bits here; `accept`'s two
// trailing `NULL`s are because a `struct sockaddr *` the kernel fills in
// is neither the "pointer and length" shape nor the "value across,
// nothing back" shape any foreign call here can express, so the peer's
// address is out of reach for either caller.

module net.sockets;

extern fn socket[&f](ffi: &f Ffi("libc"), domain: int, kind: int, proto: int) -> [ffi("libc")] c_int;

extern fn setsockopt[&f, &v](ffi: &f Ffi("libc"), fd: int, level: int, name: int, value: &v [byte]) -> [ffi("libc")] c_int;

extern fn bind[&f, &a](ffi: &f Ffi("libc"), fd: int, addr: &a [byte]) -> [ffi("libc")] c_int;

extern fn listen[&f](ffi: &f Ffi("libc"), fd: int, backlog: int) -> [ffi("libc")] c_int;

extern fn accept[&f](ffi: &f Ffi("libc"), fd: int, addr: int, len: int) -> [ffi("libc")] c_int;

extern fn read[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &!b [byte]) -> [ffi("libc")] int;

extern fn write[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &b [byte]) -> [ffi("libc")] int;

extern fn close[&f](ffi: &f Ffi("libc"), fd: int) -> [ffi("libc")] c_int;

// Copy `src` into `dst` at `at` and hand back where the next one goes.
pub fn put[&s, &d](dst: &!d [byte], at: int, src: &s [byte]) -> [] int {
    var i = 0;
    while i < len(src) {
        dst[at + i] = src[i];
        i = i + 1;
    }
    return at + len(src);
}

// A decimal integer, written into `dst` and not allocated anywhere. The
// digits come out backwards and are reversed in place.
pub fn put_nat[&d](dst: &!d [byte], at: int, n: int) -> [] int {
    if n == 0 {
        dst[at] = byte_of('0');
        return at + 1;
    }
    var rest = n;
    var end = at;
    while rest > 0 {
        dst[end] = byte_of('0' + rest - rest / 10 * 10);
        rest = rest / 10;
        end = end + 1;
    }
    var lo = at;
    var hi = end - 1;
    while lo < hi {
        let swap = dst[lo];
        dst[lo] = dst[hi];
        dst[hi] = swap;
        lo = lo + 1;
        hi = hi - 1;
    }
    return end;
}
