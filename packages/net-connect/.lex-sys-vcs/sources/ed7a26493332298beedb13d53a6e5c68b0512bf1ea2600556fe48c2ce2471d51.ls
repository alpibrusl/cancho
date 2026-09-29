// `net.connect` -- the second real `lex-sys-vcs` package, and the first
// program here to need more than one at once.
//
// `examples/fetch/fetch.ls`, `examples/report/report.ls`,
// `examples/vsock/vsock.ls` and `examples/agent_guest/agent_guest.ls`
// each declare this same `extern fn connect` independently -- the
// outbound counterpart to `packages/net-sockets/sockets.ls`'s inbound
// `bind`/`listen`/`accept`. It stays a separate package rather than
// joining `net.sockets` because no program here needs both halves of
// the network at once (`docs/net.md` §1, "the two directions do not
// meet"): an outbound program wants `socket`/`read`/`write`/`close`
// from `net.sockets` plus this, never `setsockopt`/`bind`/`listen`/
// `accept` too. Splitting on that line is what makes locking a program
// to only the names it actually calls (`docs/package-system.md` §4.5)
// mean something -- a program that fetched one combined package would
// carry declarations for a direction it never opens.

module net.connect;

// The address crosses as a pointer and a length, the way `bind` does in
// `net.sockets`: the slice is the `struct sockaddr_in` and its length is
// the `socklen_t`.
pub extern fn connect[&f, &a](ffi: &f Ffi("libc"), fd: int, addr: &a [byte])
    -> [ffi("libc")] c_int;
