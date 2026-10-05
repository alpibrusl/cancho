//! The `Poller` (`docs/native-sockets.md` §4): `epoll` on Linux, `kqueue` on
//! Darwin, behind one surface of `(token, events)` pairs. Mirrors
//! `lex-sys-codegen-llvm`'s own `body/poller.rs`, which has the longer
//! account; both are level-triggered.

use crate::*;
use lex_sys_ir::CHILD_PIDFD_SHIFT;

const READABLE: i64 = 1;
const WRITABLE: i64 = 2;
const BATCH: i64 = 64;

const EPOLL_CTL_ADD: i64 = 1;
const EPOLL_CTL_DEL: i64 = 2;
const EPOLL_CTL_MOD: i64 = 3;
const EPOLLIN: i64 = 0x001;
const EPOLLOUT: i64 = 0x004;
const EPOLLERR: i64 = 0x008;
const EPOLLHUP: i64 = 0x010;
const EPOLL_CLOEXEC: i64 = 0x80000;

const EVFILT_READ: i64 = -1;
const EVFILT_WRITE: i64 = -2;
const EVFILT_PROC: i64 = -5;
const EVFILT_USER: i64 = -10;
const EV_ONESHOT: i64 = 0x0010;
const NOTE_TRIGGER: i64 = 0x0100_0000;
const ESRCH: i64 = 3;
/// `0x8000_0000`, written as the signed 32-bit value `iconst.i32` takes.
const NOTE_EXIT: i64 = -0x8000_0000;
const EV_ADD: i64 = 0x0001;
const EV_DELETE: i64 = 0x0002;
const EV_ERROR: i64 = 0x4000;
const KEVENT_SIZE: i64 = 32;

/// A store or load through memory that may not be aligned (`epoll_event`
/// is packed on x86-64).
fn unaligned() -> MemFlags {
    MemFlags::new()
}

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `epoll_event`'s size and where its 64-bit `data` sits.
    fn epoll_layout(&self) -> (i64, i64) {
        match self.module.isa().triple().architecture {
            target_lexicon::Architecture::X86_64 => (12, 4),
            _ => (16, 8),
        }
    }

    fn scratch(&mut self, size: i64) -> Value {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size as u32,
            3,
        ));
        self.builder.ins().stack_addr(pointer, slot, 0)
    }

    /// `poller_new()`: `Polling` is `Ok` 0, `Failed` 1.
    pub(crate) fn poller_new(&mut self) -> Vec<Value> {
        let fd = if self.is_darwin() {
            self.libc_call("kqueue", &[], &[types::I32], &[])
        } else {
            let flags = self.builder.ins().iconst(types::I32, EPOLL_CLOEXEC);
            self.libc_call("epoll_create1", &[types::I32], &[types::I32], &[flags])
        };
        let reason = self.errno();
        // `kqueue` has no flag for it; `epoll_create1` was asked
        // (`docs/processes.md` §4.5).
        if self.is_darwin() {
            self.close_on_exec(fd);
        }
        let fd64 = self.builder.ins().sextend(types::I64, fd);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, fd64, reason]
    }

    /// One `kevent` change: `filter` added when `add` (an `i8` truth), else
    /// deleted. Answers `kevent`'s result.
    fn kevent_change(
        &mut self,
        kq: Value,
        fd: Value,
        filter: i64,
        add: Value,
        token: Value,
    ) -> Value {
        let pointer = self.pointer;
        let kev = self.scratch(KEVENT_SIZE);
        let fd64 = self.builder.ins().sextend(types::I64, fd);
        self.builder.ins().store(unaligned(), fd64, kev, 0);
        let filter = self.builder.ins().iconst(types::I16, filter);
        self.builder.ins().store(unaligned(), filter, kev, 8);
        let on = self.builder.ins().iconst(types::I16, EV_ADD);
        let off = self.builder.ins().iconst(types::I16, EV_DELETE);
        let flags = self.builder.ins().select(add, on, off);
        self.builder.ins().store(unaligned(), flags, kev, 10);
        let zero32 = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().store(unaligned(), zero32, kev, 12);
        let zero64 = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().store(unaligned(), zero64, kev, 16);
        self.builder.ins().store(unaligned(), token, kev, 24);
        let one = self.builder.ins().iconst(types::I32, 1);
        let none = self.builder.ins().iconst(pointer, 0);
        self.libc_call(
            "kevent",
            &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
            &[types::I32],
            &[kq, kev, one, none, zero32, none],
        )
    }

    /// `poller_add_listener`, `poller_add_conn` and `poller_modify`.
    /// `0`, or the `errno`.
    pub(crate) fn poller_ctl(
        &mut self,
        args: &[Value],
        listener: bool,
        modify: bool,
    ) -> Vec<Value> {
        let pointer = self.pointer;
        let poller = self.handle_fd(args[0]);
        let fd = self.handle_fd(args[1]);
        let token = args[2];
        let events =
            if listener { self.builder.ins().iconst(types::I64, READABLE) } else { args[3] };

        let read_bit = self.builder.ins().band_imm(events, READABLE);
        let wants_read = self.builder.ins().icmp_imm(IntCC::NotEqual, read_bit, 0);
        let write_bit = self.builder.ins().band_imm(events, WRITABLE);
        let wants_write = self.builder.ins().icmp_imm(IntCC::NotEqual, write_bit, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);

        if self.is_darwin() {
            let read = self.kevent_change(poller, fd, EVFILT_READ, wants_read, token);
            let read_reason = self.errno();
            let write = self.kevent_change(poller, fd, EVFILT_WRITE, wants_write, token);
            let write_reason = self.errno();
            let read_failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, read, 0);
            let read_bad = self.builder.ins().band(read_failed, wants_read);
            let write_failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, write, 0);
            let write_bad = self.builder.ins().band(write_failed, wants_write);
            let after_write = self.builder.ins().select(write_bad, write_reason, zero);
            return vec![self.builder.ins().select(read_bad, read_reason, after_write)];
        }

        let (size, data_at) = self.epoll_layout();
        let ev = self.scratch(size);
        let in_bit = self.builder.ins().iconst(types::I32, EPOLLIN);
        let out_bit = self.builder.ins().iconst(types::I32, EPOLLOUT);
        let none = self.builder.ins().iconst(types::I32, 0);
        let r = self.builder.ins().select(wants_read, in_bit, none);
        let w = self.builder.ins().select(wants_write, out_bit, none);
        let mask = self.builder.ins().bor(r, w);
        self.builder.ins().store(unaligned(), mask, ev, 0);
        self.builder.ins().store(unaligned(), token, ev, data_at as i32);
        let op = self
            .builder
            .ins()
            .iconst(types::I32, if modify { EPOLL_CTL_MOD } else { EPOLL_CTL_ADD });
        let result = self.libc_call(
            "epoll_ctl",
            &[types::I32, types::I32, types::I32, pointer],
            &[types::I32],
            &[poller, op, fd, ev],
        );
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        vec![self.builder.ins().select(failed, reason, zero)]
    }

    /// `poller_add_child(&!Poller, &Child, token)` (`docs/processes.md` §4.8):
    /// the child's exit, as readable. Linux watches the `pidfd` the `Child`
    /// carries; Darwin asks `kqueue` for `NOTE_EXIT` on the pid, which it
    /// reports once. `0`, or the `errno`.
    pub(crate) fn poller_add_child(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let poller = self.handle_fd(args[0]);
        let word = self.builder.ins().load(types::I64, MemFlags::trusted(), args[1], 0);
        let token = args[2];
        let zero = self.builder.ins().iconst(types::I64, 0);

        if self.is_darwin() {
            let pid = self.builder.ins().ireduce(types::I32, word);
            let pid64 = self.builder.ins().sextend(types::I64, pid);
            let kev = self.scratch(KEVENT_SIZE);
            self.builder.ins().store(unaligned(), pid64, kev, 0);
            let filter = self.builder.ins().iconst(types::I16, EVFILT_PROC);
            self.builder.ins().store(unaligned(), filter, kev, 8);
            let flags = self.builder.ins().iconst(types::I16, EV_ADD | EV_ONESHOT);
            self.builder.ins().store(unaligned(), flags, kev, 10);
            let exit = self.builder.ins().iconst(types::I32, NOTE_EXIT);
            self.builder.ins().store(unaligned(), exit, kev, 12);
            self.builder.ins().store(unaligned(), zero, kev, 16);
            self.builder.ins().store(unaligned(), token, kev, 24);
            let one = self.builder.ins().iconst(types::I32, 1);
            let zero32 = self.builder.ins().iconst(types::I32, 0);
            let none = self.builder.ins().iconst(pointer, 0);
            let result = self.libc_call(
                "kevent",
                &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
                &[types::I32],
                &[poller, kev, one, none, zero32, none],
            );
            let reason = self.errno();
            let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);

            // `ESRCH`: the process is gone, and a child nobody has reaped is gone
            // only by having exited (macOS does not take `NOTE_EXIT` for a
            // zombie). Say so the way `kqueue` says anything is ready for the
            // asking: a user event, added and triggered at once, reported with
            // the same token. With no `ESRCH` the two changes are none.
            let gone = self.builder.ins().icmp_imm(IntCC::Equal, reason, ESRCH);
            let exited = self.builder.ins().band(failed, gone);
            let pair = self.scratch(2 * KEVENT_SIZE);
            let user = self.builder.ins().iconst(types::I16, EVFILT_USER);
            let add = self.builder.ins().iconst(types::I16, EV_ADD | EV_ONESHOT);
            let off = self.builder.ins().iconst(types::I16, 0);
            let trigger = self.builder.ins().iconst(types::I32, NOTE_TRIGGER);
            self.builder.ins().store(unaligned(), pid64, pair, 0);
            self.builder.ins().store(unaligned(), user, pair, 8);
            self.builder.ins().store(unaligned(), add, pair, 10);
            self.builder.ins().store(unaligned(), zero32, pair, 12);
            self.builder.ins().store(unaligned(), zero, pair, 16);
            self.builder.ins().store(unaligned(), token, pair, 24);
            self.builder.ins().store(unaligned(), pid64, pair, KEVENT_SIZE as i32);
            self.builder.ins().store(unaligned(), user, pair, (KEVENT_SIZE + 8) as i32);
            self.builder.ins().store(unaligned(), off, pair, (KEVENT_SIZE + 10) as i32);
            self.builder.ins().store(unaligned(), trigger, pair, (KEVENT_SIZE + 12) as i32);
            self.builder.ins().store(unaligned(), zero, pair, (KEVENT_SIZE + 16) as i32);
            self.builder.ins().store(unaligned(), zero, pair, (KEVENT_SIZE + 24) as i32);
            let two = self.builder.ins().iconst(types::I32, 2);
            let changes = self.builder.ins().select(exited, two, zero32);
            let again = self.libc_call(
                "kevent",
                &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
                &[types::I32],
                &[poller, pair, changes, none, zero32, none],
            );
            let again_reason = self.errno();
            let again_failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, again, 0);
            let after_user = self.builder.ins().select(again_failed, again_reason, zero);
            let first_failed = self.builder.ins().select(exited, after_user, reason);
            return vec![self.builder.ins().select(failed, first_failed, zero)];
        }

        let high = self.builder.ins().ushr_imm(word, CHILD_PIDFD_SHIFT);
        let pidfd = self.builder.ins().ireduce(types::I32, high);
        let (size, data_at) = self.epoll_layout();
        let ev = self.scratch(size);
        let mask = self.builder.ins().iconst(types::I32, EPOLLIN);
        self.builder.ins().store(unaligned(), mask, ev, 0);
        self.builder.ins().store(unaligned(), token, ev, data_at as i32);
        let op = self.builder.ins().iconst(types::I32, EPOLL_CTL_ADD);
        let result = self.libc_call(
            "epoll_ctl",
            &[types::I32, types::I32, types::I32, pointer],
            &[types::I32],
            &[poller, op, pidfd, ev],
        );
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let answer = self.builder.ins().select(failed, reason, zero);
        // No `pidfd`: the `Child` carries why, the `errno` negated
        // (`ENOSYS` before Linux 5.3, `EMFILE` with no descriptor to spare).
        let given = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, pidfd, 0);
        let pidfd64 = self.builder.ins().sextend(types::I64, pidfd);
        let why = self.builder.ins().ineg(pidfd64);
        vec![self.builder.ins().select(given, answer, why)]
    }

    /// `poller_remove(&!Poller, &Conn)`.
    pub(crate) fn poller_remove(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let poller = self.handle_fd(args[0]);
        let fd = self.handle_fd(args[1]);
        let zero = self.builder.ins().iconst(types::I64, 0);
        if self.is_darwin() {
            let no = self.builder.ins().iconst(types::I8, 0);
            self.kevent_change(poller, fd, EVFILT_READ, no, zero);
            self.kevent_change(poller, fd, EVFILT_WRITE, no, zero);
            return vec![zero];
        }
        let (size, _) = self.epoll_layout();
        let ev = self.scratch(size);
        let op = self.builder.ins().iconst(types::I32, EPOLL_CTL_DEL);
        let result = self.libc_call(
            "epoll_ctl",
            &[types::I32, types::I32, types::I32, pointer],
            &[types::I32],
            &[poller, op, fd, ev],
        );
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        vec![self.builder.ins().select(failed, reason, zero)]
    }

    /// `poller_wait(&!Poller, &![int], timeout_ms)`: `args` is the poller,
    /// the slice's pointer and length, and the timeout.
    pub(crate) fn poller_wait(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let darwin = self.is_darwin();
        let poller = self.handle_fd(args[0]);
        let (out, len, timeout) = (args[1], args[2], args[3]);
        let stride = if darwin { KEVENT_SIZE } else { self.epoll_layout().0 };

        let pairs = self.builder.ins().udiv_imm(len, 2);
        let small = self.builder.ins().icmp_imm(IntCC::UnsignedLessThan, pairs, BATCH);
        let batch = self.builder.ins().iconst(types::I64, BATCH);
        let max = self.builder.ins().select(small, pairs, batch);
        let max32 = self.builder.ins().ireduce(types::I32, max);
        let buf = self.scratch(stride * BATCH);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let call = self.builder.create_block();
        let failed = self.builder.create_block();
        let head = self.builder.create_block();
        let check = self.builder.create_block();
        self.builder.append_block_param(check, types::I64);
        let body = self.builder.create_block();
        let done = self.builder.create_block();

        let no_room = self.builder.ins().icmp_imm(IntCC::Equal, max, 0);
        self.builder.ins().brif(no_room, refuse, &[], call, &[]);

        // A slice too short for one pair is the caller's mistake: EINVAL.
        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let einval = self.builder.ins().iconst(types::I64, -22);
        self.builder.ins().jump(merge, &[einval.into()]);

        self.builder.switch_to_block(call);
        self.builder.seal_block(call);
        let got = if darwin {
            let ts = self.scratch(16);
            let seconds = self.builder.ins().sdiv_imm(timeout, 1000);
            let millis = self.builder.ins().srem_imm(timeout, 1000);
            let nanos = self.builder.ins().imul_imm(millis, 1_000_000);
            self.builder.ins().store(unaligned(), seconds, ts, 0);
            self.builder.ins().store(unaligned(), nanos, ts, 8);
            let forever = self.builder.ins().icmp_imm(IntCC::SignedLessThan, timeout, 0);
            let null = self.builder.ins().iconst(pointer, 0);
            let wait = self.builder.ins().select(forever, null, ts);
            let zero32 = self.builder.ins().iconst(types::I32, 0);
            self.libc_call(
                "kevent",
                &[types::I32, pointer, types::I32, pointer, types::I32, pointer],
                &[types::I32],
                &[poller, null, zero32, buf, max32, wait],
            )
        } else {
            let timeout32 = self.builder.ins().ireduce(types::I32, timeout);
            self.libc_call(
                "epoll_wait",
                &[types::I32, pointer, types::I32, types::I32],
                &[types::I32],
                &[poller, buf, max32, timeout32],
            )
        };
        let reason = self.errno();
        let bad = self.builder.ins().icmp_imm(IntCC::SignedLessThan, got, 0);
        self.builder.ins().brif(bad, failed, &[], head, &[]);

        self.builder.switch_to_block(failed);
        self.builder.seal_block(failed);
        let negated = self.builder.ins().ineg(reason);
        self.builder.ins().jump(merge, &[negated.into()]);

        self.builder.switch_to_block(head);
        self.builder.seal_block(head);
        let got64 = self.builder.ins().sextend(types::I64, got);
        let start = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(check, &[start.into()]);

        self.builder.switch_to_block(check);
        let i = self.builder.block_params(check)[0];
        let more = self.builder.ins().icmp(IntCC::SignedLessThan, i, got64);
        self.builder.ins().brif(more, body, &[], done, &[]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        let entry_offset = self.builder.ins().imul_imm(i, stride);
        let entry = self.builder.ins().iadd(buf, entry_offset);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let read_event = self.builder.ins().iconst(types::I64, READABLE);
        let write_event = self.builder.ins().iconst(types::I64, WRITABLE);
        let (token, events) = if darwin {
            let filter = self.builder.ins().load(types::I16, unaligned(), entry, 8);
            let flags = self.builder.ins().load(types::I16, unaligned(), entry, 10);
            let token = self.builder.ins().load(types::I64, unaligned(), entry, 24);
            let reading = self.builder.ins().icmp_imm(IntCC::Equal, filter, EVFILT_READ);
            // A child's exit is read as a signal's arrival is: readable.
            let exited = self.builder.ins().icmp_imm(IntCC::Equal, filter, EVFILT_PROC);
            let told = self.builder.ins().icmp_imm(IntCC::Equal, filter, EVFILT_USER);
            let exited = self.builder.ins().bor(exited, told);
            let is_read = self.builder.ins().bor(reading, exited);
            let is_write = self.builder.ins().icmp_imm(IntCC::Equal, filter, EVFILT_WRITE);
            let error_bits = self.builder.ins().band_imm(flags, EV_ERROR);
            let errored = self.builder.ins().icmp_imm(IntCC::NotEqual, error_bits, 0);
            // An error is reported as readable: a read is what shows it.
            let reads = self.builder.ins().bor(is_read, errored);
            let r = self.builder.ins().select(reads, read_event, zero);
            let w = self.builder.ins().select(is_write, write_event, zero);
            (token, self.builder.ins().bor(r, w))
        } else {
            let (_, data_at) = self.epoll_layout();
            let mask = self.builder.ins().load(types::I32, unaligned(), entry, 0);
            let token = self.builder.ins().load(types::I64, unaligned(), entry, data_at as i32);
            let reading = self.builder.ins().band_imm(mask, EPOLLIN | EPOLLERR | EPOLLHUP);
            let is_read = self.builder.ins().icmp_imm(IntCC::NotEqual, reading, 0);
            let writing = self.builder.ins().band_imm(mask, EPOLLOUT);
            let is_write = self.builder.ins().icmp_imm(IntCC::NotEqual, writing, 0);
            let r = self.builder.ins().select(is_read, read_event, zero);
            let w = self.builder.ins().select(is_write, write_event, zero);
            (token, self.builder.ins().bor(r, w))
        };
        let slot = self.builder.ins().imul_imm(i, 16);
        let at = self.builder.ins().iadd(out, slot);
        self.builder.ins().store(MemFlags::trusted(), token, at, 0);
        self.builder.ins().store(MemFlags::trusted(), events, at, 8);
        let next = self.builder.ins().iadd_imm(i, 1);
        self.builder.ins().jump(check, &[next.into()]);
        self.builder.seal_block(check);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        self.builder.ins().jump(merge, &[got64.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        vec![self.builder.block_params(merge)[0]]
    }
}
