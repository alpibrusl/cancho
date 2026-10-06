# Signals: knowing you were asked to stop, with an authority that says which

Status: **built**, edition 6 (written before the code, corrected where building it disagreed; sections 9 and 10 are the corrections and the measurements).

## 1. Why

A long-running program has to learn that it was asked to stop: `SIGTERM` from a supervisor, `SIGINT` from a terminal, `SIGHUP` to reload. lex-sys had no way to observe a
signal (`std.test`'s issue, #237 part b). `lexsys-hooks` (a webhook delivery service) did it through `Ffi("libc")`, and `src/ops.ls` there says exactly what that cost:

* **Four libc functions** (`sigblock`, `sigsetmask`, `sigpending`, `signal`), reached through `Ffi("libc")`, which `docs/reach.md` section 5 established means *every* authority. The service's
  `lex-sys authority` report opens with `UNBOUNDED`, and it was bounded before the signal code was written.
* **No handler**. A callback must have an empty effect row (`docs/function-values.md` section 5), so a handler could set nothing and write nothing. The workaround blocks the signals and polls
  `sigpending` once a turn, so a stop is noticed at the loop's next wake-up (50 ms at most) rather than when it arrives.
* **The mask layout and the numbers are the program's problem**: `16386` is the `sigblock` mask for `SIGINT | SIGTERM` on Linux, `sigpending` fills a 128-byte `sigset_t` the program indexes by hand,
  and none of it is right on macOS, where `SIGUSR1` is 30.

What a service that only needs to know it was asked to stop should have is **a narrow capability**: its row names the signals it claims, the report is bounded, and the wake-up is a handle the
`Poller` can wait on, so a stop interrupts the wait instead of waiting out its timeout. The shape is the one `docs/native-sockets.md` settled for `Net`, `Listener`, `Conn`, `Poller` and `Clock`;
this document follows it and says where it deviates (section 8).

## 2. The design

Four prelude types and four builtins, all **edition 6**.

| type | what it is | consumed by |
|---|---|---|
| `Signals("INT,TERM")` | a capability, leaf-free like `Io`, *narrowed to the signals it may claim* | `release` |
| `SignalWatch` | the claim itself: `res`, one `i64` leaf, no literal form | `signals_close` |
| `Watching` | `enum { Ok(SignalWatch), Failed(int) }` (`errno`) | `match` |
| `Split` (edition 6) | edition 5's seven fields and `signals: Signals("")` | `match` / destructuring |

| builtin | signature | row |
|---|---|---|
| `signals_watch` | `(&Signals("S")) -> Watching` | `signals("S")` |
| `signals_pending` | `(&!SignalWatch) -> int` | `signals_read` |
| `poller_add_signals` | `(&!Poller, &SignalWatch, token) -> int` | `poll` |
| `signals_close` | `(SignalWatch) -> int` | none (ending a handle is not using one) |

`narrow(signals, "INT,TERM")` is the same `narrow` the other capabilities have, with the same meaning: it consumes the wider capability and answers a narrower one. The root
`Signals("")` that `split` hands out can become any set of claimable signals; `Signals("INT,TERM")` can become `Signals("INT")` and never `Signals("HUP")`.
`signals_watch` on the unnarrowed root is refused (`capability-misused`): a program must say which signals it claims, and "all of them" is not an answer the authority report could print.

```lex-sys
edition 6;

// Know that we were asked to stop. The row names the set; the report is bounded.
fn stop_requested[&w](watch: &!w SignalWatch) -> [signals_read] bool {
    return signals_pending(watch) != 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    let stop = narrow(signals, "INT,TERM");
    var status = 1;
    borrow stop as &s in {
        match signals_watch(s) {
            Watching::Ok(w) => {
                var watch = w;
                borrow mut watch as &!wh in {
                    if !stop_requested(wh) { status = 0; }
                }
                signals_close(watch);
            }
            Watching::Failed(e) => { status = 2; }
        }
    }
    release(stop);
    return status;
}
```

### 2.1 The set is a type, so the row is exact

The set of signals lives in the capability's type, as `Net`'s bound does (`Net("8080")`), and `signals_watch` performs the label `signals("INT,TERM")` with exactly that argument.
`lex-sys authority` prints it, so *which signals a program claims* is a fact the report states, and a program that never calls `narrow` on its `Signals` claims none.
Owning a `Signals("S")` discharges `signals("S")` and `signals_read`; owning a `SignalWatch` discharges `signals_read`; a function handed a `&!SignalWatch` declares `[signals_read]`
(the path was spent where the handle was minted, `docs/file-handles.md` section 4.1). A `World` discharges `signals("")`, the root, as it does `net_out("")`.

The argument is one string because a label carries one string (`ffi("libc")`, `net_out("host:port")`), so the set is a canonical comma-separated list: **names without the `SIG` prefix, in
alphabetical order, no spaces**. `narrow` accepts any order and the type it answers is the canonical one, so two programs that name the same set have the same type and the same row.

### 2.2 The claimable signals, and the bitmask

Eight signals are claimable. Each has a bit in the `int` that `signals_pending` answers, **fixed by lex-sys and the same on every target**, because the signal numbers are not
(`SIGUSR1` is 10 on Linux and 30 on macOS):

| name | bit | Linux | macOS | why it is claimable |
|---|---:|---:|---:|---|
| `HUP` | 1 | 1 | 1 | reload, or the terminal went away |
| `INT` | 2 | 2 | 2 | Ctrl-C |
| `QUIT` | 4 | 3 | 3 | Ctrl-\ |
| `TERM` | 8 | 15 | 15 | the supervisor's request to stop |
| `USR1` | 16 | 10 | 30 | the program's own |
| `USR2` | 32 | 12 | 31 | the program's own |
| `ALRM` | 64 | 14 | 14 | an `alarm` the program set through `Ffi` |
| `WINCH` | 128 | 28 | 28 | the terminal changed size |

`std.signals` names the bits (`signals.sigterm()`, `signals.sigint()`, ...) and has `signals.has(mask, bits)`, so a program says `signals.has(mask, signals.sigterm())` and never `mask & 8` (section 6).

**Every other signal is refused at compile time with the rule `signal-not-claimable`**, in `narrow`, with a sentence that says which of three reasons applies:

* *cannot be caught*: `KILL` and `STOP`. The kernel does not let a program have them.
* *a fault, not a request*: `SEGV`, `ILL`, `BUS`, `FPE` (and `ABRT`, `TRAP`, `SYS`). Blocking one that a fault raised does not defer it, the kernel kills the process; lex-sys itself ends a trap
  with `SIGILL` (`docs/defined-behaviour.md` section 1), so claiming it would make every checked-arithmetic failure *look like* something the program could handle and then kill it anyway.
* *not claimable (yet)*: everything else (`PIPE`, `CHLD`, `CONT`, `TSTP`, `TTIN`, `TTOU`, `URG`, ...). `PIPE` is deliberately not one: `conn_write` already answers `Failed(EPIPE)` and never raises it
  (`docs/native-sockets.md` section 3). `CHLD` is not, because lex-sys has no way to start a process (#237 part a), and on macOS this design would have to ignore it to see it, which turns on
  automatic reaping. A new signal is one row of a table when a program asks (`CONTRIBUTING.md`: two askers).

A malformed set (empty, a duplicate, a space, `SIGTERM` for `TERM`, lower case) is refused under the same rule.

## 3. Semantics

**What is delivered.** `signals_pending(w)` answers the bits of the watched signals that arrived **since the previous call**, and clears them. It never waits. `0` is "nothing yet".

**Exactly once, and what coalesces.** A signal that arrives is reported by exactly one call. Two *different* signals that arrive between two calls are both reported (that is the mask).
Two arrivals of the *same* signal between two calls are reported as **one**: POSIX keeps at most one pending instance of a standard signal, so the count is gone before a program could read it.
The bitmask says "at least once since". A program that needs a count is not asking about stopping.

**A second signal.** There is no automatic second behaviour: a claim stays a claim, every later signal is reported by a later call, and the program decides. What the hooks service wants,
"a second signal ends the process at once", is `signals_close` after the first: closing the `SignalWatch` **ends the claim and puts the signals' disposition back to the default**, so the next
`SIGTERM` terminates the process. Two details are part of the contract:

* A signal that arrived *between* the last `signals_pending` and the `signals_close` is not lost and is not swallowed: it is delivered when the claim ends, with the default action, which for `TERM`
  and `INT` ends the process. This holds on both kernels (on macOS by re-raising what the `kqueue` still held, section 5), except for a signal that was ignored on entry (next bullet). That is the second signal arriving before the first was acted on, and "kills at once" is what it was asked to do. A program that wants to discard it calls
  `signals_pending` last.
* A signal the process inherited as *ignored* (`nohup` ignores `HUP`; a shell runs a background job with `INT` and `QUIT` ignored) is claimable on both kernels, and is read as any other.
  Afterwards it is **ignored again on Linux** (the claim never touched the disposition, only the mask) and **at the default on macOS** (the claim ignored it to watch it and cannot know what it
  was). The difference is observable only for a signal that was ignored before the claim. The conformance suite found this the way it would be found in use: the tests were first run as
  a background job, `INT` was ignored, and "the next `INT` kills at once" did not.

**One claim at a time per signal.** Signals are process-wide, so two `SignalWatch`es over one signal would race to read it. A `signals_watch` over a signal another live `SignalWatch` already holds answers
`Failed(EBUSY)` (16 on both kernels), and closing the first frees it. `narrow` consumes the capability, so a program holds one set and the only way to reach the refusal is to borrow it twice;
a library handed `&Signals("INT,TERM")` that claims it while the program's own claim is live gets `EBUSY` rather than a stolen signal. (Disjoint claims would coexist; nothing can make two disjoint
capabilities yet, because there is no `fork_signals`, and nothing has asked for one.)

**Threads.** Signals are process-wide and a signal mask is per thread. The rule has to hold identically on both kernels, so it is the strict one:

* a `signals_watch` while **any spawned thread has not been joined** answers `Failed(EBUSY)`; watch first, then `spawn`;
* a thread spawned afterwards inherits the mask (Linux) or the process-wide disposition (macOS), so the signal cannot be taken by a thread that did not ask for it;
* `signals_pending` and `signals_close` may be called from any thread. On Linux `signals_close` unblocks the signals in the calling thread, so it should be the thread that watched (the main
  thread; the others keep them blocked until they end, which is invisible, since the signal is delivered to a thread that does not block it).

The runtime counts live threads (`spawn` adds one, `join` subtracts one) in a single word it already needs for the claimed set; that is the only change to `spawn` and `join`.

**Readiness.** `poller_add_signals(poller, watch, token)` registers the claim as *readable*, level-triggered like everything on the `Poller`: `poller_wait` reports `(token, 1)` while
a claimed signal is waiting, and keeps reporting it until `signals_pending` takes it. A program that registers a `SignalWatch` and never calls `signals_pending` spins, as one that never reads
a ready socket does. Closing the `SignalWatch` removes it from the `Poller` (the kernel drops a registration when the descriptor closes), so there is no `poller_remove_signals`.

## 4. The refusals, and what is not a refusal

| | rule | where |
|---|---|---|
| an unclaimable, unknown or malformed name | `signal-not-claimable` | `narrow` |
| `narrow` to a signal the capability was not narrowed to | `capability-not-narrowable` | `narrow` |
| `signals_watch` on the unnarrowed root | `capability-misused` | `signals_watch` |
| a `SignalWatch` not closed, used after `signals_close`, or taken apart | `linear-value-unconsumed`, `linear-use-after-move`, `linear-value-taken-apart` | the linearity pass |
| an undeclared `signals("INT")` or `signals_read` | `effect-not-declared` | the row |
| `signals_*` called in a file below edition 6 | `not-a-function` (a type: `unknown-name`) | name resolution |
| a seven-field `Split` pattern at edition 6 | `arity-mismatch` | the destructuring |

`Failed(errno)` is the *runtime's* refusal: `EBUSY` (a live thread, or a signal another claim holds), or the kernel's own (`EMFILE` when `signalfd` or `kqueue` cannot make a descriptor).
A failed `signals_watch` has changed nothing: it does not leave signals blocked.

## 5. Underneath

Both backends emit the same thing from the same table (`lex_sys_ir::signals`): the table says, for each target, which native number each claimable signal has and which edition-independent bit it
answers. The handle is one `i64`: the **native mask** of the signals it claims in the high 32 bits and the descriptor in the low 32, so `signals_close` knows what to release without a table, and
`poller_add_signals` reads the descriptor exactly as it reads a `Conn`'s.

**Linux** (measured, section 9): `pthread_sigmask(SIG_BLOCK)` for the set, then `signalfd(-1, set, SFD_NONBLOCK | SFD_CLOEXEC)`. The signals are *blocked*, not ignored and not handled, so they queue
in the kernel and no handler runs; `signals_pending` is one `read` of up to eight `signalfd_siginfo` records (128 bytes each, `ssi_signo` first) and a conversion of the numbers to bits. The
descriptor is readable while a signal is queued, so `epoll` waits on it unchanged. `signals_close` unblocks (a queued signal is then delivered with the disposition in force, which is the default
unless the process inherited `SIG_IGN`) and closes the descriptor. The disposition is never touched, so a signal ignored on entry is ignored again afterwards.

**macOS** (**not run**, section 10): there is no `signalfd`. `kqueue` has `EVFILT_SIGNAL`, which reports a signal "even if it has been marked `SIG_IGN`", so the claim sets each signal to `SIG_IGN`
(`signal(2)`), makes a `kqueue`, and registers one `EVFILT_SIGNAL` filter per signal (`EV_ADD | EV_CLEAR`). `signals_pending` is a zero-timeout `kevent` into an eight-event buffer, each event's
`ident` being the signal. The `kqueue` descriptor is itself readable while it has events, so the `Poller`'s `kqueue` registers it with `EVFILT_READ` (`poller_add_signals` is the existing
readable registration on that descriptor). `signals_close` first takes out of the `kqueue` whatever was not read (a zero-timeout `kevent`, the same drain `signals_pending` does), sets each signal back to `SIG_DFL`, closes the `kqueue`, and then
**`raise`s each signal that was unread**, so it is delivered with the default action. Without that step the signal is lost, and macOS CI found it: the kernel *discards* an ignored signal rather than leaving it
pending (only the `kqueue` kept a record of it), so restoring the default found nothing to deliver and `closing_with_a_signal_unread_delivers_it_at_once` saw the program exit 0. On Linux the signal sits
in the blocked mask and unblocking delivers it; on macOS it is re-raised, which has the same effect in the thread that closed. This path works in every thread, which Linux's does not; the live-thread
rule applies on both so that a program means the same thing on each.

**Not a handler.** Neither path installs a handler, so none of the three obstacles from `function-values.md` section 5 arises, and nothing runs in signal context: the program reads the
signal as data, in its own loop, in the order it chooses.

## 6. `std.signals`

A module of the eight bits as functions and two predicates, because a literal like `8` for `TERM` is a number a reader must look up:

```
signals.sighup()  signals.sigint()  signals.sigquit()  signals.sigterm()
signals.sigusr1() signals.sigusr2() signals.sigalrm()  signals.sigwinch()
signals.stop_signals() -> int           // INT | QUIT | TERM: the signals that mean "stop"
signals.has(mask, bits) -> bool         // every bit of `bits` is in `mask`
signals.any(mask, bits) -> bool         // some bit of `bits` is in `mask`
```

The names carry the `sig` prefix because `int` is a type name and `signals.int()` does not read; they are not `stop()` because of the gap in section 10 (a local called `stop`, the
natural name for the claim, hides `signals.stop`). It imports as `import std.signals as sg;` where a local is called `signals`, which `split` makes it.

## 7. What the hooks service changes

`src/ops.ls`'s four `extern fn`s, `hold_signals`, `pending_signal` and `second_signal_kills` become: `narrow(signals, "INT,TERM")` once in `main`; `signals_watch` before the first `spawn`;
`poller_add_signals` on the loop's `Poller` so the stop wakes `poller_wait`; `signals_pending` once per wake-up; `signals_close` on the first signal for "a second signal kills at once". The
`Ffi("libc")` that remains in the service (if any) is whatever else it uses; this removes `ffi("libc")` for signals from its row, and adds `signals("INT,TERM")`.

## 8. Where this deviates from the sockets precedent, and why

* **Edition 6, not edition 5.** Slice 4 of `native-sockets.md` added `clock` to edition 5's `Split` because no edition-5 file destructured it yet (`native-sockets.md` section 10.2: "there were
  none outside this document's own tests, which is the argument for adding it now rather than later"). That argument has expired: today 48 `.ls` files in this repository (`examples/api`,
  `examples/ocpp_ws`, `examples/tls_nb`, `tests/accept`, `tests/reject`, `tests/programs`, ...), the programs embedded in three conformance modules, and every downstream service destructure seven fields, and `editions.md` section 5 is explicit that a field
  on `Split` is the *additive* kind of change, absorbed by an edition. So `Split` becomes a fourth declaration of one name (`PRELUDE_SPLIT_SIGNALS`), an edition-5 file's `split()` still answers
  seven fields, and `edition 6;` is edition 5 plus signals. No file moves.
* **A capability and a handle, not one type.** The suggested `signals_pending(&Signals)` would make the capability carry state (a descriptor), which `Net` and `Clock` do not: they
  are authority with nothing behind them, and the thing that has a descriptor (`Listener`, `Conn`) is minted from them. `Signals("S")` is the authority, `SignalWatch` is the claim.
* **No `mask` argument to `signals_watch`.** The set is in the capability's type, checked where `narrow` is written. A runtime mask would need a runtime check against a bound, as `tcp_listen`'s port
  has (it traps), and the report would name the bound rather than what the program claims.

## 9. What it is checked by

`crates/lex-sys/tests/conformance/signals.rs` builds real programs on **both backends** and signals the real process with `kill(2)`; the programs speak on standard error and are driven by standard
input, so a signal sent before a `p` is queued before the poll that must see it. Twenty tests:

| claim | test |
|---|---|
| each of the eight is delivered once: sent, polled, polled again (empty); the process lives for all eight | `each_claimed_signal_is_delivered_exactly_once` |
| several signals between two polls are all reported; the same one three times is one bit; all eight together are `255` | `signals_between_polls_are_all_reported_and_a_repeat_is_one` |
| a claim takes only what it names: claiming `TERM`, a `USR1` still ends the process | `a_signal_that_was_not_claimed_still_takes_its_default_action` |
| a `poller_wait` with a 20 s timeout returns on the signal: token `7`, readable, then `signals_pending`, then not ready again | `a_signal_wakes_a_poller_wait_at_once` |
| "a second signal kills at once": read, close, signal again, and the process dies *of the signal* | `closing_the_claim_makes_the_next_signal_kill_at_once` |
| close with the signal unread delivers it; close after the read does not; both with the status of the process | `closing_with_a_signal_unread_delivers_it_at_once`, `closing_after_the_read_leaves_nothing_to_deliver` |
| a signal ignored on entry (`trap '' USR2` in the parent shell) is claimable, and is ignored again on Linux | `a_signal_ignored_on_entry_can_be_claimed` |
| a thread spawned after the claim does not take the signal; a claim while one runs is `EBUSY`, and granted after the `join`; an overlapping claim is `EBUSY` until the first is closed | `a_thread_spawned_after_the_claim_does_not_take_the_signal`, `a_claim_is_refused_while_a_thread_runs_and_granted_after_the_join`, `a_signal_another_claim_holds_is_refused_until_it_is_closed` |
| every unclaimable signal (23 names, three reasons), malformed sets, widening, the unnarrowed root, a non-`Signals`, linearity, rows exact both ways, owned capabilities discharge, edition 6 only | `every_unclaimable_signal_is_refused_with_its_rule` and five more |
| the authority report names the set, is `bounded`, has no `ffi`; "never touches" says `signals` when nothing is claimed | `the_authority_report_names_the_set_and_is_bounded`, `the_text_report_says_what_is_claimed_and_what_is_not` |
| `std.signals` | `std_signals_names_the_bits` |

Beside them: 9 unit tests for the table and for `Label::covers` (`lex-sys-ir`), `tests/reject/signal_not_claimable.ls` (the rule has its fixture, `every_rule_has_a_fixture`),
`tests/accept/signals_claim.ls`, and AGENTS.md section 3.2, whose block the suite compiles.

**Mutants.** 37 deliberate breakages of the new code, each run against the signals tests and each **killed**: 13 in the checker and the table (the subset check skipped; `SEGV` not a fault; set cover as a text prefix, in
`covers` and in the set test; a reversed canonical order; the row not performed; `signals_close` at edition 5; `narrow` skipping the set check; the root allowed to claim; an owned claim and an owned
`Signals` discharging nothing; a wrong `USR1` number; two signals sharing a bit) and 12 in **each backend** (the mask not blocked; the `signalfd` blocking; one record a read; no unblock on close; the claim kept
after close; an overlapping claim and a running thread each ignored; the kernel's numbers not converted to bits; the bits of a poll not accumulated; `spawn` and `join` not counted; `poller_add_signals` doing
something else). The first run of the mutants' targets also found the one test that depended on the environment (section 3, "ignored on entry").
Two more cover the macOS re-raise, which the Linux suite cannot run: a unit test in each backend builds a claim for `x86_64`/`aarch64` Linux and `x86_64`/`aarch64` Darwin (Cranelift emits Mach-O for the
host architecture; `clang -c` takes all four) and requires the Darwin module to call `sigaction`, `kqueue`, `kevent` and `raise` and the Linux one to call none of the four's Darwin calls but `signalfd` and
`pthread_sigmask`; dropping the `raise` in either backend fails it. That is a check that the call is **emitted**, not that it works.

**Strace** (`strace -f -e trace=signalfd4,rt_sigprocmask,rt_sigaction`, either backend): `rt_sigprocmask(SIG_BLOCK, [INT TERM])`, `signalfd4(-1, [INT TERM], 8, SFD_CLOEXEC|SFD_NONBLOCK)`, then `read(3, ..., 1024) = -1
EAGAIN` per poll, and **no `rt_sigaction`**: no disposition is touched, no handler exists.

**Latency.** Six runs of the wake test (three per backend): the `poller_wait` returned `124`, `180`, `234`, `263`, `292` and `496` microseconds after the `kill`, having waited `300` ms of a `20000` ms timeout.
`ops.ls`'s polling noticed a stop at the loop's next wake-up, up to 50 ms later.

**Size.** 1,404 lines added to the compiler across 24 files (the table 164, the checker 127 in `lower/signals.rs` and 110 in `defs.rs`, 385 in Cranelift, 416 in LLVM; 14 lines changed), 1,368 lines of tests
(1,218 of them conformance), 62 of `std`, 18 of fixtures, and this document.

## 10. What is not verified, and what building it found

* **macOS is written, not run here.** CI's macOS arm64 job ran the suite on the first version of this branch: 19 of the 20 signal tests passed there, so the `kqueue` path works, and the 20th found the
  unread-at-close loss fixed in section 5. The re-raise that fixes it has been built for Darwin (above) and not run. The `kqueue` path (`SIG_IGN` through `sigaction`, `EVFILT_SIGNAL` with `EV_ADD | EV_CLEAR`, a zero-timeout `kevent` to read, the `kqueue` descriptor registered in a
  `Poller`'s `kqueue` with `EVFILT_READ`) compiles into the same functions behind the target test and is exercised by the same suite when it runs on a Mac. The three assumptions the first version made from the man
  page and the headers (a `kqueue` readable in another `kqueue`; the 16-byte `struct sigaction`; `EVFILT_SIGNAL` reporting an ignored signal) held in that run. Still unrun: `raise` delivering to the closing thread
  before `signals_close` returns (POSIX says `raise` returns after the handler, or the default action, has run), and an unread signal that was *also* ignored on entry, which on macOS is re-raised with the default
  action although the process had ignored it (the claim cannot know what the disposition was).
* **aarch64 Linux** ~~is not run~~ **was run, and found a fault that is not in this section's code.** `SFD_NONBLOCK | SFD_CLOEXEC` and the 128-byte `signalfd_siginfo` are the same on both Linux
  architectures, and the claim itself behaved. The two tests that also spawn
  (`a_claim_is_refused_while_a_thread_runs_and_granted_after_the_join`, `a_thread_spawned_after_the_claim_does_not_take_the_signal`) died with `SIGBUS` on the
  Cranelift backend: the thread counter in `lexs_signal_state` was emitted unaligned and aarch64's exclusive load refuses it. Fixed and measured in `threads.md` section 6.
* **`Signals` and `SignalWatch` do not cross to a thread** as a `spawn` payload (`crosses_to_a_thread`, as `Conn` does not). Nothing asked, and the watch-before-spawn rule makes the natural program
  the one that reads signals on the thread that watched.
* **A signal's count is not available**, by design (section 3): a standard signal has one pending instance.
* **Adopting it in `lexsys-hooks`** is that repository's change (section 7); this one does not touch it.

**A gap found, not caused, and not worked around:** a local binding hides a *qualified* function of the same name. The claim's natural name is `stop`; `std.signals` first had a `stop()` and
`sg.stop()` was refused:

```
import std.math;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    let min = 3;
    return math.min(1, 2) + min;     // error: `min` is a local binding, not a function
}
```

Qualification should have been enough to say which `min` is meant. It is the same for any `std` function and any local, so this is `std.math` and not signals. The function here was renamed
`stop_signals` to avoid it, which is a workaround for this one name and not a fix.

**Corrected in this document while building it:** the first draft said a disjoint claim "coexists" with another and tested for it. It cannot be written yet (section 3); the draft's `Failed(EBUSY)` for a
second claim is the whole of what a program can reach. The first draft also said a seven-field `Split` pattern is `pattern-shape`; it is `arity-mismatch`.
