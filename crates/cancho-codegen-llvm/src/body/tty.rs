//! The serial-port verbs (`docs/tty.md` §3), LLVM: mirrors
//! `cancho-codegen`'s own `body/tty.rs`, with the numbers from
//! `cancho_ir::tty` so the two backends cannot disagree. The termios
//! layout, the B-constants and the macOS `IOSSIOSPEED` two-step are
//! emitted here, written once — the variadic-`ioctl` trap the spike
//! measured as a fixed call shaped the way the callee reads it
//! (`native-sockets.md` §3's `fcntl` precedent).

use crate::*;
use cancho_ir::{IOSSIOSPEED, LINUX_RATES, MACOS_STANDARD_STEP, TCSANOW, TTY_EINVAL, TtyOs};

impl FuncEmitter<'_> {
    /// `tty_open(tty, path)` — `openat(AT_FDCWD, path, O_RDWR | O_NOCTTY |
    /// O_NONBLOCK)` under the capability's prefix, answering a
    /// `TtyOpened`: tag 0 `Ok(Port)` with the descriptor, tag 1
    /// `Failed(errno)` — `open_file`'s tagging, `UdpOpened`'s shape.
    pub(crate) fn tty_open(&mut self, prefix: &str, args: &[Expr]) -> Result<Vec<LValue>, String> {
        let path = self.expr(&args[1])?;
        let path = self.checked_path(prefix, &path)?;
        let flags = TtyOs::for_darwin(self.is_darwin()).open_flags();
        let fd32 = self.open_at_cwd(&path, flags, 0);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = sext i32 {fd32} to i64\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let reason = self.errno();
        Ok(vec![LValue::Reg(tag), LValue::Reg(fd), reason])
    }

    /// `tty_configure(port, baud)` — raw mode, 8N1 and the speed, one call
    /// and no mode algebra (`docs/tty.md` §3). `0`, or the platform's
    /// `errno`; `EINVAL` for a speed the platform has no rate for, before
    /// anything is applied. The per-target split is `cancho_ir::TtyOs`,
    /// chosen by the target being compiled for; the walk is
    /// `cancho-codegen`'s own `tty_configure`'s.
    pub(crate) fn tty_configure(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let darwin = self.is_darwin();
        let os = TtyOs::for_darwin(darwin);
        let fd = self.handle_fd(&args[0]);
        let speed = operand(&args[1]);
        let n = self.fresh().trim_start_matches('%').to_string();
        let bad_speed = format!("tty.badspeed.{n}");
        let apply = format!("tty.apply.{n}");
        let step = format!("tty.step.{n}");
        let fail_set = format!("tty.failset.{n}");
        let fail_step = format!("tty.failstep.{n}");
        let good = format!("tty.good.{n}");
        let done = format!("tty.done.{n}");

        // The speed first. Linux: the standard rates' `Bxxx` bits by a
        // select chain over `LINUX_RATES` (`-1` until one matches, so it is
        // the unknown flag too); macOS: no bits, and a plausible positive
        // rate (the driver judges the rest).
        let (bits, unknown) = if darwin {
            let below = self.fresh();
            self.out.push_str(&format!("  {below} = add i64 {speed}, -1\n"));
            let unknown = self.fresh();
            self.out.push_str(&format!("  {unknown} = icmp uge i64 {below}, 2147483646\n"));
            ("0".to_string(), unknown)
        } else {
            let mut bits = "-1".to_string();
            for (rate, rate_bits) in LINUX_RATES {
                let is = self.fresh();
                self.out.push_str(&format!("  {is} = icmp eq i64 {speed}, {rate}\n"));
                let next = self.fresh();
                self.out
                    .push_str(&format!("  {next} = select i1 {is}, i64 {rate_bits}, i64 {bits}\n"));
                bits = next;
            }
            let unknown = self.fresh();
            self.out.push_str(&format!("  {unknown} = icmp slt i64 {bits}, 0\n"));
            (bits, unknown)
        };
        self.out.push_str(&format!("  br i1 {unknown}, label %{bad_speed}, label %{apply}\n"));
        self.out.push_str(&format!("{bad_speed}:\n  br label %{done}\n{apply}:\n"));

        // The one `struct termios` this backend owns.
        let termios = self.fresh();
        self.hoist(format!("  {termios} = alloca i8, i64 {}\n", os.termios_size));

        // The whole struct is zeroed first, one word per pass at the
        // target's own width: an alloca answers what was on the stack, and
        // raw 8N1 is *defined by* the zeros -- no input, output or local
        // flags, no line discipline, `VMIN 0 / VTIME 0` -- so this is not
        // hygiene but the configuration itself, and `c_cflag` is the one
        // field that is not zero by design.
        let width = if os.word == 8 { "i64" } else { "i32" };
        let mut at = 0;
        while at < os.termios_size {
            self.store_word(&termios, at, width, "0");
            at += os.word;
        }
        let cflag64 = self.fresh();
        self.out.push_str(&format!("  {cflag64} = or i64 {bits}, {}\n", os.cflag));
        let cflag = if os.word == 8 {
            cflag64
        } else {
            let narrowed = self.fresh();
            self.out.push_str(&format!("  {narrowed} = trunc i64 {cflag64} to i32\n"));
            narrowed
        };
        self.store_word(&termios, os.cflag_at, width, &cflag);
        if darwin {
            // macOS reads the speed from `c_ispeed`/`c_ospeed`, and zero
            // there is `B0`: hang up. The first step runs at a standard
            // rate; `IOSSIOSPEED` below sets the real one.
            let standard = MACOS_STANDARD_STEP.to_string();
            self.store_word(&termios, os.ispeed_at, width, &standard);
            self.store_word(&termios, os.ospeed_at, width, &standard);
        }
        self.store_cc(&termios, os.cc_at + os.vmin, 0);
        self.store_cc(&termios, os.cc_at + os.vtime, 0);

        // `tcsetattr(fd, TCSANOW, t)`: on Linux the whole configuration; on
        // macOS the first step, at a standard rate.
        let applied = self.fresh();
        self.out.push_str(&format!(
            "  {applied} = call i32 @tcsetattr(i32 {fd}, i32 {TCSANOW}, ptr {termios})\n"
        ));
        let set_ok = self.fresh();
        self.out.push_str(&format!("  {set_ok} = icmp sge i32 {applied}, 0\n"));
        let after_set = if darwin { &step } else { &good };
        self.out.push_str(&format!("  br i1 {set_ok}, label %{after_set}, label %{fail_set}\n"));

        if darwin {
            // `ioctl(fd, IOSSIOSPEED, &speed)` is variadic; LLVM lowers the
            // call the way the callee reads it (on Apple arm64 the pointer
            // goes on the stack), so no padding is written here -- the
            // `fcntl` calls in this backend are the precedent.
            self.out.push_str(&format!("{step}:\n"));
            let cell = self.fresh();
            self.hoist(format!("  {cell} = alloca i64\n"));
            self.out.push_str(&format!("  store i64 {speed}, ptr {cell}\n"));
            let stepped = self.fresh();
            self.out.push_str(&format!(
                "  {stepped} = call i32 (i32, i32, ...) @ioctl(i32 {fd}, i32 {}, ptr {cell})\n",
                IOSSIOSPEED as u32 as i32
            ));
            let step_ok = self.fresh();
            self.out.push_str(&format!("  {step_ok} = icmp sge i32 {stepped}, 0\n"));
            self.out.push_str(&format!("  br i1 {step_ok}, label %{good}, label %{fail_step}\n"));
            self.out.push_str(&format!("{fail_step}:\n"));
        } else {
            // no speed step on Linux: the block exists so the answer's phi
            // reads the same on both targets, and is never branched to
            self.out.push_str(&format!("{fail_step}:\n"));
        }
        // A refused step answers the `errno` of the call that failed, read
        // before anything else can overwrite it.
        let step_reason = self.errno();
        let step_reason = operand(&step_reason);
        self.out.push_str(&format!("  br label %{done}\n"));
        self.out.push_str(&format!("{fail_set}:\n"));
        let set_reason = self.errno();
        let set_reason = operand(&set_reason);
        self.out.push_str(&format!("  br label %{done}\n"));
        self.out.push_str(&format!("{good}:\n  br label %{done}\n{done}:\n"));

        let answer = self.fresh();
        self.out.push_str(&format!(
            "  {answer} = phi i64 [ {TTY_EINVAL}, %{bad_speed} ], [ {set_reason}, %{fail_set} ], [ {step_reason}, %{fail_step} ], [ 0, %{good} ]\n"
        ));
        Ok(vec![LValue::Reg(answer)])
    }

    /// `tty_read(port, into)` — one `read(2)`, never blocks. `-1` on
    /// error, the `Conn` verbs' convention.
    pub(crate) fn tty_read(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let st = self.size_ty();
        let want = self.size_arg(&operand(&args[2]));
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call {st} @read(i32 {fd}, ptr {}, {st} {want})\n",
            operand(&args[1])
        ));
        let widened = self.size_result(&moved, true);
        Ok(vec![LValue::Reg(widened)])
    }

    /// `tty_write(port, bytes)` — one `write(2)`, whole or short.
    /// `-1` on error.
    pub(crate) fn tty_write(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let st = self.size_ty();
        let want = self.size_arg(&operand(&args[2]));
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call {st} @write(i32 {fd}, ptr {}, {st} {want})\n",
            operand(&args[1])
        ));
        let widened = self.size_result(&moved, true);
        Ok(vec![LValue::Reg(widened)])
    }

    /// `tty_flush_input(port)` — `tcflush(fd, TCIFLUSH)`, the queue
    /// selector's number per target from `cancho_ir::tty`.
    pub(crate) fn tty_flush_input(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let flushed = self.fresh();
        self.out.push_str(&format!(
            "  {flushed} = call i32 @tcflush(i32 {fd}, i32 {})\n",
            TtyOs::for_darwin(self.is_darwin()).tciflush
        ));
        let widened = self.fresh();
        self.out.push_str(&format!("  {widened} = sext i32 {flushed} to i64\n"));
        Ok(vec![LValue::Reg(widened)])
    }

    // ---- helpers ---------------------------------------------------------

    /// A flags word of `struct termios`, at the target's width. The field
    /// address is an instruction, not a `getelementptr` constant expression:
    /// the base is a function-local value, and LLVM forbids one in a
    /// constant-expression position (`clang`'s "invalid use of
    /// function-local name", the emission the first cut shipped).
    fn store_word(&mut self, base: &str, at: usize, width: &str, value: &str) {
        let field = self.fresh();
        self.out
            .push_str(&format!("  {field} = getelementptr inbounds i8, ptr {base}, i64 {at}\n"));
        self.out.push_str(&format!("  store {width} {value}, ptr {field}\n"));
    }

    /// One byte of `c_cc`, the same way.
    fn store_cc(&mut self, base: &str, at: usize, value: i64) {
        let field = self.fresh();
        self.out
            .push_str(&format!("  {field} = getelementptr inbounds i8, ptr {base}, i64 {at}\n"));
        self.out.push_str(&format!("  store i8 {value}, ptr {field}\n"));
    }
}
