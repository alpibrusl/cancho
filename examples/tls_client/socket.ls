// `socket` -- the raw TCP connect `tls_client.ls`'s TLS layer sits on
// top of, kept in its own module and on its own edition for one reason:
// `net.connect`'s own `connect` is `pub extern fn connect`, an ordinary
// declaration rather than a builtin, and importing it here needs no
// edition beyond 1 -- `tls_client.ls` needs edition 3 for `c_ptr`, and
// this file, needing none of edition 2 or 3's additions, stays behind
// so the two do not have to share one.
//
// `packages/net-connect/connect.ls` (`docs/package-system.md` §6) is
// `octets_of`/`port_of`/`address`/`connect_to` -- what this file used
// to declare for itself, byte for byte, until `docs/next-phase.md`
// §4.1's standing duplication check found the copy: `net.connect`'s own
// header already named `examples/fetch/`, `examples/report/`,
// `examples/vsock/`, and `examples/agent_guest/` as the four files it
// was extracted from, and this was the pre-extraction fifth the move
// never reached. `net.sockets` supplies the `socket`/`close` `connect_to`
// itself now calls internally.

module socket;

import net.sockets;
import net.connect;

// Not named `connect_to`: this file and `net.connect` are compiled into
// one program, and the backend's own symbol for a function is its name
// alone (`crates/lex-sys-codegen/src/abi.rs`'s `lexs_` prefix, no module
// qualifier) -- two `pub fn connect_to`s in one build collide at the
// object file, caught only by `clang -c` refusing the emitted module,
// not by the type checker, which resolves the two by module just fine.
pub fn open[&f, &o](libc: &f Ffi("libc"), octets: &o [byte], port: int) -> [ffi("libc")] int {
    return connect.connect_to(libc, octets, port);
}

pub fn close_fd[&f](libc: &f Ffi("libc"), fd: int) -> [ffi("libc")] int {
    return sockets.close(libc, fd);
}
