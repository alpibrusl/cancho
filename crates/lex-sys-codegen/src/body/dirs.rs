//! Directory handles (`docs/directory-handles.md`): `open_dir`'s open, the
//! steps beneath a directory (`dir_enter`, `dir_open_read`, `dir_open_new`,
//! `dir_open_append`), the changes in one (`dir_rename`, `dir_remove`,
//! `dir_sync`) and `dir_close`. Mirrors `lex-sys-codegen-llvm`'s own
//! `body/dirs.rs`.
//!
//! Every name is one path component, checked here before any call: empty,
//! longer than `NAME_MAX`, `.`, `..`, or holding a `/` or a NUL is
//! `Failed(EINVAL)`, because `O_NOFOLLOW` refuses a link and nothing else --
//! `..` would walk out of the directory untouched (§1's probe).

use crate::*;
use cranelift_codegen::ir::Block;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    pub(crate) fn aarch64(&self) -> bool {
        matches!(self.module.isa().triple().architecture, target_lexicon::Architecture::Aarch64(_))
    }

    /// The `open` flags for this target, from the one table both backends
    /// share.
    pub(crate) fn open_flags(&self) -> lex_sys_ir::OpenFlags {
        lex_sys_ir::open_flags(self.is_darwin(), self.aarch64())
    }

    /// `open_dir`'s open: the path is already checked against the prefix and
    /// NUL-terminated. `openat(AT_FDCWD, path, O_RDONLY | O_DIRECTORY |
    /// O_CLOEXEC)`, as `open_read`'s is; links in this path are followed, since
    /// it is the anchor the caller chose. `DirOpened`'s three leaves.
    pub(crate) fn open_directory(&mut self, path: Value) -> Vec<Value> {
        let flags = self.open_flags();
        let cwd = self.builder.ins().iconst(types::I32, flags.at_fdcwd);
        let fd = self.openat(cwd, path, flags.directory | flags.cloexec, 0);
        let fd = self.builder.ins().sextend(types::I64, fd);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        let reason = self.errno();
        vec![tag, fd, reason]
    }

    /// Check one name and copy it, NUL-terminated, onto the stack. A name
    /// that is not one component jumps to `refused`; otherwise the builder is
    /// left in a fresh block and the copy's address comes back.
    fn component(&mut self, name: Value, length: Value, refused: Block) -> Value {
        let pointer = self.pointer;
        let body = self.builder.create_block();
        let next = self.builder.create_block();
        let second = self.builder.create_block();
        let good = self.builder.create_block();

        // Empty or longer than `NAME_MAX`: refused before a byte is read.
        let empty = self.builder.ins().icmp_imm(IntCC::Equal, length, 0);
        let long =
            self.builder.ins().icmp_imm(IntCC::SignedGreaterThan, length, lex_sys_ir::NAME_MAX);
        let bad = self.builder.ins().bor(empty, long);
        self.builder.ins().brif(bad, refused, &[], body, &[]);

        // A `/` or a NUL anywhere, or `.`; `..` needs its second byte, read in
        // a block of its own so a one-byte name is never read past.
        self.builder.switch_to_block(body);
        self.builder.seal_block(body);
        let slash = self.builder.ins().iconst(types::I64, i64::from(b'/'));
        let found = self.libc_call(
            "memchr",
            &[pointer, types::I64, pointer],
            &[pointer],
            &[name, slash, length],
        );
        let has_slash = self.builder.ins().icmp_imm(IntCC::NotEqual, found, 0);
        let nul = self.builder.ins().iconst(types::I64, 0);
        let found = self.libc_call(
            "memchr",
            &[pointer, types::I64, pointer],
            &[pointer],
            &[name, nul, length],
        );
        let has_nul = self.builder.ins().icmp_imm(IntCC::NotEqual, found, 0);
        let first = self.builder.ins().uload8(types::I64, MemFlags::trusted(), name, 0);
        let first_dot = self.builder.ins().icmp_imm(IntCC::Equal, first, i64::from(b'.'));
        let one_byte = self.builder.ins().icmp_imm(IntCC::Equal, length, 1);
        let dot = self.builder.ins().band(one_byte, first_dot);
        let mut bad = self.builder.ins().bor(has_slash, has_nul);
        bad = self.builder.ins().bor(bad, dot);
        let two_bytes = self.builder.ins().icmp_imm(IntCC::Equal, length, 2);
        let maybe_dotdot = self.builder.ins().band(two_bytes, first_dot);
        self.builder.ins().brif(bad, refused, &[], next, &[]);
        self.builder.switch_to_block(next);
        self.builder.seal_block(next);
        self.builder.ins().brif(maybe_dotdot, second, &[], good, &[]);

        self.builder.switch_to_block(second);
        self.builder.seal_block(second);
        let other = self.builder.ins().uload8(types::I64, MemFlags::trusted(), name, 1);
        let dotdot = self.builder.ins().icmp_imm(IntCC::Equal, other, i64::from(b'.'));
        self.builder.ins().brif(dotdot, refused, &[], good, &[]);

        self.builder.switch_to_block(good);
        self.builder.seal_block(good);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (lex_sys_ir::NAME_MAX + 1) as u32,
            0,
        ));
        let copy = self.builder.ins().stack_addr(pointer, slot, 0);
        self.libc_call("memmove", &[pointer, pointer, pointer], &[pointer], &[copy, name, length]);
        let end = self.builder.ins().iadd(copy, length);
        let terminator = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().store(MemFlags::trusted(), terminator, end, 0);
        copy
    }

    /// `openat(fd, path, flags, mode)`. `openat` is variadic and `mode` is
    /// its variadic argument, read only with `O_CREAT`. On Apple AArch64 a
    /// variadic argument is passed on the stack, so the call is shaped the way
    /// the callee reads it, as `fcntl`'s is (`native-sockets.md` §3): nine
    /// integer parameters, the ninth in the first stack slot. Everywhere else
    /// a variadic and a fixed argument travel the same way. One signature per
    /// module either way, so every `openat` here goes through this.
    pub(crate) fn openat(&mut self, fd: Value, path: Value, flags: i64, mode: i64) -> Value {
        let pointer = self.pointer;
        let flags = self.builder.ins().iconst(types::I32, flags);
        if self.is_darwin() && self.aarch64() {
            let filler = self.builder.ins().iconst(types::I64, 0);
            let mode = self.builder.ins().iconst(types::I64, mode);
            let mut params = vec![types::I32, pointer, types::I32];
            params.extend([types::I64; 6]);
            let mut args = vec![fd, path, flags];
            args.extend([filler; 5]);
            args.push(mode);
            self.libc_call("openat", &params, &[types::I32], &args)
        } else {
            let mode = self.builder.ins().iconst(types::I32, mode);
            self.libc_call(
                "openat",
                &[types::I32, pointer, types::I32, types::I32],
                &[types::I32],
                &[fd, path, flags, mode],
            )
        }
    }

    /// Check every name in `names` (pointer and length each), then run
    /// `call` on their copies. `call` answers the raw result, negative for a
    /// failure, and `errno` is read straight after it. The three leaves
    /// `Opened`, `DirOpened` and `Done` share come back: the tag, the result
    /// and the reason.
    pub(crate) fn dir_call(
        &mut self,
        names: &[(Value, Value)],
        call: impl FnOnce(&mut Self, &[Value]) -> Value,
    ) -> Vec<Value> {
        self.dir_call_mapping(names, None, call)
    }

    /// `dir_call`, and when `unsupported` is given, a kernel `EINVAL` from
    /// `call` is answered as that value instead. The name check's own
    /// `EINVAL` is not mapped, so the two stay apart.
    pub(crate) fn dir_call_mapping(
        &mut self,
        names: &[(Value, Value)],
        unsupported: Option<i64>,
        call: impl FnOnce(&mut Self, &[Value]) -> Value,
    ) -> Vec<Value> {
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refused = self.builder.create_block();

        let copies: Vec<Value> =
            names.iter().map(|&(name, length)| self.component(name, length, refused)).collect();
        let result = call(self, &copies);
        let mut reason = self.errno();
        if let Some(unsupported) = unsupported {
            let invalid = self.builder.ins().icmp_imm(IntCC::Equal, reason, lex_sys_ir::EINVAL);
            let mapped = self.builder.ins().iconst(types::I64, unsupported);
            reason = self.builder.ins().select(invalid, mapped, reason);
        }
        let result = self.builder.ins().sextend(types::I64, result);
        self.builder.ins().jump(merge, &[result.into(), reason.into()]);

        self.builder.switch_to_block(refused);
        self.builder.seal_block(refused);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let einval = self.builder.ins().iconst(types::I64, lex_sys_ir::EINVAL);
        self.builder.ins().jump(merge, &[minus_one.into(), einval.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let result = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, result, reason]
    }

    /// The descriptor behind a borrowed `Dir`: the handle arrives as its
    /// address.
    pub(crate) fn dir_fd(&mut self, handle: Value) -> Value {
        let fd = self.builder.ins().load(types::I64, MemFlags::trusted(), handle, 0);
        self.builder.ins().ireduce(types::I32, fd)
    }

    /// `dir_enter`, `dir_open_read`, `dir_open_new` and `dir_open_append`:
    /// `args` is the handle's address, then the name's pointer and length.
    pub(crate) fn dir_open(&mut self, args: &[Value], op: Builtin) -> Vec<Value> {
        let f = self.open_flags();
        let (flags, mode) = match op {
            Builtin::DirEnter => (f.directory | f.nofollow, 0),
            Builtin::DirOpenNew => {
                (f.write_only | f.create | f.exclusive | f.nofollow, lex_sys_ir::CREATE_MODE)
            }
            Builtin::DirOpenAppend => {
                (f.write_only | f.create | f.append | f.nofollow, lex_sys_ir::CREATE_MODE)
            }
            _ => (f.nofollow, 0),
        };
        // Every descriptor a builtin opens is close-on-exec (`docs/processes.md` §4.5).
        let flags = flags | f.cloexec;
        let handle = args[0];
        self.dir_call(&[(args[1], args[2])], |this, copies| {
            let fd = this.dir_fd(handle);
            this.openat(fd, copies[0], flags, mode)
        })
    }

    /// `dir_rename(dir, from, to)`: `renameat` with both names in the one
    /// directory.
    pub(crate) fn dir_rename(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let handle = args[0];
        self.dir_call(&[(args[1], args[2]), (args[3], args[4])], |this, copies| {
            let fd = this.dir_fd(handle);
            this.libc_call(
                "renameat",
                &[types::I32, pointer, types::I32, pointer],
                &[types::I32],
                &[fd, copies[0], fd, copies[1]],
            )
        })
    }

    /// `dir_rename_new(dir, from, to)`: the rename that refuses to replace.
    /// Linux `renameat2(RENAME_NOREPLACE)`, Darwin `renameatx_np(RENAME_EXCL)`;
    /// a filesystem without it is `rename_unsupported`, never a plain rename
    /// (`docs/directory-handles.md` §3, slice 4).
    pub(crate) fn dir_rename_new(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let handle = args[0];
        let darwin = self.is_darwin();
        let symbol = if darwin { "renameatx_np" } else { "renameat2" };
        self.dir_call_mapping(
            &[(args[1], args[2]), (args[3], args[4])],
            Some(lex_sys_ir::rename_unsupported(darwin)),
            |this, copies| {
                let fd = this.dir_fd(handle);
                let flag =
                    this.builder.ins().iconst(types::I32, lex_sys_ir::rename_no_replace(darwin));
                this.libc_call(
                    symbol,
                    &[types::I32, pointer, types::I32, pointer, types::I32],
                    &[types::I32],
                    &[fd, copies[0], fd, copies[1], flag],
                )
            },
        )
    }

    /// `dir_remove(dir, name)`: `unlinkat(dir, name, 0)`, which removes a
    /// link rather than what it points at.
    pub(crate) fn dir_remove(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let handle = args[0];
        self.dir_call(&[(args[1], args[2])], |this, copies| {
            let fd = this.dir_fd(handle);
            let no_flags = this.builder.ins().iconst(types::I32, 0);
            this.libc_call(
                "unlinkat",
                &[types::I32, pointer, types::I32],
                &[types::I32],
                &[fd, copies[0], no_flags],
            )
        })
    }

    /// `dir_sync(dir)`: `fsync` on the directory, as `Done`.
    pub(crate) fn dir_sync(&mut self, args: &[Value]) -> Vec<Value> {
        let handle = args[0];
        self.dir_call(&[], |this, _| {
            let fd = this.dir_fd(handle);
            this.libc_call("fsync", &[types::I32], &[types::I32], &[fd])
        })
    }

    /// `dir_close(dir)`: `close(2)`, its answer widened. The handle is one
    /// leaf and it ends here, as a `File` does at `file_close`.
    pub(crate) fn dir_close(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.builder.ins().ireduce(types::I32, args[0]);
        let answer = self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        vec![self.builder.ins().sextend(types::I64, answer)]
    }
}
