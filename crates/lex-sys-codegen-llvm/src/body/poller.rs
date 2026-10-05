//! The `Poller` (`docs/native-sockets.md` §4): `epoll` on Linux, `kqueue` on
//! Darwin, behind one surface of `(token, events)` pairs. Mirrors
//! `lex-sys-codegen`'s own `body/poller.rs`.
//!
//! Both are **level-triggered**, which is what `poll(2)` was and what a
//! loop that reads one buffer's worth at a time wants. The kernel structs
//! -- `epoll_event` (12 packed bytes on x86-64, 16 elsewhere) and `kevent`
//! (32) -- are written and read here and never seen by a program.

use crate::*;
use lex_sys_ir::CHILD_PIDFD_SHIFT;

/// Events a program names: readable and writable.
const READABLE: i64 = 1;
const WRITABLE: i64 = 2;

/// At most this many events come back from one `poller_wait`; a program
/// that has more ready simply asks again.
const BATCH: i64 = 64;

/// `epoll_ctl` operations and `epoll_event.events` bits.
const EPOLL_CTL_ADD: i64 = 1;
const EPOLL_CTL_DEL: i64 = 2;
const EPOLL_CTL_MOD: i64 = 3;
const EPOLLIN: i64 = 0x001;
const EPOLLOUT: i64 = 0x004;
const EPOLLERR: i64 = 0x008;
const EPOLLHUP: i64 = 0x010;
const EPOLL_CLOEXEC: i64 = 0x80000;

/// `kevent` filters and flags.
const EVFILT_READ: i64 = -1;
const EVFILT_WRITE: i64 = -2;
const EVFILT_PROC: i64 = -5;
const EV_ONESHOT: i64 = 0x0010;
const NOTE_EXIT: i64 = 0x8000_0000;
const EV_ADD: i64 = 0x0001;
const EV_DELETE: i64 = 0x0002;
const EV_ERROR: i64 = 0x4000;
const KEVENT_SIZE: i64 = 32;

impl<'a> FuncEmitter<'a> {
    /// `epoll_event`'s size and where its 64-bit `data` sits: the struct is
    /// `__attribute__((packed))` on x86-64 and naturally aligned elsewhere.
    fn epoll_layout(&self) -> (i64, i64) {
        match self.triple.architecture {
            target_lexicon::Architecture::X86_64 => (12, 4),
            _ => (16, 8),
        }
    }

    /// A store through a pointer that may not be aligned.
    fn store_unaligned(&mut self, base: &str, offset: i64, ty: &str, value: &str) {
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {base}, i64 {offset}\n"));
        self.out.push_str(&format!("  store {ty} {value}, ptr {at}, align 1\n"));
    }

    fn load_unaligned(&mut self, base: &str, offset: i64, ty: &str) -> String {
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {base}, i64 {offset}\n"));
        let value = self.fresh();
        self.out.push_str(&format!("  {value} = load {ty}, ptr {at}, align 1\n"));
        value
    }

    /// `poller_new()`: an empty set. `Polling` is `Ok` 0, `Failed` 1.
    pub(crate) fn poller_new(&mut self) -> Result<Vec<LValue>, String> {
        let fd = self.fresh();
        if self.is_darwin() {
            self.out.push_str(&format!("  {fd} = call i32 @kqueue()\n"));
        } else {
            self.out.push_str(&format!("  {fd} = call i32 @epoll_create1(i32 {EPOLL_CLOEXEC})\n"));
        }
        let reason = self.errno();
        // `kqueue` has no flag for it; `epoll_create1` was asked
        // (`docs/processes.md` §4.5).
        if self.is_darwin() {
            self.close_on_exec(&fd);
        }
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = sext i32 {fd} to i64\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {fd}, 0\n"));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        Ok(vec![LValue::Reg(tag), LValue::Reg(fd64), reason])
    }

    /// One `kevent` change: `filter` added when `add`, deleted otherwise.
    /// Answers `kevent`'s result (`-1` on failure, with `errno` set).
    fn kevent_change(&mut self, kq: &str, fd: &str, filter: i64, add: &str, token: &str) -> String {
        let kev = self.fresh();
        self.hoist(format!("  {kev} = alloca i8, i64 {KEVENT_SIZE}\n"));
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = sext i32 {fd} to i64\n"));
        self.store_unaligned(&kev, 0, "i64", &fd64);
        self.store_unaligned(&kev, 8, "i16", &filter.to_string());
        let flags = self.fresh();
        self.out.push_str(&format!("  {flags} = select i1 {add}, i16 {EV_ADD}, i16 {EV_DELETE}\n"));
        self.store_unaligned(&kev, 10, "i16", &flags);
        self.store_unaligned(&kev, 12, "i32", "0");
        self.store_unaligned(&kev, 16, "i64", "0");
        self.store_unaligned(&kev, 24, "i64", token);
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @kevent(i32 {kq}, ptr {kev}, i32 1, ptr null, i32 0, ptr null)\n"
        ));
        result
    }

    /// `poller_add_listener`, `poller_add_conn` and `poller_modify`:
    /// `args` is the poller, the handle, the token and (but for a listener)
    /// the events. `0` on success, otherwise the `errno`.
    pub(crate) fn poller_ctl(
        &mut self,
        args: &[LValue],
        listener: bool,
        modify: bool,
    ) -> Result<Vec<LValue>, String> {
        let poller = self.handle_fd(&args[0]);
        let fd = self.handle_fd(&args[1]);
        let token = operand(&args[2]);
        let events = if listener { LValue::Const(READABLE) } else { args[3].clone() };

        let wants_read = self.fresh();
        let masked = self.fresh();
        self.out.push_str(&format!("  {masked} = and i64 {}, {READABLE}\n", operand(&events)));
        self.out.push_str(&format!("  {wants_read} = icmp ne i64 {masked}, 0\n"));
        let wants_write = self.fresh();
        let masked = self.fresh();
        self.out.push_str(&format!("  {masked} = and i64 {}, {WRITABLE}\n", operand(&events)));
        self.out.push_str(&format!("  {wants_write} = icmp ne i64 {masked}, 0\n"));

        if self.is_darwin() {
            // One change per filter: added if asked for, deleted if not
            // (an error from deleting what was never added is no failure).
            let read = self.kevent_change(&poller, &fd, EVFILT_READ, &wants_read, &token);
            let read_reason = self.errno();
            let write = self.kevent_change(&poller, &fd, EVFILT_WRITE, &wants_write, &token);
            let write_reason = self.errno();
            let read_failed = self.fresh();
            self.out.push_str(&format!("  {read_failed} = icmp slt i32 {read}, 0\n"));
            let read_bad = self.fresh();
            self.out.push_str(&format!("  {read_bad} = and i1 {read_failed}, {wants_read}\n"));
            let write_failed = self.fresh();
            self.out.push_str(&format!("  {write_failed} = icmp slt i32 {write}, 0\n"));
            let write_bad = self.fresh();
            self.out.push_str(&format!("  {write_bad} = and i1 {write_failed}, {wants_write}\n"));
            let after_write = self.fresh();
            self.out.push_str(&format!(
                "  {after_write} = select i1 {write_bad}, i64 {}, i64 0\n",
                operand(&write_reason)
            ));
            let answer = self.fresh();
            self.out.push_str(&format!(
                "  {answer} = select i1 {read_bad}, i64 {}, i64 {after_write}\n",
                operand(&read_reason)
            ));
            return Ok(vec![LValue::Reg(answer)]);
        }

        let (size, data_at) = self.epoll_layout();
        let ev = self.fresh();
        self.hoist(format!("  {ev} = alloca i8, i64 {size}\n"));
        let in_bit = self.fresh();
        self.out.push_str(&format!("  {in_bit} = select i1 {wants_read}, i32 {EPOLLIN}, i32 0\n"));
        let out_bit = self.fresh();
        self.out
            .push_str(&format!("  {out_bit} = select i1 {wants_write}, i32 {EPOLLOUT}, i32 0\n"));
        let mask = self.fresh();
        self.out.push_str(&format!("  {mask} = or i32 {in_bit}, {out_bit}\n"));
        self.store_unaligned(&ev, 0, "i32", &mask);
        self.store_unaligned(&ev, data_at, "i64", &token);
        let op = if modify { EPOLL_CTL_MOD } else { EPOLL_CTL_ADD };
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @epoll_ctl(i32 {poller}, i32 {op}, i32 {fd}, ptr {ev})\n"
        ));
        let reason = self.errno();
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        let answer = self.fresh();
        self.out.push_str(&format!(
            "  {answer} = select i1 {failed}, i64 {}, i64 0\n",
            operand(&reason)
        ));
        Ok(vec![LValue::Reg(answer)])
    }

    /// `poller_add_child(&!Poller, &Child, token)` (`docs/processes.md` §4.8):
    /// the child's exit, as readable. Linux watches the `pidfd` the `Child`
    /// carries; Darwin asks `kqueue` for `NOTE_EXIT` on the pid, which it
    /// reports once. `0`, or the `errno`.
    pub(crate) fn poller_add_child(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let poller = self.handle_fd(&args[0]);
        let word = self.fresh();
        self.out.push_str(&format!("  {word} = load i64, ptr {}\n", operand(&args[1])));
        let token = operand(&args[2]);

        if self.is_darwin() {
            let pid = self.fresh();
            self.out.push_str(&format!("  {pid} = trunc i64 {word} to i32\n"));
            let pid64 = self.fresh();
            self.out.push_str(&format!("  {pid64} = sext i32 {pid} to i64\n"));
            let kev = self.fresh();
            self.hoist(format!("  {kev} = alloca i8, i64 {KEVENT_SIZE}\n"));
            self.store_unaligned(&kev, 0, "i64", &pid64);
            self.store_unaligned(&kev, 8, "i16", &EVFILT_PROC.to_string());
            self.store_unaligned(&kev, 10, "i16", &(EV_ADD | EV_ONESHOT).to_string());
            self.store_unaligned(&kev, 12, "i32", &(NOTE_EXIT as i32).to_string());
            self.store_unaligned(&kev, 16, "i64", "0");
            self.store_unaligned(&kev, 24, "i64", &token);
            let result = self.fresh();
            self.out.push_str(&format!(
                "  {result} = call i32 @kevent(i32 {poller}, ptr {kev}, i32 1, ptr null, i32 0, ptr null)\n"
            ));
            let reason = self.errno();
            let failed = self.fresh();
            self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
            let answer = self.fresh();
            self.out.push_str(&format!(
                "  {answer} = select i1 {failed}, i64 {}, i64 0\n",
                operand(&reason)
            ));
            return Ok(vec![LValue::Reg(answer)]);
        }

        let high = self.fresh();
        self.out.push_str(&format!("  {high} = lshr i64 {word}, {CHILD_PIDFD_SHIFT}\n"));
        let pidfd = self.fresh();
        self.out.push_str(&format!("  {pidfd} = trunc i64 {high} to i32\n"));
        let (size, data_at) = self.epoll_layout();
        let ev = self.fresh();
        self.hoist(format!("  {ev} = alloca i8, i64 {size}\n"));
        self.store_unaligned(&ev, 0, "i32", &EPOLLIN.to_string());
        self.store_unaligned(&ev, data_at, "i64", &token);
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @epoll_ctl(i32 {poller}, i32 {EPOLL_CTL_ADD}, i32 {pidfd}, ptr {ev})\n"
        ));
        let reason = self.errno();
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        let answer = self.fresh();
        self.out.push_str(&format!(
            "  {answer} = select i1 {failed}, i64 {}, i64 0\n",
            operand(&reason)
        ));
        // No `pidfd`: the `Child` carries why, the `errno` negated
        // (`ENOSYS` before Linux 5.3, `EMFILE` with no descriptor to spare).
        let given = self.fresh();
        self.out.push_str(&format!("  {given} = icmp sge i32 {pidfd}, 0\n"));
        let pidfd64 = self.fresh();
        self.out.push_str(&format!("  {pidfd64} = sext i32 {pidfd} to i64\n"));
        let why = self.fresh();
        self.out.push_str(&format!("  {why} = sub i64 0, {pidfd64}\n"));
        let chosen = self.fresh();
        self.out.push_str(&format!("  {chosen} = select i1 {given}, i64 {answer}, i64 {why}\n"));
        Ok(vec![LValue::Reg(chosen)])
    }

    /// `poller_remove(&!Poller, &Conn)`: `0`, or the `errno` (Linux; on
    /// Darwin deleting what is not there is not an error).
    pub(crate) fn poller_remove(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let poller = self.handle_fd(&args[0]);
        let fd = self.handle_fd(&args[1]);
        if self.is_darwin() {
            self.kevent_change(&poller, &fd, EVFILT_READ, "false", "0");
            self.kevent_change(&poller, &fd, EVFILT_WRITE, "false", "0");
            return Ok(vec![LValue::Const(0)]);
        }
        let (size, _) = self.epoll_layout();
        let ev = self.fresh();
        self.hoist(format!("  {ev} = alloca i8, i64 {size}\n"));
        let result = self.fresh();
        self.out.push_str(&format!(
            "  {result} = call i32 @epoll_ctl(i32 {poller}, i32 {EPOLL_CTL_DEL}, i32 {fd}, ptr {ev})\n"
        ));
        let reason = self.errno();
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i32 {result}, 0\n"));
        let answer = self.fresh();
        self.out.push_str(&format!(
            "  {answer} = select i1 {failed}, i64 {}, i64 0\n",
            operand(&reason)
        ));
        Ok(vec![LValue::Reg(answer)])
    }

    /// `poller_wait(&!Poller, &![int], timeout_ms)`: waits, then writes
    /// `(token, events)` pairs into the slice and answers how many -- or
    /// `-errno`. `args` is the poller, the slice's pointer and length, and
    /// the timeout. Errors are read straight after the call that failed.
    pub(crate) fn poller_wait(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let poller = self.handle_fd(&args[0]);
        let out = operand(&args[1]);
        let timeout = operand(&args[3]);
        let darwin = self.is_darwin();
        let stride = if darwin { KEVENT_SIZE } else { self.epoll_layout().0 };

        // How many pairs fit, and no more than one batch.
        let pairs = self.fresh();
        self.out.push_str(&format!("  {pairs} = udiv i64 {}, 2\n", operand(&args[2])));
        let small = self.fresh();
        self.out.push_str(&format!("  {small} = icmp ult i64 {pairs}, {BATCH}\n"));
        let max = self.fresh();
        self.out.push_str(&format!("  {max} = select i1 {small}, i64 {pairs}, i64 {BATCH}\n"));
        let max32 = self.fresh();
        self.out.push_str(&format!("  {max32} = trunc i64 {max} to i32\n"));

        let buf = self.fresh();
        self.hoist(format!("  {buf} = alloca i8, i64 {}\n", stride * BATCH));
        let answer_cell = self.fresh();
        self.hoist(format!("  {answer_cell} = alloca i64\n"));
        let index = self.fresh();
        self.hoist(format!("  {index} = alloca i64\n"));

        let no_room = self.fresh();
        self.out.push_str(&format!("  {no_room} = icmp eq i64 {max}, 0\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (refuse, call, failed_l, head, body, done, merge) = (
            format!("pwrefuse{n}"),
            format!("pwcall{n}"),
            format!("pwfailed{n}"),
            format!("pwhead{n}"),
            format!("pwbody{n}"),
            format!("pwdone{n}"),
            format!("pwmerge{n}"),
        );
        self.out.push_str(&format!("  br i1 {no_room}, label %{refuse}, label %{call}\n"));

        // A slice too short for one pair is the caller's mistake: EINVAL.
        self.out.push_str(&format!("{refuse}:\n"));
        self.out.push_str(&format!("  store i64 -22, ptr {answer_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{call}:\n"));
        let timeout32 = self.fresh();
        self.out.push_str(&format!("  {timeout32} = trunc i64 {timeout} to i32\n"));
        let got = self.fresh();
        if darwin {
            // A negative timeout waits for ever: a null `timespec`.
            let ts = self.fresh();
            self.hoist(format!("  {ts} = alloca i8, i64 16\n"));
            let seconds = self.fresh();
            self.out.push_str(&format!("  {seconds} = sdiv i64 {timeout}, 1000\n"));
            let millis = self.fresh();
            self.out.push_str(&format!("  {millis} = srem i64 {timeout}, 1000\n"));
            let nanos = self.fresh();
            self.out.push_str(&format!("  {nanos} = mul i64 {millis}, 1000000\n"));
            self.store_unaligned(&ts, 0, "i64", &seconds);
            self.store_unaligned(&ts, 8, "i64", &nanos);
            let forever = self.fresh();
            self.out.push_str(&format!("  {forever} = icmp slt i64 {timeout}, 0\n"));
            let wait = self.fresh();
            self.out.push_str(&format!("  {wait} = select i1 {forever}, ptr null, ptr {ts}\n"));
            self.out.push_str(&format!(
                "  {got} = call i32 @kevent(i32 {poller}, ptr null, i32 0, ptr {buf}, i32 {max32}, ptr {wait})\n"
            ));
        } else {
            self.out.push_str(&format!(
                "  {got} = call i32 @epoll_wait(i32 {poller}, ptr {buf}, i32 {max32}, i32 {timeout32})\n"
            ));
        }
        let reason = self.errno();
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = icmp slt i32 {got}, 0\n"));
        self.out.push_str(&format!("  br i1 {bad}, label %{failed_l}, label %{head}\n"));

        self.out.push_str(&format!("{failed_l}:\n"));
        let negated = self.fresh();
        self.out.push_str(&format!("  {negated} = sub i64 0, {}\n", operand(&reason)));
        self.out.push_str(&format!("  store i64 {negated}, ptr {answer_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        let got64 = self.fresh();
        self.out.push_str(&format!("{head}:\n"));
        self.out.push_str(&format!("  {got64} = sext i32 {got} to i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {index}\n"));
        let (check, _) = (format!("pwcheck{n}"), ());
        self.out.push_str(&format!("  br label %{check}\n"));

        self.out.push_str(&format!("{check}:\n"));
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {index}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp slt i64 {i}, {got64}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));

        self.out.push_str(&format!("{body}:\n"));
        let entry_offset = self.fresh();
        self.out.push_str(&format!("  {entry_offset} = mul i64 {i}, {stride}\n"));
        let entry = self.fresh();
        self.out
            .push_str(&format!("  {entry} = getelementptr i8, ptr {buf}, i64 {entry_offset}\n"));
        let (token, events) = if darwin {
            let filter = self.load_unaligned(&entry, 8, "i16");
            let flags = self.load_unaligned(&entry, 10, "i16");
            let token = self.load_unaligned(&entry, 24, "i64");
            let reading = self.fresh();
            self.out.push_str(&format!("  {reading} = icmp eq i16 {filter}, {EVFILT_READ}\n"));
            // A child's exit is read as a signal's arrival is: readable.
            let exited = self.fresh();
            self.out.push_str(&format!("  {exited} = icmp eq i16 {filter}, {EVFILT_PROC}\n"));
            let is_read = self.fresh();
            self.out.push_str(&format!("  {is_read} = or i1 {reading}, {exited}\n"));
            let is_write = self.fresh();
            self.out.push_str(&format!("  {is_write} = icmp eq i16 {filter}, {EVFILT_WRITE}\n"));
            let errored_bits = self.fresh();
            self.out.push_str(&format!("  {errored_bits} = and i16 {flags}, {EV_ERROR}\n"));
            let errored = self.fresh();
            self.out.push_str(&format!("  {errored} = icmp ne i16 {errored_bits}, 0\n"));
            // An error is reported as readable: a read is what shows it.
            let reads = self.fresh();
            self.out.push_str(&format!("  {reads} = or i1 {is_read}, {errored}\n"));
            let r = self.fresh();
            self.out.push_str(&format!("  {r} = select i1 {reads}, i64 {READABLE}, i64 0\n"));
            let w = self.fresh();
            self.out.push_str(&format!("  {w} = select i1 {is_write}, i64 {WRITABLE}, i64 0\n"));
            let events = self.fresh();
            self.out.push_str(&format!("  {events} = or i64 {r}, {w}\n"));
            (token, events)
        } else {
            let (_, data_at) = self.epoll_layout();
            let mask = self.load_unaligned(&entry, 0, "i32");
            let token = self.load_unaligned(&entry, data_at, "i64");
            // Error and hang-up are reported as readable, as above.
            let reading = self.fresh();
            self.out.push_str(&format!(
                "  {reading} = and i32 {mask}, {}\n",
                EPOLLIN | EPOLLERR | EPOLLHUP
            ));
            let is_read = self.fresh();
            self.out.push_str(&format!("  {is_read} = icmp ne i32 {reading}, 0\n"));
            let writing = self.fresh();
            self.out.push_str(&format!("  {writing} = and i32 {mask}, {EPOLLOUT}\n"));
            let is_write = self.fresh();
            self.out.push_str(&format!("  {is_write} = icmp ne i32 {writing}, 0\n"));
            let r = self.fresh();
            self.out.push_str(&format!("  {r} = select i1 {is_read}, i64 {READABLE}, i64 0\n"));
            let w = self.fresh();
            self.out.push_str(&format!("  {w} = select i1 {is_write}, i64 {WRITABLE}, i64 0\n"));
            let events = self.fresh();
            self.out.push_str(&format!("  {events} = or i64 {r}, {w}\n"));
            (token, events)
        };
        let slot = self.fresh();
        self.out.push_str(&format!("  {slot} = mul i64 {i}, 2\n"));
        let token_at = self.fresh();
        self.out.push_str(&format!("  {token_at} = getelementptr i64, ptr {out}, i64 {slot}\n"));
        self.out.push_str(&format!("  store i64 {token}, ptr {token_at}\n"));
        let next_slot = self.fresh();
        self.out.push_str(&format!("  {next_slot} = add i64 {slot}, 1\n"));
        let events_at = self.fresh();
        self.out
            .push_str(&format!("  {events_at} = getelementptr i64, ptr {out}, i64 {next_slot}\n"));
        self.out.push_str(&format!("  store i64 {events}, ptr {events_at}\n"));
        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i64 {i}, 1\n"));
        self.out.push_str(&format!("  store i64 {next}, ptr {index}\n"));
        self.out.push_str(&format!("  br label %{check}\n"));

        self.out.push_str(&format!("{done}:\n"));
        self.out.push_str(&format!("  store i64 {got64}, ptr {answer_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = load i64, ptr {answer_cell}\n"));
        Ok(vec![LValue::Reg(answer)])
    }
}
