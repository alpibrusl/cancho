//! Directory handles (`docs/directory-handles.md`): `open_dir`'s open,
//! `dir_enter`, `dir_open_read` and `dir_close`. Mirrors
//! `lex-sys-codegen`'s own `body/dirs.rs`.
//!
//! A step beneath a directory is one `openat(dir, name, flags | O_NOFOLLOW)`
//! on one path component, checked first: empty, longer than `NAME_MAX`, `.`,
//! `..`, or holding a `/` or a NUL is `Failed(EINVAL)` with no call, because
//! `O_NOFOLLOW` refuses a link and nothing else (§1's probe walked out of the
//! directory with `..`).

use crate::*;

impl<'a> FuncEmitter<'a> {
    /// `(O_DIRECTORY, O_NOFOLLOW)` for this target, from the one table both
    /// backends share.
    fn directory_flags(&self) -> (i64, i64) {
        let aarch64 = matches!(self.triple.architecture, target_lexicon::Architecture::Aarch64(_));
        lex_sys_ir::directory_flags(self.is_darwin(), aarch64)
    }

    /// `open_dir`'s open: `path` is already checked against the prefix and
    /// NUL-terminated. `open(path, O_RDONLY | O_DIRECTORY)`, two fixed
    /// arguments as `open_read`'s is. `DirOpened`'s three leaves.
    pub(crate) fn open_directory(&mut self, path: &str) -> Vec<LValue> {
        let (directory, _) = self.directory_flags();
        let fd32 = self.fresh();
        self.out.push_str(&format!("  {fd32} = call i32 @open(ptr {path}, i32 {directory})\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = sext i32 {fd32} to i64\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let reason = self.errno();
        vec![LValue::Reg(tag), LValue::Reg(fd), reason]
    }

    /// `dir_enter(dir, name)` and `dir_open_read(dir, name)`: `args` is the
    /// handle's address, then the name's pointer and length. The tag, the
    /// descriptor and the reason -- the leaves `DirOpened` and `Opened` share.
    pub(crate) fn dir_open(
        &mut self,
        args: &[LValue],
        directory: bool,
    ) -> Result<Vec<LValue>, String> {
        if args.len() != 3 {
            return Err(format!("a `dir_*` open needs 3 leaves but {} were given", args.len()));
        }
        let (handle, name, length) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let fd_cell = self.fresh();
        self.hoist(format!("  {fd_cell} = alloca i64\n"));
        let reason_cell = self.fresh();
        self.hoist(format!("  {reason_cell} = alloca i64\n"));
        let copy = self.fresh();
        self.hoist(format!("  {copy} = alloca i8, i64 {}\n", lex_sys_ir::NAME_MAX + 1));

        let n = self.blocks;
        self.blocks += 1;
        let (body, next, second, call, refused, merge) = (
            format!("dirbody{n}"),
            format!("dirnext{n}"),
            format!("dirsecond{n}"),
            format!("dircall{n}"),
            format!("dirrefused{n}"),
            format!("dirmerge{n}"),
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
        self.out.push_str(&format!("  br i1 {maybe}, label %{second}, label %{call}\n"));

        self.out.push_str(&format!("{second}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {name}, i64 1\n"));
        let other = self.fresh();
        self.out.push_str(&format!("  {other} = load i8, ptr {at}\n"));
        let dotdot = self.fresh();
        self.out.push_str(&format!("  {dotdot} = icmp eq i8 {other}, 46\n"));
        self.out.push_str(&format!("  br i1 {dotdot}, label %{refused}, label %{call}\n"));

        // The name, NUL-terminated on the stack, then one `openat` with its
        // three fixed arguments: `mode` is read only with `O_CREAT`.
        self.out.push_str(&format!("{call}:\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call ptr @memmove(ptr {copy}, ptr {name}, i64 {length})\n"
        ));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {copy}, i64 {length}\n"));
        self.out.push_str(&format!("  store i8 0, ptr {end}\n"));
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = load i64, ptr {handle}\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {fd64} to i32\n"));
        let (o_directory, o_nofollow) = self.directory_flags();
        let flags = if directory { o_directory | o_nofollow } else { o_nofollow };
        let opened = self.fresh();
        self.out.push_str(&format!(
            "  {opened} = call i32 @openat(i32 {fd}, ptr {copy}, i32 {flags})\n"
        ));
        let reason = self.errno();
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {opened} to i64\n"));
        self.out.push_str(&format!("  store i64 {wide}, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{refused}:\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", lex_sys_ir::EINVAL));
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
        Ok(vec![LValue::Reg(tag), LValue::Reg(fd), LValue::Reg(reason)])
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
