//! Directory handles (`docs/directory-handles.md`): `open_dir`'s open,
//! `dir_enter`, `dir_open_read` and `dir_close`. Mirrors
//! `lex-sys-codegen-llvm`'s own `body/dirs.rs`.
//!
//! A step beneath a directory is one `openat(dir, name, flags | O_NOFOLLOW)`
//! on one path component. The component is checked here, before any call:
//! empty, longer than `NAME_MAX`, `.`, `..`, or holding a `/` or a NUL is
//! `Failed(EINVAL)`, because `O_NOFOLLOW` refuses a link and nothing else --
//! `..` would walk out of the directory untouched (§1's probe).

use crate::*;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `(O_DIRECTORY, O_NOFOLLOW)` for this target, from the one table both
    /// backends share.
    fn directory_flags(&self) -> (i64, i64) {
        let aarch64 = matches!(
            self.module.isa().triple().architecture,
            target_lexicon::Architecture::Aarch64(_)
        );
        lex_sys_ir::directory_flags(self.is_darwin(), aarch64)
    }

    /// `open_dir`'s open: the path is already checked against the prefix and
    /// NUL-terminated. `open(path, O_RDONLY | O_DIRECTORY)`, two fixed
    /// arguments as `open_read`'s is; links in this path are followed, since
    /// it is the anchor the caller chose. `DirOpened`'s three leaves.
    pub(crate) fn open_directory(&mut self, path: Value) -> Vec<Value> {
        let pointer = self.pointer;
        let (directory, _) = self.directory_flags();
        let flags = self.builder.ins().iconst(types::I32, directory);
        let fd = self.libc_call("open", &[pointer, types::I32], &[types::I32], &[path, flags]);
        let fd = self.builder.ins().sextend(types::I64, fd);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        let reason = self.errno();
        vec![tag, fd, reason]
    }

    /// `dir_enter(dir, name)` and `dir_open_read(dir, name)`: `args` is the
    /// handle's address, then the name's pointer and length. Answers the
    /// three leaves `DirOpened` and `Opened` share: the tag, the descriptor
    /// and the reason.
    pub(crate) fn dir_open(&mut self, args: &[Value], directory: bool) -> Vec<Value> {
        let pointer = self.pointer;
        let (handle, name, length) = (args[0], args[1], args[2]);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refused = self.builder.create_block();
        let body = self.builder.create_block();
        let second = self.builder.create_block();
        let call = self.builder.create_block();

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
        let next = self.builder.create_block();
        self.builder.ins().brif(bad, refused, &[], next, &[]);
        self.builder.switch_to_block(next);
        self.builder.seal_block(next);
        self.builder.ins().brif(maybe_dotdot, second, &[], call, &[]);

        self.builder.switch_to_block(second);
        self.builder.seal_block(second);
        let other = self.builder.ins().uload8(types::I64, MemFlags::trusted(), name, 1);
        let dotdot = self.builder.ins().icmp_imm(IntCC::Equal, other, i64::from(b'.'));
        self.builder.ins().brif(dotdot, refused, &[], call, &[]);

        // The name, NUL-terminated on the stack, then one `openat`. Its
        // variadic `mode` is read only with `O_CREAT`, which is not passed,
        // so three fixed arguments are where the callee looks on every ABI.
        self.builder.switch_to_block(call);
        self.builder.seal_block(call);
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
        let fd = self.builder.ins().load(types::I64, MemFlags::trusted(), handle, 0);
        let fd = self.builder.ins().ireduce(types::I32, fd);
        let (o_directory, o_nofollow) = self.directory_flags();
        let flags = if directory { o_directory | o_nofollow } else { o_nofollow };
        let flags = self.builder.ins().iconst(types::I32, flags);
        let opened = self.libc_call(
            "openat",
            &[types::I32, pointer, types::I32],
            &[types::I32],
            &[fd, copy, flags],
        );
        let reason = self.errno();
        let opened = self.builder.ins().sextend(types::I64, opened);
        self.builder.ins().jump(merge, &[opened.into(), reason.into()]);

        self.builder.switch_to_block(refused);
        self.builder.seal_block(refused);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let einval = self.builder.ins().iconst(types::I64, lex_sys_ir::EINVAL);
        self.builder.ins().jump(merge, &[minus_one.into(), einval.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let fd = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, fd, reason]
    }

    /// `dir_close(dir)`: `close(2)`, its answer widened. The handle is one
    /// leaf and it ends here, as a `File` does at `file_close`.
    pub(crate) fn dir_close(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.builder.ins().ireduce(types::I32, args[0]);
        let answer = self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        vec![self.builder.ins().sextend(types::I64, answer)]
    }
}
