//! The write side of a file handle (`docs/file-writes.md`): the opens that
//! append, create and update, and the verbs that write, sync and cut.
//! Mirrors `lex-sys-codegen`'s own `body/files.rs`. The read side stays in
//! `fs.rs`.

use crate::*;
use lex_sys_ir::{OpenMode, PathOp};

impl<'a> FuncEmitter<'a> {
    /// `Done`'s three leaves from a signed 64-bit result: negative is
    /// `Failed` with `errno`, anything else is `Ok` with the value.
    fn done(&mut self, result: &str) -> Vec<LValue> {
        let reason = self.errno();
        let negative = self.fresh();
        self.out.push_str(&format!("  {negative} = icmp slt i64 {result}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {negative}, i64 1, i64 0\n"));
        vec![LValue::Reg(tag), LValue::Reg(result.to_owned()), reason]
    }

    /// `flush_out(io)`: `fflush(stdout)`, then `ferror(stdout)`, as `Done`
    /// (`docs/checked-output.md`). Mirrors `lex-sys-codegen`'s own
    /// `flush_out`: the errno is read straight after `fflush`, and when only
    /// the error indicator says a write failed the answer is `EIO` (5).
    pub(crate) fn flush_out(&mut self) -> Vec<LValue> {
        let symbol = match self.triple.operating_system {
            target_lexicon::OperatingSystem::Darwin(_) => "__stdoutp",
            _ => "stdout",
        };
        let stream = self.fresh();
        self.out.push_str(&format!("  {stream} = load ptr, ptr @{symbol}\n"));
        let flushed = self.fresh();
        self.out.push_str(&format!("  {flushed} = call i32 @fflush(ptr {stream})\n"));
        let reason = self.errno();
        let indicator = self.fresh();
        self.out.push_str(&format!("  {indicator} = call i32 @ferror(ptr {stream})\n"));
        let flush_failed = self.fresh();
        self.out.push_str(&format!("  {flush_failed} = icmp ne i32 {flushed}, 0\n"));
        let earlier = self.fresh();
        self.out.push_str(&format!("  {earlier} = icmp ne i32 {indicator}, 0\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = or i1 {flush_failed}, {earlier}\n"));
        let why = self.fresh();
        self.out.push_str(&format!(
            "  {why} = select i1 {flush_failed}, i64 {}, i64 5\n",
            operand(&reason)
        ));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        vec![LValue::Reg(tag), LValue::Const(0), LValue::Reg(why)]
    }

    fn widen(&mut self, narrow: &str) -> String {
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {narrow} to i64\n"));
        wide
    }

    /// Open `path` (checked, NUL-terminated) in a mode that is not `Read`,
    /// answering `Opened`'s three leaves. `fopen` has a mode string for each
    /// way a log opens a file and is not variadic; the descriptor is taken
    /// with `dup` and the `FILE` closed, which keeps `O_APPEND` and the
    /// access mode (one open file description) and leaks nothing
    /// (`docs/file-writes.md` section 3). A bridge until a libc-free runtime
    /// (section 2.1).
    pub(crate) fn open_with_fopen(&mut self, path: &str, mode: OpenMode) -> Vec<LValue> {
        let text = mode.fopen_mode();
        let mode_cell = self.fresh();
        self.hoist(format!("  {mode_cell} = alloca i8, i64 {}\n", text.len() + 1));
        for (i, byte) in text.bytes().chain(std::iter::once(0)).enumerate() {
            let at = self.fresh();
            self.out.push_str(&format!("  {at} = getelementptr i8, ptr {mode_cell}, i64 {i}\n"));
            self.out.push_str(&format!("  store i8 {byte}, ptr {at}\n"));
        }
        let fd_cell = self.fresh();
        self.hoist(format!("  {fd_cell} = alloca i64\n"));
        let reason_cell = self.fresh();
        self.hoist(format!("  {reason_cell} = alloca i64\n"));

        let fp = self.fresh();
        self.out.push_str(&format!("  {fp} = call ptr @fopen(ptr {path}, ptr {mode_cell})\n"));
        let null = self.fresh();
        self.out.push_str(&format!("  {null} = icmp eq ptr {fp}, null\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (refused, opened, merge) =
            (format!("fopenrefused{n}"), format!("fopenopened{n}"), format!("fopenmerge{n}"));
        self.out.push_str(&format!("  br i1 {null}, label %{refused}, label %{opened}\n"));

        // `fopen` failed: the reason is `errno` as it stands.
        self.out.push_str(&format!("{refused}:\n"));
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 -1, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{opened}:\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = call i32 @fileno(ptr {fp})\n"));
        let copy = self.fresh();
        self.out.push_str(&format!("  {copy} = call i32 @dup(i32 {fd})\n"));
        // `dup` can fail; read `errno` before `fclose` can change it.
        let reason = self.errno();
        self.out.push_str(&format!("  call i32 @fclose(ptr {fp})\n"));
        let copy = self.widen(&copy);
        self.out.push_str(&format!("  store i64 {copy}, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = load i64, ptr {fd_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {reason_cell}\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        vec![LValue::Reg(tag), LValue::Reg(fd), LValue::Reg(reason)]
    }

    /// `Read`'s three leaves from the byte count of a `read`-shaped call.
    fn read_answer(&mut self, moved: &str) -> Vec<LValue> {
        let negative = self.fresh();
        self.out.push_str(&format!("  {negative} = icmp slt i64 {moved}, 0\n"));
        let empty = self.fresh();
        self.out.push_str(&format!("  {empty} = icmp eq i64 {moved}, 0\n"));
        let not_failed = self.fresh();
        self.out.push_str(&format!("  {not_failed} = select i1 {empty}, i64 1, i64 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {negative}, i64 2, i64 {not_failed}\n"));
        let reason = self.errno();
        vec![LValue::Reg(tag), LValue::Reg(moved.to_owned()), reason]
    }

    /// `file_write(file, bytes)`: one `write(2)`. `args` is the handle's
    /// address, then the slice's pointer and length.
    pub(crate) fn file_write(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call i64 @write(i32 {fd}, ptr {}, i64 {})\n",
            operand(&args[1]),
            operand(&args[2])
        ));
        Ok(self.done(&moved))
    }

    /// `file_pwrite(file, at, bytes)`: one `pwrite(2)`.
    pub(crate) fn file_pwrite(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call i64 @pwrite(i32 {fd}, ptr {}, i64 {}, i64 {})\n",
            operand(&args[2]),
            operand(&args[3]),
            operand(&args[1])
        ));
        Ok(self.done(&moved))
    }

    /// `file_pread(file, at, into)`: one `pread(2)`, sorted into `Read`.
    pub(crate) fn file_pread(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call i64 @pread(i32 {fd}, ptr {}, i64 {}, i64 {})\n",
            operand(&args[2]),
            operand(&args[3]),
            operand(&args[1])
        ));
        Ok(self.read_answer(&moved))
    }

    /// `file_sync(file)`: `fsync(2)`.
    pub(crate) fn file_sync(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = call i32 @fsync(i32 {fd})\n"));
        let result = self.widen(&result);
        Ok(self.done(&result))
    }

    /// `file_truncate(file, len)`: `ftruncate(2)`.
    pub(crate) fn file_truncate(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @ftruncate(i32 {fd}, i64 {})\n",
            operand(&args[1])
        ));
        let result = self.widen(&result);
        Ok(self.done(&result))
    }

    /// `file_size(file)`: the end by `lseek`, the cursor put back. `SEEK_*`
    /// are 0, 1, 2 on both targets (`docs/file-writes.md` section 4.3).
    pub(crate) fn file_size(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let here = self.fresh();
        self.out.push_str(&format!("  {here} = call i64 @lseek(i32 {fd}, i64 0, i32 1)\n"));
        let size = self.fresh();
        self.out.push_str(&format!("  {size} = call i64 @lseek(i32 {fd}, i64 0, i32 2)\n"));
        // The reason is read now: putting the cursor back is another call.
        let answer = self.done(&size);
        let here_failed = self.fresh();
        self.out.push_str(&format!("  {here_failed} = icmp slt i64 {here}, 0\n"));
        let restore_to = self.fresh();
        self.out
            .push_str(&format!("  {restore_to} = select i1 {here_failed}, i64 0, i64 {here}\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call i64 @lseek(i32 {fd}, i64 {restore_to}, i32 0)\n"
        ));
        // A cursor that could not be read is a failure too, with its own value.
        let tag = self.fresh();
        self.out.push_str(&format!(
            "  {tag} = select i1 {here_failed}, i64 1, i64 {}\n",
            operand(&answer[0])
        ));
        let value = self.fresh();
        self.out.push_str(&format!(
            "  {value} = select i1 {here_failed}, i64 {here}, i64 {}\n",
            operand(&answer[1])
        ));
        Ok(vec![LValue::Reg(tag), LValue::Reg(value), answer[2].clone()])
    }

    /// `fs_remove(fs, path)` and `fs_rename(fs, from, to)`
    /// (`docs/file-writes.md` section 7): every path checked against the
    /// prefix, then one `unlink(2)` or `rename(2)`.
    pub(crate) fn path_op(
        &mut self,
        op: PathOp,
        prefix: &str,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        let first = self.expr(&args[1])?;
        let first = self.checked_path(prefix, &first)?;
        let result = self.fresh();
        match op {
            PathOp::Remove => {
                self.out.push_str(&format!("  {result} = call i32 @unlink(ptr {first})\n"));
            }
            PathOp::Rename => {
                let second = self.expr(&args[2])?;
                let second = self.checked_path(prefix, &second)?;
                self.out.push_str(&format!(
                    "  {result} = call i32 @rename(ptr {first}, ptr {second})\n"
                ));
            }
        }
        let result = self.widen(&result);
        Ok(self.done(&result))
    }

    /// `file_lock(file)`: `flock(fd, LOCK_EX | LOCK_NB)`, which is 6 on both targets.
    pub(crate) fn file_lock(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = call i32 @flock(i32 {fd}, i32 6)\n"));
        let result = self.widen(&result);
        Ok(self.done(&result))
    }
}
