//! The numbers the process builtins need (`docs/processes.md` §6), in one
//! place so the two backends cannot disagree.

use crate::{CLAIMABLE_SIGNALS, native_signal_number};

/// `child_kill`'s bit for `SIGKILL`, which no program can claim and every
/// parent may send (`docs/processes.md` §4.7). 9 on both kernels.
pub const KILL_BIT: i64 = 256;
pub const SIGKILL: i64 = 9;

/// Room for a `posix_spawn_file_actions_t` or a `posix_spawnattr_t`: 80 and
/// 336 bytes on glibc (measured), one pointer on Darwin. Initialised and
/// destroyed by libc; the backend only owns the bytes.
pub const SPAWN_OBJECT_BYTES: i64 = 512;
/// Room for a `sigset_t`: 128 bytes on glibc, 4 on Darwin.
pub const SIGSET_BYTES: i64 = 128;
/// `O_RDWR`, for a stream that is `/dev/null`; the same on both.
pub const O_RDWR: i64 = 2;
/// `AF_UNIX` and `SOCK_STREAM`, the same on both.
pub const AF_UNIX: i64 = 1;
pub const SOCK_STREAM: i64 = 1;

/// `posix_spawnattr_setflags`'s flags: `POSIX_SPAWN_SETSIGDEF` and
/// `POSIX_SPAWN_SETSIGMASK` (4 and 8 on both), and on Darwin
/// `POSIX_SPAWN_CLOEXEC_DEFAULT` (0x4000), which closes every descriptor the
/// file actions do not name -- the window slice 0 leaves on Darwin (§4.5).
pub fn spawn_flags(darwin: bool) -> i64 {
    let both = 0x4 | 0x8;
    if darwin { both | 0x4000 } else { both }
}

/// The signals `child_kill` can send, as `(bit, native number)`: every
/// claimable one, in `std.signals`' bits, and `KILL`.
pub fn sendable_signals(darwin: bool) -> Vec<(i64, i64)> {
    let mut all: Vec<(i64, i64)> =
        CLAIMABLE_SIGNALS.iter().map(|s| (s.bit, native_signal_number(s, darwin))).collect();
    all.push((KILL_BIT, SIGKILL));
    all
}
