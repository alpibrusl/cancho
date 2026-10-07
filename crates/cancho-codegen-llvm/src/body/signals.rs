//! The signal claim (`docs/signals.md` section 5): `signals_watch`,
//! `signals_pending`, `signals_close`, and the count of live threads `spawn`
//! and `join` keep for them. Mirrors `cancho-codegen`'s own
//! `body/signals.rs`.
//!
//! Linux: `pthread_sigmask` blocks the set and a `signalfd` reads it. The
//! signals are *blocked*, not ignored and not handled, so they queue in the
//! kernel, no handler runs, and a program reads them as data. Darwin has no
//! `signalfd`: each signal is ignored and a `kqueue` watches it with
//! `EVFILT_SIGNAL`, which reports a signal even when it is `SIG_IGN`; the
//! `kqueue` descriptor is itself readable while it has events, which is what
//! lets a `Poller` wait on it. Either way the handle is the **native mask**
//! in the high 32 bits over the descriptor in the low 32, so a close needs no
//! table and `handle_fd` reads the descriptor as it reads a `Conn`'s.

use crate::*;
use cancho_ir::{CLAIMABLE_SIGNALS, EBUSY, SIGNAL_STATE_GLOBAL, native_signal_number};

const SIG_BLOCK: i64 = 0;
const SIG_UNBLOCK: i64 = 1;
/// `SFD_NONBLOCK | SFD_CLOEXEC` (`O_NONBLOCK | O_CLOEXEC`, the same on x86-64 and aarch64 Linux).
const SFD_FLAGS: i64 = 0o4000 | 0o2000000;
const SIGSET_BYTES: i64 = 128;
const SIGNALFD_RECORD: i64 = 128;
/// More than the eight claimable signals, so one read takes every queued one.
const RECORDS: i64 = 8;
const EVFILT_SIGNAL: i64 = -6;
const EV_ADD: i64 = 0x0001;
const EV_CLEAR: i64 = 0x0020;
const KEVENT_SIZE: i64 = 32;
/// The word in `SIGNAL_STATE_GLOBAL` after the claimed mask: spawned threads not yet joined.
const THREADS_AT: i64 = 1;

impl<'a> FuncEmitter<'a> {
    /// `spawn` adds one and `join` takes one away, atomically: any thread may spawn.
    pub(crate) fn count_thread(&mut self, delta: i64) {
        let at = self.fresh();
        self.out.push_str(&format!(
            "  {at} = getelementptr i64, ptr @{SIGNAL_STATE_GLOBAL}, i64 {THREADS_AT}\n"
        ));
        let ignored = self.fresh();
        self.out.push_str(&format!("  {ignored} = atomicrmw add ptr {at}, i64 {delta} seq_cst\n"));
    }

    /// `select i1 cond, i64 a, i64 0`.
    fn select_or_zero(&mut self, condition: &str, value: i64) -> String {
        let part = self.fresh();
        self.out.push_str(&format!("  {part} = select i1 {condition}, i64 {value}, i64 0\n"));
        part
    }

    /// The kernel's mask for a set of cancho bits.
    fn native_from_bits(&mut self, bits: &str) -> String {
        let darwin = self.is_darwin();
        let mut native = "0".to_owned();
        for signal in CLAIMABLE_SIGNALS {
            let held = self.fresh();
            self.out.push_str(&format!("  {held} = and i64 {bits}, {}\n", signal.bit));
            let on = self.fresh();
            self.out.push_str(&format!("  {on} = icmp ne i64 {held}, 0\n"));
            let part = self.select_or_zero(&on, 1 << (native_signal_number(&signal, darwin) - 1));
            let joined = self.fresh();
            self.out.push_str(&format!("  {joined} = or i64 {native}, {part}\n"));
            native = joined;
        }
        native
    }

    /// The cancho bits of a kernel mask: the inverse of [`Self::native_from_bits`].
    fn bits_from_native(&mut self, native: &str) -> String {
        let darwin = self.is_darwin();
        let mut bits = "0".to_owned();
        for signal in CLAIMABLE_SIGNALS {
            let held = self.fresh();
            self.out.push_str(&format!(
                "  {held} = and i64 {native}, {}\n",
                1 << (native_signal_number(&signal, darwin) - 1)
            ));
            let on = self.fresh();
            self.out.push_str(&format!("  {on} = icmp ne i64 {held}, 0\n"));
            let part = self.select_or_zero(&on, signal.bit);
            let joined = self.fresh();
            self.out.push_str(&format!("  {joined} = or i64 {bits}, {part}\n"));
            bits = joined;
        }
        bits
    }

    /// A 128-byte `sigset_t` holding `native` in its first word, the rest zero.
    fn sigset_of(&mut self, native: &str) -> String {
        let set = self.fresh();
        self.hoist(format!("  {set} = alloca i8, i64 {SIGSET_BYTES}\n"));
        for word in 1..SIGSET_BYTES / 8 {
            self.store_field(&set, (word * 8) as i32, "i64", "0");
        }
        self.out.push_str(&format!("  store i64 {native}, ptr {set}\n"));
        set
    }

    /// `pthread_sigmask(how, set, NULL)`.
    fn sigmask(&mut self, how: i64, set: &str) {
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call i32 @pthread_sigmask(i32 {how}, ptr {set}, ptr null)\n"
        ));
    }

    /// Run `body` once for each signal number set in the kernel mask `native`.
    fn each_native_signal(&mut self, native: &str, mut body: impl FnMut(&mut Self, &str)) {
        let cell = self.fresh();
        self.hoist(format!("  {cell} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 1, ptr {cell}\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (head, test, run, next, done) = (
            format!("eachhead{n}"),
            format!("eachtest{n}"),
            format!("eachrun{n}"),
            format!("eachnext{n}"),
            format!("eachdone{n}"),
        );
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{head}:\n"));
        let number = self.fresh();
        self.out.push_str(&format!("  {number} = load i64, ptr {cell}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp sle i64 {number}, 31\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{test}, label %{done}\n"));

        self.out.push_str(&format!("{test}:\n"));
        let below = self.fresh();
        self.out.push_str(&format!("  {below} = sub i64 {number}, 1\n"));
        let shifted = self.fresh();
        self.out.push_str(&format!("  {shifted} = lshr i64 {native}, {below}\n"));
        let bit = self.fresh();
        self.out.push_str(&format!("  {bit} = and i64 {shifted}, 1\n"));
        let on = self.fresh();
        self.out.push_str(&format!("  {on} = icmp ne i64 {bit}, 0\n"));
        self.out.push_str(&format!("  br i1 {on}, label %{run}, label %{next}\n"));

        self.out.push_str(&format!("{run}:\n"));
        body(self, &number);
        self.out.push_str(&format!("  br label %{next}\n"));

        self.out.push_str(&format!("{next}:\n"));
        let after = self.fresh();
        self.out.push_str(&format!("  {after} = add i64 {number}, 1\n"));
        self.out.push_str(&format!("  store i64 {after}, ptr {cell}\n"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{done}:\n"));
    }

    /// `sigaction(n, {handler, 0, 0}, NULL)` with a handler of `0` (`SIG_DFL`) or
    /// `1` (`SIG_IGN`). `sigaction` and not `signal`, because a program
    /// moving off `Ffi("libc")` may still declare its own `extern fn signal`
    /// with other types. Darwin's `struct sigaction` is a handler, a 32-bit
    /// mask and 32-bit flags: 16 bytes.
    fn set_disposition(&mut self, number: &str, handler: i64) {
        let action = self.fresh();
        self.hoist(format!("  {action} = alloca i8, i64 16\n"));
        self.store_field(&action, 0, "i64", &handler.to_string());
        self.store_field(&action, 8, "i64", "0");
        let number32 = self.fresh();
        self.out.push_str(&format!("  {number32} = trunc i64 {number} to i32\n"));
        let ignored = self.fresh();
        self.out.push_str(&format!(
            "  {ignored} = call i32 @sigaction(i32 {number32}, ptr {action}, ptr null)\n"
        ));
    }

    /// `signals_watch`: `args` is the borrowed capability (a pointer, which
    /// is not read) and the cancho bits. Answers `Watching`'s three
    /// leaves: the tag (`Ok` 0, `Failed` 1), the handle, the `errno`.
    pub(crate) fn signals_watch(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let darwin = self.is_darwin();
        let bits = operand(&args[1]);
        let native = self.native_from_bits(&bits);
        let threads_at = self.fresh();
        self.out.push_str(&format!(
            "  {threads_at} = getelementptr i64, ptr @{SIGNAL_STATE_GLOBAL}, i64 {THREADS_AT}\n"
        ));
        let claimed = self.fresh();
        self.out.push_str(&format!("  {claimed} = load i64, ptr @{SIGNAL_STATE_GLOBAL}\n"));
        let threads = self.fresh();
        self.out.push_str(&format!("  {threads} = load i64, ptr {threads_at}\n"));
        let overlap = self.fresh();
        self.out.push_str(&format!("  {overlap} = and i64 {claimed}, {native}\n"));
        let taken = self.fresh();
        self.out.push_str(&format!("  {taken} = icmp ne i64 {overlap}, 0\n"));
        let running = self.fresh();
        self.out.push_str(&format!("  {running} = icmp ne i64 {threads}, 0\n"));
        let busy = self.fresh();
        self.out.push_str(&format!("  {busy} = or i1 {taken}, {running}\n"));

        let tag_cell = self.fresh();
        let handle_cell = self.fresh();
        let reason_cell = self.fresh();
        for cell in [&tag_cell, &handle_cell, &reason_cell] {
            self.hoist(format!("  {cell} = alloca i64\n"));
        }
        let n = self.blocks;
        self.blocks += 1;
        let (refuse, grant, ok, bad, merge) = (
            format!("sigrefuse{n}"),
            format!("siggrant{n}"),
            format!("sigok{n}"),
            format!("sigbad{n}"),
            format!("sigmerge{n}"),
        );
        self.out.push_str(&format!("  br i1 {busy}, label %{refuse}, label %{grant}\n"));

        // A thread is running, or another claim holds a signal: nothing changed.
        self.out.push_str(&format!("{refuse}:\n"));
        self.out.push_str(&format!("  store i64 1, ptr {tag_cell}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {handle_cell}\n"));
        self.out.push_str(&format!("  store i64 {EBUSY}, ptr {reason_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{grant}:\n"));
        let (descriptor, reason, set) = if darwin {
            self.each_native_signal(&native, |this, number| this.set_disposition(number, 1));
            let kq = self.fresh();
            self.out.push_str(&format!("  {kq} = call i32 @kqueue()\n"));
            let reason = self.errno();
            self.close_on_exec(&kq);
            let failed = self.fresh();
            self.out.push_str(&format!("  {failed} = icmp slt i32 {kq}, 0\n"));
            self.out.push_str(&format!("  br i1 {failed}, label %{bad}, label %{ok}\n"));
            (kq, reason, None)
        } else {
            let set = self.sigset_of(&native);
            self.sigmask(SIG_BLOCK, &set);
            let fd = self.fresh();
            self.out.push_str(&format!(
                "  {fd} = call i32 @signalfd(i32 -1, ptr {set}, i32 {SFD_FLAGS})\n"
            ));
            let reason = self.errno();
            let failed = self.fresh();
            self.out.push_str(&format!("  {failed} = icmp slt i32 {fd}, 0\n"));
            self.out.push_str(&format!("  br i1 {failed}, label %{bad}, label %{ok}\n"));
            (fd, reason, Some(set))
        };

        // The kernel refused a descriptor: put every signal back, and say why.
        self.out.push_str(&format!("{bad}:\n"));
        match set {
            Some(set) => self.sigmask(SIG_UNBLOCK, &set),
            None => {
                self.each_native_signal(&native, |this, number| this.set_disposition(number, 0))
            }
        }
        self.out.push_str(&format!("  store i64 1, ptr {tag_cell}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {handle_cell}\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{ok}:\n"));
        if darwin {
            self.each_native_signal(&native, |this, number| {
                let event = this.fresh();
                this.hoist(format!("  {event} = alloca i8, i64 {KEVENT_SIZE}\n"));
                this.store_field(&event, 0, "i64", number);
                this.store_field(&event, 8, "i16", &EVFILT_SIGNAL.to_string());
                this.store_field(&event, 10, "i16", &(EV_ADD | EV_CLEAR).to_string());
                this.store_field(&event, 12, "i32", "0");
                this.store_field(&event, 16, "i64", "0");
                this.store_field(&event, 24, "i64", "0");
                let ignored = this.fresh();
                this.out.push_str(&format!(
                    "  {ignored} = call i32 @kevent(i32 {descriptor}, ptr {event}, i32 1, ptr null, i32 0, ptr null)\n"
                ));
            });
        }
        let now = self.fresh();
        self.out.push_str(&format!("  {now} = or i64 {claimed}, {native}\n"));
        self.out.push_str(&format!("  store i64 {now}, ptr @{SIGNAL_STATE_GLOBAL}\n"));
        let high = self.fresh();
        self.out.push_str(&format!("  {high} = shl i64 {native}, 32\n"));
        let low = self.fresh();
        self.out.push_str(&format!("  {low} = zext i32 {descriptor} to i64\n"));
        let handle = self.fresh();
        self.out.push_str(&format!("  {handle} = or i64 {high}, {low}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {tag_cell}\n"));
        self.out.push_str(&format!("  store i64 {handle}, ptr {handle_cell}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {reason_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let mut leaves = Vec::new();
        for cell in [&tag_cell, &handle_cell, &reason_cell] {
            let value = self.fresh();
            self.out.push_str(&format!("  {value} = load i64, ptr {cell}\n"));
            leaves.push(LValue::Reg(value));
        }
        Ok(leaves)
    }

    /// `signals_pending(&!SignalWatch)`: take what the kernel has queued and
    /// answer it as cancho bits. Never waits: the descriptor is non-blocking
    /// on Linux, and the `kevent` has a zero timeout on Darwin.
    pub(crate) fn signals_pending(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let fd = self.handle_fd(&args[0]);
        let native = self.drain_native(&fd);
        let bits = self.bits_from_native(&native);
        Ok(vec![LValue::Reg(bits)])
    }

    /// Take what the kernel has queued on a claim's descriptor and answer it as
    /// a kernel mask. Never waits. `signals_pending` and Darwin's
    /// `signals_close` (which must not lose what was not read) share it.
    fn drain_native(&mut self, fd: &str) -> String {
        let darwin = self.is_darwin();
        let stride = if darwin { KEVENT_SIZE } else { SIGNALFD_RECORD };
        let buf = self.fresh();
        self.hoist(format!("  {buf} = alloca i8, i64 {}\n", stride * RECORDS));
        let count = self.fresh();
        if darwin {
            let timeout = self.fresh();
            self.hoist(format!("  {timeout} = alloca i8, i64 16\n"));
            self.store_field(&timeout, 0, "i64", "0");
            self.store_field(&timeout, 8, "i64", "0");
            let got = self.fresh();
            self.out.push_str(&format!(
                "  {got} = call i32 @kevent(i32 {fd}, ptr null, i32 0, ptr {buf}, i32 {RECORDS}, ptr {timeout})\n"
            ));
            self.out.push_str(&format!("  {count} = sext i32 {got} to i64\n"));
        } else {
            let st = self.size_ty();
            let want = self.size_arg(&(stride * RECORDS).to_string());
            let raw = self.fresh();
            self.out.push_str(&format!(
                "  {raw} = call {st} @read(i32 {fd}, ptr {buf}, {st} {want})\n"
            ));
            let got = self.size_result(&raw, true);
            // `-1` (nothing queued: `EAGAIN`) divides to zero records.
            self.out.push_str(&format!("  {count} = sdiv i64 {got}, {SIGNALFD_RECORD}\n"));
        }

        let index = self.fresh();
        let seen_cell = self.fresh();
        self.hoist(format!("  {index} = alloca i64\n"));
        self.hoist(format!("  {seen_cell} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {index}\n"));
        self.out.push_str(&format!("  store i64 0, ptr {seen_cell}\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (head, body, done) =
            (format!("sigphead{n}"), format!("sigpbody{n}"), format!("sigpdone{n}"));
        self.out.push_str(&format!("  br label %{head}\n"));
        self.out.push_str(&format!("{head}:\n"));
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {index}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp slt i64 {i}, {count}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));

        self.out.push_str(&format!("{body}:\n"));
        let offset = self.fresh();
        self.out.push_str(&format!("  {offset} = mul i64 {i}, {stride}\n"));
        let record = self.fresh();
        self.out.push_str(&format!("  {record} = getelementptr i8, ptr {buf}, i64 {offset}\n"));
        // `ssi_signo` is the first `u32` of a `signalfd_siginfo`; `ident` the first `u64` of a `kevent`.
        let number = if darwin {
            self.load_field(&record, 0, "i64")
        } else {
            let narrow = self.load_field(&record, 0, "i32");
            let wide = self.fresh();
            self.out.push_str(&format!("  {wide} = zext i32 {narrow} to i64\n"));
            wide
        };
        // Only 1..=31 can be a bit of the mask; anything else adds nothing.
        let below = self.fresh();
        self.out.push_str(&format!("  {below} = sub i64 {number}, 1\n"));
        let in_range = self.fresh();
        self.out.push_str(&format!("  {in_range} = icmp ult i64 {below}, 31\n"));
        let shift = self.fresh();
        self.out.push_str(&format!("  {shift} = select i1 {in_range}, i64 {below}, i64 0\n"));
        let bit = self.fresh();
        self.out.push_str(&format!("  {bit} = shl i64 1, {shift}\n"));
        let added = self.fresh();
        self.out.push_str(&format!("  {added} = select i1 {in_range}, i64 {bit}, i64 0\n"));
        let seen = self.fresh();
        self.out.push_str(&format!("  {seen} = load i64, ptr {seen_cell}\n"));
        let seen_now = self.fresh();
        self.out.push_str(&format!("  {seen_now} = or i64 {seen}, {added}\n"));
        self.out.push_str(&format!("  store i64 {seen_now}, ptr {seen_cell}\n"));
        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i64 {i}, 1\n"));
        self.out.push_str(&format!("  store i64 {next}, ptr {index}\n"));
        self.out.push_str(&format!("  br label %{head}\n"));

        self.out.push_str(&format!("{done}:\n"));
        let native = self.fresh();
        self.out.push_str(&format!("  {native} = load i64, ptr {seen_cell}\n"));
        native
    }

    /// `signals_close(SignalWatch)`: end the claim. A signal still queued is
    /// delivered now, with the disposition in force -- the default, so a
    /// `TERM` ends the process -- which is the second signal arriving before
    /// the first was acted on (`docs/signals.md` section 3).
    pub(crate) fn signals_close(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let handle = operand(&args[0]);
        let fd = self.fresh();
        self.out.push_str(&format!("  {fd} = trunc i64 {handle} to i32\n"));
        let native = self.fresh();
        self.out.push_str(&format!("  {native} = lshr i64 {handle}, 32\n"));
        let mut unread = None;
        if self.is_darwin() {
            // The claim ignored these signals, so one that arrived and was not
            // read is in the `kqueue` and nowhere else: the kernel discarded
            // it as ignored. Take it out first, restore the default, then
            // raise it, so it is delivered with the default action as the
            // queued signal is on Linux (`docs/signals.md` section 3).
            let queued = self.drain_native(&fd);
            self.each_native_signal(&native, |this, number| this.set_disposition(number, 0));
            unread = Some(queued);
        } else {
            let set = self.sigset_of(&native);
            self.sigmask(SIG_UNBLOCK, &set);
        }
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = call i32 @close(i32 {fd})\n"));
        if let Some(queued) = unread {
            self.each_native_signal(&queued, |this, number| {
                let number32 = this.fresh();
                this.out.push_str(&format!("  {number32} = trunc i64 {number} to i32\n"));
                let ignored = this.fresh();
                this.out.push_str(&format!("  {ignored} = call i32 @raise(i32 {number32})\n"));
            });
        }
        let claimed = self.fresh();
        self.out.push_str(&format!("  {claimed} = load i64, ptr @{SIGNAL_STATE_GLOBAL}\n"));
        let kept = self.fresh();
        self.out.push_str(&format!("  {kept} = xor i64 {native}, -1\n"));
        let rest = self.fresh();
        self.out.push_str(&format!("  {rest} = and i64 {claimed}, {kept}\n"));
        self.out.push_str(&format!("  store i64 {rest}, ptr @{SIGNAL_STATE_GLOBAL}\n"));
        let wide = self.fresh();
        self.out.push_str(&format!("  {wide} = sext i32 {answer} to i64\n"));
        Ok(vec![LValue::Reg(wide)])
    }
}
