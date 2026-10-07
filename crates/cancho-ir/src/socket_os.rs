//! The numbers the socket builtins need that Linux and Darwin disagree on
//! (`docs/native-sockets.md` §3), in one place so the two backends cannot.
//!
//! The point of making the socket verbs builtins rather than leaving them
//! to `extern fn` is that a program no longer has to know these: they are
//! chosen by the target, once, here.

/// One kernel's answers.
#[derive(Clone, Copy, Debug)]
pub struct SocketOs {
    /// `SOL_SOCKET`.
    pub sol_socket: i64,
    pub so_reuseaddr: i64,
    pub so_reuseport: i64,
    /// `SO_NOSIGPIPE` where the platform has it (Darwin), else 0.
    pub so_nosigpipe: i64,
    /// `MSG_NOSIGNAL` where the platform has it (Linux), else 0.
    pub msg_nosignal: i64,
    pub o_nonblock: i64,
    pub eagain: i64,
    /// `SO_ERROR`: the pending error of a socket, which is how a non-blocking `connect` says how it ended.
    pub so_error: i64,
    /// `EINPROGRESS`: what a non-blocking `connect` that has not finished answers.
    pub einprogress: i64,
    /// `SOCK_CLOEXEC`, or'd into `socket`'s type and passed to `accept4`,
    /// where the platform has it (Linux); 0 on Darwin, which has neither and
    /// sets `FD_CLOEXEC` with `fcntl` straight after (`docs/processes.md` §4.5).
    pub sock_cloexec: i64,
    /// `F_DUPFD_CLOEXEC`: a duplicate that is close-on-exec from the start.
    pub f_dupfd_cloexec: i64,
    /// `MSG_TRUNC` as a `recv` flag, which makes a datagram socket answer the datagram's
    /// *real* length even when the buffer was shorter (Linux); 0 on Darwin, which has no such
    /// flag on input, so a full buffer is the only sign (`docs/udp.md` §3).
    pub msg_trunc: i64,
}

impl SocketOs {
    pub const LINUX: SocketOs = SocketOs {
        sol_socket: 1,
        so_reuseaddr: 2,
        so_reuseport: 15,
        so_nosigpipe: 0,
        msg_nosignal: 0x4000,
        o_nonblock: 0o4000,
        eagain: 11,
        so_error: 4,
        einprogress: 115,
        sock_cloexec: 0x80000,
        f_dupfd_cloexec: 1030,
        msg_trunc: 0x20,
    };

    pub const DARWIN: SocketOs = SocketOs {
        sol_socket: 0xffff,
        so_reuseaddr: 4,
        so_reuseport: 0x200,
        so_nosigpipe: 0x1022,
        msg_nosignal: 0,
        o_nonblock: 4,
        eagain: 35,
        so_error: 0x1007,
        einprogress: 36,
        sock_cloexec: 0,
        f_dupfd_cloexec: 67,
        msg_trunc: 0,
    };

    pub fn for_darwin(darwin: bool) -> SocketOs {
        if darwin { SocketOs::DARWIN } else { SocketOs::LINUX }
    }
}

/// `EINVAL`, the same on both.
pub const EINVAL: i64 = 22;
/// `fcntl` commands, the same on both.
pub const F_GETFL: i64 = 3;
pub const F_SETFL: i64 = 4;
pub const F_SETFD: i64 = 2;
/// The one descriptor flag, the same on both.
pub const FD_CLOEXEC: i64 = 1;

/// How many descriptors a ticket can name (`conn_detach`): the epoch table
/// has one 32-bit counter per descriptor below this.
pub const FD_EPOCH_SLOTS: i64 = 65536;
/// The epoch table's symbol, defined once per program by the entry point.
pub const FD_EPOCH_GLOBAL: &str = "lexs_fd_epoch";
