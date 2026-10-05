//! The signal claim (`docs/signals.md` section 5): `signals_watch`,
//! `signals_pending`, `signals_close`, and the count of live threads
//! `spawn` and `join` keep for them. Mirrors `lex-sys-codegen-llvm`'s own
//! `body/signals.rs`, which has the longer account.
//!
//! Linux: `pthread_sigmask` blocks the set and a `signalfd` reads it.
//! Darwin: each signal is ignored and a `kqueue` watches it with
//! `EVFILT_SIGNAL`. Either way the handle is the **native mask** in the
//! high 32 bits over the descriptor in the low 32, so a close needs no table.

use crate::*;
use cranelift_codegen::ir::AtomicRmwOp;
use lex_sys_ir::{CLAIMABLE_SIGNALS, EBUSY, SIGNAL_STATE_GLOBAL, native_signal_number};

const SIG_BLOCK: i64 = 0;
const SIG_UNBLOCK: i64 = 1;
/// `SFD_NONBLOCK | SFD_CLOEXEC` (`O_NONBLOCK | O_CLOEXEC`, the same on x86-64 and aarch64 Linux).
const SFD_FLAGS: i64 = 0o4000 | 0o2000000;
const SIGSET_BYTES: i64 = 128;
const SIGNALFD_RECORD: i64 = 128;
/// More than the eight claimable signals, so one read takes every queued one.
const RECORDS: i64 = 8;
const EVFILT_SIGNAL: i64 = -6;
const EV_ADD: i64 = 0x0001;
const EV_CLEAR: i64 = 0x0020;
const KEVENT_SIZE: i64 = 32;
/// The word in `SIGNAL_STATE_GLOBAL` after the claimed mask: spawned threads not yet joined.
const THREADS_AT: i32 = 8;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `spawn` adds one and `join` takes one away, atomically: any thread may spawn.
    pub(crate) fn count_thread(&mut self, delta: i64) {
        let state = self.global(SIGNAL_STATE_GLOBAL);
        let at = self.builder.ins().iadd_imm(state, i64::from(THREADS_AT));
        let amount = self.builder.ins().iconst(types::I64, delta);
        self.builder.ins().atomic_rmw(
            types::I64,
            MemFlags::trusted(),
            AtomicRmwOp::Add,
            at,
            amount,
        );
    }

    /// The kernel's mask for a set of lex-sys bits: eight selects, folded away
    /// where the bits are a literal.
    fn native_from_bits(&mut self, bits: Value) -> Value {
        let darwin = self.is_darwin();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut native = zero;
        for signal in CLAIMABLE_SIGNALS {
            let held = self.builder.ins().band_imm(bits, signal.bit);
            let on = self.builder.ins().icmp_imm(IntCC::NotEqual, held, 0);
            let mask = self
                .builder
                .ins()
                .iconst(types::I64, 1 << (native_signal_number(&signal, darwin) - 1));
            let part = self.builder.ins().select(on, mask, zero);
            native = self.builder.ins().bor(native, part);
        }
        native
    }

    /// The lex-sys bits of a kernel mask: the inverse of [`Self::native_from_bits`].
    fn bits_from_native(&mut self, native: Value) -> Value {
        let darwin = self.is_darwin();
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut bits = zero;
        for signal in CLAIMABLE_SIGNALS {
            let mask = 1 << (native_signal_number(&signal, darwin) - 1);
            let held = self.builder.ins().band_imm(native, mask);
            let on = self.builder.ins().icmp_imm(IntCC::NotEqual, held, 0);
            let bit = self.builder.ins().iconst(types::I64, signal.bit);
            let part = self.builder.ins().select(on, bit, zero);
            bits = self.builder.ins().bor(bits, part);
        }
        bits
    }

    /// A 128-byte `sigset_t` holding `native` in its first word, the rest zero.
    fn sigset_of(&mut self, native: Value) -> Value {
        let set = self.scratch_bytes(SIGSET_BYTES);
        let zero = self.builder.ins().iconst(types::I64, 0);
        for word in 1..SIGSET_BYTES / 8 {
            self.builder.ins().store(MemFlags::trusted(), zero, set, (word * 8) as i32);
        }
        self.builder.ins().store(MemFlags::trusted(), native, set, 0);
        set
    }

    pub(crate) fn scratch_bytes(&mut self, size: i64) -> Value {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size as u32,
            3,
        ));
        self.builder.ins().stack_addr(pointer, slot, 0)
    }

    /// `pthread_sigmask(how, set, NULL)`.
    fn sigmask(&mut self, how: i64, set: Value) {
        let pointer = self.pointer;
        let how = self.builder.ins().iconst(types::I32, how);
        let none = self.builder.ins().iconst(pointer, 0);
        self.libc_call(
            "pthread_sigmask",
            &[types::I32, pointer, pointer],
            &[types::I32],
            &[how, set, none],
        );
    }

    /// Run `body` once for each signal number set in the kernel mask `native`.
    fn each_native_signal(&mut self, native: Value, mut body: impl FnMut(&mut Self, Value)) {
        let head = self.builder.create_block();
        self.builder.append_block_param(head, types::I64);
        let test = self.builder.create_block();
        let run = self.builder.create_block();
        let next = self.builder.create_block();
        let done = self.builder.create_block();
        let first = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().jump(head, &[first.into()]);

        self.builder.switch_to_block(head);
        let n = self.builder.block_params(head)[0];
        let more = self.builder.ins().icmp_imm(IntCC::SignedLessThanOrEqual, n, 31);
        self.builder.ins().brif(more, test, &[], done, &[]);

        self.builder.switch_to_block(test);
        self.builder.seal_block(test);
        let below = self.builder.ins().iadd_imm(n, -1);
        let shifted = self.builder.ins().ushr(native, below);
        let bit = self.builder.ins().band_imm(shifted, 1);
        let on = self.builder.ins().icmp_imm(IntCC::NotEqual, bit, 0);
        self.builder.ins().brif(on, run, &[], next, &[]);

        self.builder.switch_to_block(run);
        self.builder.seal_block(run);
        body(self, n);
        self.builder.ins().jump(next, &[]);

        self.builder.switch_to_block(next);
        self.builder.seal_block(next);
        let after = self.builder.ins().iadd_imm(n, 1);
        self.builder.ins().jump(head, &[after.into()]);
        self.builder.seal_block(head);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
    }

    /// `sigaction(n, {handler, 0, 0}, NULL)` with a handler of `0` (`SIG_DFL`) or
    /// `1` (`SIG_IGN`). `sigaction` and not `signal`, because a program
    /// moving off `Ffi("libc")` may still declare its own `extern fn signal`
    /// with other types, and the module refuses two signatures for a symbol.
    /// Darwin's `struct sigaction` is a handler, a 32-bit mask and 32-bit
    /// flags: 16 bytes.
    fn set_disposition(&mut self, number: Value, handler: i64) {
        let pointer = self.pointer;
        let number = self.builder.ins().ireduce(types::I32, number);
        let action = self.scratch_bytes(16);
        let handler = self.builder.ins().iconst(types::I64, handler);
        self.builder.ins().store(MemFlags::trusted(), handler, action, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().store(MemFlags::trusted(), zero, action, 8);
        let none = self.builder.ins().iconst(pointer, 0);
        self.libc_call(
            "sigaction",
            &[types::I32, pointer, pointer],
            &[types::I32],
            &[number, action, none],
        );
    }

    /// `signals_watch`: `args` is the borrowed capability (a pointer, which
    /// is not read) and the lex-sys bits. Answers `Watching`'s three
    /// leaves: the tag (`Ok` 0, `Failed` 1), the handle, the `errno`.
    pub(crate) fn signals_watch(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let darwin = self.is_darwin();
        let native = self.native_from_bits(args[1]);
        let state = self.global(SIGNAL_STATE_GLOBAL);
        let claimed = self.builder.ins().load(types::I64, MemFlags::trusted(), state, 0);
        let threads = self.builder.ins().load(types::I64, MemFlags::trusted(), state, THREADS_AT);
        let overlap = self.builder.ins().band(claimed, native);
        let taken = self.builder.ins().icmp_imm(IntCC::NotEqual, overlap, 0);
        let running = self.builder.ins().icmp_imm(IntCC::NotEqual, threads, 0);
        let busy = self.builder.ins().bor(taken, running);

        let merge = self.builder.create_block();
        for _ in 0..3 {
            self.builder.append_block_param(merge, types::I64);
        }
        let refuse = self.builder.create_block();
        let grant = self.builder.create_block();
        self.builder.ins().brif(busy, refuse, &[], grant, &[]);

        // A thread is running, or another claim holds a signal: nothing changed.
        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let ebusy = self.builder.ins().iconst(types::I64, EBUSY);
        self.builder.ins().jump(merge, &[one.into(), zero.into(), ebusy.into()]);

        self.builder.switch_to_block(grant);
        self.builder.seal_block(grant);
        let ok = self.builder.create_block();
        let bad = self.builder.create_block();
        let (descriptor, reason, set) = if darwin {
            self.each_native_signal(native, |this, n| this.set_disposition(n, 1));
            let kq = self.libc_call("kqueue", &[], &[types::I32], &[]);
            let reason = self.errno();
            self.close_on_exec(kq);
            let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, kq, 0);
            self.builder.ins().brif(failed, bad, &[], ok, &[]);
            (kq, reason, None)
        } else {
            let set = self.sigset_of(native);
            self.sigmask(SIG_BLOCK, set);
            let flags = self.builder.ins().iconst(types::I32, SFD_FLAGS);
            let any = self.builder.ins().iconst(types::I32, -1);
            let fd = self.libc_call(
                "signalfd",
                &[types::I32, pointer, types::I32],
                &[types::I32],
                &[any, set, flags],
            );
            let reason = self.errno();
            let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
            self.builder.ins().brif(failed, bad, &[], ok, &[]);
            (fd, reason, Some(set))
        };

        // The kernel refused a descriptor: put every signal back, and say why.
        self.builder.switch_to_block(bad);
        self.builder.seal_block(bad);
        match set {
            Some(set) => self.sigmask(SIG_UNBLOCK, set),
            None => self.each_native_signal(native, |this, n| this.set_disposition(n, 0)),
        }
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[one.into(), zero.into(), reason.into()]);

        self.builder.switch_to_block(ok);
        self.builder.seal_block(ok);
        if darwin {
            self.each_native_signal(native, |this, n| {
                let event = this.scratch_bytes(KEVENT_SIZE);
                this.builder.ins().store(MemFlags::trusted(), n, event, 0);
                let filter = this.builder.ins().iconst(types::I16, EVFILT_SIGNAL);
                this.builder.ins().store(MemFlags::trusted(), filter, event, 8);
                let flags = this.builder.ins().iconst(types::I16, EV_ADD | EV_CLEAR);
                this.builder.ins().store(MemFlags::trusted(), flags, event, 10);
                let nothing = this.builder.ins().iconst(types::I32, 0);
                this.builder.ins().store(MemFlags::trusted(), nothing, event, 12);
                let none64 = this.builder.ins().iconst(types::I64, 0);
                this.builder.ins().store(MemFlags::trusted(), none64, event, 16);
                this.builder.ins().store(MemFlags::trusted(), none64, event, 24);
                let one32 = this.builder.ins().iconst(types::I32, 1);
                let null = this.builder.ins().iconst(pointer, 0);
                this.libc_call(
                    "kevent",
                    &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
                    &[types::I32],
                    &[descriptor, event, one32, null, nothing, null],
                );
            });
        }
        let now = self.builder.ins().bor(claimed, native);
        self.builder.ins().store(MemFlags::trusted(), now, state, 0);
        let high = self.builder.ins().ishl_imm(native, 32);
        let low = self.builder.ins().uextend(types::I64, descriptor);
        let handle = self.builder.ins().bor(high, low);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into(), handle.into(), zero.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge).to_vec()
    }

    /// `signals_pending(&!SignalWatch)`: take what the kernel has queued and
    /// answer it as lex-sys bits. Never waits: the descriptor is non-blocking
    /// on Linux, and the `kevent` has a zero timeout on Darwin.
    pub(crate) fn signals_pending(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.handle_fd(args[0]);
        let native = self.drain_native(fd);
        vec![self.bits_from_native(native)]
    }

    /// Take what the kernel has queued on a claim's descriptor and answer it as
    /// a kernel mask. Never waits. `signals_pending` and Darwin's
    /// `signals_close` (which must not lose what was not read) share it.
    fn drain_native(&mut self, fd: Value) -> Value {
        let pointer = self.pointer;
        let darwin = self.is_darwin();
        let stride = if darwin { KEVENT_SIZE } else { SIGNALFD_RECORD };
        let buf = self.scratch_bytes(stride * RECORDS);
        let count = if darwin {
            let timeout = self.scratch_bytes(16);
            let zero = self.builder.ins().iconst(types::I64, 0);
            self.builder.ins().store(MemFlags::trusted(), zero, timeout, 0);
            self.builder.ins().store(MemFlags::trusted(), zero, timeout, 8);
            let null = self.builder.ins().iconst(pointer, 0);
            let none = self.builder.ins().iconst(types::I32, 0);
            let room = self.builder.ins().iconst(types::I32, RECORDS);
            let got = self.libc_call(
                "kevent",
                &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
                &[types::I32],
                &[fd, null, none, buf, room, timeout],
            );
            self.builder.ins().sextend(types::I64, got)
        } else {
            let room = self.builder.ins().iconst(pointer, stride * RECORDS);
            let got = self.libc_call(
                "read",
                &[types::I32, pointer, pointer],
                &[types::I64],
                &[fd, buf, room],
            );
            // `-1` (nothing queued: `EAGAIN`) divides to zero records.
            self.builder.ins().sdiv_imm(got, SIGNALFD_RECORD)
        };

        let head = self.builder.create_block();
        self.builder.append_block_param(head, types::I64);
        self.builder.append_block_param(head, types::I64);
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(head, &[zero.into(), zero.into()]);

        self.builder.switch_to_block(head);
        let i = self.builder.block_params(head)[0];
        let seen = self.builder.block_params(head)[1];
        let more = self.builder.ins().icmp(IntCC::SignedLessThan, i, count);
        self.builder.ins().brif(more, body, &[], done, &[seen.into()]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        let offset = self.builder.ins().imul_imm(i, stride);
        let record = self.builder.ins().iadd(buf, offset);
        // `ssi_signo` is the first `u32` of a `signalfd_siginfo`; `ident` the first `u64` of a `kevent`.
        let number = if darwin {
            self.builder.ins().load(types::I64, MemFlags::trusted(), record, 0)
        } else {
            let narrow = self.builder.ins().load(types::I32, MemFlags::trusted(), record, 0);
            self.builder.ins().uextend(types::I64, narrow)
        };
        // Only 1..=31 can be a bit of the mask; anything else adds nothing.
        let below = self.builder.ins().iadd_imm(number, -1);
        let in_range = self.builder.ins().icmp_imm(IntCC::UnsignedLessThan, below, 31);
        let shift = self.builder.ins().select(in_range, below, zero);
        let one = self.builder.ins().iconst(types::I64, 1);
        let bit = self.builder.ins().ishl(one, shift);
        let added = self.builder.ins().select(in_range, bit, zero);
        let seen_now = self.builder.ins().bor(seen, added);
        let next = self.builder.ins().iadd_imm(i, 1);
        self.builder.ins().jump(head, &[next.into(), seen_now.into()]);
        self.builder.seal_block(head);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        self.builder.block_params(done)[0]
    }

    /// `signals_close(SignalWatch)`: end the claim. A signal still queued is
    /// delivered now, with the disposition in force -- the default, so a
    /// `TERM` ends the process -- which is the second signal arriving before
    /// the first was acted on (`docs/signals.md` section 3).
    pub(crate) fn signals_close(&mut self, args: &[Value]) -> Vec<Value> {
        let handle = args[0];
        let fd = self.builder.ins().ireduce(types::I32, handle);
        let native = self.builder.ins().ushr_imm(handle, 32);
        let mut unread = None;
        if self.is_darwin() {
            // The claim ignored these signals, so one that arrived and was not
            // read is in the `kqueue` and nowhere else: the kernel discarded
            // it as ignored. Take it out first, restore the default, then
            // raise it, so it is delivered with the default action as the
            // queued signal is on Linux (`docs/signals.md` section 3).
            let queued = self.drain_native(fd);
            self.each_native_signal(native, |this, n| this.set_disposition(n, 0));
            unread = Some(queued);
        } else {
            let set = self.sigset_of(native);
            self.sigmask(SIG_UNBLOCK, set);
        }
        let answer = self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        if let Some(queued) = unread {
            self.each_native_signal(queued, |this, n| {
                let number = this.builder.ins().ireduce(types::I32, n);
                this.libc_call("raise", &[types::I32], &[types::I32], &[number]);
            });
        }
        let state = self.global(SIGNAL_STATE_GLOBAL);
        let claimed = self.builder.ins().load(types::I64, MemFlags::trusted(), state, 0);
        let rest = self.builder.ins().band_not(claimed, native);
        self.builder.ins().store(MemFlags::trusted(), rest, state, 0);
        vec![self.builder.ins().sextend(types::I64, answer)]
    }
}
