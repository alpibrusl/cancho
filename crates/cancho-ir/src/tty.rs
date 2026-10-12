//! The serial-port numbers, in one place so the two backends cannot
//! disagree (`docs/tty.md` §2, every value measured by cancho-robot's
//! spike: `spikes/ping/abi.c` on darwin-aarch64 and
//! `spikes/ping/abi_linux.c` on the robot's Raspberry Pi 5, glibc 2.41).
//!
//! A program writing `extern fn` carries these per platform in source,
//! with no help from the compiler; under `Tty` they are the backend's,
//! written once in Rust. They differ between the two targets in almost
//! every number, so they are chosen by the *target being compiled for*
//! ([`TtyOs::for_darwin`], as `SocketOs` is), never by the host the
//! compiler happens to run on. The macOS half contains the
//! variadic-`ioctl` trap the spike measured (research log 2026-10-08,
//! findings 4 and 9): declared plainly, `ioctl` answers 0 and configures
//! garbage, a silent dead bus rather than an error.

/// `EINVAL`, what `tty_configure` answers for a speed the platform has no
/// rate for (the same number on both targets).
pub const TTY_EINVAL: i64 = 22;

/// `O_RDWR`, the access mode `tty_open` asks for (the same on both targets).
pub const TTY_O_RDWR: i64 = 2;

/// `TCSANOW`: configure now, not after drains -- a servo bus has nothing
/// to drain and a drain would be a hang (the same on both targets).
pub const TCSANOW: i64 = 0;

/// `IOSSIOSPEED`, macOS only: **measured** on that target (`0x80085402`,
/// `_IOW('T', 2, speed_t)` with an 8-byte `speed_t`). pyserial 3.5
/// hard-codes `0x80045402` (a 4-byte size); both are accepted by the
/// observed driver, but the number in the backend is the measured one
/// (`docs/tty.md` §2's note).
pub const IOSSIOSPEED: i64 = 0x8008_5402;

/// The standard rate macOS's first step runs at, before `IOSSIOSPEED`
/// sets the real one: `tcsetattr` refuses (`EINVAL`) a speed past
/// macOS's standard set, and a zero speed is `B0`, which hangs the line
/// up. The spike used 115200 and it was accepted.
pub const MACOS_STANDARD_STEP: i64 = 115_200;

/// Everything about `struct termios` and the port's flags that differs
/// between the two targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtyOs {
    /// `sizeof(struct termios)`: 72 on macOS, 60 on Linux/glibc.
    pub termios_size: usize,
    /// The width of one `tcflag_t`/`cc_t`-padded word, which the struct is
    /// zeroed in: 8 on macOS, 4 on Linux.
    pub word: usize,
    /// `offsetof(struct termios, c_cflag)`.
    pub cflag_at: usize,
    /// `offsetof(struct termios, c_cc)`.
    pub cc_at: usize,
    /// `VMIN`'s and `VTIME`'s indices in `c_cc`.
    pub vmin: usize,
    pub vtime: usize,
    /// `offsetof(struct termios, c_ispeed)` and `c_ospeed`, which macOS
    /// reads on `tcsetattr`. Linux/glibc takes the speed from `c_cflag`'s
    /// `CBAUD` bits, so there they are left zero.
    pub ispeed_at: usize,
    pub ospeed_at: usize,
    pub o_noctty: i64,
    pub o_nonblock: i64,
    /// `CS8 | CLOCAL | CREAD`, raw 8N1 without a modem: the one setting
    /// `tty_configure` makes (`docs/tty.md` §3: no mode algebra).
    /// `CS8` is `0x30` on Linux and `0x300` on macOS.
    pub cflag: i64,
    /// `TCIFLUSH`, what `tty_flush_input` drops.
    pub tciflush: i64,
}

impl TtyOs {
    pub const fn for_darwin(darwin: bool) -> TtyOs {
        if darwin {
            TtyOs {
                termios_size: 72,
                word: 8,
                cflag_at: 16,
                cc_at: 32,
                vmin: 16,
                vtime: 17,
                ispeed_at: 56,
                ospeed_at: 64,
                o_noctty: 131072,
                o_nonblock: 4,
                // CS8 0x300 | CLOCAL 0x8000 | CREAD 0x800
                cflag: 0x300 | 0x8000 | 0x800,
                tciflush: 1,
            }
        } else {
            TtyOs {
                termios_size: 60,
                word: 4,
                cflag_at: 8,
                cc_at: 17,
                vmin: 6,
                vtime: 5,
                ispeed_at: 0,
                ospeed_at: 0,
                o_noctty: 256,
                o_nonblock: 2048,
                // CS8 0x30 | CLOCAL 0x800 | CREAD 0x80
                cflag: 0x30 | 0x800 | 0x80,
                tciflush: 0,
            }
        }
    }

    /// `O_RDWR | O_NOCTTY | O_NONBLOCK`, the flags `tty_open` uses. The
    /// nonblocking bit is why `tty_read` never blocks and the poller is
    /// the waiting story (`docs/tty.md` §3).
    pub const fn open_flags(&self) -> i64 {
        TTY_O_RDWR | self.o_noctty | self.o_nonblock
    }
}

/// Linux's standard rates and their `Bxxx` constants (`termbits.h`), which
/// `tty_configure` ORs into `c_cflag`'s `CBAUD` bits. A rate not in this
/// table is refused with [`TTY_EINVAL`]; macOS has no table, because its
/// speed goes in by `IOSSIOSPEED` and the driver is the judge.
pub const LINUX_RATES: [(i64, i64); 30] = [
    (50, 0o1),
    (75, 0o2),
    (110, 0o3),
    (134, 0o4),
    (150, 0o5),
    (200, 0o6),
    (300, 0o7),
    (600, 0o10),
    (1200, 0o11),
    (1800, 0o12),
    (2400, 0o13),
    (4800, 0o14),
    (9600, 0o15),
    (19200, 0o16),
    (38400, 0o17),
    (57600, 0o10001),
    (115200, 0o10002),
    (230400, 0o10003),
    (460800, 0o10004),
    (500000, 0o10005),
    (576000, 0o10006),
    (921600, 0o10007),
    (1000000, 0o10010),
    (1152000, 0o10011),
    (1500000, 0o10012),
    (2000000, 0o10013),
    (2500000, 0o10014),
    (3000000, 0o10015),
    (3500000, 0o10016),
    (4000000, 0o10017),
];

/// The `c_cflag` speed bits for `baud` on Linux, or `None` when it is not a
/// standard rate there.
pub fn linux_speed_bits(baud: i64) -> Option<i64> {
    LINUX_RATES.iter().find(|(rate, _)| *rate == baud).map(|(_, bits)| *bits)
}

/// Whether macOS's `IOSSIOSPEED` is worth asking about `baud`: a positive
/// rate that fits `speed_t`'s positive range. The driver judges the rest.
pub const fn macos_speed_plausible(baud: i64) -> bool {
    baud > 0 && baud < 0x7fff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_targets_keep_their_measured_layouts() {
        let mac = TtyOs::for_darwin(true);
        let linux = TtyOs::for_darwin(false);
        assert_eq!((mac.termios_size, mac.word, mac.cflag_at, mac.cc_at), (72, 8, 16, 32));
        assert_eq!((linux.termios_size, linux.word, linux.cflag_at, linux.cc_at), (60, 4, 8, 17));
        assert_eq!((mac.vmin, mac.vtime, linux.vmin, linux.vtime), (16, 17, 6, 5));
        assert_eq!((mac.tciflush, linux.tciflush), (1, 0));
    }

    #[test]
    fn the_flags_fit_inside_the_struct_on_both_targets() {
        for darwin in [false, true] {
            let os = TtyOs::for_darwin(darwin);
            // every offset a store is made at, plus the width stored
            assert!(os.cflag_at + os.word <= os.termios_size);
            assert!(os.cc_at + os.vmin.max(os.vtime) < os.termios_size);
            if darwin {
                assert!(os.ospeed_at + 8 <= os.termios_size);
                assert!(os.ispeed_at + 8 <= os.ospeed_at);
            }
            // the struct is zeroed in whole words
            assert_eq!(os.termios_size % os.word, 0, "darwin={darwin}");
        }
    }

    #[test]
    fn cs8_is_the_targets_own() {
        // the two targets' CSIZE fields sit at different bits; the byte is
        // 8 bits wide only if the right pattern is set
        assert_eq!(TtyOs::for_darwin(false).cflag & 0x30, 0x30);
        assert_eq!(TtyOs::for_darwin(true).cflag & 0x300, 0x300);
    }

    #[test]
    fn a_standard_rate_has_its_bits_and_anything_else_is_refused() {
        assert_eq!(linux_speed_bits(1_000_000), Some(0o10010));
        assert_eq!(linux_speed_bits(115_200), Some(0o10002));
        assert_eq!(linux_speed_bits(9600), Some(0o15));
        assert_eq!(linux_speed_bits(12_345), None);
        assert_eq!(linux_speed_bits(0), None);
        assert_eq!(linux_speed_bits(-1), None);
        // every rate in the table is distinct, in both columns
        for (i, (rate, bits)) in LINUX_RATES.iter().enumerate() {
            for (other_rate, other_bits) in &LINUX_RATES[i + 1..] {
                assert_ne!(rate, other_rate);
                assert_ne!(bits, other_bits);
            }
            // CBAUD is 0o10017: no rate sets a bit outside it
            assert_eq!(bits & !0o10017, 0);
        }
    }

    #[test]
    fn macos_asks_the_driver_about_any_positive_rate() {
        assert!(macos_speed_plausible(1_000_000));
        assert!(!macos_speed_plausible(0));
        assert!(!macos_speed_plausible(-5));
        assert!(!macos_speed_plausible(0x7fff_ffff));
    }
}
