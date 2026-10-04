//! Directory listing (`docs/directory-listing.md`): `dir_list`, `dir_next`
//! and `dir_list_close`. Mirrors `lex-sys-codegen-llvm`'s own
//! `body/listing.rs`.
//!
//! A listing is libc's `DIR` stream on a descriptor of its own --
//! `openat(dir, ".", O_RDONLY | O_DIRECTORY)`, not a `dup`, so two listings of
//! one `Dir` do not share a position -- and `struct dirent` is read at the
//! offsets `lex_sys_ir::dirent_layout` gives for the target (§3.4).

use crate::*;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// `errno = 0`: `readdir` reports an error only through `errno`, so a
    /// step clears it first and reads it when the answer is null.
    fn clear_errno(&mut self) {
        let pointer = self.pointer;
        let symbol = match self.module.isa().triple().operating_system {
            target_lexicon::OperatingSystem::Darwin(_) => "__error",
            _ => "__errno_location",
        };
        let id = self.libc_fn(symbol, &[], &[pointer]);
        let at = self.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(at, &[]);
        let address = self.builder.inst_results(call)[0];
        let zero = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().store(MemFlags::trusted(), zero, address, 0);
    }

    /// `dir_list(dir)`: `args` is the handle's address. `Listing`'s three
    /// leaves: the tag, the stream and the reason.
    pub(crate) fn dir_list(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let directory = lex_sys_ir::open_flags(self.is_darwin(), self.aarch64()).directory;

        // `"."`, NUL-terminated, on the stack.
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            2,
            0,
        ));
        let dot = self.builder.ins().stack_addr(pointer, slot, 0);
        let byte = self.builder.ins().iconst(types::I8, i64::from(b'.'));
        self.builder.ins().store(MemFlags::trusted(), byte, dot, 0);
        let nul = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().store(MemFlags::trusted(), nul, dot, 1);

        let fd = self.dir_fd(args[0]);
        let own = self.openat(fd, dot, directory, 0);
        let reason = self.errno();

        let opened = self.builder.create_block();
        let merge = self.builder.create_block();
        for _ in 0..3 {
            self.builder.append_block_param(merge, types::I64);
        }
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, own, 0);
        self.builder.ins().brif(
            failed,
            merge,
            &[one.into(), zero.into(), reason.into()],
            opened,
            &[],
        );

        // The stream takes the descriptor; if it cannot, the descriptor is
        // closed here, after `errno` is read.
        self.builder.switch_to_block(opened);
        self.builder.seal_block(opened);
        let stream = self.libc_call("fdopendir", &[types::I32], &[pointer], &[own]);
        let reason = self.errno();
        let null = self.builder.ins().icmp_imm(IntCC::Equal, stream, 0);
        let refused = self.builder.create_block();
        let started = self.builder.create_block();
        self.builder.ins().brif(null, refused, &[], started, &[]);

        self.builder.switch_to_block(refused);
        self.builder.seal_block(refused);
        self.libc_call("close", &[types::I32], &[types::I32], &[own]);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[one.into(), zero.into(), reason.into()]);

        self.builder.switch_to_block(started);
        self.builder.seal_block(started);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into(), stream.into(), zero.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge).to_vec()
    }

    /// `dir_next(list, name)`: `args` is the listing's address, then the
    /// buffer's pointer and length. `Listed`'s four leaves: the tag (`Name`
    /// 0, `End` 1, `Failed` 2), the length, the kind and the reason.
    pub(crate) fn dir_next(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let layout = lex_sys_ir::dirent_layout(self.is_darwin());
        let (handle, buffer, room) = (args[0], args[1], args[2]);
        let stream = self.builder.ins().load(pointer, MemFlags::trusted(), handle, 0);

        let step = self.builder.create_block();
        let ended = self.builder.create_block();
        let entry = self.builder.create_block();
        let second = self.builder.create_block();
        let keep = self.builder.create_block();
        let copy = self.builder.create_block();
        let short = self.builder.create_block();
        let merge = self.builder.create_block();
        for _ in 0..4 {
            self.builder.append_block_param(merge, types::I64);
        }
        self.builder.ins().jump(step, &[]);

        // One `readdir`, `errno` cleared first.
        self.builder.switch_to_block(step);
        self.clear_errno();
        let found = self.libc_call("readdir", &[pointer], &[pointer], &[stream]);
        let null = self.builder.ins().icmp_imm(IntCC::Equal, found, 0);
        self.builder.ins().brif(null, ended, &[], entry, &[]);

        // Null: the end if `errno` is still zero, a failure otherwise.
        self.builder.switch_to_block(ended);
        self.builder.seal_block(ended);
        let reason = self.errno();
        let quiet = self.builder.ins().icmp_imm(IntCC::Equal, reason, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let tag = self.builder.ins().select(quiet, one, two);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[tag.into(), zero.into(), zero.into(), reason.into()]);

        // `.` and `..` are skipped. The name is NUL-terminated, so its second
        // byte is always there to read.
        self.builder.switch_to_block(entry);
        self.builder.seal_block(entry);
        let name = self.builder.ins().iadd_imm(found, i64::from(layout.d_name));
        let length = self.libc_call("strlen", &[pointer], &[types::I64], &[name]);
        let first = self.builder.ins().uload8(types::I64, MemFlags::trusted(), name, 0);
        let first_dot = self.builder.ins().icmp_imm(IntCC::Equal, first, i64::from(b'.'));
        let one_byte = self.builder.ins().icmp_imm(IntCC::Equal, length, 1);
        let dot = self.builder.ins().band(one_byte, first_dot);
        let two_bytes = self.builder.ins().icmp_imm(IntCC::Equal, length, 2);
        let maybe = self.builder.ins().band(two_bytes, first_dot);
        let either = self.builder.ins().bor(dot, maybe);
        self.builder.ins().brif(either, second, &[], keep, &[]);

        self.builder.switch_to_block(second);
        self.builder.seal_block(second);
        let other = self.builder.ins().uload8(types::I64, MemFlags::trusted(), name, 1);
        let other_dot = self.builder.ins().icmp_imm(IntCC::Equal, other, i64::from(b'.'));
        let dotdot = self.builder.ins().band(maybe, other_dot);
        let skip = self.builder.ins().bor(dot, dotdot);
        self.builder.ins().brif(skip, step, &[], keep, &[]);
        self.builder.seal_block(step);

        // A name longer than the buffer is refused whole, never cut.
        self.builder.switch_to_block(keep);
        self.builder.seal_block(keep);
        let long = self.builder.ins().icmp(IntCC::SignedGreaterThan, length, room);
        self.builder.ins().brif(long, short, &[], copy, &[]);

        self.builder.switch_to_block(short);
        self.builder.seal_block(short);
        let two = self.builder.ins().iconst(types::I64, 2);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let errno = lex_sys_ir::enametoolong(self.is_darwin());
        let too_long = self.builder.ins().iconst(types::I64, errno);
        self.builder.ins().jump(merge, &[two.into(), zero.into(), zero.into(), too_long.into()]);

        self.builder.switch_to_block(copy);
        self.builder.seal_block(copy);
        self.libc_call(
            "memmove",
            &[pointer, pointer, pointer],
            &[pointer],
            &[buffer, name, length],
        );
        let raw = self.builder.ins().uload8(types::I64, MemFlags::trusted(), found, layout.d_type);
        let kind = self.kind_of(raw);
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[zero.into(), length.into(), kind.into(), zero.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.builder.block_params(merge).to_vec()
    }

    /// `d_type` as the language numbers a kind (§3.1).
    fn kind_of(&mut self, raw: Value) -> Value {
        let mut kind = self.builder.ins().iconst(types::I64, lex_sys_ir::KIND_OTHER);
        for (dt, ours) in [
            (lex_sys_ir::DT_UNKNOWN, lex_sys_ir::KIND_UNKNOWN),
            (lex_sys_ir::DT_LNK, lex_sys_ir::KIND_LINK),
            (lex_sys_ir::DT_DIR, lex_sys_ir::KIND_DIRECTORY),
            (lex_sys_ir::DT_REG, lex_sys_ir::KIND_FILE),
        ] {
            let is = self.builder.ins().icmp_imm(IntCC::Equal, raw, dt);
            let value = self.builder.ins().iconst(types::I64, ours);
            kind = self.builder.ins().select(is, value, kind);
        }
        kind
    }

    /// `dir_list_close(list)`: `closedir`, which closes the listing's own
    /// descriptor and leaves the directory open.
    pub(crate) fn dir_list_close(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let answer = self.libc_call("closedir", &[pointer], &[types::I32], &[args[0]]);
        vec![self.builder.ins().sextend(types::I64, answer)]
    }
}
