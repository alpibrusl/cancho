//! `Fs` (`docs/filesystem.md` §3-4, `docs/file-handles.md`): `fs_read`/
//! `fs_write`, `open_read`, `file_read` and `file_close`.

use crate::*;

impl<'a> FuncEmitter<'a> {
    /// libc's `errno`, read through the per-thread accessor `emit.rs`
    /// declared -- `__errno_location` on glibc, `__error` on Darwin,
    /// both answering a pointer to it. Mirrors `cancho-codegen`'s own
    /// `errno` (`body/memory.rs`).
    pub(crate) fn errno(&mut self) -> LValue {
        let symbol = match self.triple.operating_system {
            target_lexicon::OperatingSystem::Darwin(_) => "__error",
            _ => "__errno_location",
        };
        let addr = self.fresh();
        self.out.push_str(&format!("  {addr} = call ptr @{symbol}()\n"));
        let raw32 = self.fresh();
        self.out.push_str(&format!("  {raw32} = load i32, ptr {addr}\n"));
        // WASI's `errno` numbers are unrelated to Linux's; a program sees one
        // numbering, so they are translated here, the one place a failure's
        // `errno` is read (`cancho_ir::WASI_ERRNO_TO_LINUX`, `docs/wasm.md`).
        let value32 = if self.file_os() == cancho_ir::Os::Wasi {
            let translated = self.fresh();
            self.out
                .push_str(&format!("  {translated} = call i32 @cancho_wasi_errno(i32 {raw32})\n"));
            translated
        } else {
            raw32
        };
        let value = self.fresh();
        self.out.push_str(&format!("  {value} = sext i32 {value32} to i64\n"));
        LValue::Reg(value)
    }

    /// Trap when byte `i` of a path or a host differs from byte `i` of the
    /// `length`-byte bound at `expected`, while `i` is inside the bound.
    pub(crate) fn check_against(
        &mut self,
        expected: &str,
        length: usize,
        i: &str,
        byte: &str,
    ) -> Result<(), String> {
        let escaped = self.differs_inside(expected, length, i, byte);
        self.trap_if(&escaped)
    }

    /// Does byte `i` of a path or a host differ from byte `i` of the `length`-byte bound at `expected`, while `i` is
    /// inside the bound? An `i1`, so the check can be a trap or an answer (`path_escapes`).
    ///
    /// The bound's byte is read at `i` only while `i` is inside it, and at 0
    /// otherwise. Reading it at every `i` read past the end of its global, and
    /// LLVM folds that load to `poison`: `false && poison` is still `poison`,
    /// and a branch on `poison` is undefined behaviour, so `-O2` deleted every
    /// path through a short operation under a narrowed capability -- a
    /// `fs_read("/tmp/q/f")` on `Fs("/tmp")` trapped (`docs/filesystem.md`
    /// §4). An empty bound contains everything, and its global has no byte
    /// to read at all, so there is nothing to compare.
    pub(crate) fn differs_inside(
        &mut self,
        expected: &str,
        length: usize,
        i: &str,
        byte: &str,
    ) -> String {
        if length == 0 {
            return "false".to_owned();
        }
        let inside = self.fresh();
        self.out.push_str(&format!("  {inside} = icmp ult i64 {i}, {length}\n"));
        let index = self.fresh();
        self.out.push_str(&format!("  {index} = select i1 {inside}, i64 {i}, i64 0\n"));
        let want_at = self.fresh();
        self.out
            .push_str(&format!("  {want_at} = getelementptr i8, ptr {expected}, i64 {index}\n"));
        let want = self.fresh();
        self.out.push_str(&format!("  {want} = load i8, ptr {want_at}\n"));
        let differs = self.fresh();
        self.out.push_str(&format!("  {differs} = icmp ne i8 {byte}, {want}\n"));
        let escaped = self.fresh();
        self.out.push_str(&format!("  {escaped} = and i1 {inside}, {differs}\n"));
        escaped
    }

    /// The path rule, once (`docs/filesystem.md` §1.1, §4, `docs/processes.md` §4.13): is the path (a pointer and a
    /// length) *outside* the bound `prefix`? An `i1`, 1 when it is outside. Outside is any of: 4096 bytes or more,
    /// shorter than the prefix, a `..` anywhere, bytes that differ from the prefix's while inside it, and (for a
    /// prefix that does not end in `/`) a path that goes on from it at a byte other than `/`.
    ///
    /// **Every operation and `exec_covers`/`fs_covers` go through here**: `checked_path` traps on the answer and the
    /// predicates return it, so what a program is told and what the runtime enforces cannot differ. With `buffer`,
    /// each byte is also copied there (the caller has already refused a path that does not fit). Mirrors
    /// `cancho-codegen`'s `path_escapes` (`body/memory.rs`); the loop is built the "no `phi`" way.
    pub(crate) fn path_escapes(
        &mut self,
        prefix: &str,
        path: &[LValue],
        buffer: Option<&str>,
    ) -> String {
        const PATH_MAX: i64 = 4096;
        let (source, length) = (operand(&path[0]), operand(&path[1]));

        // Room for the bytes and the NUL; a path shorter than the prefix cannot start with it.
        let too_long = self.fresh();
        self.out.push_str(&format!("  {too_long} = icmp uge i64 {length}, {PATH_MAX}\n"));
        let short = self.fresh();
        self.out.push_str(&format!("  {short} = icmp ult i64 {length}, {}\n", prefix.len()));
        let first = self.fresh();
        self.out.push_str(&format!("  {first} = or i1 {too_long}, {short}\n"));

        let expected = operand(&self.bytes_lit(prefix)[0]);
        let cursor = self.fresh();
        self.hoist(format!("  {cursor} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {cursor}\n"));
        let previous = self.fresh();
        self.hoist(format!("  {previous} = alloca i8\n"));
        self.out.push_str(&format!("  store i8 0, ptr {previous}\n"));
        let wrong = self.fresh();
        self.hoist(format!("  {wrong} = alloca i1\n"));
        self.out.push_str(&format!("  store i1 {first}, ptr {wrong}\n"));

        let n = self.blocks;
        self.blocks += 1;
        let (header, body, done) =
            (format!("pathhdr{n}"), format!("pathbody{n}"), format!("pathdone{n}"));
        self.out.push_str(&format!("  br label %{header}\n"));

        self.out.push_str(&format!("{header}:\n"));
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {cursor}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp ult i64 {i}, {length}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));

        self.out.push_str(&format!("{body}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {source}, i64 {i}\n"));
        let byte = self.fresh();
        self.out.push_str(&format!("  {byte} = load i8, ptr {at}\n"));
        if let Some(buffer) = buffer {
            let into = self.fresh();
            self.out.push_str(&format!("  {into} = getelementptr i8, ptr {buffer}, i64 {i}\n"));
            self.out.push_str(&format!("  store i8 {byte}, ptr {into}\n"));
        }

        // `..` is refused rather than normalised (`docs/filesystem.md`
        // §4.1): normalising is a security function with its own
        // design, and a prefix check that quietly admitted
        // `/tmp/../etc` would be the dishonest third option.
        let dot = self.fresh();
        self.out.push_str(&format!("  {dot} = icmp eq i8 {byte}, 46\n"));
        let prior = self.fresh();
        self.out.push_str(&format!("  {prior} = load i8, ptr {previous}\n"));
        let prior_dot = self.fresh();
        self.out.push_str(&format!("  {prior_dot} = icmp eq i8 {prior}, 46\n"));
        let traversal = self.fresh();
        self.out.push_str(&format!("  {traversal} = and i1 {dot}, {prior_dot}\n"));
        self.out.push_str(&format!("  store i8 {byte}, ptr {previous}\n"));

        // Inside the prefix, the bytes have to match: a path outside what
        // the capability granted.
        let differs = self.differs_inside(&expected, prefix.len(), &i, &byte);
        let this = self.fresh();
        self.out.push_str(&format!("  {this} = or i1 {traversal}, {differs}\n"));
        let so_far = self.fresh();
        self.out.push_str(&format!("  {so_far} = load i1, ptr {wrong}\n"));
        let all = self.fresh();
        self.out.push_str(&format!("  {all} = or i1 {so_far}, {this}\n"));
        self.out.push_str(&format!("  store i1 {all}, ptr {wrong}\n"));

        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i64 {i}, 1\n"));
        self.out.push_str(&format!("  store i64 {next}, ptr {cursor}\n"));
        self.out.push_str(&format!("  br label %{header}\n"));

        self.out.push_str(&format!("{done}:\n"));
        let wrong_so_far = self.fresh();
        self.out.push_str(&format!("  {wrong_so_far} = load i1, ptr {wrong}\n"));

        // The bytes matching is not yet the same as being inside the
        // directory: `/tmp` does not contain `/tmpevil`. The path
        // either *is* the granted name or continues from it at a `/` (a
        // NUL there ends the C string at the prefix, so it is the prefix).
        // The byte is read only from a path that has one: a shorter path
        // has been marked outside already, and may be a pointer to nothing.
        if prefix.is_empty() || prefix.ends_with('/') {
            return wrong_so_far;
        }
        let longer = self.fresh();
        self.out.push_str(&format!("  {longer} = icmp ugt i64 {length}, {}\n", prefix.len()));
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i8\n"));
        self.out.push_str(&format!("  store i8 0, ptr {cell}\n"));
        let m = self.blocks;
        self.blocks += 1;
        let (read, join) = (format!("pathread{m}"), format!("pathjoin{m}"));
        self.out.push_str(&format!("  br i1 {longer}, label %{read}, label %{join}\n"));
        self.out.push_str(&format!("{read}:\n"));
        let beyond = self.fresh();
        self.out.push_str(&format!(
            "  {beyond} = getelementptr i8, ptr {source}, i64 {}\n",
            prefix.len()
        ));
        let next_byte = self.fresh();
        self.out.push_str(&format!("  {next_byte} = load i8, ptr {beyond}\n"));
        self.out.push_str(&format!("  store i8 {next_byte}, ptr {cell}\n"));
        self.out.push_str(&format!("  br label %{join}\n"));
        self.out.push_str(&format!("{join}:\n"));
        let byte = self.fresh();
        self.out.push_str(&format!("  {byte} = load i8, ptr {cell}\n"));
        let separator = self.fresh();
        self.out.push_str(&format!("  {separator} = icmp eq i8 {byte}, 47\n"));
        let ended = self.fresh();
        self.out.push_str(&format!("  {ended} = icmp eq i8 {byte}, 0\n"));
        let boundary = self.fresh();
        self.out.push_str(&format!("  {boundary} = or i1 {separator}, {ended}\n"));
        let across = self.fresh();
        self.out.push_str(&format!("  {across} = xor i1 {boundary}, true\n"));
        let outside = self.fresh();
        self.out.push_str(&format!("  {outside} = or i1 {wrong_so_far}, {across}\n"));
        outside
    }

    /// The path of a file operation, NUL-terminated on the stack and checked against `prefix`; a path that is not
    /// inside it traps (`path_escapes` is the rule, `docs/filesystem.md` §4). A longer path traps rather than being
    /// cut short, because a silently truncated path names a different file (`docs/strings.md` §6).
    pub(crate) fn checked_path(&mut self, prefix: &str, path: &[LValue]) -> Result<String, String> {
        const PATH_MAX: i64 = 4096;
        let length = operand(&path[1]);

        // The copy has 4096 bytes, so a longer path is refused before it starts.
        let too_long = self.fresh();
        self.out.push_str(&format!("  {too_long} = icmp uge i64 {length}, {PATH_MAX}\n"));
        self.trap_if(&too_long)?;

        let buffer = self.fresh();
        self.hoist(format!("  {buffer} = alloca i8, i64 {PATH_MAX}\n"));
        let escapes = self.path_escapes(prefix, path, Some(&buffer));
        self.trap_if(&escapes)?;
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {buffer}, i64 {length}\n"));
        self.out.push_str(&format!("  store i8 0, ptr {end}\n"));
        Ok(buffer)
    }

    /// `exec_covers(exec, path)` and `fs_covers(fs, path)` (`docs/processes.md` §4.13): `path_escapes`' answer, not,
    /// as this backend's `bool` leaf (`i8`).
    pub(crate) fn path_covers(
        &mut self,
        prefix: &str,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        let path = self.expr(&args[1])?;
        let escapes = self.path_escapes(prefix, &path, None);
        let covers = self.fresh();
        self.out.push_str(&format!("  {covers} = xor i1 {escapes}, true\n"));
        let widened = self.fresh();
        self.out.push_str(&format!("  {widened} = zext i1 {covers} to i8\n"));
        Ok(vec![LValue::Reg(widened)])
    }

    /// `fs_read`/`fs_write` (`docs/filesystem.md` §3). Both open with
    /// `openat(AT_FDCWD, path, flags, mode)` and `O_CLOEXEC`
    /// (`docs/processes.md` §4.5): `O_WRONLY|O_CREAT|O_TRUNC` and `0644`
    /// to write, which is what `creat` was, and `O_RDONLY` to read.
    /// `openat` is declared variadic and called as one, so `mode` reaches
    /// the callee on Apple ARM64 too, where a variadic argument is passed
    /// on the stack (`docs/filesystem.md` §2.2).
    pub(crate) fn file_op(
        &mut self,
        write: bool,
        prefix: &str,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        let path = self.expr(&args[1])?;
        let bytes = self.expr(&args[2])?;
        let path = self.checked_path(prefix, &path)?;

        let f = self.open_flags();
        let (flags, mode) = if write {
            (f.write_only | f.create | f.truncate | f.cloexec, cancho_ir::CREATE_MODE)
        } else {
            (f.read_only | f.cloexec, 0)
        };
        let fd = self.open_at_cwd(&path, flags, mode);

        let result_cell = self.fresh();
        self.hoist(format!("  {result_cell} = alloca i64\n"));

        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = icmp slt i32 {fd}, 0\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (failed, opened, merge) =
            (format!("fileopfailed{n}"), format!("fileopopened{n}"), format!("fileopmerge{n}"));
        self.out.push_str(&format!("  br i1 {bad}, label %{failed}, label %{opened}\n"));

        self.out.push_str(&format!("{failed}:\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {result_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{opened}:\n"));
        let name = if write { "write" } else { "read" };
        let st = self.size_ty();
        let size = self.size_arg(&operand(&bytes[1]));
        let raw = self.fresh();
        self.out.push_str(&format!(
            "  {raw} = call {st} @{name}(i32 {fd}, ptr {}, {st} {size})\n",
            operand(&bytes[0])
        ));
        let moved = self.size_result(&raw, true);
        self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  store i64 {moved}, ptr {result_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = load i64, ptr {result_cell}\n"));
        Ok(vec![LValue::Reg(result)])
    }

    /// `open_read(fs, path)` (`docs/file-handles.md` §2.1): the first
    /// half of [`Self::file_op`] and then it stops -- the same prefix
    /// check, the same `openat(AT_FDCWD, path, O_RDONLY|O_CLOEXEC)`, and the
    /// descriptor *kept* rather than spent on one transfer and closed.
    /// What comes back is `Opened`'s three leaves: the tag, `Ok`'s
    /// descriptor, `Failed`'s reason.
    pub(crate) fn open_file(
        &mut self,
        prefix: &str,
        mode: cancho_ir::OpenMode,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        let path = self.expr(&args[1])?;
        let path = self.checked_path(prefix, &path)?;
        if mode == cancho_ir::OpenMode::Directory {
            return Ok(self.open_directory(&path));
        }
        if mode != cancho_ir::OpenMode::Read {
            // WASI has no `dup`, which `fopen`'s bridge needs, so it opens with
            // `openat` and the mode's own flags (`OpenMode::open_flags`).
            if self.file_os() == cancho_ir::Os::Wasi {
                return Ok(self.open_with_openat(&path, mode));
            }
            return Ok(self.open_with_fopen(&path, mode));
        }

        let flags = self.open_flags();
        let fd32 = self.open_at_cwd(&path, flags.read_only | flags.cloexec, 0);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = sext i32 {fd32} to i64\n"));

        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let reason = self.errno();
        Ok(vec![LValue::Reg(tag), LValue::Reg(fd), reason])
    }

    /// `file_read(file, into)` (`docs/file-handles.md` §3): one
    /// `read(2)`, its three outcomes sorted into `Read`'s three
    /// constructors. `args[0]` is the handle's own address -- a
    /// reference, one pointer leaf, so the descriptor is a load rather
    /// than the value itself, unlike `file_close` below.
    pub(crate) fn read_file(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = load i64, ptr {}\n", operand(&args[0])));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {fd64} to i32\n"));

        let st = self.size_ty();
        let want = self.size_arg(&operand(&args[2]));
        let raw = self.fresh();
        self.out.push_str(&format!(
            "  {raw} = call {st} @read(i32 {fd}, ptr {}, {st} {want})\n",
            operand(&args[1])
        ));
        let moved = self.size_result(&raw, true);

        let negative = self.fresh();
        self.out.push_str(&format!("  {negative} = icmp slt i64 {moved}, 0\n"));
        let empty = self.fresh();
        self.out.push_str(&format!("  {empty} = icmp eq i64 {moved}, 0\n"));
        let not_failed = self.fresh();
        self.out.push_str(&format!("  {not_failed} = select i1 {empty}, i64 1, i64 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {negative}, i64 2, i64 {not_failed}\n"));
        let reason = self.errno();
        Ok(vec![LValue::Reg(tag), LValue::Reg(moved), reason])
    }
}
