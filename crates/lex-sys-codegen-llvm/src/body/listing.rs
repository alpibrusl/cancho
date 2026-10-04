//! Directory listing (`docs/directory-listing.md`): `dir_list`, `dir_next`
//! and `dir_list_close`. Mirrors `lex-sys-codegen`'s own `body/listing.rs`.
//!
//! A listing is libc's `DIR` stream on a descriptor of its own --
//! `openat(dir, ".", O_RDONLY | O_DIRECTORY)`, not a `dup`, so two listings of
//! one `Dir` do not share a position -- and `struct dirent` is read at the
//! offsets `lex_sys_ir::dirent_layout` gives for the target (§3.4). Branches
//! join through `alloca` cells, as everywhere in this backend.

use crate::*;

impl<'a> FuncEmitter<'a> {
    /// The address of `errno` for this target.
    fn errno_address(&mut self) -> String {
        let symbol = if self.is_darwin() { "__error" } else { "__errno_location" };
        let addr = self.fresh();
        self.out.push_str(&format!("  {addr} = call ptr @{symbol}()\n"));
        addr
    }

    /// `count` `i64` cells, for the leaves a join hands on.
    fn cells(&mut self, count: usize) -> Vec<String> {
        (0..count)
            .map(|_| {
                let cell = self.fresh();
                self.hoist(format!("  {cell} = alloca i64\n"));
                cell
            })
            .collect()
    }

    fn store_all(&mut self, cells: &[String], values: &[&str]) {
        for (cell, value) in cells.iter().zip(values) {
            self.out.push_str(&format!("  store i64 {value}, ptr {cell}\n"));
        }
    }

    fn load_all(&mut self, cells: &[String]) -> Vec<LValue> {
        cells
            .iter()
            .map(|cell| {
                let value = self.fresh();
                self.out.push_str(&format!("  {value} = load i64, ptr {cell}\n"));
                LValue::Reg(value)
            })
            .collect()
    }

    /// `dir_list(dir)`: `args` is the handle's address. `Listing`'s three
    /// leaves: the tag, the stream and the reason.
    pub(crate) fn dir_list(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let handle =
            args.first().map(operand).ok_or_else(|| "`dir_list` needs its handle".to_owned())?;
        let directory = self.open_flags().directory;
        let cells = self.cells(3);
        let dot = self.fresh();
        self.hoist(format!("  {dot} = alloca [2 x i8]\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (opened, refused, started, merge) = (
            format!("listopened{n}"),
            format!("listrefused{n}"),
            format!("liststarted{n}"),
            format!("listmerge{n}"),
        );

        self.out.push_str(&format!("  store i8 46, ptr {dot}\n"));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {dot}, i64 1\n"));
        self.out.push_str(&format!("  store i8 0, ptr {end}\n"));
        let fd = self.dir_fd(&handle);
        let own = self.fresh();
        self.out.push_str(&format!(
            "  {own} = call i32 (i32, ptr, i32, ...) @openat(i32 {fd}, ptr {dot}, i32 {directory}, i32 0)\n"
        ));
        let reason = self.errno();
        self.store_all(&cells, &["1", "0", &operand(&reason)]);
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {own}, 0\n"));
        self.out.push_str(&format!("  br i1 {failed}, label %{merge}, label %{opened}\n"));

        // The stream takes the descriptor; if it cannot, the descriptor is
        // closed here, after `errno` is read.
        self.out.push_str(&format!("{opened}:\n"));
        let stream = self.fresh();
        self.out.push_str(&format!("  {stream} = call ptr @fdopendir(i32 {own})\n"));
        let reason = self.errno();
        let null = self.fresh();
        self.out.push_str(&format!("  {null} = icmp eq ptr {stream}, null\n"));
        self.out.push_str(&format!("  br i1 {null}, label %{refused}, label %{started}\n"));

        self.out.push_str(&format!("{refused}:\n"));
        self.store_all(&cells, &["1", "0", &operand(&reason)]);
        let ignored = self.fresh();
        self.out.push_str(&format!("  {ignored} = call i32 @close(i32 {own})\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{started}:\n"));
        let address = self.fresh();
        self.out.push_str(&format!("  {address} = ptrtoint ptr {stream} to i64\n"));
        self.store_all(&cells, &["0", &address, "0"]);
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        Ok(self.load_all(&cells))
    }

    /// `dir_next(list, name)`: `args` is the listing's address, then the
    /// buffer's pointer and length. `Listed`'s four leaves: the tag (`Name`
    /// 0, `End` 1, `Failed` 2), the length, the kind and the reason.
    pub(crate) fn dir_next(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 3 {
            return Err(format!("`dir_next` needs 3 leaves but {} were given", args.len()));
        }
        let (handle, buffer, room) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let layout = lex_sys_ir::dirent_layout(self.is_darwin());
        let cells = self.cells(4);
        let n = self.blocks;
        self.blocks += 1;
        let (step, ended, entry, second, keep, short, copy, merge) = (
            format!("liststep{n}"),
            format!("listended{n}"),
            format!("listentry{n}"),
            format!("listsecond{n}"),
            format!("listkeep{n}"),
            format!("listshort{n}"),
            format!("listcopy{n}"),
            format!("listnext{n}"),
        );

        let address = self.fresh();
        self.out.push_str(&format!("  {address} = load i64, ptr {handle}\n"));
        let stream = self.fresh();
        self.out.push_str(&format!("  {stream} = inttoptr i64 {address} to ptr\n"));
        self.out.push_str(&format!("  br label %{step}\n"));

        // One `readdir`, `errno` cleared first.
        self.out.push_str(&format!("{step}:\n"));
        let errno = self.errno_address();
        self.out.push_str(&format!("  store i32 0, ptr {errno}\n"));
        let found = self.fresh();
        self.out.push_str(&format!("  {found} = call ptr @readdir(ptr {stream})\n"));
        let null = self.fresh();
        self.out.push_str(&format!("  {null} = icmp eq ptr {found}, null\n"));
        self.out.push_str(&format!("  br i1 {null}, label %{ended}, label %{entry}\n"));

        // Null: the end if `errno` is still zero, a failure otherwise.
        self.out.push_str(&format!("{ended}:\n"));
        let reason = self.errno();
        let reason = operand(&reason);
        let quiet = self.fresh();
        self.out.push_str(&format!("  {quiet} = icmp eq i64 {reason}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {quiet}, i64 1, i64 2\n"));
        self.store_all(&cells, &[&tag, "0", "0", &reason]);
        self.out.push_str(&format!("  br label %{merge}\n"));

        // `.` and `..` are skipped. The name is NUL-terminated, so its second
        // byte is always there to read.
        self.out.push_str(&format!("{entry}:\n"));
        let name = self.fresh();
        self.out.push_str(&format!(
            "  {name} = getelementptr i8, ptr {found}, i64 {}\n",
            layout.d_name
        ));
        let length = self.fresh();
        self.out.push_str(&format!("  {length} = call i64 @strlen(ptr {name})\n"));
        let first = self.fresh();
        self.out.push_str(&format!("  {first} = load i8, ptr {name}\n"));
        let first_dot = self.fresh();
        self.out.push_str(&format!("  {first_dot} = icmp eq i8 {first}, 46\n"));
        let one_byte = self.fresh();
        self.out.push_str(&format!("  {one_byte} = icmp eq i64 {length}, 1\n"));
        let dot = self.fresh();
        self.out.push_str(&format!("  {dot} = and i1 {one_byte}, {first_dot}\n"));
        let two_bytes = self.fresh();
        self.out.push_str(&format!("  {two_bytes} = icmp eq i64 {length}, 2\n"));
        let maybe = self.fresh();
        self.out.push_str(&format!("  {maybe} = and i1 {two_bytes}, {first_dot}\n"));
        let either = self.fresh();
        self.out.push_str(&format!("  {either} = or i1 {dot}, {maybe}\n"));
        self.out.push_str(&format!("  br i1 {either}, label %{second}, label %{keep}\n"));

        self.out.push_str(&format!("{second}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {name}, i64 1\n"));
        let other = self.fresh();
        self.out.push_str(&format!("  {other} = load i8, ptr {at}\n"));
        let other_dot = self.fresh();
        self.out.push_str(&format!("  {other_dot} = icmp eq i8 {other}, 46\n"));
        let dotdot = self.fresh();
        self.out.push_str(&format!("  {dotdot} = and i1 {maybe}, {other_dot}\n"));
        let skip = self.fresh();
        self.out.push_str(&format!("  {skip} = or i1 {dot}, {dotdot}\n"));
        self.out.push_str(&format!("  br i1 {skip}, label %{step}, label %{keep}\n"));

        // A name longer than the buffer is refused whole, never cut.
        self.out.push_str(&format!("{keep}:\n"));
        let long = self.fresh();
        self.out.push_str(&format!("  {long} = icmp sgt i64 {length}, {room}\n"));
        self.out.push_str(&format!("  br i1 {long}, label %{short}, label %{copy}\n"));

        self.out.push_str(&format!("{short}:\n"));
        let too_long = lex_sys_ir::enametoolong(self.is_darwin()).to_string();
        self.store_all(&cells, &["2", "0", "0", &too_long]);
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{copy}:\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call ptr @memmove(ptr {buffer}, ptr {name}, i64 {length})\n"
        ));
        let type_at = self.fresh();
        self.out.push_str(&format!(
            "  {type_at} = getelementptr i8, ptr {found}, i64 {}\n",
            layout.d_type
        ));
        let raw8 = self.fresh();
        self.out.push_str(&format!("  {raw8} = load i8, ptr {type_at}\n"));
        let raw = self.fresh();
        self.out.push_str(&format!("  {raw} = zext i8 {raw8} to i64\n"));
        let kind = self.kind_of(&raw);
        self.store_all(&cells, &["0", &length, &kind, "0"]);
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        Ok(self.load_all(&cells))
    }

    /// `d_type` as the language numbers a kind (§3.1).
    fn kind_of(&mut self, raw: &str) -> String {
        let mut kind = lex_sys_ir::KIND_OTHER.to_string();
        for (dt, ours) in [
            (lex_sys_ir::DT_UNKNOWN, lex_sys_ir::KIND_UNKNOWN),
            (lex_sys_ir::DT_LNK, lex_sys_ir::KIND_LINK),
            (lex_sys_ir::DT_DIR, lex_sys_ir::KIND_DIRECTORY),
            (lex_sys_ir::DT_REG, lex_sys_ir::KIND_FILE),
        ] {
            let is = self.fresh();
            self.out.push_str(&format!("  {is} = icmp eq i64 {raw}, {dt}\n"));
            let chosen = self.fresh();
            self.out.push_str(&format!("  {chosen} = select i1 {is}, i64 {ours}, i64 {kind}\n"));
            kind = chosen;
        }
        kind
    }

    /// `dir_list_close(list)`: `closedir`, which closes the listing's own
    /// descriptor and leaves the directory open.
    pub(crate) fn dir_list_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let list = args.first().ok_or_else(|| "`dir_list_close` needs its listing".to_owned())?;
        let stream = self.fresh();
        self.out.push_str(&format!("  {stream} = inttoptr i64 {} to ptr\n", operand(list)));
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = call i32 @closedir(ptr {stream})\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {answer} to i64\n"));
        Ok(vec![LValue::Reg(wide)])
    }
}
