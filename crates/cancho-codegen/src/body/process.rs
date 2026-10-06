//! Processes (`docs/processes.md` §3 to §6): `pipe_open`, `exec_spawn`,
//! `child_wait` and `child_kill`. The pipe verbs are the socket handles'
//! (a channel is a socket pair, §4.4), so they have no code of their own here.
//! Mirrors `cancho-codegen-llvm`'s own `body/process.rs`.

use crate::*;
use cancho_ir::{
    AF_UNIX, CHILD_PIDFD_SHIFT, Expr, F_SETFD, FD_CLOEXEC, O_RDWR, SIGSET_BYTES, SOCK_STREAM,
    SPAWN_OBJECT_BYTES, SYS_PIDFD_OPEN, sendable_signals, spawn_flags,
};

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `pipe_open()`: a socket pair, close-on-exec, and on Darwin each end
    /// with `SO_NOSIGPIPE` (Linux suppresses `SIGPIPE` per send). `Piped`'s
    /// four leaves: the tag (`Ok` 0, `Failed` 1), the parent's end, the
    /// child's end, the reason.
    pub(crate) fn pipe_open(&mut self) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let pair = self.scratch_bytes(8);
        let domain = self.builder.ins().iconst(types::I32, AF_UNIX);
        let kind = self.builder.ins().iconst(types::I32, SOCK_STREAM | os.sock_cloexec);
        let proto = self.builder.ins().iconst(types::I32, 0);
        let result = self.libc_call(
            "socketpair",
            &[types::I32, types::I32, types::I32, pointer],
            &[types::I32],
            &[domain, kind, proto, pair],
        );
        let reason = self.errno();
        let parent = self.builder.ins().load(types::I32, MemFlags::trusted(), pair, 0);
        let child = self.builder.ins().load(types::I32, MemFlags::trusted(), pair, 4);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        if self.is_darwin() {
            let fix = self.builder.create_block();
            let done = self.builder.create_block();
            self.builder.ins().brif(failed, done, &[], fix, &[]);
            self.builder.switch_to_block(fix);
            self.builder.seal_block(fix);
            for end in [parent, child] {
                let flag = self.builder.ins().iconst(types::I32, FD_CLOEXEC);
                self.fcntl(end, F_SETFD, flag);
                self.suppress_sigpipe(end);
            }
            self.builder.ins().jump(done, &[]);
            self.builder.switch_to_block(done);
            self.builder.seal_block(done);
        }
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        let parent = self.builder.ins().sextend(types::I64, parent);
        let child = self.builder.ins().sextend(types::I64, child);
        vec![tag, parent, child, reason]
    }

    /// `exec_spawn(exec, path, args, env, stdin, stdout, stderr)` (§4.1 to
    /// §4.6), and with `in_dir` `exec_spawn_in(exec, dir, path, ...)` (§4.10).
    /// `Spawned`'s three leaves: the tag (`Ok` 0, `Failed` 1), the pid, the
    /// reason.
    pub(crate) fn exec_spawn(&mut self, prefix: &str, in_dir: bool, args: &[Expr]) -> Vec<Value> {
        let pointer = self.pointer;
        // The capability is zero-sized and stops here.
        let at = 1 + usize::from(in_dir);
        let dir = in_dir.then(|| self.expr(&args[1]));
        let path = self.expr(&args[at]);
        let arguments = self.expr(&args[at + 1]);
        let environment = self.expr(&args[at + 2]);
        let streams: Vec<Vec<Value>> = args[at + 3..at + 6].iter().map(|s| self.expr(s)).collect();

        // §4.1: under the prefix, no `..`; a broken promise traps.
        let program = self.checked_path(prefix, &path);
        // §4.2, §4.3: `\0`-separated lists, as `argv` and `envp`.
        let argv = self.pointer_list(&arguments, Some(program), false);
        let envp = self.pointer_list(&environment, None, true);

        // §4.4: each stream is exactly what was given.
        let actions = self.scratch_bytes(SPAWN_OBJECT_BYTES);
        self.libc_call("posix_spawn_file_actions_init", &[pointer], &[types::I32], &[actions]);
        let null_device = self.c_string("/dev/null");
        for (target, stream) in streams.iter().enumerate() {
            let (tag, end, file) = (stream[0], stream[1], stream[2]);
            let open = self.builder.create_block();
            let dup = self.builder.create_block();
            let next = self.builder.create_block();
            let is_null = self.builder.ins().icmp_imm(IntCC::Equal, tag, 0);
            self.builder.ins().brif(is_null, open, &[], dup, &[]);

            self.builder.switch_to_block(open);
            self.builder.seal_block(open);
            let fd = self.builder.ins().iconst(types::I32, target as i64);
            let flags = self.builder.ins().iconst(types::I32, O_RDWR);
            let mode = self.builder.ins().iconst(types::I32, 0);
            self.libc_call(
                "posix_spawn_file_actions_addopen",
                &[pointer, types::I32, pointer, types::I32, types::I32],
                &[types::I32],
                &[actions, fd, null_device, flags, mode],
            );
            self.builder.ins().jump(next, &[]);

            self.builder.switch_to_block(dup);
            self.builder.seal_block(dup);
            let is_pipe = self.builder.ins().icmp_imm(IntCC::Equal, tag, 1);
            let source = self.builder.ins().select(is_pipe, end, file);
            let source = self.builder.ins().ireduce(types::I32, source);
            let fd = self.builder.ins().iconst(types::I32, target as i64);
            self.libc_call(
                "posix_spawn_file_actions_adddup2",
                &[pointer, types::I32, types::I32],
                &[types::I32],
                &[actions, source, fd],
            );
            self.builder.ins().jump(next, &[]);

            self.builder.switch_to_block(next);
            self.builder.seal_block(next);
        }

        // §4.10: the child's working directory, by descriptor, ahead of the
        // `closefrom` below: that closes the `Dir`'s descriptor, and a
        // `chdir` after it is `EBADF` (measured). The `Dir` stays the
        // parent's. A refusal here is the allocator's alone -- the descriptor
        // is a live `Dir`'s -- and traps as `malloc`'s does.
        if let Some(dir) = &dir {
            let fd = self.dir_fd(dir[0]);
            let refused = self.libc_call(
                "posix_spawn_file_actions_addfchdir_np",
                &[pointer, types::I32],
                &[types::I32],
                &[actions, fd],
            );
            self.builder.ins().trapnz(refused, TrapCode::HEAP_OUT_OF_BOUNDS);
        }

        // §4.5: and nothing else. Close-on-exec covers what this program
        // opened; this covers what it inherited without the flag, so the child
        // holds exactly the three streams. Darwin does the same with
        // `POSIX_SPAWN_CLOEXEC_DEFAULT` (`spawn_flags`); glibc has
        // `addclosefrom_np` from 2.34.
        if !self.is_darwin() {
            let lowest = self.builder.ins().iconst(types::I32, 3);
            self.libc_call(
                "posix_spawn_file_actions_addclosefrom_np",
                &[pointer, types::I32],
                &[types::I32],
                &[actions, lowest],
            );
        }

        // §4.6: an empty mask, and every signal at its default.
        let attributes = self.scratch_bytes(SPAWN_OBJECT_BYTES);
        self.libc_call("posix_spawnattr_init", &[pointer], &[types::I32], &[attributes]);
        let empty = self.scratch_bytes(SIGSET_BYTES);
        self.libc_call("sigemptyset", &[pointer], &[types::I32], &[empty]);
        self.libc_call(
            "posix_spawnattr_setsigmask",
            &[pointer, pointer],
            &[types::I32],
            &[attributes, empty],
        );
        let every = self.scratch_bytes(SIGSET_BYTES);
        self.libc_call("sigfillset", &[pointer], &[types::I32], &[every]);
        self.libc_call(
            "posix_spawnattr_setsigdefault",
            &[pointer, pointer],
            &[types::I32],
            &[attributes, every],
        );
        let wanted = spawn_flags(self.is_darwin());
        let flags = self.builder.ins().iconst(types::I16, wanted);
        self.libc_call(
            "posix_spawnattr_setflags",
            &[pointer, types::I16],
            &[types::I32],
            &[attributes, flags],
        );

        // `posix_spawn` answers the error number itself; `errno` is not used.
        let pid = self.scratch_bytes(8);
        let zero32 = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().store(MemFlags::trusted(), zero32, pid, 0);
        let error = self.libc_call(
            "posix_spawn",
            &[pointer, pointer, pointer, pointer, pointer, pointer],
            &[types::I32],
            &[pid, program, actions, attributes, argv, envp],
        );
        self.libc_call("posix_spawn_file_actions_destroy", &[pointer], &[types::I32], &[actions]);
        self.libc_call("posix_spawnattr_destroy", &[pointer], &[types::I32], &[attributes]);
        self.free(argv);
        self.free(envp);

        // What was handed to the child is the parent's no longer, whether or
        // not the child started (§4.1: a failed spawn leaks nothing).
        for stream in &streams {
            let (tag, end, file) = (stream[0], stream[1], stream[2]);
            let close = self.builder.create_block();
            let next = self.builder.create_block();
            let given = self.builder.ins().icmp_imm(IntCC::NotEqual, tag, 0);
            self.builder.ins().brif(given, close, &[], next, &[]);
            self.builder.switch_to_block(close);
            self.builder.seal_block(close);
            let is_pipe = self.builder.ins().icmp_imm(IntCC::Equal, tag, 1);
            let fd = self.builder.ins().select(is_pipe, end, file);
            let fd = self.builder.ins().ireduce(types::I32, fd);
            self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
            self.builder.ins().jump(next, &[]);
            self.builder.switch_to_block(next);
            self.builder.seal_block(next);
        }

        let child = self.builder.ins().load(types::I32, MemFlags::trusted(), pid, 0);
        let child = self.child_word(child);
        let reason = self.builder.ins().sextend(types::I64, error);
        let failed = self.builder.ins().icmp_imm(IntCC::NotEqual, error, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, child, reason]
    }

    /// A `Child`'s one word from its pid (§4.8): on Linux the pid with a
    /// `pidfd` for it in the high half, opened here -- the child has not been
    /// reaped, so the pid is still its own -- and on Darwin the pid alone.
    fn child_word(&mut self, pid: Value) -> Value {
        let low = self.builder.ins().uextend(types::I64, pid);
        if self.is_darwin() {
            return low;
        }
        let number = self.builder.ins().iconst(types::I64, SYS_PIDFD_OPEN);
        let pid64 = self.builder.ins().sextend(types::I64, pid);
        let flags = self.builder.ins().iconst(types::I64, 0);
        let opened = self.libc_call(
            "syscall",
            &[types::I64, types::I64, types::I64],
            &[types::I64],
            &[number, pid64, flags],
        );
        let reason = self.errno();
        let refused = self.builder.ins().icmp_imm(IntCC::SignedLessThan, opened, 0);
        let negated = self.builder.ins().ineg(reason);
        let opened = self.builder.ins().select(refused, negated, opened);
        let opened = self.builder.ins().ireduce(types::I32, opened);
        let opened = self.builder.ins().uextend(types::I64, opened);
        let high = self.builder.ins().ishl_imm(opened, CHILD_PIDFD_SHIFT);
        self.builder.ins().bor(low, high)
    }

    /// A `\0`-separated list as a `NULL`-terminated array of pointers into
    /// it, `malloc`ed: `first` (the program, for `argv[0]`) ahead of the
    /// entries when given. A list that is not empty and does not end in a
    /// `\0` traps (§4.2). For the environment, an entry naming a
    /// loader variable traps (§4.3).
    fn pointer_list(&mut self, list: &[Value], first: Option<Value>, environment: bool) -> Value {
        let pointer = self.pointer;
        let (base, length) = (list[0], list[1]);

        // Not empty, and not ending in `\0`: a program bug, as an index past
        // the end is.
        let check = self.builder.create_block();
        let checked = self.builder.create_block();
        let empty = self.builder.ins().icmp_imm(IntCC::Equal, length, 0);
        self.builder.ins().brif(empty, checked, &[], check, &[]);
        self.builder.switch_to_block(check);
        self.builder.seal_block(check);
        let last = self.builder.ins().iadd(base, length);
        let last = self.builder.ins().load(types::I8, MemFlags::trusted(), last, -1);
        let unterminated = self.builder.ins().icmp_imm(IntCC::NotEqual, last, 0);
        self.builder.ins().trapnz(unterminated, TrapCode::HEAP_OUT_OF_BOUNDS);
        self.builder.ins().jump(checked, &[]);
        self.builder.switch_to_block(checked);
        self.builder.seal_block(checked);

        // Every entry ends in a `\0`, so the count of `\0`s is the count.
        let count = self.count_zeros(base, length);
        let lead = i64::from(first.is_some());
        let slots = self.builder.ins().iadd_imm(count, lead + 1);
        let bytes = self.builder.ins().imul_imm(slots, 8);
        let array = self.libc_call("malloc", &[pointer], &[pointer], &[bytes]);
        let missing = self.builder.ins().icmp_imm(IntCC::Equal, array, 0);
        self.builder.ins().trapnz(missing, TrapCode::HEAP_OUT_OF_BOUNDS);
        if let Some(first) = first {
            self.builder.ins().store(MemFlags::trusted(), first, array, 0);
        }

        // One pass: store each entry's address and step past its `\0`.
        let cursor = self.temporary(pointer);
        let index = self.temporary(types::I64);
        self.builder.def_var(cursor, base);
        let start = self.builder.ins().iconst(types::I64, lead);
        self.builder.def_var(index, start);
        let end = self.builder.ins().iadd(base, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let at = self.builder.use_var(cursor);
        let more = self.builder.ins().icmp(IntCC::UnsignedLessThan, at, end);
        self.builder.ins().brif(more, body, &[], done, &[]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        let at = self.builder.use_var(cursor);
        if environment {
            self.refuse_loader_variable(at);
        }
        let i = self.builder.use_var(index);
        let offset = self.builder.ins().imul_imm(i, 8);
        let slot = self.builder.ins().iadd(array, offset);
        self.builder.ins().store(MemFlags::trusted(), at, slot, 0);
        let remaining = self.builder.ins().isub(end, at);
        let nul = self.builder.ins().iconst(types::I64, 0);
        let found = self.libc_call(
            "memchr",
            &[pointer, types::I64, pointer],
            &[pointer],
            &[at, nul, remaining],
        );
        let after = self.builder.ins().iadd_imm(found, 1);
        self.builder.def_var(cursor, after);
        let next = self.builder.ins().iadd_imm(i, 1);
        self.builder.def_var(index, next);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        let i = self.builder.use_var(index);
        let offset = self.builder.ins().imul_imm(i, 8);
        let slot = self.builder.ins().iadd(array, offset);
        let null = self.builder.ins().iconst(pointer, 0);
        self.builder.ins().store(MemFlags::trusted(), null, slot, 0);
        array
    }

    /// How many `\0` bytes `length` bytes at `base` hold, by `memchr`.
    fn count_zeros(&mut self, base: Value, length: Value) -> Value {
        let pointer = self.pointer;
        let cursor = self.temporary(pointer);
        let count = self.temporary(types::I64);
        self.builder.def_var(cursor, base);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.def_var(count, zero);
        let end = self.builder.ins().iadd(base, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let at = self.builder.use_var(cursor);
        let more = self.builder.ins().icmp(IntCC::UnsignedLessThan, at, end);
        self.builder.ins().brif(more, body, &[], done, &[]);

        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        let at = self.builder.use_var(cursor);
        let remaining = self.builder.ins().isub(end, at);
        let nul = self.builder.ins().iconst(types::I64, 0);
        let found = self.libc_call(
            "memchr",
            &[pointer, types::I64, pointer],
            &[pointer],
            &[at, nul, remaining],
        );
        // The list was checked to end in a `\0`, so `memchr` always finds one.
        let after = self.builder.ins().iadd_imm(found, 1);
        self.builder.def_var(cursor, after);
        let n = self.builder.use_var(count);
        let n = self.builder.ins().iadd_imm(n, 1);
        self.builder.def_var(count, n);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        self.builder.use_var(count)
    }

    /// Trap when the `\0`-terminated entry at `entry` names a variable the
    /// dynamic loader reads (`LD_*`, `DYLD_*`): it would run code from outside
    /// the capability's bound (§4.3). `strncmp` stops at the entry's `\0`, so
    /// a short entry is never read past.
    fn refuse_loader_variable(&mut self, entry: Value) {
        let pointer = self.pointer;
        for name in ["LD_", "DYLD_"] {
            let wanted = self.c_string(name);
            let n = self.builder.ins().iconst(types::I64, name.len() as i64);
            let order = self.libc_call(
                "strncmp",
                &[pointer, pointer, types::I64],
                &[types::I32],
                &[entry, wanted, n],
            );
            let same = self.builder.ins().icmp_imm(IntCC::Equal, order, 0);
            self.builder.ins().trapnz(same, TrapCode::HEAP_OUT_OF_BOUNDS);
        }
    }

    /// `child_wait(Child)`: `waitpid` on this child alone, again after an
    /// interrupted wait. `Exited`'s four leaves: the tag (`Code` 0, `Signaled`
    /// 1, `Failed` 2), the exit code, the signal as a `std.signals` bit (`0`
    /// for one without), the reason.
    pub(crate) fn child_wait(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let pid = self.builder.ins().ireduce(types::I32, args[0]);
        let status = self.scratch_bytes(8);
        let zero32 = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().store(MemFlags::trusted(), zero32, status, 0);

        let again = self.builder.create_block();
        let after = self.builder.create_block();
        self.builder.append_block_param(after, types::I32);
        self.builder.append_block_param(after, types::I64);
        self.builder.ins().jump(again, &[]);
        self.builder.switch_to_block(again);
        let options = self.builder.ins().iconst(types::I32, 0);
        let answered = self.libc_call(
            "waitpid",
            &[types::I32, pointer, types::I32],
            &[types::I32],
            &[pid, status, options],
        );
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, answered, 0);
        let interrupted = self.builder.ins().icmp_imm(IntCC::Equal, reason, 4);
        let retry = self.builder.ins().band(failed, interrupted);
        self.builder.ins().brif(retry, again, &[], after, &[answered.into(), reason.into()]);
        self.builder.seal_block(again);

        self.builder.switch_to_block(after);
        self.builder.seal_block(after);
        let answered = self.builder.block_params(after)[0];
        let reason = self.builder.block_params(after)[1];
        let word = self.builder.ins().load(types::I32, MemFlags::trusted(), status, 0);
        let word = self.builder.ins().sextend(types::I64, word);
        // The traditional layout, the same on both kernels: the low seven bits
        // are the signal that ended the child, or 0 when it exited, and then
        // the next eight are its code.
        let signal = self.builder.ins().band_imm(word, 0x7f);
        let shifted = self.builder.ins().ushr_imm(word, 8);
        let code = self.builder.ins().band_imm(shifted, 0xff);
        let bit = self.bit_of_native(signal);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, answered, 0);
        let exited = self.builder.ins().icmp_imm(IntCC::Equal, signal, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let ended = self.builder.ins().select(exited, zero, one);
        let tag = self.builder.ins().select(failed, two, ended);
        // The `pidfd` has done its work once the child is reaped (§4.8); the
        // negated `errno` that stands in for a refused one is an `EBADF` that
        // harms nothing.
        if !self.is_darwin() {
            let high = self.builder.ins().ushr_imm(args[0], CHILD_PIDFD_SHIFT);
            let pidfd = self.builder.ins().ireduce(types::I32, high);
            self.libc_call("close", &[types::I32], &[types::I32], &[pidfd]);
        }
        vec![tag, code, bit, reason]
    }

    /// The `std.signals` bit of a native signal number, `0` for one without.
    fn bit_of_native(&mut self, native: Value) -> Value {
        let mut bit = self.builder.ins().iconst(types::I64, 0);
        for (wanted, number) in sendable_signals(self.is_darwin()) {
            let same = self.builder.ins().icmp_imm(IntCC::Equal, native, number);
            let this = self.builder.ins().iconst(types::I64, wanted);
            bit = self.builder.ins().select(same, this, bit);
        }
        bit
    }

    /// `child_kill(&Child, bit)`: one of the sendable signals by its bit, or
    /// `EINVAL` with no call. `0`, or the `errno`. The `Child` is unreaped, so
    /// its pid cannot have been reused (§4.7).
    pub(crate) fn child_kill(&mut self, args: &[Value]) -> Vec<Value> {
        let pid = self.handle_fd(args[0]);
        let bit = args[1];
        let mut native = self.builder.ins().iconst(types::I64, 0);
        for (wanted, number) in sendable_signals(self.is_darwin()) {
            let same = self.builder.ins().icmp_imm(IntCC::Equal, bit, wanted);
            let this = self.builder.ins().iconst(types::I64, number);
            native = self.builder.ins().select(same, this, native);
        }
        let send = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let known = self.builder.ins().icmp_imm(IntCC::NotEqual, native, 0);
        let einval = self.builder.ins().iconst(types::I64, cancho_ir::EINVAL);
        self.builder.ins().brif(known, send, &[], merge, &[einval.into()]);

        self.builder.switch_to_block(send);
        self.builder.seal_block(send);
        let signal = self.builder.ins().ireduce(types::I32, native);
        let result =
            self.libc_call("kill", &[types::I32, types::I32], &[types::I32], &[pid, signal]);
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let answer = self.builder.ins().select(failed, reason, zero);
        self.builder.ins().jump(merge, &[answer.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        vec![self.builder.block_params(merge)[0]]
    }
}
