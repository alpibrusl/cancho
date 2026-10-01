//! The socket handles (`docs/native-sockets.md` §3): `tcp_listen`,
//! `tcp_accept`, `conn_read`, `conn_write`, the non-blocking switches and
//! the two closers. Mirrors `lex-sys-codegen`'s own `body/sockets.rs`.
//!
//! Every constant here that differs between Linux and Darwin is chosen by
//! the target triple, once, in [`Os`] -- the point of making these builtins
//! rather than leaving them to `extern fn` is that a program no longer has
//! to know them.

use super::net::port_bound_of;
use crate::*;

use lex_sys_ir::{EINVAL, F_GETFL, F_SETFL, SocketOs as Os};

impl<'a> FuncEmitter<'a> {
    pub(crate) fn os(&self) -> Os {
        Os::for_darwin(self.is_darwin())
    }

    pub(crate) fn is_darwin(&self) -> bool {
        matches!(self.triple.operating_system, target_lexicon::OperatingSystem::Darwin(_))
    }

    /// A handle's descriptor, read through the reference it arrived as and
    /// narrowed for libc.
    pub(crate) fn handle_fd(&mut self, reference: &LValue) -> String {
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = load i64, ptr {}\n", operand(reference)));
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {fd64} to i32\n"));
        fd
    }

    /// Darwin has no `MSG_NOSIGNAL`: a socket opts out of `SIGPIPE` for
    /// good with `SO_NOSIGPIPE`. Linux does it per send, so this is nothing.
    pub(crate) fn suppress_sigpipe(&mut self, fd: &str) {
        if self.is_darwin() {
            let os = self.os();
            self.set_int_option(fd, os.sol_socket, os.so_nosigpipe, 1);
        }
    }

    /// `setsockopt(fd, level, name, &value, 4)` with a C `int` value.
    fn set_int_option(&mut self, fd: &str, level: i64, name: i64, value: i64) {
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i32\n"));
        self.out.push_str(&format!("  store i32 {value}, ptr {cell}\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call i32 @setsockopt(i32 {fd}, i32 {level}, i32 {name}, ptr {cell}, i32 4)\n"
        ));
    }

    /// `tcp_listen(net, port, backlog, flags)`: `bind`'s check, then
    /// `socket`, `SO_REUSEADDR` (and `SO_REUSEPORT` if bit 1 of `flags`),
    /// `bind`, `listen`. Answers `Listening`'s three leaves -- the tag
    /// (`Ok` 0, `Failed` 1), the descriptor, the `errno` -- and every
    /// failure reads `errno` *before* the `close` that would overwrite it.
    pub(crate) fn tcp_listen(&mut self, bound: &str, args: &[Expr]) -> Result<Vec<LValue>, String> {
        let os = self.os();
        let port = self.scalar(&args[1])?;
        let backlog = self.scalar(&args[2])?;
        let flags = self.scalar(&args[3])?;

        if let Some(expected) = port_bound_of(bound) {
            let wrong_port = self.fresh();
            self.out.push_str(&format!(
                "  {wrong_port} = icmp ne i64 {}, {expected}\n",
                operand(&port)
            ));
            self.trap_if(&wrong_port)?;
        }

        let addr = self.fresh();
        self.hoist(format!("  {addr} = alloca i8, i64 16\n"));
        // Family bytes `2, 0`, as `bind` writes them: BSD kernels read a
        // zero family as `AF_INET`, so one layout serves both targets.
        self.store_byte(&addr, 0, "2");
        self.store_byte(&addr, 1, "0");
        let port32 = self.fresh();
        self.out.push_str(&format!("  {port32} = trunc i64 {} to i32\n", operand(&port)));
        let high32 = self.fresh();
        self.out.push_str(&format!("  {high32} = lshr i32 {port32}, 8\n"));
        let high8 = self.fresh();
        self.out.push_str(&format!("  {high8} = trunc i32 {high32} to i8\n"));
        let low8 = self.fresh();
        self.out.push_str(&format!("  {low8} = trunc i32 {port32} to i8\n"));
        self.store_byte(&addr, 2, &high8);
        self.store_byte(&addr, 3, &low8);
        for offset in 4..16 {
            self.store_byte(&addr, offset, "0");
        }

        let fd_cell = self.fresh();
        let err_cell = self.fresh();
        self.hoist(format!("  {fd_cell} = alloca i64\n"));
        self.hoist(format!("  {err_cell} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {err_cell}\n"));

        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = call i32 @socket(i32 2, i32 1, i32 0)\n"));
        let bad_socket = self.fresh();
        self.out.push_str(&format!("  {bad_socket} = icmp slt i32 {fd}, 0\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (no_socket, have_socket, bind_failed, listen_step, listen_failed, done, merge) = (
            format!("nosocket{n}"),
            format!("havesocket{n}"),
            format!("bindfailed{n}"),
            format!("listenstep{n}"),
            format!("listenfailed{n}"),
            format!("listening{n}"),
            format!("listenmerge{n}"),
        );
        self.out
            .push_str(&format!("  br i1 {bad_socket}, label %{no_socket}, label %{have_socket}\n"));

        self.out.push_str(&format!("{no_socket}:\n"));
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 {}, ptr {err_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  store i64 -1, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{have_socket}:\n"));
        self.set_int_option(&fd, os.sol_socket, os.so_reuseaddr, 1);
        // Bit 1 of `flags`: `SO_REUSEPORT`, for one process per core.
        let wants_port = self.fresh();
        self.out.push_str(&format!("  {wants_port} = and i64 {}, 1\n", operand(&flags)));
        let reuse_port = self.fresh();
        self.out.push_str(&format!("  {reuse_port} = icmp ne i64 {wants_port}, 0\n"));
        let (port_on, port_off) = (format!("reuseport{n}"), format!("noreuseport{n}"));
        self.out.push_str(&format!("  br i1 {reuse_port}, label %{port_on}, label %{port_off}\n"));
        self.out.push_str(&format!("{port_on}:\n"));
        self.set_int_option(&fd, os.sol_socket, os.so_reuseport, 1);
        self.out.push_str(&format!("  br label %{port_off}\n"));
        self.out.push_str(&format!("{port_off}:\n"));

        let bind_result = self.fresh();
        self.out
            .push_str(&format!("  {bind_result} = call i32 @bind(i32 {fd}, ptr {addr}, i32 16)\n"));
        let bound_ok = self.fresh();
        self.out.push_str(&format!("  {bound_ok} = icmp eq i32 {bind_result}, 0\n"));
        self.out
            .push_str(&format!("  br i1 {bound_ok}, label %{listen_step}, label %{bind_failed}\n"));

        self.out.push_str(&format!("{bind_failed}:\n"));
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 {}, ptr {err_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{listen_step}:\n"));
        let backlog32 = self.fresh();
        self.out.push_str(&format!("  {backlog32} = trunc i64 {} to i32\n", operand(&backlog)));
        let listen_result = self.fresh();
        self.out.push_str(&format!(
            "  {listen_result} = call i32 @listen(i32 {fd}, i32 {backlog32})\n"
        ));
        let listening = self.fresh();
        self.out.push_str(&format!("  {listening} = icmp eq i32 {listen_result}, 0\n"));
        self.out.push_str(&format!("  br i1 {listening}, label %{done}, label %{listen_failed}\n"));

        self.out.push_str(&format!("{listen_failed}:\n"));
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 {}, ptr {err_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{done}:\n"));
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = sext i32 {fd} to i64\n"));
        self.out.push_str(&format!("  store i64 {fd64}, ptr {fd_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let handle = self.fresh();
        self.out.push_str(&format!("  {handle} = load i64, ptr {fd_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {err_cell}\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {handle}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(handle), LValue::Reg(reason)])
    }

    /// `tcp_accept(&!Listener)`: one `accept(2)`. `Accepted` is `Ok` 0,
    /// `Again` 1, `Failed` 2. A connection arrives **blocking** on both
    /// kernels -- Darwin hands one the listener's `O_NONBLOCK`, which is
    /// cleared here so the two agree -- and with `SIGPIPE` already
    /// suppressed (`SO_NOSIGPIPE` on Darwin; Linux does it per send).
    pub(crate) fn tcp_accept(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let os = self.os();
        let listener = self.handle_fd(&args[0]);
        let conn = self.fresh();
        self.out.push_str(&format!(
            "  {conn} = call i32 @accept(i32 {listener}, ptr null, ptr null)\n"
        ));
        let conn64 = self.fresh();
        self.out.push_str(&format!("  {conn64} = sext i32 {conn} to i64\n"));
        let reason = self.errno();

        if self.is_darwin() {
            // Only on success: act on the new descriptor, or on nothing.
            let n = self.blocks;
            self.blocks += 1;
            let (fix, skip) = (format!("acceptfix{n}"), format!("acceptdone{n}"));
            let accepted = self.fresh();
            self.out.push_str(&format!("  {accepted} = icmp sge i32 {conn}, 0\n"));
            self.out.push_str(&format!("  br i1 {accepted}, label %{fix}, label %{skip}\n"));
            self.out.push_str(&format!("{fix}:\n"));
            self.set_int_option(&conn, os.sol_socket, os.so_nosigpipe, 1);
            let flags = self.fresh();
            self.out.push_str(&format!(
                "  {flags} = call i32 (i32, i32, ...) @fcntl(i32 {conn}, i32 {F_GETFL})\n"
            ));
            let cleared = self.fresh();
            self.out
                .push_str(&format!("  {cleared} = and i32 {flags}, {}\n", !(os.o_nonblock as i32)));
            let ignored = self.fresh();
            self.out.push_str(&format!(
                "  {ignored} = call i32 (i32, i32, ...) @fcntl(i32 {conn}, i32 {F_SETFL}, i32 {cleared})\n"
            ));
            self.out.push_str(&format!("  br label %{skip}\n"));
            self.out.push_str(&format!("{skip}:\n"));
        }

        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {conn}, 0\n"));
        let would_wait = self.fresh();
        self.out.push_str(&format!(
            "  {would_wait} = icmp eq i64 {}, {}\n",
            operand(&reason),
            os.eagain
        ));
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = select i1 {would_wait}, i64 1, i64 2\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 {bad}, i64 0\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(conn64), reason])
    }

    /// `conn_read(&!Conn, &![byte])`: one `recv(2)`. `Received` is `Data`
    /// 0, `End` 1, `Again` 2, `Failed` 3. An empty buffer is
    /// `Failed(EINVAL)` and **never reaches the kernel**: a blocking
    /// `recv` of zero bytes waits for data before answering the zero that
    /// would read as the peer closing.
    pub(crate) fn conn_read(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let os = self.os();
        let fd = self.handle_fd(&args[0]);

        let moved_cell = self.fresh();
        let reason_cell = self.fresh();
        self.hoist(format!("  {moved_cell} = alloca i64\n"));
        self.hoist(format!("  {reason_cell} = alloca i64\n"));
        let no_room = self.fresh();
        self.out.push_str(&format!("  {no_room} = icmp eq i64 {}, 0\n", operand(&args[2])));
        let n = self.blocks;
        self.blocks += 1;
        let (refuse, receive, merge) =
            (format!("norecv{n}"), format!("recv{n}"), format!("recvmerge{n}"));
        self.out.push_str(&format!("  br i1 {no_room}, label %{refuse}, label %{receive}\n"));

        self.out.push_str(&format!("{refuse}:\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {moved_cell}\n"));
        self.out.push_str(&format!("  store i64 {EINVAL}, ptr {reason_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{receive}:\n"));
        let got = self.fresh();
        self.out.push_str(&format!(
            "  {got} = call i64 @recv(i32 {fd}, ptr {}, i64 {}, i32 0)\n",
            operand(&args[1]),
            operand(&args[2])
        ));
        let errno = self.errno();
        self.out.push_str(&format!("  store i64 {got}, ptr {moved_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&errno)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let moved = self.fresh();
        self.out.push_str(&format!("  {moved} = load i64, ptr {moved_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {reason_cell}\n"));

        let negative = self.fresh();
        self.out.push_str(&format!("  {negative} = icmp slt i64 {moved}, 0\n"));
        let empty = self.fresh();
        self.out.push_str(&format!("  {empty} = icmp eq i64 {moved}, 0\n"));
        let would_wait = self.fresh();
        self.out.push_str(&format!("  {would_wait} = icmp eq i64 {reason}, {}\n", os.eagain));
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = select i1 {would_wait}, i64 2, i64 3\n"));
        let not_failed = self.fresh();
        self.out.push_str(&format!("  {not_failed} = select i1 {empty}, i64 1, i64 0\n"));
        let tag = self.fresh();
        self.out
            .push_str(&format!("  {tag} = select i1 {negative}, i64 {bad}, i64 {not_failed}\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(moved), LValue::Reg(reason)])
    }

    /// `conn_write(&!Conn, &[byte])`: one `send(2)` that cannot raise
    /// `SIGPIPE`. `Sent` is `Wrote` 0, `Again` 1, `Failed` 2.
    pub(crate) fn conn_write(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let os = self.os();
        let fd = self.handle_fd(&args[0]);
        let moved = self.fresh();
        self.out.push_str(&format!(
            "  {moved} = call i64 @send(i32 {fd}, ptr {}, i64 {}, i32 {})\n",
            operand(&args[1]),
            operand(&args[2]),
            os.msg_nosignal
        ));
        let reason = self.errno();
        let negative = self.fresh();
        self.out.push_str(&format!("  {negative} = icmp slt i64 {moved}, 0\n"));
        let would_wait = self.fresh();
        self.out.push_str(&format!(
            "  {would_wait} = icmp eq i64 {}, {}\n",
            operand(&reason),
            os.eagain
        ));
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = select i1 {would_wait}, i64 1, i64 2\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {negative}, i64 {bad}, i64 0\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(moved), reason])
    }

    /// `conn_nonblocking` / `listener_nonblocking`: `O_NONBLOCK`, one way.
    /// `0` on success, otherwise the `errno`. `fcntl` is variadic, and is
    /// called as such -- which is what Apple arm64 requires and the reason
    /// this is a builtin rather than an `extern`.
    pub(crate) fn nonblocking(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let os = self.os();
        let fd = self.handle_fd(&args[0]);
        let flags = self.fresh();
        self.out.push_str(&format!(
            "  {flags} = call i32 (i32, i32, ...) @fcntl(i32 {fd}, i32 {F_GETFL})\n"
        ));
        let set = self.fresh();
        self.out.push_str(&format!("  {set} = or i32 {flags}, {}\n", os.o_nonblock as i32));
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 (i32, i32, ...) @fcntl(i32 {fd}, i32 {F_SETFL}, i32 {set})\n"
        ));
        let reason_set = self.errno();
        // A failing `F_GETFL` leaves -1 in `flags` and the `F_SETFL` after
        // it fails too (`EBADF`); either way the answer is the last `errno`.
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        let ok = self.fresh();
        self.out.push_str(&format!(
            "  {ok} = select i1 {failed}, i64 {}, i64 0\n",
            operand(&reason_set)
        ));
        Ok(vec![LValue::Reg(ok)])
    }

    /// `clock_ms(&Clock)` (`docs/native-sockets.md` §5): `CLOCK_MONOTONIC`
    /// as milliseconds (clock id 1 on Linux, 6 on Darwin).
    pub(crate) fn clock_ms(&mut self) -> Result<Vec<LValue>, String> {
        let ts = self.fresh();
        self.hoist(format!("  {ts} = alloca i8, i64 16\n"));
        let id = if self.is_darwin() { 6 } else { 1 };
        let ignored = self.fresh();
        self.out.push_str(&format!("  {ignored} = call i32 @clock_gettime(i32 {id}, ptr {ts})\n"));
        let seconds = self.load_field(&ts, 0, "i64");
        let nanos = self.load_field(&ts, 8, "i64");
        let millis = self.fresh();
        self.out.push_str(&format!("  {millis} = mul i64 {seconds}, 1000\n"));
        let rest = self.fresh();
        self.out.push_str(&format!("  {rest} = udiv i64 {nanos}, 1000000\n"));
        let total = self.fresh();
        self.out.push_str(&format!("  {total} = add i64 {millis}, {rest}\n"));
        Ok(vec![LValue::Reg(total)])
    }

    /// The address of a descriptor's epoch counter (`fd` an `i64`).
    fn epoch_slot(&mut self, fd: &str) -> String {
        let slot = self.fresh();
        self.out.push_str(&format!(
            "  {slot} = getelementptr i32, ptr @{}, i64 {fd}\n",
            lex_sys_ir::FD_EPOCH_GLOBAL
        ));
        slot
    }

    /// `conn_detach(Conn)` (`docs/native-sockets.md` §10.3): the descriptor
    /// stays open, the `Conn` ends, and what comes back is a ticket -- the
    /// descriptor's epoch, bumped to an odd number, over its number. A
    /// descriptor too large for the table is closed and answers `-1`.
    pub(crate) fn conn_detach(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = operand(&args[0]);
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i64\n"));
        let too_big = self.fresh();
        self.out.push_str(&format!(
            "  {too_big} = icmp uge i64 {fd}, {}\n",
            lex_sys_ir::FD_EPOCH_SLOTS
        ));
        let n = self.blocks;
        self.blocks += 1;
        let (refuse, issue, merge) =
            (format!("detachrefuse{n}"), format!("detachissue{n}"), format!("detachmerge{n}"));
        self.out.push_str(&format!("  br i1 {too_big}, label %{refuse}, label %{issue}\n"));

        self.out.push_str(&format!("{refuse}:\n"));
        let fd32 = self.fresh();
        self.out.push_str(&format!("  {fd32} = trunc i64 {fd} to i32\n"));
        self.out.push_str(&format!("  call i32 @close(i32 {fd32})\n"));
        self.out.push_str(&format!("  store i64 -1, ptr {cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{issue}:\n"));
        let slot = self.epoch_slot(&fd);
        let epoch = self.fresh();
        self.out.push_str(&format!("  {epoch} = load i32, ptr {slot}\n"));
        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i32 {epoch}, 1\n"));
        self.out.push_str(&format!("  store i32 {next}, ptr {slot}\n"));
        let next64 = self.fresh();
        self.out.push_str(&format!("  {next64} = zext i32 {next} to i64\n"));
        let masked = self.fresh();
        self.out.push_str(&format!("  {masked} = and i64 {next64}, 2147483647\n"));
        let high = self.fresh();
        self.out.push_str(&format!("  {high} = shl i64 {masked}, 32\n"));
        let ticket = self.fresh();
        self.out.push_str(&format!("  {ticket} = or i64 {high}, {fd}\n"));
        self.out.push_str(&format!("  store i64 {ticket}, ptr {cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = load i64, ptr {cell}\n"));
        Ok(vec![LValue::Reg(result)])
    }

    /// `conn_attach(int)`: valid only if the descriptor is in range, the
    /// ticket's epoch is odd, and it is the descriptor's *current* epoch --
    /// then the epoch moves on, so the ticket is spent. `Attached` is `Ok`
    /// 0 with the descriptor, `Failed` 1 with `EBADF`.
    pub(crate) fn conn_attach(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let ticket = operand(&args[0]);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = and i64 {ticket}, 4294967295\n"));
        let epoch = self.fresh();
        self.out.push_str(&format!("  {epoch} = lshr i64 {ticket}, 32\n"));
        let in_range = self.fresh();
        self.out.push_str(&format!(
            "  {in_range} = icmp ult i64 {fd}, {}\n",
            lex_sys_ir::FD_EPOCH_SLOTS
        ));
        // Index zero when out of range, so the load is inside the table.
        let index = self.fresh();
        self.out.push_str(&format!("  {index} = select i1 {in_range}, i64 {fd}, i64 0\n"));
        let slot = self.epoch_slot(&index);
        let current = self.fresh();
        self.out.push_str(&format!("  {current} = load i32, ptr {slot}\n"));
        let current64 = self.fresh();
        self.out.push_str(&format!("  {current64} = zext i32 {current} to i64\n"));
        let current_masked = self.fresh();
        self.out.push_str(&format!("  {current_masked} = and i64 {current64}, 2147483647\n"));
        let same = self.fresh();
        self.out.push_str(&format!("  {same} = icmp eq i64 {current_masked}, {epoch}\n"));
        let odd_bit = self.fresh();
        self.out.push_str(&format!("  {odd_bit} = and i64 {epoch}, 1\n"));
        let odd = self.fresh();
        self.out.push_str(&format!("  {odd} = icmp ne i64 {odd_bit}, 0\n"));
        let non_negative = self.fresh();
        self.out.push_str(&format!("  {non_negative} = icmp sge i64 {ticket}, 0\n"));
        let a = self.fresh();
        self.out.push_str(&format!("  {a} = and i1 {in_range}, {same}\n"));
        let b = self.fresh();
        self.out.push_str(&format!("  {b} = and i1 {odd}, {non_negative}\n"));
        let valid = self.fresh();
        self.out.push_str(&format!("  {valid} = and i1 {a}, {b}\n"));

        let n = self.blocks;
        self.blocks += 1;
        let (spend, after) = (format!("attachspend{n}"), format!("attachdone{n}"));
        self.out.push_str(&format!("  br i1 {valid}, label %{spend}, label %{after}\n"));
        self.out.push_str(&format!("{spend}:\n"));
        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i32 {current}, 1\n"));
        self.out.push_str(&format!("  store i32 {next}, ptr {slot}\n"));
        self.out.push_str(&format!("  br label %{after}\n"));
        self.out.push_str(&format!("{after}:\n"));

        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {valid}, i64 0, i64 1\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(fd), LValue::Const(9)])
    }
}
