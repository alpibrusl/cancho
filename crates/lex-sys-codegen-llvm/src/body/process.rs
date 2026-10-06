//! Processes (`docs/processes.md` §3 to §6): `pipe_open`, `exec_spawn`,
//! `child_wait` and `child_kill`. The pipe verbs are the socket handles' (a
//! channel is a socket pair, §4.4). Mirrors `lex-sys-codegen`'s own
//! `body/process.rs`; the loops are built the same "no `phi`" way
//! `checked_path`'s is, through cells.

use crate::*;
use lex_sys_ir::{
    AF_UNIX, CHILD_PIDFD_SHIFT, EINVAL, Expr, F_SETFD, FD_CLOEXEC, O_RDWR, SIGSET_BYTES,
    SOCK_STREAM, SPAWN_OBJECT_BYTES, SYS_PIDFD_OPEN, sendable_signals, spawn_flags,
};

impl<'a> FuncEmitter<'a> {
    /// A fresh block number, for a group of labels.
    fn block_number(&mut self) -> u32 {
        let n = self.blocks;
        self.blocks += 1;
        n
    }

    /// A hoisted cell of `bytes` bytes, 16-aligned.
    fn cell(&mut self, bytes: i64) -> String {
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i8, i64 {bytes}, align 16\n"));
        cell
    }

    /// `pipe_open()`: a socket pair, close-on-exec, and on Darwin each end with
    /// `SO_NOSIGPIPE`. `Piped`'s four leaves: the tag (`Ok` 0, `Failed` 1), the
    /// parent's end, the child's end, the reason.
    pub(crate) fn pipe_open(&mut self) -> Result<Vec<LValue>, String> {
        let kind = SOCK_STREAM | self.os().sock_cloexec;
        let pair = self.cell(8);
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @socketpair(i32 {AF_UNIX}, i32 {kind}, i32 0, ptr {pair})\n"
        ));
        let reason = self.errno();
        let parent = self.fresh();
        self.out.push_str(&format!("  {parent} = load i32, ptr {pair}\n"));
        let second = self.fresh();
        self.out.push_str(&format!("  {second} = getelementptr i8, ptr {pair}, i64 4\n"));
        let child = self.fresh();
        self.out.push_str(&format!("  {child} = load i32, ptr {second}\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        if self.is_darwin() {
            let n = self.block_number();
            let (fix, done) = (format!("pipefix{n}"), format!("pipedone{n}"));
            self.out.push_str(&format!("  br i1 {failed}, label %{done}, label %{fix}\n"));
            self.out.push_str(&format!("{fix}:\n"));
            for end in [&parent, &child] {
                let ignored = self.fresh();
                self.out.push_str(&format!(
                    "  {ignored} = call i32 (i32, i32, ...) @fcntl(i32 {end}, i32 {F_SETFD}, i32 {FD_CLOEXEC})\n"
                ));
                self.suppress_sigpipe(end);
            }
            self.out.push_str(&format!("  br label %{done}\n"));
            self.out.push_str(&format!("{done}:\n"));
        }
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        let parent64 = self.fresh();
        self.out.push_str(&format!("  {parent64} = sext i32 {parent} to i64\n"));
        let child64 = self.fresh();
        self.out.push_str(&format!("  {child64} = sext i32 {child} to i64\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(parent64), LValue::Reg(child64), reason])
    }

    /// `exec_spawn(exec, path, args, env, stdin, stdout, stderr)` (§4.1 to
    /// §4.6). `Spawned`'s three leaves: the tag (`Ok` 0, `Failed` 1), the pid,
    /// the reason.
    pub(crate) fn exec_spawn(
        &mut self,
        prefix: &str,
        args: &[Expr],
    ) -> Result<Vec<LValue>, String> {
        // The capability is zero-sized and stops here.
        let path = self.expr(&args[1])?;
        let arguments = self.expr(&args[2])?;
        let environment = self.expr(&args[3])?;
        let mut streams = Vec::new();
        for stream in &args[4..7] {
            let leaves = self.expr(stream)?;
            if leaves.len() != 3 {
                return Err(format!("a `Stdio` needs 3 leaves but {} were given", leaves.len()));
            }
            streams.push(leaves);
        }

        // §4.1: under the prefix, no `..`; a broken promise traps.
        let program = self.checked_path(prefix, &path)?;
        // §4.2, §4.3: `\0`-separated lists, as `argv` and `envp`.
        let argv = self.pointer_list(&arguments, Some(&program), false)?;
        let envp = self.pointer_list(&environment, None, true)?;

        // §4.4: each stream is exactly what was given.
        let actions = self.cell(SPAWN_OBJECT_BYTES);
        self.out.push_str(&format!("  call i32 @posix_spawn_file_actions_init(ptr {actions})\n"));
        let null_device = operand(&self.bytes_lit("/dev/null\0")[0]);
        for (target, stream) in streams.iter().enumerate() {
            let (tag, end, file) = (operand(&stream[0]), operand(&stream[1]), operand(&stream[2]));
            let n = self.block_number();
            let (open, dup, next) =
                (format!("stdioopen{n}"), format!("stdiodup{n}"), format!("stdionext{n}"));
            let is_null = self.fresh();
            self.out.push_str(&format!("  {is_null} = icmp eq i64 {tag}, 0\n"));
            self.out.push_str(&format!("  br i1 {is_null}, label %{open}, label %{dup}\n"));
            self.out.push_str(&format!("{open}:\n"));
            self.out.push_str(&format!(
                "  call i32 @posix_spawn_file_actions_addopen(ptr {actions}, i32 {target}, ptr {null_device}, i32 {O_RDWR}, i32 0)\n"
            ));
            self.out.push_str(&format!("  br label %{next}\n"));
            self.out.push_str(&format!("{dup}:\n"));
            let source = self.stream_fd(&tag, &end, &file);
            self.out.push_str(&format!(
                "  call i32 @posix_spawn_file_actions_adddup2(ptr {actions}, i32 {source}, i32 {target})\n"
            ));
            self.out.push_str(&format!("  br label %{next}\n"));
            self.out.push_str(&format!("{next}:\n"));
        }

        // §4.5: and nothing else. Close-on-exec covers what this program
        // opened; this covers what it inherited without the flag, so the child
        // holds exactly the three streams. Darwin does the same with
        // `POSIX_SPAWN_CLOEXEC_DEFAULT` (`spawn_flags`); glibc has
        // `addclosefrom_np` from 2.34.
        if !self.is_darwin() {
            self.out.push_str(&format!(
                "  call i32 @posix_spawn_file_actions_addclosefrom_np(ptr {actions}, i32 3)\n"
            ));
        }

        // §4.6: an empty mask, and every signal at its default.
        let attributes = self.cell(SPAWN_OBJECT_BYTES);
        self.out.push_str(&format!("  call i32 @posix_spawnattr_init(ptr {attributes})\n"));
        let empty = self.cell(SIGSET_BYTES);
        self.out.push_str(&format!("  call i32 @sigemptyset(ptr {empty})\n"));
        self.out.push_str(&format!(
            "  call i32 @posix_spawnattr_setsigmask(ptr {attributes}, ptr {empty})\n"
        ));
        let every = self.cell(SIGSET_BYTES);
        self.out.push_str(&format!("  call i32 @sigfillset(ptr {every})\n"));
        self.out.push_str(&format!(
            "  call i32 @posix_spawnattr_setsigdefault(ptr {attributes}, ptr {every})\n"
        ));
        let flags = spawn_flags(self.is_darwin());
        self.out.push_str(&format!(
            "  call i32 @posix_spawnattr_setflags(ptr {attributes}, i16 {flags})\n"
        ));

        // `posix_spawn` answers the error number itself; `errno` is not used.
        let pid = self.cell(8);
        self.out.push_str(&format!("  store i32 0, ptr {pid}\n"));
        let error = self.fresh();
        self.out.push_str(&format!(
            "  {error} = call i32 @posix_spawn(ptr {pid}, ptr {program}, ptr {actions}, ptr {attributes}, ptr {argv}, ptr {envp})\n"
        ));
        self.out
            .push_str(&format!("  call i32 @posix_spawn_file_actions_destroy(ptr {actions})\n"));
        self.out.push_str(&format!("  call i32 @posix_spawnattr_destroy(ptr {attributes})\n"));
        self.out.push_str(&format!("  call void @free(ptr {argv})\n"));
        self.out.push_str(&format!("  call void @free(ptr {envp})\n"));

        // What was handed to the child is the parent's no longer, whether or
        // not the child started (§4.1: a failed spawn leaks nothing).
        for stream in &streams {
            let (tag, end, file) = (operand(&stream[0]), operand(&stream[1]), operand(&stream[2]));
            let n = self.block_number();
            let (close, next) = (format!("stdioclose{n}"), format!("stdiokept{n}"));
            let given = self.fresh();
            self.out.push_str(&format!("  {given} = icmp ne i64 {tag}, 0\n"));
            self.out.push_str(&format!("  br i1 {given}, label %{close}, label %{next}\n"));
            self.out.push_str(&format!("{close}:\n"));
            let fd = self.stream_fd(&tag, &end, &file);
            self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
            self.out.push_str(&format!("  br label %{next}\n"));
            self.out.push_str(&format!("{next}:\n"));
        }

        let child = self.fresh();
        self.out.push_str(&format!("  {child} = load i32, ptr {pid}\n"));
        let child64 = self.child_word(&child);
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = sext i32 {error} to i64\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp ne i32 {error}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(child64), LValue::Reg(reason)])
    }

    /// A `Child`'s one word from its pid (§4.8): on Linux the pid with a
    /// `pidfd` for it in the high half, opened here -- the child has not been
    /// reaped, so the pid is still its own -- and on Darwin the pid alone.
    fn child_word(&mut self, pid: &str) -> String {
        let low = self.fresh();
        self.out.push_str(&format!("  {low} = zext i32 {pid} to i64\n"));
        if self.is_darwin() {
            return low;
        }
        let pid64 = self.fresh();
        self.out.push_str(&format!("  {pid64} = sext i32 {pid} to i64\n"));
        let opened = self.fresh();
        self.out.push_str(&format!(
            "  {opened} = call i64 (i64, ...) @syscall(i64 {SYS_PIDFD_OPEN}, i64 {pid64}, i64 0)\n"
        ));
        let reason = self.errno();
        let refused = self.fresh();
        self.out.push_str(&format!("  {refused} = icmp slt i64 {opened}, 0\n"));
        let negated = self.fresh();
        self.out.push_str(&format!("  {negated} = sub i64 0, {}\n", operand(&reason)));
        let chosen = self.fresh();
        self.out
            .push_str(&format!("  {chosen} = select i1 {refused}, i64 {negated}, i64 {opened}\n"));
        let narrow = self.fresh();
        self.out.push_str(&format!("  {narrow} = trunc i64 {chosen} to i32\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = zext i32 {narrow} to i64\n"));
        let high = self.fresh();
        self.out.push_str(&format!("  {high} = shl i64 {wide}, {CHILD_PIDFD_SHIFT}\n"));
        let word = self.fresh();
        self.out.push_str(&format!("  {word} = or i64 {low}, {high}\n"));
        word
    }

    /// The descriptor a `Stdio` that is not `Null` holds: the child's end for
    /// `Pipe` (tag 1), the file for `File`, narrowed for libc.
    fn stream_fd(&mut self, tag: &str, end: &str, file: &str) -> String {
        let is_pipe = self.fresh();
        self.out.push_str(&format!("  {is_pipe} = icmp eq i64 {tag}, 1\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = select i1 {is_pipe}, i64 {end}, i64 {file}\n"));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {wide} to i32\n"));
        fd
    }

    /// A `\0`-separated list as a `NULL`-terminated array of pointers into it,
    /// `malloc`ed: `first` (the program, for `argv[0]`) ahead of the entries
    /// when given. A list that is not empty and does not end in a `\0` traps
    /// (§4.2). For the environment, an entry naming a loader variable traps
    /// (§4.3).
    fn pointer_list(
        &mut self,
        list: &[LValue],
        first: Option<&str>,
        environment: bool,
    ) -> Result<String, String> {
        let (base, length) = (operand(&list[0]), operand(&list[1]));
        let n = self.block_number();
        let (check, checked) = (format!("listcheck{n}"), format!("listchecked{n}"));
        let empty = self.fresh();
        self.out.push_str(&format!("  {empty} = icmp eq i64 {length}, 0\n"));
        self.out.push_str(&format!("  br i1 {empty}, label %{checked}, label %{check}\n"));
        self.out.push_str(&format!("{check}:\n"));
        let back = self.fresh();
        self.out.push_str(&format!("  {back} = add i64 {length}, -1\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {base}, i64 {back}\n"));
        let last = self.fresh();
        self.out.push_str(&format!("  {last} = load i8, ptr {at}\n"));
        let unterminated = self.fresh();
        self.out.push_str(&format!("  {unterminated} = icmp ne i8 {last}, 0\n"));
        self.trap_if(&unterminated)?;
        self.out.push_str(&format!("  br label %{checked}\n"));
        self.out.push_str(&format!("{checked}:\n"));

        // Every entry ends in a `\0`, so the count of `\0`s is the count.
        let count = self.count_zeros(&base, &length);
        let lead = i64::from(first.is_some());
        let slots = self.fresh();
        self.out.push_str(&format!("  {slots} = add i64 {count}, {}\n", lead + 1));
        let bytes = self.fresh();
        self.out.push_str(&format!("  {bytes} = mul i64 {slots}, 8\n"));
        let st = self.size_ty();
        let size = self.size_arg(&bytes);
        let array = self.fresh();
        self.out.push_str(&format!("  {array} = call ptr @malloc({st} {size})\n"));
        let missing = self.fresh();
        self.out.push_str(&format!("  {missing} = icmp eq ptr {array}, null\n"));
        self.trap_if(&missing)?;
        if let Some(first) = first {
            self.out.push_str(&format!("  store ptr {first}, ptr {array}\n"));
        }

        // One pass: store each entry's address and step past its `\0`.
        let cursor = self.fresh();
        self.hoist(format!("  {cursor} = alloca ptr\n"));
        let index = self.fresh();
        self.hoist(format!("  {index} = alloca i64\n"));
        self.out.push_str(&format!("  store ptr {base}, ptr {cursor}\n"));
        self.out.push_str(&format!("  store i64 {lead}, ptr {index}\n"));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {base}, i64 {length}\n"));
        let n = self.block_number();
        let (head, body, done) =
            (format!("listhead{n}"), format!("listbody{n}"), format!("listdone{n}"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{head}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = load ptr, ptr {cursor}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp ult ptr {at}, {end}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));
        self.out.push_str(&format!("{body}:\n"));
        if environment {
            self.refuse_loader_variable(&at)?;
        }
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {index}\n"));
        let slot = self.fresh();
        self.out.push_str(&format!("  {slot} = getelementptr ptr, ptr {array}, i64 {i}\n"));
        self.out.push_str(&format!("  store ptr {at}, ptr {slot}\n"));
        let found = self.memchr_zero(&at, &end);
        let after = self.fresh();
        self.out.push_str(&format!("  {after} = getelementptr i8, ptr {found}, i64 1\n"));
        self.out.push_str(&format!("  store ptr {after}, ptr {cursor}\n"));
        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i64 {i}, 1\n"));
        self.out.push_str(&format!("  store i64 {next}, ptr {index}\n"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{done}:\n"));
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {index}\n"));
        let slot = self.fresh();
        self.out.push_str(&format!("  {slot} = getelementptr ptr, ptr {array}, i64 {i}\n"));
        self.out.push_str(&format!("  store ptr null, ptr {slot}\n"));
        Ok(array)
    }

    /// `memchr(at, 0, end - at)`.
    fn memchr_zero(&mut self, at: &str, end: &str) -> String {
        let from = self.fresh();
        self.out.push_str(&format!("  {from} = ptrtoint ptr {at} to i64\n"));
        let to = self.fresh();
        self.out.push_str(&format!("  {to} = ptrtoint ptr {end} to i64\n"));
        let remaining = self.fresh();
        self.out.push_str(&format!("  {remaining} = sub i64 {to}, {from}\n"));
        let st = self.size_ty();
        let size = self.size_arg(&remaining);
        let found = self.fresh();
        self.out.push_str(&format!("  {found} = call ptr @memchr(ptr {at}, i32 0, {st} {size})\n"));
        found
    }

    /// How many `\0` bytes `length` bytes at `base` hold, by `memchr`.
    fn count_zeros(&mut self, base: &str, length: &str) -> String {
        let cursor = self.fresh();
        self.hoist(format!("  {cursor} = alloca ptr\n"));
        let count = self.fresh();
        self.hoist(format!("  {count} = alloca i64\n"));
        self.out.push_str(&format!("  store ptr {base}, ptr {cursor}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {count}\n"));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {base}, i64 {length}\n"));
        let n = self.block_number();
        let (head, body, done) =
            (format!("zeroshead{n}"), format!("zerosbody{n}"), format!("zerosdone{n}"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{head}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = load ptr, ptr {cursor}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp ult ptr {at}, {end}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));
        self.out.push_str(&format!("{body}:\n"));
        // The list was checked to end in a `\0`, so `memchr` always finds one.
        let found = self.memchr_zero(&at, &end);
        let after = self.fresh();
        self.out.push_str(&format!("  {after} = getelementptr i8, ptr {found}, i64 1\n"));
        self.out.push_str(&format!("  store ptr {after}, ptr {cursor}\n"));
        let so_far = self.fresh();
        self.out.push_str(&format!("  {so_far} = load i64, ptr {count}\n"));
        let more_one = self.fresh();
        self.out.push_str(&format!("  {more_one} = add i64 {so_far}, 1\n"));
        self.out.push_str(&format!("  store i64 {more_one}, ptr {count}\n"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{done}:\n"));
        let total = self.fresh();
        self.out.push_str(&format!("  {total} = load i64, ptr {count}\n"));
        total
    }

    /// Trap when the `\0`-terminated entry at `entry` names a variable the
    /// dynamic loader reads (`LD_*`, `DYLD_*`), §4.3. `strncmp` stops at the
    /// entry's `\0`, so a short entry is never read past.
    fn refuse_loader_variable(&mut self, entry: &str) -> Result<(), String> {
        for name in ["LD_", "DYLD_"] {
            let wanted = operand(&self.bytes_lit(&format!("{name}\0"))[0]);
            let st = self.size_ty();
            let order = self.fresh();
            self.out.push_str(&format!(
                "  {order} = call i32 @strncmp(ptr {entry}, ptr {wanted}, {st} {})\n",
                name.len()
            ));
            let same = self.fresh();
            self.out.push_str(&format!("  {same} = icmp eq i32 {order}, 0\n"));
            self.trap_if(&same)?;
        }
        Ok(())
    }

    /// `child_wait(Child)`: `waitpid` on this child alone, again after an
    /// interrupted wait. `Exited`'s four leaves: the tag (`Code` 0, `Signaled`
    /// 1, `Failed` 2), the exit code, the signal as a `std.signals` bit (`0`
    /// for one without), the reason.
    pub(crate) fn child_wait(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let [pid64] = args else {
            return Err(format!("`child_wait` needs 1 leaf but {} were given", args.len()));
        };
        let pid = self.fresh();
        self.out.push_str(&format!("  {pid} = trunc i64 {} to i32\n", operand(pid64)));
        let status = self.cell(8);
        self.out.push_str(&format!("  store i32 0, ptr {status}\n"));
        let answered_cell = self.fresh();
        self.hoist(format!("  {answered_cell} = alloca i32\n"));
        let reason_cell = self.fresh();
        self.hoist(format!("  {reason_cell} = alloca i64\n"));
        let n = self.block_number();
        let (again, after) = (format!("waitagain{n}"), format!("waitafter{n}"));
        self.out.push_str(&format!("  br label %{again}\n"));
        self.out.push_str(&format!("{again}:\n"));
        let answered = self.fresh();
        self.out.push_str(&format!(
            "  {answered} = call i32 @waitpid(i32 {pid}, ptr {status}, i32 0)\n"
        ));
        let reason = self.errno();
        self.out.push_str(&format!("  store i32 {answered}, ptr {answered_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {answered}, 0\n"));
        let interrupted = self.fresh();
        self.out.push_str(&format!("  {interrupted} = icmp eq i64 {}, 4\n", operand(&reason)));
        let retry = self.fresh();
        self.out.push_str(&format!("  {retry} = and i1 {failed}, {interrupted}\n"));
        self.out.push_str(&format!("  br i1 {retry}, label %{again}, label %{after}\n"));
        self.out.push_str(&format!("{after}:\n"));

        let answered = self.fresh();
        self.out.push_str(&format!("  {answered} = load i32, ptr {answered_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {reason_cell}\n"));
        let word32 = self.fresh();
        self.out.push_str(&format!("  {word32} = load i32, ptr {status}\n"));
        let word = self.fresh();
        self.out.push_str(&format!("  {word} = sext i32 {word32} to i64\n"));
        // The traditional layout, the same on both kernels: the low seven bits
        // are the signal that ended the child, or 0 when it exited, and then
        // the next eight are its code.
        let signal = self.fresh();
        self.out.push_str(&format!("  {signal} = and i64 {word}, 127\n"));
        let shifted = self.fresh();
        self.out.push_str(&format!("  {shifted} = lshr i64 {word}, 8\n"));
        let code = self.fresh();
        self.out.push_str(&format!("  {code} = and i64 {shifted}, 255\n"));
        let mut bit = "0".to_owned();
        for (wanted, number) in sendable_signals(self.is_darwin()) {
            let same = self.fresh();
            self.out.push_str(&format!("  {same} = icmp eq i64 {signal}, {number}\n"));
            let chosen = self.fresh();
            self.out.push_str(&format!("  {chosen} = select i1 {same}, i64 {wanted}, i64 {bit}\n"));
            bit = chosen;
        }
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {answered}, 0\n"));
        let exited = self.fresh();
        self.out.push_str(&format!("  {exited} = icmp eq i64 {signal}, 0\n"));
        let ended = self.fresh();
        self.out.push_str(&format!("  {ended} = select i1 {exited}, i64 0, i64 1\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 2, i64 {ended}\n"));
        // The `pidfd` has done its work once the child is reaped (§4.8); the
        // negated `errno` that stands in for a refused one is an `EBADF` that
        // harms nothing.
        if !self.is_darwin() {
            let high = self.fresh();
            self.out.push_str(&format!(
                "  {high} = lshr i64 {}, {CHILD_PIDFD_SHIFT}\n",
                operand(pid64)
            ));
            let pidfd = self.fresh();
            self.out.push_str(&format!("  {pidfd} = trunc i64 {high} to i32\n"));
            self.out.push_str(&format!("  call i32 @close(i32 {pidfd})\n"));
        }
        Ok(vec![LValue::Reg(tag), LValue::Reg(code), LValue::Reg(bit), LValue::Reg(reason)])
    }

    /// `child_kill(&Child, bit)`: one of the sendable signals by its bit, or
    /// `EINVAL` with no call. `0`, or the `errno`. The `Child` is unreaped, so
    /// its pid cannot have been reused (§4.7).
    pub(crate) fn child_kill(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let [child, bit] = args else {
            return Err(format!("`child_kill` needs 2 leaves but {} were given", args.len()));
        };
        let pid = self.handle_fd(child);
        let bit = operand(bit);
        let mut native = "0".to_owned();
        for (wanted, number) in sendable_signals(self.is_darwin()) {
            let same = self.fresh();
            self.out.push_str(&format!("  {same} = icmp eq i64 {bit}, {wanted}\n"));
            let chosen = self.fresh();
            self.out
                .push_str(&format!("  {chosen} = select i1 {same}, i64 {number}, i64 {native}\n"));
            native = chosen;
        }
        let answer = self.fresh();
        self.hoist(format!("  {answer} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 {EINVAL}, ptr {answer}\n"));
        let n = self.block_number();
        let (send, merge) = (format!("killsend{n}"), format!("killmerge{n}"));
        let known = self.fresh();
        self.out.push_str(&format!("  {known} = icmp ne i64 {native}, 0\n"));
        self.out.push_str(&format!("  br i1 {known}, label %{send}, label %{merge}\n"));
        self.out.push_str(&format!("{send}:\n"));
        let signal = self.fresh();
        self.out.push_str(&format!("  {signal} = trunc i64 {native} to i32\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = call i32 @kill(i32 {pid}, i32 {signal})\n"));
        let reason = self.errno();
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        let chosen = self.fresh();
        self.out.push_str(&format!(
            "  {chosen} = select i1 {failed}, i64 {}, i64 0\n",
            operand(&reason)
        ));
        self.out.push_str(&format!("  store i64 {chosen}, ptr {answer}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));
        self.out.push_str(&format!("{merge}:\n"));
        let value = self.fresh();
        self.out.push_str(&format!("  {value} = load i64, ptr {answer}\n"));
        Ok(vec![LValue::Reg(value)])
    }
}
