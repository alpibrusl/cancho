//! `errno`, translated: WASI's numbers to the language's (`docs/wasm.md`).
//!
//! A program that reads a failure's `errno` and compares it (`e == 2` for a
//! missing file) must mean the same thing on every target it is built for.
//! The numbers this language fixes itself -- `std.dirs.einval()` is 22,
//! `ENAMETOOLONG` is [`enametoolong`](crate::enametoolong)'s -- are Linux's, so
//! on WASI, whose own numbering is unrelated (`ENOENT` is 44, `EINVAL` 28),
//! the backend translates at the one place it reads `errno`.
//!
//! The rows are every `E*` in wasi-libc's `__errno_values.h`, as `(name, WASI
//! number, Linux number)`; a test checks the Linux numbers are distinct where
//! the names are, and that nothing maps to zero. Two have no Linux name of their
//! own and are judgment calls:
//!
//! * `ENOTSUP` is Linux's `EOPNOTSUPP` (95): the same errno under two names.
//! * `ENOTCAPABLE` -- the capability was not held -- has no Linux errno. It is
//!   `EPERM` (1), "operation not permitted", because a missing capability is a
//!   refusal of the operation rather than a missing file permission (`EACCES`).
//!
//! Darwin is **not** translated: its raw `errno` still reaches a program
//! (which is why `enametoolong` has a per-OS answer). That inconsistency is
//! older than WASI and is not changed here.

/// `(name, WASI errno, Linux errno)`, ordered by WASI number.
pub const WASI_ERRNO_TO_LINUX: &[(&str, i64, i64)] = &[
    ("E2BIG", 1, 7),
    ("EACCES", 2, 13),
    ("EADDRINUSE", 3, 98),
    ("EADDRNOTAVAIL", 4, 99),
    ("EAFNOSUPPORT", 5, 97),
    ("EAGAIN", 6, 11),
    ("EALREADY", 7, 114),
    ("EBADF", 8, 9),
    ("EBADMSG", 9, 74),
    ("EBUSY", 10, 16),
    ("ECANCELED", 11, 125),
    ("ECHILD", 12, 10),
    ("ECONNABORTED", 13, 103),
    ("ECONNREFUSED", 14, 111),
    ("ECONNRESET", 15, 104),
    ("EDEADLK", 16, 35),
    ("EDESTADDRREQ", 17, 89),
    ("EDOM", 18, 33),
    ("EDQUOT", 19, 122),
    ("EEXIST", 20, 17),
    ("EFAULT", 21, 14),
    ("EFBIG", 22, 27),
    ("EHOSTUNREACH", 23, 113),
    ("EIDRM", 24, 43),
    ("EILSEQ", 25, 84),
    ("EINPROGRESS", 26, 115),
    ("EINTR", 27, 4),
    ("EINVAL", 28, 22),
    ("EIO", 29, 5),
    ("EISCONN", 30, 106),
    ("EISDIR", 31, 21),
    ("ELOOP", 32, 40),
    ("EMFILE", 33, 24),
    ("EMLINK", 34, 31),
    ("EMSGSIZE", 35, 90),
    ("EMULTIHOP", 36, 72),
    ("ENAMETOOLONG", 37, 36),
    ("ENETDOWN", 38, 100),
    ("ENETRESET", 39, 102),
    ("ENETUNREACH", 40, 101),
    ("ENFILE", 41, 23),
    ("ENOBUFS", 42, 105),
    ("ENODEV", 43, 19),
    ("ENOENT", 44, 2),
    ("ENOEXEC", 45, 8),
    ("ENOLCK", 46, 37),
    ("ENOLINK", 47, 67),
    ("ENOMEM", 48, 12),
    ("ENOMSG", 49, 42),
    ("ENOPROTOOPT", 50, 92),
    ("ENOSPC", 51, 28),
    ("ENOSYS", 52, 38),
    ("ENOTCONN", 53, 107),
    ("ENOTDIR", 54, 20),
    ("ENOTEMPTY", 55, 39),
    ("ENOTRECOVERABLE", 56, 131),
    ("ENOTSOCK", 57, 88),
    ("ENOTSUP", 58, 95),
    ("ENOTTY", 59, 25),
    ("ENXIO", 60, 6),
    ("EOVERFLOW", 61, 75),
    ("EOWNERDEAD", 62, 130),
    ("EPERM", 63, 1),
    ("EPIPE", 64, 32),
    ("EPROTO", 65, 71),
    ("EPROTONOSUPPORT", 66, 93),
    ("EPROTOTYPE", 67, 91),
    ("ERANGE", 68, 34),
    ("EROFS", 69, 30),
    ("ESPIPE", 70, 29),
    ("ESRCH", 71, 3),
    ("ESTALE", 72, 116),
    ("ETIMEDOUT", 73, 110),
    ("ETXTBSY", 74, 26),
    ("EXDEV", 75, 18),
    ("ENOTCAPABLE", 76, 1),
];

/// The Linux number for a WASI `errno`; zero stays zero, and a number WASI
/// does not define passes through unchanged rather than being invented.
pub fn linux_errno_from_wasi(code: i64) -> i64 {
    WASI_ERRNO_TO_LINUX
        .iter()
        .find(|&&(_, wasi, _)| wasi == code)
        .map_or(code, |&(_, _, linux)| linux)
}
