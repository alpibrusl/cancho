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

`std.signals` names the bits (`signals.term()`, `signals.int()`, ...) and has `signals.has(mask, bit)`, so a program says `has(mask, signals.term())` and never `mask & 8`.

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
  and `INT` ends the process. That is the second signal arriving before the first was acted on, and "kills at once" is what it was asked to do. A program that wants to discard it calls
  `signals_pending` last.
* A signal whose disposition the process inherited as *ignored* (`nohup` ignores `HUP`) is claimable, and is back to the default, not to ignored, afterwards on macOS, and back to ignored on Linux
  (section 5). The difference is observable only for a signal that was ignored before the claim.

**One claim at a time per signal.** Signals are process-wide, so two `SignalWatch`es over one signal would race to read it. A `signals_watch` over a signal another live `SignalWatch` already holds answers
`Failed(EBUSY)` (16 on both kernels). Disjoint sets coexist. Closing frees the signals.

**Threads.** Signals are process-wide and a signal mask is per thread. The rule has to hold identically on both kernels, so it is the strict one:

* a `signals_watch` while **any spawned thread is still running** answers `Failed(EBUSY)`; watch first, then `spawn`;
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
| `signals_*` named in a file below edition 6 | `unknown-name` | name resolution |
| a seven-field `Split` pattern at edition 6 | `pattern-shape` | the destructuring |

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
readable registration on that descriptor). `signals_close` sets each signal back to `SIG_DFL` and closes the `kqueue`. This path works in every thread, which Linux's does not; the live-thread
rule applies on both so that a program means the same thing on each.

**Not a handler.** Neither path installs a handler, so none of the three obstacles from `function-values.md` section 5 arises, and nothing runs in signal context: the program reads the
signal as data, in its own loop, in the order it chooses.

## 6. `std.signals`

A module of the eight bits as functions and one predicate, because a literal like `8` for `TERM` is a number a reader must look up:

```
signals.hup()  signals.int()  signals.quit()  signals.term()
signals.usr1() signals.usr2() signals.alrm()  signals.winch()
signals.has(mask, bit) -> bool          // mask & bit != 0
signals.stop() -> int                   // INT | TERM | QUIT: the signals that mean "stop"
```

## 7. What the hooks service changes

`src/ops.ls`'s four `extern fn`s, `hold_signals`, `pending_signal` and `second_signal_kills` become: `narrow(signals, "INT,TERM")` once in `main`; `signals_watch` before the first `spawn`;
`poller_add_signals` on the loop's `Poller` so the stop wakes `poller_wait`; `signals_pending` once per wake-up; `signals_close` on the first signal for "a second signal kills at once". The
`Ffi("libc")` that remains in the service (if any) is whatever else it uses; this removes `ffi("libc")` for signals from its row, and adds `signals("INT,TERM")`.

## 8. Where this deviates from the sockets precedent, and why

* **Edition 6, not edition 5.** Slice 4 of `native-sockets.md` added `clock` to edition 5's `Split` because no edition-5 file destructured it yet (`native-sockets.md` section 10.2: "there were
  none outside this document's own tests, which is the argument for adding it now rather than later"). That argument has expired: today 61 files in this repository (`examples/api`,
  `examples/ocpp_ws`, `examples/tls_nb`, `tests/programs`, `packages/http-server`, ...) and every downstream service destructure seven fields, and `editions.md` section 5 is explicit that a field
  on `Split` is the *additive* kind of change, absorbed by an edition. So `Split` becomes a fourth declaration of one name (`PRELUDE_SPLIT_SIGNALS`), an edition-5 file's `split()` still answers
  seven fields, and `edition 6;` is edition 5 plus signals. No file moves.
* **A capability and a handle, not one type.** The suggested `signals_pending(&Signals)` would make the capability carry state (a descriptor), which `Net` and `Clock` do not: they
  are authority with nothing behind them, and the thing that has a descriptor (`Listener`, `Conn`) is minted from them. `Signals("S")` is the authority, `SignalWatch` is the claim.
* **No `mask` argument to `signals_watch`.** The set is in the capability's type, checked where `narrow` is written. A runtime mask would need a runtime check against a bound, as `tcp_listen`'s port
  has (it traps), and the report would name the bound rather than what the program claims.

## 9. What it is checked by

(Filled in with the measurements; see the final section of this document as built.)

## 10. What is not verified

(Filled in with the measurements.)
