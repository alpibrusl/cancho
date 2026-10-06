//! Directory handles (`docs/directory-handles.md`): `open_dir`'s open, the
//! steps beneath a directory (`dir_enter`, `dir_open_read`, `dir_open_new`,
//! `dir_open_append`), the changes in one (`dir_rename`, `dir_remove`,
//! `dir_sync`) and `dir_close`. Mirrors `lex-sys-codegen`'s own
//! `body/dirs.rs`.
//!
//! Every name is one path component, checked first: empty, longer than
//! `NAME_MAX`, `.`, `..`, or holding a `/` or a NUL is `Failed(EINVAL)` with
//! no call, because `O_NOFOLLOW` refuses a link and nothing else (§1's probe
//! walked out of the directory with `..`).

use crate::*;

impl<'a> FuncEmitter<'a> {
    /// The `open` flags for this target, from the one table both backends
    /// share.
    pub(crate) fn open_flags(&self) -> lex_sys_ir::OpenFlags {
        let aarch64 = matches!(self.triple.architecture, target_lexicon::Architecture::Aarch64(_));
        lex_sys_ir::open_flags_for(self.file_os(), aarch64)
    }

    /// `openat(AT_FDCWD, path, flags, mode)`, the one way a path (rather
    /// than a name beneath a `Dir`) is opened: declared variadic and called
    /// as one, so `mode` reaches the callee wherever the target passes a
    /// variadic argument. `flags` carries `O_CLOEXEC` (`docs/processes.md`
    /// §4.5). Answers the `i32` result.
    pub(crate) fn open_at_cwd(&mut self, path: &str, flags: i64, mode: i64) -> String {
        let cwd = self.open_flags().at_fdcwd;
        let fd = self.fresh();
        self.out.push_str(&format!(
            "  {fd} = call i32 (i32, ptr, i32, ...) @openat(i32 {cwd}, ptr {path}, i32 {flags}, i32 {mode})\n"
        ));
        fd
    }

    /// `open_dir`'s open: `path` is already checked against the prefix and
    /// NUL-terminated. `openat(AT_FDCWD, path, O_RDONLY | O_DIRECTORY |
    /// O_CLOEXEC)`, as `open_read`'s is. `DirOpened`'s three leaves.
    pub(crate) fn open_directory(&mut self, path: &str) -> Vec<LValue> {
        let f = self.open_flags();
        let fd32 = self.open_at_cwd(path, f.read_only | f.directory | f.cloexec, 0);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = sext i32 {fd32} to i64\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let reason = self.errno();
        vec![LValue::Reg(tag), LValue::Reg(fd), reason]
    }

    /// Check one name and copy it, NUL-terminated, onto the stack. A name
    /// that is not one component branches to `refused`; otherwise emission
    /// continues in a fresh block and the copy's register comes back.
    fn component(&mut self, name: &str, length: &str, refused: &str) -> String {
        let copy = self.fresh();
        self.hoist(format!("  {copy} = alloca i8, i64 {}\n", lex_sys_ir::NAME_MAX + 1));
        let n = self.blocks;
        self.blocks += 1;
        let (body, next, second, good) = (
            format!("namebody{n}"),
            format!("namenext{n}"),
            format!("namesecond{n}"),
            format!("namegood{n}"),
        );

        // Empty or longer than `NAME_MAX`: refused before a byte is read.
        let empty = self.fresh();
        self.out.push_str(&format!("  {empty} = icmp eq i64 {length}, 0\n"));
        let long = self.fresh();
        self.out.push_str(&format!("  {long} = icmp sgt i64 {length}, {}\n", lex_sys_ir::NAME_MAX));
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = or i1 {empty}, {long}\n"));
        self.out.push_str(&format!("  br i1 {bad}, label %{refused}, label %{body}\n"));

        // A `/` or a NUL anywhere, or `.`; `..` reads its second byte in a
        // block of its own, so a one-byte name is never read past.
        self.out.push_str(&format!("{body}:\n"));
        let slash = self.fresh();
        self.out
            .push_str(&format!("  {slash} = call ptr @memchr(ptr {name}, i32 47, i64 {length})\n"));
        let has_slash = self.fresh();
        self.out.push_str(&format!("  {has_slash} = icmp ne ptr {slash}, null\n"));
        let nul = self.fresh();
        self.out
            .push_str(&format!("  {nul} = call ptr @memchr(ptr {name}, i32 0, i64 {length})\n"));
        let has_nul = self.fresh();
        self.out.push_str(&format!("  {has_nul} = icmp ne ptr {nul}, null\n"));
        let first = self.fresh();
        self.out.push_str(&format!("  {first} = load i8, ptr {name}\n"));
        let first_dot = self.fresh();
        self.out.push_str(&format!("  {first_dot} = icmp eq i8 {first}, 46\n"));
        let one_byte = self.fresh();
        self.out.push_str(&format!("  {one_byte} = icmp eq i64 {length}, 1\n"));
        let dot = self.fresh();
        self.out.push_str(&format!("  {dot} = and i1 {one_byte}, {first_dot}\n"));
        let either = self.fresh();
        self.out.push_str(&format!("  {either} = or i1 {has_slash}, {has_nul}\n"));
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = or i1 {either}, {dot}\n"));
        self.out.push_str(&format!("  br i1 {bad}, label %{refused}, label %{next}\n"));

        self.out.push_str(&format!("{next}:\n"));
        let two_bytes = self.fresh();
        self.out.push_str(&format!("  {two_bytes} = icmp eq i64 {length}, 2\n"));
        let maybe = self.fresh();
        self.out.push_str(&format!("  {maybe} = and i1 {two_bytes}, {first_dot}\n"));
        self.out.push_str(&format!("  br i1 {maybe}, label %{second}, label %{good}\n"));

        self.out.push_str(&format!("{second}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {name}, i64 1\n"));
        let other = self.fresh();
        self.out.push_str(&format!("  {other} = load i8, ptr {at}\n"));
        let dotdot = self.fresh();
        self.out.push_str(&format!("  {dotdot} = icmp eq i8 {other}, 46\n"));
        self.out.push_str(&format!("  br i1 {dotdot}, label %{refused}, label %{good}\n"));

        self.out.push_str(&format!("{good}:\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call ptr @memmove(ptr {copy}, ptr {name}, i64 {length})\n"
        ));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {copy}, i64 {length}\n"));
        self.out.push_str(&format!("  store i8 0, ptr {end}\n"));
        copy
    }

    /// Check every name in `names` (pointer and length each), then emit
    /// `call` on their copies; `call` answers an `i32` register, negative for
    /// a failure, and `errno` is read straight after it. The three leaves
    /// `Opened`, `DirOpened` and `Done` share come back.
    pub(crate) fn dir_call(
        &mut self,
        names: &[(String, String)],
        call: impl FnOnce(&mut Self, &[String]) -> String,
    ) -> Vec<LValue> {
        self.dir_call_mapping(names, None, call)
    }

    /// `dir_call`, and when `unsupported` is given, a kernel `EINVAL` from
    /// `call` is answered as that value instead. The name check's own
    /// `EINVAL` is not mapped, so the two stay apart.
    pub(crate) fn dir_call_mapping(
        &mut self,
        names: &[(String, String)],
        unsupported: Option<i64>,
        call: impl FnOnce(&mut Self, &[String]) -> String,
    ) -> Vec<LValue> {
        let result_cell = self.fresh();
        self.hoist(format!("  {result_cell} = alloca i64\n"));
        let reason_cell = self.fresh();
        self.hoist(format!("  {reason_cell} = alloca i64\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (refused, merge) = (format!("dirrefused{n}"), format!("dirmerge{n}"));

        let copies: Vec<String> =
            names.iter().map(|(name, length)| self.component(name, length, &refused)).collect();
        let result = call(self, &copies);
        let mut reason = self.errno();
        if let Some(unsupported) = unsupported {
            let invalid = self.fresh();
            self.out.push_str(&format!(
                "  {invalid} = icmp eq i64 {}, {}\n",
                operand(&reason),
                lex_sys_ir::EINVAL
            ));
            let mapped = self.fresh();
            self.out.push_str(&format!(
                "  {mapped} = select i1 {invalid}, i64 {unsupported}, i64 {}\n",
                operand(&reason)
            ));
            reason = LValue::Reg(mapped);
        }
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {result} to i64\n"));
        self.out.push_str(&format!("  store i64 {wide}, ptr {result_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{refused}:\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {result_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", lex_sys_ir::EINVAL));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = load i64, ptr {result_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {reason_cell}\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {result}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        vec![LValue::Reg(tag), LValue::Reg(result), LValue::Reg(reason)]
    }

    /// The descriptor behind a borrowed `Dir`: the handle arrives as its
    /// address.
    pub(crate) fn dir_fd(&mut self, handle: &str) -> String {
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = load i64, ptr {handle}\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {fd64} to i32\n"));
        fd
    }

    /// `dir_enter`, `dir_open_read`, `dir_open_new` and `dir_open_append`:
    /// `args` is the handle's address, then the name's pointer and length.
    /// `openat` is declared variadic and called as one, so its `mode` reaches
    /// the callee wherever the target passes a variadic argument.
    pub(crate) fn dir_open(&mut self, args: &[LValue], op: Builtin) -> Result<Vec<LValue>, String> {
        if args.len() != 3 {
            return Err(format!("`{}` needs 3 leaves but {} were given", op.name(), args.len()));
        }
        let f = self.open_flags();
        let (flags, mode) = match op {
            Builtin::DirEnter => (f.read_only | f.directory | f.nofollow, 0),
            Builtin::DirOpenNew => {
                (f.write_only | f.create | f.exclusive | f.nofollow, lex_sys_ir::CREATE_MODE)
            }
            Builtin::DirOpenAppend => {
                (f.write_only | f.create | f.append | f.nofollow, lex_sys_ir::CREATE_MODE)
            }
            _ => (f.read_only | f.nofollow, 0),
        };
        // Every descriptor a builtin opens is close-on-exec (`docs/processes.md` §4.5).
        let flags = flags | f.cloexec;
        let handle = operand(&args[0]);
        let name = (operand(&args[1]), operand(&args[2]));
        Ok(self.dir_call(&[name], |this, copies| {
            let fd = this.dir_fd(&handle);
            let opened = this.fresh();
            this.out.push_str(&format!(
                "  {opened} = call i32 (i32, ptr, i32, ...) @openat(i32 {fd}, ptr {}, i32 {flags}, i32 {mode})\n",
                copies[0]
            ));
            opened
        }))
    }

    /// `dir_rename(dir, from, to)`: `renameat` with both names in the one
    /// directory.
    pub(crate) fn dir_rename(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 5 {
            return Err(format!("`dir_rename` needs 5 leaves but {} were given", args.len()));
        }
        let handle = operand(&args[0]);
        let names =
            [(operand(&args[1]), operand(&args[2])), (operand(&args[3]), operand(&args[4]))];
        Ok(self.dir_call(&names, |this, copies| {
            let fd = this.dir_fd(&handle);
            let renamed = this.fresh();
            this.out.push_str(&format!(
                "  {renamed} = call i32 @renameat(i32 {fd}, ptr {}, i32 {fd}, ptr {})\n",
                copies[0], copies[1]
            ));
            renamed
        }))
    }

    /// `dir_rename_new(dir, from, to)`: the rename that refuses to replace.
    /// Linux `renameat2(RENAME_NOREPLACE)`, Darwin `renameatx_np(RENAME_EXCL)`;
    /// a filesystem without it is `rename_unsupported`, never a plain rename
    /// (`docs/directory-handles.md` §3, slice 4).
    pub(crate) fn dir_rename_new(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 5 {
            return Err(format!("`dir_rename_new` needs 5 leaves but {} were given", args.len()));
        }
        let handle = operand(&args[0]);
        let names =
            [(operand(&args[1]), operand(&args[2])), (operand(&args[3]), operand(&args[4]))];
        let darwin = self.is_darwin();
        let symbol = if darwin { "renameatx_np" } else { "renameat2" };
        let flag = lex_sys_ir::rename_no_replace(darwin);
        Ok(self.dir_call_mapping(&names, Some(lex_sys_ir::rename_unsupported(darwin)), |this, copies| {
            let fd = this.dir_fd(&handle);
            let renamed = this.fresh();
            this.out.push_str(&format!(
                "  {renamed} = call i32 @{symbol}(i32 {fd}, ptr {}, i32 {fd}, ptr {}, i32 {flag})\n",
                copies[0], copies[1]
            ));
            renamed
        }))
    }

    /// `dir_remove(dir, name)`: `unlinkat(dir, name, 0)`, which removes a
    /// link rather than what it points at.
    pub(crate) fn dir_remove(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 3 {
            return Err(format!("`dir_remove` needs 3 leaves but {} were given", args.len()));
        }
        let handle = operand(&args[0]);
        let name = (operand(&args[1]), operand(&args[2]));
        Ok(self.dir_call(&[name], |this, copies| {
            let fd = this.dir_fd(&handle);
            let removed = this.fresh();
            this.out.push_str(&format!(
                "  {removed} = call i32 @unlinkat(i32 {fd}, ptr {}, i32 0)\n",
                copies[0]
            ));
            removed
        }))
    }

    /// `dir_sync(dir)`: `fsync` on the directory, as `Done`.
    pub(crate) fn dir_sync(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let handle =
            args.first().map(operand).ok_or_else(|| "`dir_sync` needs its handle".to_owned())?;
        Ok(self.dir_call(&[], |this, _| {
            let fd = this.dir_fd(&handle);
            let synced = this.fresh();
            this.out.push_str(&format!("  {synced} = call i32 @fsync(i32 {fd})\n"));
            synced
        }))
    }

    /// `dir_close(dir)`: `close(2)`, its answer widened.
    pub(crate) fn dir_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let handle = args.first().ok_or_else(|| "`dir_close` needs its handle".to_owned())?;
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {} to i32\n", operand(handle)));
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = call i32 @close(i32 {fd})\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {answer} to i64\n"));
        Ok(vec![LValue::Reg(wide)])
    }
}
