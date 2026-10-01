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
    };

    pub const DARWIN: SocketOs = SocketOs {
        sol_socket: 0xffff,
        so_reuseaddr: 4,
        so_reuseport: 0x200,
        so_nosigpipe: 0x1022,
        msg_nosignal: 0,
        o_nonblock: 4,
        eagain: 35,
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
