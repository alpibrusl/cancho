//! The write side of a file handle (`docs/file-writes.md`): the opens that
//! append, create and update, and the verbs that write, sync and cut. The
//! read side (`open_read`, `file_read`) stays in `memory.rs`. Mirrors
//! `lex-sys-codegen-llvm`'s own `body/files.rs`.

use crate::*;
use lex_sys_ir::{OpenMode, PathOp};

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// A NUL-terminated copy of `text` on the stack, for a C call that wants a
    /// string. A slice literal carries no terminator.
    fn c_string(&mut self, text: &str) -> Value {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            (text.len() + 1) as u32,
            0,
        ));
        let at = self.builder.ins().stack_addr(pointer, slot, 0);
        for (i, byte) in text.bytes().chain(std::iter::once(0)).enumerate() {
            let value = self.builder.ins().iconst(types::I8, i64::from(byte));
            self.builder.ins().store(MemFlags::trusted(), value, at, i as i32);
        }
        at
    }

    /// Open `path` (already checked and NUL-terminated) in a mode that is not
    /// `Read`, and answer `Opened`'s three leaves: the tag, `Ok`'s descriptor
    /// and `Failed`'s reason.
    ///
    /// `fopen` has a mode string for each way a log opens a file and is not
    /// variadic, so no argument can land where the callee does not look and no
    /// `O_*` constant is guessed per target (`docs/file-writes.md` section 3).
    /// The descriptor is taken with `dup` and the `FILE` is then closed,
    /// which closes the original descriptor and leaves the copy: the two
    /// share one open file description, so `O_APPEND` and the access mode
    /// survive, and nothing leaks a `FILE`. This is a bridge: nothing outside
    /// the backend can see it, and a libc-free runtime replaces it with one
    /// `openat` (section 2.1).
    pub(crate) fn open_with_fopen(&mut self, path: Value, mode: OpenMode) -> Vec<Value> {
        let pointer = self.pointer;
        let mode = self.c_string(mode.fopen_mode());
        let fp = self.libc_call("fopen", &[pointer, pointer], &[pointer], &[path, mode]);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refused = self.builder.create_block();
        let opened = self.builder.create_block();
        let is_null = self.builder.ins().icmp_imm(IntCC::Equal, fp, 0);
        self.builder.ins().brif(is_null, refused, &[], opened, &[]);

        // `fopen` failed: the reason is `errno` as it stands, before anything
        // else is called.
        self.builder.switch_to_block(refused);
        self.builder.seal_block(refused);
        let reason = self.errno();
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        self.builder.ins().jump(merge, &[minus_one.into(), reason.into()]);

        self.builder.switch_to_block(opened);
        self.builder.seal_block(opened);
        let fd = self.libc_call("fileno", &[pointer], &[types::I32], &[fp]);
        let copy = self.libc_call("dup", &[types::I32], &[types::I32], &[fd]);
        // `dup` can fail (the process is out of descriptors); read `errno`
        // before `fclose` can change it.
        let reason = self.errno();
        self.libc_call("fclose", &[pointer], &[types::I32], &[fp]);
        let copy = self.builder.ins().sextend(types::I64, copy);
        self.builder.ins().jump(merge, &[copy.into(), reason.into()]);

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

    /// The descriptor behind a borrowed handle: the handle arrives as its
    /// address, one pointer leaf.
    fn file_fd(&mut self, handle: Value) -> Value {
        let fd = self.builder.ins().load(types::I64, MemFlags::trusted(), handle, 0);
        self.builder.ins().ireduce(types::I32, fd)
    }

    /// `Done`'s three leaves from a signed result: negative is `Failed` with
    /// `errno`, anything else is `Ok` with the value.
    fn done(&mut self, result: Value) -> Vec<Value> {
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, result, reason]
    }

    /// `flush_out(io)`: `fflush(stdout)`, then `ferror(stdout)`, as `Done`
    /// (`docs/checked-output.md`). The errno is read straight after
    /// `fflush`, before `ferror` can disturb it; when only the error
    /// indicator says a write failed, the original errno is gone and the
    /// answer is `EIO` (5).
    pub(crate) fn flush_out(&mut self) -> Vec<Value> {
        let pointer = self.pointer;
        let stream = self.module.declare_data_in_func(self.console.stdout, self.builder.func);
        let stream = self.builder.ins().global_value(pointer, stream);
        // `stdout` is a `FILE *` variable: the symbol is its address.
        let stream = self.builder.ins().load(pointer, MemFlags::trusted(), stream, 0);
        let flushed = self.libc_call("fflush", &[pointer], &[types::I32], &[stream]);
        let reason = self.errno();
        let indicator = self.libc_call("ferror", &[pointer], &[types::I32], &[stream]);
        let flush_failed = self.builder.ins().icmp_imm(IntCC::NotEqual, flushed, 0);
        let earlier = self.builder.ins().icmp_imm(IntCC::NotEqual, indicator, 0);
        let failed = self.builder.ins().bor(flush_failed, earlier);
        let eio = self.builder.ins().iconst(types::I64, 5);
        let reason = self.builder.ins().select(flush_failed, reason, eio);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, zero, reason]
    }

    /// `Read`'s three leaves from the byte count a `read`-shaped call gave.
    fn read_answer(&mut self, moved: Value) -> Vec<Value> {
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let empty = self.builder.ins().icmp_imm(IntCC::Equal, moved, 0);
        let two = self.builder.ins().iconst(types::I64, 2);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let not_failed = self.builder.ins().select(empty, one, zero);
        let tag = self.builder.ins().select(negative, two, not_failed);
        let reason = self.errno();
        vec![tag, moved, reason]
    }

    /// `file_write(file, bytes)`: one `write(2)`.
    pub(crate) fn file_write(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.file_fd(args[0]);
        let moved = self.libc_call(
            "write",
            &[types::I32, pointer, types::I64],
            &[types::I64],
            &[fd, args[1], args[2]],
        );
        self.done(moved)
    }

    /// `file_pwrite(file, at, bytes)`: one `pwrite(2)`.
    pub(crate) fn file_pwrite(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.file_fd(args[0]);
        let moved = self.libc_call(
            "pwrite",
            &[types::I32, pointer, types::I64, types::I64],
            &[types::I64],
            &[fd, args[2], args[3], args[1]],
        );
        self.done(moved)
    }

    /// `file_pread(file, at, into)`: one `pread(2)`, sorted into `Read`.
    pub(crate) fn file_pread(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.file_fd(args[0]);
        let moved = self.libc_call(
            "pread",
            &[types::I32, pointer, types::I64, types::I64],
            &[types::I64],
            &[fd, args[2], args[3], args[1]],
        );
        self.read_answer(moved)
    }

    /// `file_sync(file)`: `fsync(2)`. The answer is `0` or the `errno`.
    pub(crate) fn file_sync(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.file_fd(args[0]);
        let result = self.libc_call("fsync", &[types::I32], &[types::I32], &[fd]);
        let result = self.builder.ins().sextend(types::I64, result);
        self.done(result)
    }

    /// `file_truncate(file, len)`: `ftruncate(2)`.
    pub(crate) fn file_truncate(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.file_fd(args[0]);
        let result =
            self.libc_call("ftruncate", &[types::I32, types::I64], &[types::I32], &[fd, args[1]]);
        let result = self.builder.ins().sextend(types::I64, result);
        self.done(result)
    }

    /// `file_size(file)`: the end of the file by `lseek`, with the cursor put
    /// back where it was. `SEEK_SET`, `SEEK_CUR` and `SEEK_END` are 0, 1 and
    /// 2 on both targets. `fstat` is not used because `st_size` sits at a
    /// different offset on each (`docs/file-writes.md` section 4.3).
    pub(crate) fn file_size(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.file_fd(args[0]);
        let sig = [types::I32, types::I64, types::I32];
        let zero = self.builder.ins().iconst(types::I64, 0);
        let current = self.builder.ins().iconst(types::I32, 1);
        let end = self.builder.ins().iconst(types::I32, 2);
        let set = self.builder.ins().iconst(types::I32, 0);
        let here = self.libc_call("lseek", &sig, &[types::I64], &[fd, zero, current]);
        let size = self.libc_call("lseek", &sig, &[types::I64], &[fd, zero, end]);
        // The reason is read now: putting the cursor back is another call.
        let answer = self.done(size);
        let here_failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, here, 0);
        let restore_to = self.builder.ins().select(here_failed, zero, here);
        self.libc_call("lseek", &sig, &[types::I64], &[fd, restore_to, set]);
        // A cursor that could not be read is a failure too, with its own value.
        let tag = answer[0];
        let one = self.builder.ins().iconst(types::I64, 1);
        let tag = self.builder.ins().select(here_failed, one, tag);
        let value = self.builder.ins().select(here_failed, here, answer[1]);
        vec![tag, value, answer[2]]
    }

    /// `fs_remove(fs, path)` and `fs_rename(fs, from, to)`
    /// (`docs/file-writes.md` section 7): every path is checked against the
    /// prefix, `..` refused, then one `unlink(2)` or `rename(2)`. A rename
    /// whose destination is outside the prefix traps like any other path.
    pub(crate) fn path_op(&mut self, op: PathOp, prefix: &str, args: &[Expr]) -> Vec<Value> {
        let pointer = self.pointer;
        let first = self.expr(&args[1]);
        let first = self.checked_path(prefix, &first);
        let result = match op {
            PathOp::Remove => self.libc_call("unlink", &[pointer], &[types::I32], &[first]),
            PathOp::Rename => {
                let second = self.expr(&args[2]);
                let second = self.checked_path(prefix, &second);
                self.libc_call("rename", &[pointer, pointer], &[types::I32], &[first, second])
            }
        };
        let result = self.builder.ins().sextend(types::I64, result);
        self.done(result)
    }

    /// `file_lock(file)`: `flock(fd, LOCK_EX | LOCK_NB)`. `LOCK_EX` is 2 and
    /// `LOCK_NB` is 4, so 6, on Linux (read from its headers) and, from the
    /// BSD headers, on Darwin. Held by someone else it is `Failed(EWOULDBLOCK)`.
    pub(crate) fn file_lock(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = self.file_fd(args[0]);
        let how = self.builder.ins().iconst(types::I32, 6);
        let result = self.libc_call("flock", &[types::I32, types::I32], &[types::I32], &[fd, how]);
        let result = self.builder.ins().sextend(types::I64, result);
        self.done(result)
    }
}
