//! The serial-port verbs (`docs/tty.md` §3), Cranelift: the termios layout,
//! the B-constants and the macOS two-step live here, written once, instead
//! of in every program's source — the whole reason the capability exists
//! (`cancho-robot`'s spike measured a program carrying all of them, and the
//! variadic-`ioctl` trap that makes a silently dead bus, research log
//! findings 4 and 9).

use crate::body::BodyEmitter;
use crate::*;
use cancho_ir::{IOSSIOSPEED, LINUX_RATES, MACOS_STANDARD_STEP, TCSANOW, TTY_EINVAL, TtyOs};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{MemFlags, Value};

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `tty_open(tty, path)` — `openat(AT_FDCWD, path, O_RDWR | O_NOCTTY |
    /// O_NONBLOCK)`, under the capability's prefix, answering a
    /// `TtyOpened`: tag 0 `Ok(Port)` with the descriptor, tag 1
    /// `Failed(errno)` — `open_file`'s tagging, `UdpOpened`'s shape.
    pub(crate) fn tty_open(&mut self, prefix: &str, args: &[cancho_ir::Expr]) -> Vec<Value> {
        let path = self.expr(&args[1]);
        let path = self.checked_path(prefix, &path);
        let flags = TtyOs::for_darwin(self.is_darwin()).open_flags();
        let cwd = self.builder.ins().iconst(types::I32, -100);
        let fd = self.openat(cwd, path, flags, 0);
        let fd = self.builder.ins().sextend(types::I64, fd);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        let reason = self.errno();
        vec![tag, fd, reason]
    }

    /// `tty_configure(port, baud)` — raw mode, 8N1, and the speed, the one
    /// setting the robot needs (`docs/tty.md` §3: no mode algebra). `0`,
    /// or the `errno` the platform answered; `EINVAL` for a speed the
    /// platform has no rate for, before anything is applied.
    ///
    /// The two ABIs (`docs/tty.md` §2, both measured by the spike): Linux
    /// keeps a standard rate in `c_cflag`'s `CBAUD` bits and `tcsetattr`
    /// alone is enough; macOS answers `EINVAL` for a speed past its
    /// standard set, so raw mode is set at a standard rate first and the
    /// speed then goes in by the `IOSSIOSPEED` ioctl — the variadic call
    /// the spike measured failing *silently* when declared plainly. The
    /// target split is `cancho_ir::TtyOs`, chosen by the target being
    /// compiled for, so this one body serves both.
    pub(crate) fn tty_configure(&mut self, args: &[Value]) -> Vec<Value> {
        let os = TtyOs::for_darwin(self.is_darwin());
        let pointer = self.pointer;
        let fd = self.handle_fd(args[0]);
        let speed = args[1];

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);

        // The speed first: a rate the platform has no bits for is refused
        // before the port is touched. Linux maps the standard rates to
        // their `Bxxx` bits (a select chain over `LINUX_RATES`); macOS
        // only needs a plausible positive rate, the driver judges it.
        let (speed_bits, unknown) = self.tty_speed_bits(&os, speed);
        let refuse_speed = self.builder.create_block();
        let proceed = self.builder.create_block();
        self.builder.ins().brif(unknown, refuse_speed, &[], proceed, &[]);
        self.builder.switch_to_block(refuse_speed);
        self.builder.seal_block(refuse_speed);
        let invalid = self.builder.ins().iconst(types::I64, TTY_EINVAL);
        self.builder.ins().jump(merge, &[invalid.into()]);
        self.builder.switch_to_block(proceed);
        self.builder.seal_block(proceed);

        // The one `struct termios` the backend owns, on its stack slot
        // (`tcp_listen`'s own-slot shape).
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            os.termios_size as u32,
            3,
        ));
        let termios = self.builder.ins().stack_addr(pointer, slot, 0);

        // The whole struct is zeroed first, one word per pass at the
        // target's own width: the stack slot answers what was on the
        // stack, and raw 8N1 is *defined by* the zeros -- no input,
        // output or local flags, no line discipline, `VMIN 0 / VTIME 0`
        // -- so this is not hygiene but the configuration itself, and
        // `c_cflag` is the one field that is not zero by design.
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut at = 0;
        while at < os.termios_size {
            self.store_word(&os, termios, at, zero);
            at += os.word;
        }
        let cflag = self.builder.ins().bor_imm(speed_bits, os.cflag);
        self.store_word(&os, termios, os.cflag_at, cflag);
        if self.is_darwin() {
            // macOS reads the speed from `c_ispeed`/`c_ospeed`, and zero
            // there is `B0`: hang up. The first step runs at a standard
            // rate; `IOSSIOSPEED` below sets the real one.
            let standard = self.builder.ins().iconst(types::I64, MACOS_STANDARD_STEP);
            self.store_word(&os, termios, os.ispeed_at, standard);
            self.store_word(&os, termios, os.ospeed_at, standard);
        }
        // `VMIN 0 / VTIME 0` are the zeros already stored; the stores keep
        // the offsets in the code, where a layout change would show.
        self.store_byte(termios, os.cc_at + os.vmin, 0);
        self.store_byte(termios, os.cc_at + os.vtime, 0);

        let now = self.const_i32(TCSANOW);
        let applied = self.libc_call(
            "tcsetattr",
            &[types::I32, types::I32, pointer],
            &[types::I32],
            &[fd, now, termios],
        );
        let bad = self.builder.ins().icmp_imm(IntCC::SignedLessThan, applied, 0);
        let refuse = self.builder.create_block();
        let go_on = self.builder.create_block();
        self.builder.ins().brif(bad, refuse, &[], go_on, &[]);
        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let reason = self.errno();
        self.builder.ins().jump(merge, &[reason.into()]);
        self.builder.switch_to_block(go_on);
        self.builder.seal_block(go_on);

        // The speed step, macOS only: Linux's standard rate is applied.
        if self.is_darwin() {
            let stepped = self.tty_iossiospeed(fd, speed);
            let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, stepped, 0);
            let refuse = self.builder.create_block();
            let done = self.builder.create_block();
            self.builder.ins().brif(failed, refuse, &[], done, &[]);
            self.builder.switch_to_block(refuse);
            self.builder.seal_block(refuse);
            let reason = self.errno();
            self.builder.ins().jump(merge, &[reason.into()]);
            self.builder.switch_to_block(done);
            self.builder.seal_block(done);
        }

        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into()]);
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        vec![self.builder.block_params(merge)[0]]
    }

    /// The `c_cflag` speed bits for `speed` and whether the platform has
    /// no rate for it. Linux: a select chain over the standard rates
    /// (`bits` is `-1` until one matches, so it is the unknown flag too);
    /// macOS: no bits, and a plausibility check on the rate.
    fn tty_speed_bits(&mut self, os: &TtyOs, speed: Value) -> (Value, Value) {
        if self.is_darwin() {
            let _ = os;
            let below = self.builder.ins().iadd_imm(speed, -1);
            // unsigned: 0 and negatives wrap past the limit with the too-large
            let unknown = self.builder.ins().icmp_imm(
                IntCC::UnsignedGreaterThanOrEqual,
                below,
                0x7fff_ffff - 1,
            );
            let none = self.builder.ins().iconst(types::I64, 0);
            return (none, unknown);
        }
        let mut bits = self.builder.ins().iconst(types::I64, -1);
        for (rate, rate_bits) in LINUX_RATES {
            let is = self.builder.ins().icmp_imm(IntCC::Equal, speed, rate);
            let candidate = self.builder.ins().iconst(types::I64, rate_bits);
            bits = self.builder.ins().select(is, candidate, bits);
        }
        let unknown = self.builder.ins().icmp_imm(IntCC::SignedLessThan, bits, 0);
        // an unknown rate never reaches the `bor` below (the branch refuses
        // it first), so the -1 is not or'd into the flags
        (bits, unknown)
    }

    /// macOS: `ioctl(fd, IOSSIOSPEED, &speed)` on an 8-byte speed cell,
    /// answering `ioctl`'s result. `ioctl` is variadic, and on Apple arm64 a
    /// variadic argument travels on the *stack* where a fixed one goes in a
    /// register, so on that one target the call is shaped the way the callee
    /// reads it, as `fcntl`'s is (`native-sockets.md` §3): nine integer
    /// parameters, the first eight in `x0..x7` (the descriptor, the request
    /// and six fillers) and the ninth -- the pointer `va_arg` reads -- in the
    /// first stack slot. Everywhere else a variadic and a fixed call agree.
    fn tty_iossiospeed(&mut self, fd: Value, speed: Value) -> Value {
        let pointer = self.pointer;
        let cell = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        let cell_at = self.builder.ins().stack_addr(pointer, cell, 0);
        self.builder.ins().store(MemFlags::trusted(), speed, cell_at, 0);
        // the request has its high bit set: as the signed 32-bit value `iconst` takes
        let request = self.const_i32(IOSSIOSPEED as u32 as i32 as i64);
        let apple_arm64 = matches!(
            self.module.isa().triple().architecture,
            target_lexicon::Architecture::Aarch64(_)
        );
        if apple_arm64 {
            let filler = self.builder.ins().iconst(types::I64, 0);
            let mut params = vec![types::I32, types::I32];
            params.extend([types::I64; 6]);
            params.push(pointer);
            let mut args = vec![fd, request];
            args.extend([filler; 6]);
            args.push(cell_at);
            self.libc_call("ioctl", &params, &[types::I32], &args)
        } else {
            self.libc_call(
                "ioctl",
                &[types::I32, types::I32, pointer],
                &[types::I32],
                &[fd, request, cell_at],
            )
        }
    }

    /// `tty_read(port, into)` — one `read(2)`, never blocks
    /// (`O_NONBLOCK`; the poller is the waiting story, `docs/tty.md` §3).
    /// `-1` on error, the `Conn` verbs' convention.
    pub(crate) fn tty_read(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.handle_fd(args[0]);
        vec![self.libc_call(
            "read",
            &[types::I32, pointer, types::I64],
            &[types::I64],
            &[fd, args[1], args[2]],
        )]
    }

    /// `tty_write(port, bytes)` — one `write(2)`, whole or short, on a
    /// shared reference as `conn_write`'s is. `-1` on error.
    pub(crate) fn tty_write(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.handle_fd(args[0]);
        vec![self.libc_call(
            "write",
            &[types::I32, pointer, types::I64],
            &[types::I64],
            &[fd, args[1], args[2]],
        )]
    }

    /// `tty_flush_input(port)` — `tcflush(fd, TCIFLUSH)`. The queue
    /// selector's number differs per target (1 on macOS, 0 on Linux —
    /// `docs/tty.md` §2), exactly a constant that does not belong in a
    /// program.
    pub(crate) fn tty_flush_input(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.handle_fd(args[0]);
        let queue = self.const_i32(TtyOs::for_darwin(self.is_darwin()).tciflush);
        let flushed =
            self.libc_call("tcflush", &[types::I32, types::I32], &[types::I32], &[fd, queue]);
        vec![self.builder.ins().sextend(types::I64, flushed)]
    }

    // ---- the target-split stores ----------------------------------------

    /// A word of `struct termios` at `at`, at the target's own width: 4
    /// bytes on Linux, 8 on macOS (`docs/tty.md` §2's table -- the
    /// difference that makes these the backend's, not a program's).
    fn store_word(&mut self, os: &TtyOs, base: Value, at: usize, value: Value) {
        if os.word == 8 {
            self.builder.ins().store(MemFlags::trusted(), value, base, at as i32);
        } else {
            let narrowed = self.builder.ins().ireduce(types::I32, value);
            self.builder.ins().store(MemFlags::trusted(), narrowed, base, at as i32);
        }
    }

    /// One byte of `c_cc`, at the target's own index.
    fn store_byte(&mut self, base: Value, at: usize, value: i64) {
        let stored = self.builder.ins().iconst(types::I8, value);
        self.builder.ins().store(MemFlags::trusted(), stored, base, at as i32);
    }

    fn const_i32(&mut self, value: i64) -> Value {
        self.builder.ins().iconst(types::I32, value)
    }
}
