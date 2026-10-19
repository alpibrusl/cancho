# Processes: running a program, with an authority that says which

> **Status: slices 0 to 6 are built; slice 6 is letting go of a child (§4.11, #524) and signalling its process group
> (§4.12, #533). Nothing in this document is designed-only.** Checked against `main` at the time of #536, by `grep` of
> `std/process.cho`, `crates/cancho-ir/src/builtin.rs` and `crates/cancho/tests/conformance/` for each name:
>
> | Slice (§7) | What | Where it is | Built |
> |---|---|---|---|
> | 0 | close-on-exec everywhere (§4.5) | every backend; `close_on_exec.rs` | yes |
> | 1 | `Exec`, `exec_spawn`, `child_wait`, `child_kill`, `pipe_open`, `pipe_read`, `pipe_write`, `pipe_nonblocking`, `pipe_close`, `child_end_close` (§3.2) | `builtin.rs`; `processes.rs` | yes |
> | 2 | `poller_add_pipe`, `poller_add_child` (§4.8) | `builtin.rs`; `processes.rs` | yes |
> | 3 | `std.process`: `argv`/`add`/`list`/`count`/`drop`, `channels`, `close`, **`capture`** (§7.1) | `std/process.cho`; `capture.rs` | yes |
> | 4 | the MCP server (cancho-tools#10) | in cancho-tools (cancho-tools#17), on `std.process` | yes, outside this repository |
> | 5 | `exec_spawn_in` (§4.10), `std.process.channels_with_errors` and **`capture_both`** (§7.2) | `builtin.rs`, `std/process.cho`; `processes.rs`, `capture.rs` | yes |
> | 6 (edition 9) | `exec_spawn_detached`, `child_pid`, `child_release` and the label `exec_detached` (§4.11, #524) | `builtin.rs`; `process_release.rs` | yes |
> | 6 (edition 9) | `child_kill_group` (§4.12, #533), and `capture` using it | `builtin.rs`, `std/process.cho`; `process_group.rs` | yes |
> | 6 (edition 9) | `exec_covers` and `fs_covers` (§4.13, #535): is this path inside the bound, by the rule the operations trap on | `builtin.rs`, `memory.rs`/`fs.rs` of both backends; `path_covers.rs` | yes |
>
> What is still only designed or open is §9, and only that. *Corrected (#536):* this line used to say "slices 0 to 2
> built; slices 3 and 4 designed, not built", which was true when slice 2 merged and was never revised when `capture`
> (#276), the MCP server (cancho-tools#17) and slice 5 (#292) were. A reader took it to mean `std.process` did not
> exist.
>
> Written before the code, the way
> [`filesystem.md`](filesystem.md), [`net.md`](net.md) and
> [`signals.md`](signals.md) were. When building it disagrees with this
> document, this document is corrected in place.
>
> Two askers ([`CONTRIBUTING.md`](../CONTRIBUTING.md)):
> [#237](https://github.com/alpibrusl/cancho/issues/237) part (a), a test
> that starts a service, kills it and restarts it; and
> [cancho-tools#10](https://github.com/alpibrusl/cancho-tools/issues/10),
> an MCP server that runs each tool as its own process so each keeps its
> own authority. [`editions.md`](editions.md) §6.4 has expected an `Exec`
> since edition 2.

---

## 1. Why

cancho has no way to start a process. A program that needs one declares
`fork`, `execve` and `waitpid` through `Ffi("libc")`, and
[`reach.md`](reach.md) §5 is what that costs: `Ffi("libc")` is every
authority at once. The report says `UNBOUNDED`, and a supervisor holding
`lex-os`'s grant (`filesystem`, `network`, `exec`) cannot decide the third
dimension at all ([`under-a-grant.md`](under-a-grant.md) §2: *"`exec`: No"*).

So the gap is the same one `Net` closed for the network: the authority
has no name of its own. This document gives it one, and most of the work
is saying exactly what that name does and does not bound (§2), because
running a program is unlike every capability cancho has so far.

### 1.1 What the two askers need

| | #237 (a test harness) | cancho-tools#10 (an MCP server) |
|---|---|---|
| start a built program with arguments | yes | yes |
| environment | yes, chosen by the test | none |
| feed standard input | sometimes | yes (`write --stdin`) |
| read standard output | sometimes | yes, bounded |
| kill it | yes, `KILL` and `TERM`, at a moment the test chooses | yes, on a timeout |
| its exit status | yes | yes |
| wait for it beside sockets and a deadline | yes (fake peers in the same program) | yes (a timeout) |

Neither asks for a shell, for `PATH` lookup, or for the child to share the
parent's console. Both want the child gone or reaped on every path.

---

## 2. What executing a program is an authority over

Every capability so far bounds what *this* program does: `Fs("/tmp")` is
the files it touches, `Net("api:443")` the host it dials. Running a
program is different in kind:

> **Executing a program delegates.** The child does whatever its own
> binary does, with its own authority, and the parent's row cannot state
> that authority, because it is a property of a file the parent did not
> compile.

That leaves two honest things a capability can bound, and this design
bounds both:

1. **Which programs may be started.** `Exec("/opt/cancho-tools/bin")` may
   start what lies under that directory and nothing else. That is a path
   prefix, with `Fs`'s rules (§4.1), and it is the part a grant can check:
   `exec("p")` in a report is a static bound, as `fs_read("p")` is.
2. **What the parent hands the child.** A child must get no authority the
   parent did not deliberately pass: no inherited descriptor (§4.5), no
   inherited environment (§4.3), no inherited console (§4.4), and no
   blocked signal (§4.6). Without this, `Exec` would be a way to move
   authority out of the type system: release `Fs` in `main`, but leave an
   open `Dir` for a child to inherit.

What the design does **not** claim, stated here rather than discovered
later:

* **The child's own authority is the child's.** A report that says
  `exec("/opt/cancho-tools/bin")` composes with the reports of what lies
  there. When those are cancho programs, each carries its own (`cancho
  authority`, or, for cancho-tools, `<tool> introspect`), and the closure
  is computable. When they are not, it is not. The report says *exec*, so
  a supervisor knows the closure is a question.
* **The bound is on the name, not on the bytes.** A script under the
  prefix whose `#!` line names `/bin/sh` runs `/bin/sh`. A symbolic link
  under the prefix that points outside it is followed (§4.1). This is
  `Fs(prefix)`'s position exactly, with `filesystem.md` §2.1's
  reasoning: containment against a *hostile* program is a sandbox's job
  (`lex-os`), and this is containment against an honest program's
  mistakes, plus a report that says what to put in the sandbox.

---

## 3. The design

Edition 7: purely additive ([`editions.md`](editions.md) §5), so no
earlier file changes.

### 3.1 Types

| type | what it is | consumed by |
|---|---|---|
| `Exec("prefix")` | a capability carrying a path prefix, like `Fs`; borrowed shared | `release` |
| `Child` | a started process, not yet reaped: `res`, one leaf (the pid) | `child_wait`, or `child_release` (§4.11) |
| `Pipe` | the parent's end of a channel to a child: `res`, one descriptor | `pipe_close` |
| `ChildEnd` | the child's end of the same channel, before it is handed over: `res` | `exec_spawn` (as a `Stdio`), or `child_end_close` |
| `Stdio` | `enum { Null, Pipe(ChildEnd), File(File) }`: what one of the child's three streams is | `exec_spawn` |
| `Piped` | `enum { Ok(Pipe, ChildEnd), Failed(int) }` | `match` |
| `Spawned` | `enum { Ok(Child), Failed(int) }` (`errno`) | `match` |
| `Exited` | `enum { Code(int), Signaled(int), Failed(int) }` | `match` |
| `Split` (edition 7) | edition 6's eight fields and `exec: Exec("")` | destructuring |

### 3.2 Builtins

| builtin | signature | row |
|---|---|---|
| `pipe_open` | `() -> Piped` | none (an unnamed channel inside this process reaches nothing) |
| `exec_spawn` | `(&Exec("p"), path: &[byte], args: &[byte], env: &[byte], stdin: Stdio, stdout: Stdio, stderr: Stdio) -> Spawned` | `exec("p")` |
| `exec_spawn_in` | `(&Exec("p"), &Dir, path: &[byte], args: &[byte], env: &[byte], stdin: Stdio, stdout: Stdio, stderr: Stdio) -> Spawned` | `exec("p")`; `exec_spawn` whose child starts in the `Dir` (§4.10) |
| `exec_spawn_detached` (edition 9) | as `exec_spawn` | `exec("p")`, `exec_detached`; the child leads a session of its own (§4.11) |
| `child_wait` | `(Child) -> Exited` | none (ending a handle is not using one) |
| `child_release` (edition 9) | `(Child) -> int` | `exec_detached`; the child runs on and is reaped by the runtime (§4.11) |
| `child_pid` (edition 9) | `(&Child) -> int` | none |
| `child_kill` | `(&Child, signal: int) -> int` | `child_signal` |
| `child_kill_group` (edition 9) | `(&Child, signal: int) -> int` | `child_signal`; `child_kill`'s signals to the process group the child leads (§4.12) |
| `exec_covers` (edition 9) | `(&Exec("p"), path: &[byte]) -> bool` | none; whether `exec_spawn` would take `path` or trap (§4.13). `fs_covers` is the same question of an `&Fs("p")` and the file operations |
| `pipe_read` | `(&!Pipe, &![byte]) -> Received` | `pipe_read` |
| `pipe_write` | `(&!Pipe, &[byte]) -> Sent` | `pipe_write` |
| `pipe_nonblocking` | `(&!Pipe) -> int` | none |
| `pipe_close` | `(Pipe) -> int` | none |
| `child_end_close` | `(ChildEnd) -> int` | none |
| `poller_add_pipe` | `(&!Poller, &Pipe, token, events) -> int` | `poll` |
| `poller_add_child` | `(&!Poller, &Child, token) -> int` | `poll` |

`Received` and `Sent` are the socket handles' enums
([`native-sockets.md`](native-sockets.md) §3.1): a pipe is a byte stream
to a peer, and its outcomes are a socket's, `Again` included.

Discharge follows the existing pattern: owning an `Exec("p")` discharges
`exec("p")`; owning a `Child` discharges `child_signal`; owning a `Pipe`
discharges `pipe_read` and `pipe_write`; a `World` discharges `exec("")`.

### 3.3 One example

The MCP server's call, in full: start one tool with a fixed argument
list, give it a request on standard input, read its answer, reap it.

```cancho
edition 7;

// A sketch: the request is written and the answer read in the elided part.
fn call[&h, &x, &q](heap: &!h Heap, exec: &x Exec("/opt/cancho-tools/bin"), request: &q [byte])
    -> [heap, exec("/opt/cancho-tools/bin"), pipe_read, pipe_write] (buffer.Buffer, int) {
    var answer = buffer.empty(heap, 4096);
    var status = 0 - 1;
    match pipe_open() {
        Piped::Failed(e) => { }
        Piped::Ok(to_child, child_in) => {
            match pipe_open() {
                Piped::Failed(e) => { pipe_close(to_child); child_end_close(child_in); }
                Piped::Ok(from_child, child_out) => {
                    // `\0`-separated (§4.2); a real caller builds this with `std.process.Argv` (§7.1).
                    let args = "--root\0/work\0--create\0--stdin\0notes.txt\0";
                    match exec_spawn(exec, "/opt/cancho-tools/bin/write", args, "",
                                     Stdio::Pipe(child_in), Stdio::Pipe(child_out), Stdio::Null) {
                        Spawned::Failed(e) => { pipe_close(to_child); pipe_close(from_child); }
                        Spawned::Ok(child) => {
                            // write the request, close to signal EOF, read to the end, reap.
                            ...
                            match child_wait(child) {
                                Exited::Code(n) => { status = n; }
                                Exited::Signaled(s) => { }
                                Exited::Failed(e) => { }
                            }
                        }
                    }
                }
            }
        }
    }
    return (answer, status);
}
```

~~`std.process` (§7, slice 3) wraps this into `run(heap, exec, path, args,
input, most, timeout)`, which is what the MCP server and a test actually
call~~. **Corrected (#275):** `run` cannot take an `Exec` narrowed
by its caller (§7.1), so the spawn stays here and `std.process.capture`
does the rest: the input written while the output is read, a bound, a
deadline, and the reap.

---

## 4. Semantics

### 4.1 The path, and the bound

* `path` is the program to run, **absolute or relative to the working
  directory, never looked up in `PATH`**. ~~Absolute or relative~~
  **Corrected (#292, §4.10):** a relative path is outside
  every bound but `""`: it does not begin with the prefix, so under a narrowed
  `Exec` it traps like any path outside it (measured with `Exec("/bin")`,
  path `true`, run from `/bin`: `SIGILL`; `every_refused_spawn_traps_on_both_backends`
  has the case). Relative paths are therefore for a program holding
  `Exec("")`, where the working directory is the parent's or, with
  `exec_spawn_in`, a `Dir`'s (§4.10). A program that has no
  environment (cancho programs have none) has no `PATH` to search, and a
  search would make the program that runs depend on something the row
  does not name.
* It is checked against the capability's prefix with `Fs`'s rule
  ([`filesystem.md`](filesystem.md) §1.1): a path prefix extends at a `/`
  or not at all, exactly the prefix is allowed (one program), and `""`
  contains everything. A path containing a `..` component is refused, as
  `Fs` refuses it (§4.1 there). Both are **traps**, for `filesystem.md`
  §4's reason: a path outside the bound is a program doing what its type
  said it would not, not an outcome to handle. *(Added, #535: a program that wants the answer
  as a value asks `exec_covers` first, §4.13; the trap stays as the backstop and its measured behaviour is in §4.13.)*
* A missing program, one that is not executable, or one the kernel cannot
  load is an outcome: `Spawned::Failed(errno)`, with the parent's
  `ChildEnd`s and `File`s closed by the builtin, so a failed spawn leaks
  nothing.
* `argv[0]` is `path` as given. `args` is the rest.

`narrow(exec, "/opt/cancho-tools/bin")` is the same `narrow` `Fs` has, at
compile time, against a literal.

### 4.2 Arguments are `\0`-separated bytes

`args` is one `&[byte]`: each argument followed by a `\0`. `""` is no
arguments. This is C's own encoding, so the builtin builds the `argv`
array from it without copying a byte, and it needs nothing the language
lacks: a slice of slices would need a region per element. An argument
cannot contain `\0` in any encoding a process can receive, so the
separator excludes nothing. A malformed list (not empty, and not ending
in `\0`) is a trap, like an out-of-range index: it is a program bug, not
a kernel outcome. `std.process.Argv` builds one in a buffer (§7.1).

### 4.3 The environment is exactly what is passed

`env` is the same encoding, each entry `NAME=value`. The child's
environment is **exactly** `env`: nothing is inherited, and `""` is an
empty environment. A cancho program cannot read its own environment, so
it could not forward it knowingly. Inheriting it silently would hand the
child an authority (`HOME`, credentials in variables, `LD_*`) that no row
names.

One rule on what may be passed: **an entry whose name starts with `LD_`
or `DYLD_` traps.** Those variables make the dynamic loader run code the
parent chose (`LD_PRELOAD=/tmp/x.so`), from a file outside the `Exec`
bound, so passing one would run a program the capability does not name.
That is §4.1's broken promise by another route, so it is §4.1's trap.
#237 item 4 (a preloaded shim) is therefore deliberately out of reach
through `Exec`; the issue already says it can stay a C file.

### 4.4 The three streams are what the parent gives

Each of the child's `stdin`, `stdout` and `stderr` is one `Stdio`:

* `Stdio::Null`: `/dev/null`, opened by the builtin.
* `Stdio::Pipe(end)`: the child's end of a `pipe_open`. The builtin
  consumes it, so after `exec_spawn` the parent **cannot** still hold the
  child's end. That is the classic pipe bug (the parent keeps a copy of
  the write end, so the reader never sees end-of-file and both wait
  forever), and here it is not a discipline: `ChildEnd` is `res`, and
  `exec_spawn` is the only thing besides `child_end_close` that consumes
  one.
* `Stdio::File(file)`: an open `File`, for "write the child's output to
  this file". The parent needed the authority to open it, so passing it
  passes nothing new.

There is **no `Inherit`**. A child that inherited the parent's descriptor
1 would write to the parent's console, and the parent may have released
its `Io`: inheriting would be a way to print without `io_write`. Giving a
child the console is an open question (§9) because the shape that is
exact needs the parent to lend its `Io`, which a `Stdio` value cannot
carry.

`pipe_open` answers two ends of a **socket pair** (`AF_UNIX`,
`SOCK_STREAM`), not a `pipe(2)`. The reason is `SIGPIPE`. A write to a
`pipe(2)` whose reader has gone raises `SIGPIPE`, which ends the parent,
and Linux has no per-write way to suppress it (`MSG_NOSIGNAL` is for
sockets only). A socket pair takes `MSG_NOSIGNAL` on Linux and
`SO_NOSIGPIPE` on macOS, the way `Conn` already does
([`native-sockets.md`](native-sockets.md) §3), so `pipe_write` answers
`Sent::Failed(EPIPE)` like `conn_write` and never raises a signal. The
child sees an ordinary descriptor it reads and writes. Changing `SIGPIPE`'s
process-wide disposition instead was considered and refused for the same
reason `native-sockets.md` refused it: it is global state a library would
change behind the program's back.

### 4.5 No descriptor crosses unless it is given

Today every descriptor the backends open is inherited across `exec`.
Measured, in a cancho program, by reading `/proc/self/fdinfo`: a `File`
(`flags: 0100000`), a `Dir` (`0300000`) and a `Listener` (`02`) all have
`O_CLOEXEC` (`02000000`) clear. Only the `Poller` and the signal
descriptor set it.

So **slice 0** (§7) is a tightening in every edition: every descriptor
the backends open is opened close-on-exec (`O_CLOEXEC`, `SOCK_CLOEXEC`,
`F_DUPFD_CLOEXEC`, and `fcntl` where a platform has no flag). After it,
the child's descriptors are exactly 0, 1 and 2, as `Stdio` says. It is a
tightening rather than an edition matter ([`editions.md`](editions.md)
§5): the only program it changes is one that passed a descriptor to a
child through `Ffi`, which is the leak this closes. `file-writes.md`'s
"No `O_CLOEXEC`" note is corrected by the same change.

> **Built (slice 0).** What each builtin's descriptor now comes from:
>
> | builtins | Linux | Darwin |
> |---|---|---|
> | `fs_read`, `fs_write`, `open_read`, `open_dir` | `openat(AT_FDCWD, path, ... \| O_CLOEXEC)` | the same |
> | `dir_enter`, `dir_open_*`, `dir_list` | `openat(dir, name, ... \| O_CLOEXEC)` | the same |
> | `open_write`, `open_append`, `open_new`, `open_rw` | `fcntl(F_DUPFD_CLOEXEC)` of `fopen`'s descriptor | the same |
> | `tcp_listen`, `tcp_connect`, `bind`, `connect` | `socket(..., SOCK_STREAM \| SOCK_CLOEXEC, ...)` | `socket`, then `fcntl(F_SETFD, FD_CLOEXEC)` |
> | `tcp_accept`, `accept` | `accept4(..., SOCK_CLOEXEC)` | `accept`, then `fcntl` |
> | `poller_new`, `signals_watch` | `epoll_create1(EPOLL_CLOEXEC)`, `signalfd(SFD_CLOEXEC)`, as before | `kqueue`, then `fcntl` |
>
> `creat`, `open` and `dup` are no longer called. The flag values live once,
> in `cancho_ir::open_flags` and `SocketOs`.
>
> **Two windows remain, and both are stated rather than closed.** Where
> Darwin has no flag, `fcntl` follows the call that made the descriptor,
> so an `exec` on another thread between the two would still inherit it;
> and `fopen`'s own descriptor is not close-on-exec for the instant before
> its `fclose`, on both targets. Neither is reachable from a cancho
> program until slice 1 (only a foreign `exec` can start a process, and a
> cancho program has one thread per `spawn` it wrote), and slice 1 closes
> the first one for spawns it makes itself: `posix_spawn` on Darwin can be
> asked to close every descriptor but the three it is given
> (`POSIX_SPAWN_CLOEXEC_DEFAULT`).
>
> **Built (slice 1), and stronger than this section first said.** A child
> started with `exec_spawn` holds exactly its three streams on both kernels,
> whatever the parent holds: macOS with `POSIX_SPAWN_CLOEXEC_DEFAULT`, Linux
> with `posix_spawn_file_actions_addclosefrom_np(3)` (glibc 2.34 and later)
> after the three `dup2`s. Close-on-exec alone could not promise that:
> it covers what a cancho program opens, and a descriptor the program
> *inherited* without the flag (a CI runner hands every process several,
> measured in slice 0's test) would have passed straight through to a child.
> So both windows above are closed for every child `exec_spawn` starts; they
> remain only for a foreign `exec`.
>
> **Checked by** `close_on_exec.rs`: a program runs `system("ls /dev/fd")`
> through `Ffi` once before opening anything and once holding one of
> everything a builtin opens (fourteen descriptors), and the two listings
> must be equal. Before the change the second had twelve more, on both
> backends. Equal rather than `0 1 2`, because macOS CI found the first
> version of the test wrong: the runner hands every process descriptors of
> its own (131 and up), and those pass through a program untouched. `fs_read` and `fs_write` close
> their descriptor before returning, so no child can be shown one, and the
> test cannot tell those two from before.
>
> **Mutants:** each site reverted alone, in each backend, 14 in all; 12
> are killed. The two that survive are `dir_list`'s `O_CLOEXEC`, and they
> are equivalent on Linux: glibc's `fdopendir` sets `FD_CLOEXEC` on the
> descriptor it is given (measured: `F_GETFD` answers 0 before it and 1
> after). The flag stays, because it closes the moment between `openat` and
> `fdopendir` and does not rest on what one libc does.

### 4.6 Signals start at their defaults

A program that claimed `TERM` holds it blocked on Linux and ignored on
macOS ([`signals.md`](signals.md) §5), and `exec` keeps both a blocked
mask and an ignored disposition, so a child would be unkillable by
`TERM`. The spawn therefore sets the child's signal mask to
empty and every catchable signal's disposition to the default
(`POSIX_SPAWN_SETSIGMASK`, `POSIX_SPAWN_SETSIGDEF`). The child starts the
way a shell would start it.

### 4.7 A `Child` is reaped on every path, and a kill cannot miss

`Child` is `res`, and `child_wait` is the only thing that consumes it. So
a program that starts a process and returns without waiting for it does
not compile: the zombie process is a linearity error, not a leak to find
later.

The same fact makes `child_kill` safe. A process that has exited but not
been reaped keeps its pid, so a pid cannot be reused while its `Child`
exists. `kill(pid, s)` on a live `Child` can only reach that child, on
both kernels, with no `pidfd` needed for correctness.

`child_kill(child, signal)` takes one of `std.signals`' fixed bits
([`signals.md`](signals.md) §2.2) and adds one only sending can use:
`KILL` (bit 256). `STOP` and the fault signals are refused
(`Failed(EINVAL)`): stopping a child leaves it in a state `child_wait`
would wait on forever. The answer is `0` or the `errno`; signalling a
child that has already exited answers `0`.

`child_wait` blocks until the child ends and answers `Exited::Code(n)`
(it called `exit(n)`), `Exited::Signaled(bit)` (a signal ended it,
reported in `std.signals`' bits, `0` for one with no bit), or
`Exited::Failed(errno)`. It waits for **its** pid only, never `-1`, so a
thread waiting for one child cannot reap another thread's.

**Letting go (§4.11).** `child_release` is the third way to consume a
`Child`: the process keeps running and the runtime reaps it, so the sentence
above holds for it too.

**What linearity cannot do.** If the parent traps or is killed, its
children are not killed with it. Linux could ask for that
(`PR_SET_PDEATHSIG`), macOS cannot, and `posix_spawn` cannot ask on
either. The guarantee is "reaped on every path the program completes", and
it is stated that way.

### 4.8 Waiting beside everything else

`poller_add_child` registers a child's exit as readable, as
`poller_add_signals` does a signal: a `pidfd` on Linux (`pidfd_open`,
5.3 and later), `EVFILT_PROC`/`NOTE_EXIT` on macOS. `poller_add_pipe`
registers a `Pipe` like a `Conn`. Together they give both askers the loop
they need: a child, its output, a deadline (`poller_wait`'s timeout and
the `Clock`), and, for #237, fake peers on `Conn`s in the same loop.
After `poller_wait` reports the child, `child_wait` answers without
blocking.

**Built (slice 2), and what building it settled.**

* *The `pidfd` lives in the `Child`.* `epoll` watches a descriptor and a pid
  is not one, so `exec_spawn` opens the `pidfd` itself, straight after the
  spawn and before anything can reap the child (an unreaped pid is still its
  own, §4.7). A `Child` is still one word: the pid in the low 32 bits and
  the `pidfd` in the high 32, and `child_wait` closes it as it reaps. The
  alternative, opening it when `poller_add_child` is called, leaves a
  descriptor with no owner: the `Poller` cannot know which to close, and the
  `Child` is consumed by `child_wait` before the `Poller` is. A `pidfd` per
  live child is the price; it is one descriptor, and the same one `child_wait`
  gives back (`reaping_a_child_gives_back_its_pidfd`).
* *`pidfd_open` goes through `syscall`.* `glibc` has a wrapper only from
  2.36, and the number, 434, is the same on both Linux architectures.
* *A refusal is the `errno`, carried.* If `pidfd_open` is refused (a kernel
  before 5.3, or a policy that forbids the call), the high half holds the
  `errno` negated and `poller_add_child` answers it: `ENOSYS`, not `EBADF`
  for a descriptor the program never held. ~~`EMFILE` is not among the
  reachable ones: the descriptor a child's end held is closed before the
  `pidfd` is asked for, so one is always free.~~ **Corrected (#275):**
  `EMFILE` is reachable. A spawn frees a descriptor only when it hands one
  over (a `Pipe` end, a `File`); a `Null` stream is opened in the child, so a
  spawn whose three streams are `Null`, with every descriptor in use, leaves
  none for the `pidfd`. Measured under a limit of 8 with 4 to 7 held: `24`;
  under 9: `0` (`a_child_with_no_descriptor_to_spare_says_emfile`). The child
  is still started and reaped; only watching it is refused. A test installs
  a `seccomp` filter answering `pidfd_open` with `ENOSYS` to reach the
  other path.
* *A channel is watched as a `Conn` is.* `poller_add_pipe` is
  `poller_add_conn` for a `Pipe`: a channel is a socket pair, so there is
  nothing more to it. There is no `poller_remove` for either: closing the
  descriptor (`pipe_close`, `child_wait`) takes it out of the set.
* *macOS does not take a zombie.* `kevent` with `EVFILT_PROC`/`NOTE_EXIT` on
  a child that has already exited and not been reaped answers `ESRCH` (3),
  where a `pidfd` is simply readable. Measured on the first macOS CI run of
  slice 2, and in C on macOS 26.2 (`scripts/kqueue_zombie.c`): 200 children of 200, each confirmed a
  zombie with `waitid(WNOWAIT)` first. A `Child` nobody has reaped still owns
  its pid (§4.7), so `ESRCH` can only mean it has exited, and
  `poller_add_child` says so the way `kqueue` says anything is ready for the
  asking: an `EVFILT_USER` event, `EV_ADD | EV_ONESHOT` with `NOTE_TRIGGER`,
  **in one change**, carrying the token. `poller_wait` reads it as readable,
  as it does the `EVFILT_PROC` one.
  **Corrected (#275):** the first fix sent two changes, an add carrying the
  token and then a trigger, and the second change's `udata` (`0`) replaced the
  first's: the event was reported with token `0` (the second macOS CI run;
  reproduced in C). A later change to a knote rewrites its `udata`, so the
  add and the trigger are one change.
* *Darwin reports an exit once.* `EVFILT_PROC` with `NOTE_EXIT` is
  registered `EV_ONESHOT`, and `poller_wait` reads it as readable. The `pidfd`
  of Linux stays readable until the child is reaped (level-triggered, as the
  rest of the poller is); on macOS the report comes once, which is all a
  program that calls `child_wait` on it needs.

### 4.9 Threads

`posix_spawn` is safe from any thread, and nothing here is process-wide
state except what the kernel already makes so. A child started by one
thread can be waited for by another, since the `Child` moves.

### 4.10 The working directory

A child started with `exec_spawn` starts in its parent's working directory.
A tool that is told to work "in this project" would otherwise have to be
handed an absolute path for everything or to `cd` itself, and the program
that runs it (cancho-tools' future `run`) has already opened that directory
as a `Dir`. So there is a second builtin,

```
exec_spawn_in(&Exec("p"), &Dir, path, args, env, stdin, stdout, stderr) -> Spawned
```

which is `exec_spawn` with the child's working directory the directory the
`Dir` holds.

* *A second builtin, not an eighth parameter.* Edition 7's `exec_spawn` has
  callers (the MCP server's among them), and a parameter added to it would
  stop them compiling. The two lower to one IR node (`Expr::ExecSpawn`, with
  an `in_dir` flag) and one function per backend, so nothing about the
  spawn is written twice.
* *The `Dir` is lent.* It is `&Dir`: shared, not consumed, so the program
  keeps it and can open beneath it, list it and `dir_close` it after the spawn.
  `exec_spawn` consumes a `ChildEnd` because the child's end must not stay
  open in the parent (§4.4); a directory descriptor held open by the parent
  harms nothing, and the child never holds it (below). A `Dir` by value, or a
  `File`, is a type error (`tests/reject/exec_spawn_in_owned_dir.cho`).
* *No new row.* The row is `exec("p")` and no more. The `Dir` came from an
  `Fs` the program already held, and §2's point stands: starting a program is
  an authority over what that program can do, whatever directory it starts
  in. A program holding `Exec("")` and a `Dir` could already start a
  child with a path to everything under it.
* *Underneath.* `posix_spawn_file_actions_addfchdir_np` (glibc 2.29, macOS
  10.15): the descriptor is named to the kernel and the child changes
  directory before it runs the program. Nothing changes in the parent, so
  it is safe from any thread (§4.9), as a `chdir` before the spawn would not
  be. The file actions run in the order they are added, so it is added after
  the three streams and **before** the `closefrom` of §4.5: the `Dir`'s
  descriptor is above 2, and `closefrom` would close it.

**Measured** (`scripts/process_cwd_stderr_measure.py`, ctypes into libc with
the flags `exec_spawn` sets; macOS 26.2 and Linux 7.0 with glibc 2.43):

| | macOS | Linux |
|---|---|---|
| `addfchdir_np` is there | yes | yes |
| the child's `pwd` is the `Dir`'s directory; without a `Dir`, the parent's | both | both |
| the parent's working directory afterwards | unchanged | unchanged |
| a relative program path (`./tool`) resolves against the `Dir`; without one, `ENOENT` (2) | both | both |
| descriptors the child lists (`ls /dev/fd`, `/proc/self/fd`), with the `Dir` and without | equal | equal |
| the `chdir` added **after** `addclosefrom_np` | (no `closefrom`; under `CLOEXEC_DEFAULT` the `chdir` last works) | `EBADF` (9): the spawn fails |
| a descriptor of a regular file | `ENOTDIR` (20) | `ENOTDIR` (20) |
| a descriptor that is not open | `EBADF` (9) | `EBADF` (9) |
| a directory opened for reading that has no search permission | `EACCES` (13) | `EACCES` (13) |
| a `Dir` whose directory has since been removed | the spawn succeeds, the child runs in a directory that is gone (`pwd` exits 1) | the same |

The first three failures cannot come from a `Dir` the type system gave
(`ENOTDIR` and `EBADF` need a descriptor that is not a directory); `EACCES`
can, and it is an outcome: `Spawned::Failed(13)`, with the parent's
`ChildEnd`s and `File`s closed as for any failed spawn (§4.1). The one case
that is not an outcome is the allocator refusing the file action
(`ENOMEM`), which traps as a failed `malloc` for the argument list does; a
dropped `chdir` would start the child in the wrong place, and nothing the
program can arrange reaches that path, so it has no mutant (§8).

### 4.11 Letting go: `child_release`, and a child in a session of its own

[#524](https://github.com/alpibrusl/cancho/issues/524). A server that starts
a job (cancho-code's MCP server starts a build that takes minutes, answers
with a run id at once, and lets a client poll) cannot be written with §4.7
alone. A `Child` has to be waited for, which blocks, or killed, which stops
the job, and a program that does neither does not compile. The server worked
around it with four constant `sh -c` scripts, `ps`, and a heuristic for
whether a pid is still the run's. Three builtins, edition 9, replace that:

```
exec_spawn_detached(&Exec("p"), path, args, env, stdin, stdout, stderr) -> Spawned   // [exec("p"), exec_detached]
child_pid(&Child) -> int                                                            // []
child_release(Child) -> int                                                         // [exec_detached]
```

```cancho
fn start[&x](exec: &x Exec("/opt/builds/bin")) -> [exec("/opt/builds/bin"), exec_detached] int {
    match exec_spawn_detached(exec, "/opt/builds/bin/build", "--all\0", "", Stdio::Null, Stdio::Null, Stdio::Null) {
        Spawned::Ok(child) => {
            var pid = 0;
            borrow child as &c in { pid = child_pid(c); }
            child_release(child);       // the job runs on; nothing is owed
            return pid;                 // the run id the client polls with
        }
        Spawned::Failed(e) => { return 0 - e; }
    }
}
```

* *What each does.* `exec_spawn_detached` is `exec_spawn` whose child leads a
  new session and process group of its own. `child_pid` reads the pid out of
  the `Child`, which is read-only and must come before the release because the
  release ends the handle. `child_release` consumes the `Child`: the job keeps
  running, the parent holds no handle and owes no wait. It answers `0`.
* *Two builtins and a reader, not a changed signature.* `exec_spawn` has
  callers, as §4.10 found, so the detached start is a second builtin that
  lowers to the same IR node (`Expr::ExecSpawn`, with a `detached` flag) and
  the same function in each backend. A detached start with a working
  directory (`exec_spawn_detached_in`) is not built: no program has asked.
* *Release works on any child.* A child from plain `exec_spawn` can be
  released too, and runs on in its parent's session. The two are independent:
  *detached* is about where the child lives, *released* is about whether the
  parent owes it a wait. A server wants both; a test that starts a service and
  lets the harness's end clean up wants release alone.

**A released child is not the parent's zombie.** The weak point of "just let
go" is the process table: a child that ends is a zombie until its parent
reaps it, and a server that releases thousands of jobs over its life would
fill the table. Three mechanisms were considered:

| Mechanism | Why not |
|---|---|
| Ignore `SIGCHLD` (or `SA_NOCLDWAIT`), so the kernel reaps | It is a per-process setting. With it set, `waitpid` for a child the program *did* keep fails with `ECHILD`, so `child_wait` stops working for every other child (measured, Python's `os.waitpid` after `SIGCHLD = SIG_IGN`: `No child processes`, on macOS 26.2 and Linux 7.0) |
| Start the child through a double fork, so init adopts it at once | The started process is then not the parent's child: `child_wait` and `poller_add_child` cannot work on it, and §4.7's guarantee that a pid is not reused while its `Child` exists is gone. A `fork` in a program with threads (§4.9) may call only async-signal-safe functions before the exec |
| **A thread of the runtime reaps it** | Chosen. `child_release` closes the `pidfd` (§4.8) and starts a detached thread whose only work is `waitpid` on **that pid**, then ends. It never waits for `-1`, so it cannot take the status of a child the program kept, and nothing about `child_wait` changes |

Its costs, measured on macOS 26.2 (Cranelift): 200 released children still
running are 200 threads and 3.3 MB more resident memory than none (1.3 MB to
4.6 MB, about 16 KB each); 50 that ended leave no thread and no process
(`ps` finds nothing of any of them while the parent lives, on both backends).
The thread has a 64 KiB stack, since it makes one call. If `pthread_create`
fails (`EAGAIN`), `child_release` answers its error number instead of `0`: the
child still runs, but it is a zombie when it ends until the parent ends. When
the parent ends the threads go with it and the kernel hands the children to
init. The thread inherits the parent's signal mask, so a claimed signal
(`signals.md`) stays blocked in it too, and it installs no handler, so
nothing interrupts its `waitpid`.

**The session.** `exec_spawn_detached` adds `POSIX_SPAWN_SETSID` to the flags
of §4.6 (`0x80` in glibc, `0x400` in macOS's `spawn.h`, both read from the
header). The child is its own session leader and group leader (`pgid` and, on
Linux, `sid` equal to its pid) and has no controlling terminal. That is what
the use needs: a hangup of the parent's terminal, or a `kill -- -pgid` of the
parent's group by a shell or a supervisor, does not reach it. The tests start
two released children and kill the parent's group: the plain one dies with it
and the detached one lives. What it does not do: a container's or a service
manager's own kill (a `cgroup`, a launchd job) takes every process it holds,
session or not; and nothing here keeps a plain child alive when the parent's
group is killed, which is the point of the second builtin.

**What the caller can do with the pid.** `child_pid` answers a number, and
nothing in cancho signals a number: `child_kill` takes a `&Child`, and the
`Signals` capability is about the signals a program claims for itself. That
is deliberate and stays so. A `kill(pid, s)` taking any pid would give a
program holding only `child_signal` a way to signal every process of its user,
which is far more than the label says. The same effect is available today
under the authority it really is: start `/bin/kill` through an `Exec` narrowed
to it (`exec("/bin/kill")` in the report). Because the child leads its own
group, `kill -TERM -- -<pid>` stops the job and everything it started, and
`kill -0 <pid>` asks whether it is there. What a server must remember: a pid
is only a name. After the process is gone and reaped, the number can belong
to another one, and §4.7's guarantee ended with the handle. A server that
polls by pid should record the pid with something that tells runs apart (a
start time, a run directory the job writes into) and not trust the number
alone. A kill-by-pid builtin, with a capability of its own, is §9's open
question, not part of this slice.

**The authority report.** A new plain label, `exec_detached`, performed by
`exec_spawn_detached` and by `child_release`, never by `child_pid`. It is
path-free: *which* programs may be started is still `exec("p")`, and
`narrow` bounds the detached start exactly as any other (the checked path is
the same code, and a test starts one outside the prefix and sees the trap).
So a report that says

```
performs
    exec("/opt/builds/bin")
    exec_detached
```

tells a reader: this program starts only what is under that directory, and
what it starts may be left running when the program ends. The label sits on
the release as well as on the detached start because the release is the act
after which *any* child outlives the program; a program that starts a child
in its own session and waits for it is reported as well, which is the
conservative side. Owning an `Exec` discharges it, as it does `child_signal`
(§3.2), and so does a `World`. It is edition 9 like the three builtins:
nothing an older file can write reaches it ([`editions.md`](editions.md) §5,
an addition is absent from older editions), and a file that already declares
a function called `child_release` is unchanged. `cancho authority`'s
"never touches" list counts it with `exec`: a program with neither still
reports "other programs" as untouched.

WebAssembly has no processes, so the three are refused there like the other
process builtins (`unsupported-on-target`, `tests/reject/exec_spawn_detached_on_wasi.cho`).

### 4.12 Stopping what a worker started: `child_kill_group`

[#533](https://github.com/alpibrusl/cancho/issues/533), found by the cancho-forge spike
(`spike/s7_grandchild.cho`, `spike/RESULTS.md` S7). A supervisor runs a worker that itself starts subprocesses (a
compiler, a test runner) and stops it at a deadline. `capture`'s kill reaches the worker's pid only: measured, with
`/bin/sh -c 'sleep 77 & wait'` and a 300 ms deadline, the `sleep` is still in state `S` when the supervisor has
ended. §4.11 named the way out, `/bin/kill -KILL -- -<pid>` through a narrowed `Exec`, and it costs what the issue
says: `cancho authority` reports `exec("/bin")`, which is wider than "one worker directory" and undoes the point of
a narrow bound, and the pid is a bare number.

```
child_kill_group(&Child, signal) -> int      // [child_signal]
```

`child_kill_group(c, signal)` sends `signal` (the same set as `child_kill`, §4.7: `std.signals`' bits and `KILL`,
256; `STOP` and the faults are `EINVAL`) to the **process group the child leads**: `kill(-pid, native)`. `0`, or the
`errno`. It is edition 9.

**The shape chosen, and what it was chosen over.**

| Alternative | Why not |
|---|---|
| A flag in `child_kill`'s `signal` argument | `child_kill` is edition 7, and edition 7 is closed (§6.4 of `editions.md`): a value that answered `EINVAL` yesterday would send a signal to a group today, in a file that did not change. The argument is also a signal bit; a second meaning in the same integer is the mistake `Stdio::Merge` was kept out for (§5) |
| A `Signals` bit, or a new capability | `Signals` is about the signals a program *claims for itself* (§4.11). The program already holds what this needs: a `Child` it started. A capability for "a group" would have to be handed around beside the `Child` and could be pointed at any group |
| `kill(pid, s)` on a number (`pid_kill`), or a "pid with start time" handle | §4.11 and §9: it needs a capability of its own, since it would reach every process of the user. The issue asks for the signal "held by whoever holds its `Child`"; this is that, and nothing wider |
| A distinct type for a detached child (`DetachedChild`), so a plain child cannot be asked | Every function that takes a `Child` (`child_wait`, `poller_add_child`, `capture`, `child_release`) would have to take both, or `Spawned` would split in two; an edition, for a question the kernel already answers (below) |
| **A second builtin, same row, same signals** | Chosen. The precedent is §4.11's own: `exec_spawn` has callers, so the detached start is a second builtin over the same code. Here `child_kill_group` lowers to the same function in each backend as `child_kill`, with the target negated |

**Only a detached child can be signalled this way, and that holds by construction, not by a check.** `kill(-P, s)`
addresses the group whose id is `P`, and a group's id is the pid of its leader. The child's pid is `P`. A child
started by `exec_spawn_detached` leads a group of its own, so the group `P` exists and is its. A child started by
`exec_spawn` is in its parent's group, whose id is another process's pid; no group is named `P` unless the child
makes one for itself (a shell with job control does, and then the group holds only that child's own descendants).
Linux does not hand the number `P` to a second process while `P` is in use as a pid or a group id, so there is
no group named `P` that is anyone else's (macOS's allocator is not read here; its CI run is what checks it). The answer for a plain child is therefore `ESRCH` (the constant is 3 on both kernels; measured on Linux), nothing
is sent, and **the parent is not signalled**: `kill(-P)` can never address the parent's group, since that group's id
is not `P`. Measured, both backends on Linux: a plain child answers `3` and the program goes on to kill the child and
print what it did (`tests/programs/process_group.cho`, mode `r`). Refusing it at compile time would need the type
split of the last row. A guard remains for what no child can have: a pid of 1 or less would make the call `kill(-1)`
(every process the user may signal) or `kill(0)` (the caller's own group), so those answer `EINVAL` with no call.
No program can reach that guard (a child's pid is never 1), and it has no test and no mutant, for that reason.

**The authority report.** No new label. `child_kill_group` performs `child_signal`, the label of `child_kill`, and a
program that owns a `Child` discharges it (§3.2). It does not need `exec("/bin")`, or any `Exec` at all, beyond
the one that started the worker: a function `fn stop[&c](c: &c Child) -> [child_signal] int` is the whole of it.
`tests/accept/process_group_kill.cho` starts `/bin/sleep` in its own group, stops the group and waits, and its report is

```
performs
    child_signal
    exec("/bin/sleep")
    exec_detached
    io_write
```

and the supervisor's bound can be the worker's directory. Why the same label is honest: the group a detached child
leads is in a session of its own (§4.11), and a process can join a group only from the same session, so the group
holds the child's descendants and nothing else. A signal to it is a signal on processes the program started, which
is what `child_signal` says. What the label does **not** promise is that every descendant is in the group: one that
calls `setsid` or `setpgid` leaves it (a build tool that does is not stopped), and a group is not a cgroup. The
guarantee is the one the issue asked for, the worker's ordinary subprocesses.

**The pid-reuse hazard.** The call takes a `&Child`, not a number. While the `Child` exists the leader's pid is
held, zombie or not (§4.7), so `-P` cannot be another process's group: the hazard of §4.11's `/bin/kill` is
gone for this path. A group outlives its reaped leader while it has members (measured, S7: `Z` after the kill), and
the kernel does not reuse its number as a pid meanwhile. After the group is empty *and* the `Child` has been waited
for there is nothing to signal and no handle to signal with. The test `a_leader_that_has_ended_does_not_hide_its_group`
signals a group whose leader has ended and not been waited for, and reaches the grandchild.

**`capture` uses it.** A supervisor holds the `Child` only inside `capture`, which consumes it, so signalling "after
`capture`" is signalling a number again. `capture` therefore sends the group `KILL` before the pid's, on the paths on
which it kills (the deadline, the output bound, a failed watch). For a plain child the first is `ESRCH` and nothing
more happens, so the behaviour of a plain worker is what it was. What it does not do: a worker that **exits by itself**
and leaves a subprocess behind is not killed (the exit ends the capture, `capture`'s comment says so), and the left
behind process runs on. A caller that wants it stopped holds the `Child` itself and calls `child_kill_group` before
`child_wait`.

**Both backends, and the compiler in cancho.** Cranelift and LLVM emit the call as `child_kill`'s with the pid negated
and the guard above (`child_kill(args, group)` in each). The compiler in cancho (`examples/selfhost`) has it as
builtin 138, `cancho_child_kill_group` in `runtime.ll`; `tables.cho`, `builtins.cho` and `signatures.cho` are
regenerated, so it is not in `unported_builtins.txt`. WebAssembly refuses it as it does the other process builtins
(`tests/reject/child_kill_group_on_wasi.cho`). No new rule: `child_kill_group_is_edition_nine` is `not-a-function`
(an older edition does not have the name) and `child_kill_group_undeclared` is `effect-not-declared`.

**What this does not close** is in §9: after a supervisor *crashes* there is no `Child` to hold, and the group id in
its journal is a number again.

### 4.13 Asking first: `exec_covers` and `fs_covers`

[#535](https://github.com/alpibrusl/cancho/issues/535), found by the cancho-forge spike (`spike/RESULTS.md` S3, gap G2).
§4.1 makes a spawn outside the bound a trap, by design. The issue does not dispute the trap; it says what the trap
costs a **supervisor**: a runtime that enforces policy for agents must turn a denied action into a recorded refusal and
go on, and a bug in its policy layer that lets a bad path through ends the process with no journal entry. cancho-forge
therefore checks the path itself before it spawns, and the capability is only the backstop: *"That duplicates the
bound's logic in program code, which the capability model was meant to avoid."* The issue asks for either a helper that
answers "is this path inside my bound?" with the rule the runtime uses, so the two cannot drift, or a spawn that returns
the refusal as a value.

**Measured first** (this checkout, Linux x86-64; a program that spawns, or opens, the path in `argv[2]` under
`Exec("/usr/bin")` and `Fs("/usr/bin")`; Cranelift, LLVM through the Rust compiler, and LLVM through the compiler in
cancho, which all print the same except where the last paragraph of this section says):

| operation, path | what happened |
|---|---|
| `exec_spawn` `/usr/bin/true` | `Spawned::Ok`, exit 0 |
| `exec_spawn` `/usr/bin`, `/usr/bin/` (the bound itself) | `Spawned::Failed(13)` (`EACCES`): inside, so a value |
| `exec_spawn` `/usr/bin/nonexist` | `Spawned::Failed(2)`: inside, so a value |
| `exec_spawn` `/usr/bin/../../bin/true` (`..`) | **killed, `SIGILL`, exit 132** |
| `exec_spawn` `/usr/bin2/true` (a sibling that shares the bytes) | **`SIGILL`** |
| `exec_spawn` `/bin/true`, `true`, `/usr/bi` | **`SIGILL`** |
| `exec_spawn` `/usr/bin/true..x` (`..` inside a name) | **`SIGILL`**: the rule is on the byte pair, not the component |
| `open_read` `/usr/bin/true`, `/usr/bin` | opened |
| `open_read` `/usr/bin/../etc/passwd`, `/etc/passwd`, `/usr/bin2` | **`SIGILL`** |

So the issue's two lines reproduce for `Fs` as well as `Exec`, on both Rust code generators and on the compiler in
cancho. The same rule is written in **four places**: `checked_path` in `cancho-codegen` and in `cancho-codegen-llvm`
(about 90 lines each, mirrored by hand), `cancho_checked_path` in the compiler in cancho's `runtime.ll`, and
`extends_path` in `cancho-ir`, which `narrow` and the effect rows use at compile time. A fifth, in a program, is what
the issue is asking not to need.

**What was chosen: one predicate per capability, over the same emitted check.**

```
exec_covers(&Exec("p"), path: &[byte]) -> bool      // [] -- edition 9
fs_covers(&Fs("p"), path: &[byte]) -> bool          // [] -- edition 9
```

`true` exactly when `exec_spawn` (`open_read`, `fs_remove`, ...) under that capability would **not** trap on `path`. It
touches nothing, so it has **no row**: a function that asks has the row `[]`, and a program that asks before it spawns
has the authority report of one that does not (`tests/accept/path_covers_asks_first.cho`: `io_write` and nothing
else). The bound is read off the capability's type at the call, as the operations read it, so it cannot be asked about
a different bound than the one that will be enforced.

*The rule* is `filesystem.md` §1.1 and §4.1 with its corners written down, because the predicate has to say them. A
path is **outside** when any of these holds, and covered otherwise:

1. it is 4096 bytes or more (the operations copy it to a 4096-byte buffer with its NUL and refuse a longer path rather
   than cut it);
2. it is shorter than the bound;
3. it has `..` anywhere (two dots in a row, so `true..x` as well, which is stricter than a component rule);
4. within the bound's length its bytes differ from the bound's;
5. the bound is not empty and does not end in `/`, the path is longer than the bound, and the next byte is neither `/`
   nor NUL (so `/bin2` is not under `/bin`; a NUL ends the C string the kernel sees at the bound, so it is the bound).

The empty bound contains everything (1 and 3 still apply). A relative path is outside every bound but `""`. Nothing is
normalised and no file is looked at: a symlink inside the bound that points out of it is inside (`filesystem.md` §2.1
says what `Fs` is not).

**How the two cannot drift.** There is one emitter. In each code generator the loop of `checked_path` became
`path_escapes(prefix, path, copy_to)`, which builds the flag "outside" from the five conditions; `checked_path` is
`path_escapes`, a trap on the flag, and the terminating NUL; `exec_covers` and `fs_covers` are `path_escapes` and the
negation. The compiler in cancho does the same in `runtime.ll`: `cancho_path_covers` is the rule, `cancho_checked_path`
calls it and traps on `false`, and the new call is `cancho_path_covers` as it stands. A change to the rule is a change
to the one function the trap and the answer both use. The remaining copy is `extends_path`, which decides at compile
time for literals (it has no `..`, length or NUL case, which a literal's rows and `narrow` do not need); `path_covers.rs`
holds it to the same table of paths as the emitted check, for every path without those three.

**Chosen over, and why:**

| Alternative | Why not |
|---|---|
| **(2) A checked spawn**, `exec_spawn_checked(...) -> Spawned` with a `Spawned::Refused` arm or `Failed(EACCES)` | Smaller for the supervisor in lines, larger in the language. `Spawned::Refused` would make every `match` on a `Spawned` in every edition non-exhaustive, so it is a new type or a new edition for the enum, and `Failed(errno)` would confuse "policy said no" with the kernel's own `EACCES`, which §4.1 already returns for the bound itself (measured above). A new seven-argument builtin duplicates `exec_spawn`, `exec_spawn_in` and `exec_spawn_detached` (three), and **it answers only spawn**: `Fs` has `open_*` (five), `fs_read`, `fs_write`, `fs_rename`, `fs_remove`, `tty_open`, and the supervisor of the issue reads and writes under `Fs` bounds as much as it spawns. A predicate closes every one of them with two names. And the refusal must be a *value the supervisor can journal with the reason*, which the predicate gives before anything is attempted |
| **(3) Both** | The predicate is the whole need, and (2) can still be added later by whoever has an asker: nothing here makes it harder, and the bar is two askers (`CONTRIBUTING.md`). One asker, a supervisor, has asked, and it asked for either |
| A predicate over two *values*, `path_within(prefix: &[byte], path: &[byte])` | It is the second copy of the rule that the issue wants gone, and the prefix would have to be a string the program wrote, which can differ from the capability's bound. The bound is read from the capability so the question and the enforcement share their prefix |
| Changing `exec_spawn` to return `Failed` instead of trapping | Changes the meaning of every unchanged file (`editions.md` §5): a program relying on the trap as its backstop would continue with a value. §4.1's reasoning, that a path outside the bound is a program doing what its type said it would not, stands for a program that did not ask |
| One builtin for both capabilities | The parameter would be "a capability with a prefix", which the type system has no name for; the two builtins share their lowering (`Expr::PathCovers`) and their emitter |

**Edition 9, and the targets.** Both names are ones a program may declare for itself (cancho-forge does), and edition
7 is closed (`editions.md` §6.4), so they are absent before edition 9 (`tests/reject/exec_covers_is_edition_nine.cho`,
`fs_covers_is_edition_nine.cho`) and the identity of no earlier file moves. `exec_covers` is refused on WebAssembly with
the other `Exec` builtins (`tests/reject/exec_covers_on_wasi.cho`, `unsupported-on-target`); `fs_covers` is not, since
`Fs` is on that target and the question is the one `open_read` is asked (`os_tables.rs`: 168 builtins, 75 refused, 93
supported; not run under `wasmtime`, which this checkout does not have). Rules: none new. A non-`Exec` first argument
is `capability-misused` (`exec_covers_not_an_exec.cho`, `fs_covers_not_an_fs.cho`), a wrong count `arity-mismatch`
(`exec_covers_arity.cho`).

**The compiler in cancho.** `exec_covers` and `fs_covers` are calls 14 and 15 of `capability_call` in
`examples/selfhost/exprs.cho`; the tables are regenerated, `runtime.ll` and `runtime.cho` carry
`cancho_path_covers`, and nothing is added to `unported_builtins.txt`, `reject_gaps.txt` or the known differences. To
make room (`exprs.cho` was 1,998 lines against a ceiling of 2,000) the dozen `is_word` ifs that chose the call moved
to `filecode.capability_which`, unchanged apart from being a function, and `atoms.push_prefix`, which marks "performed
a label with an argument", is split from `atoms.push_bound`, which only pushes the bound: that is what keeps the
selfhost row of a program that asks empty. **A finding, and a correction in place:** asking the question of the
compiler in cancho found that its `cancho_checked_path` *trapped* on a NUL right after the bound (`"/bin\0etc"` under
`Exec("/bin")`) where both Rust code generators have always taken it as the bound and returned
`Spawned::Failed(13)` (measured, the frozen compiler of this session: `SIGILL` through `--compiler auto`, `failed
errno 13` through the other two). It now takes it, as the oracle does (`a_nul_after_the_bound`). It is a refusal
becoming an acceptance, in a compiler that is not yet what builds a program by default alone, and for a path the
operation would have cut at the NUL anyway.

**What this does not do.** It does not remove the trap: a program that does not ask, or asks about another path than
the one it spawns, still traps, which is the point of a backstop. It answers the textual rule only; a path that is
covered can still fail (`Failed(ENOENT)`) or name a symlink that leaves the bound. It covers `Exec` and `Fs`; a
`Net("host:port")` bound traps the same way (`connect`) and has no asker. The `Dir` operations take a name relative to
a handle and check that, not a bound. macOS is unmeasured: the rule is bytes, the code is shared by both targets, and
the CI run on darwin-aarch64 is what checks it.

### 4.14 A bound chosen at deployment: measured, and the decision it waits for

[#534](https://github.com/alpibrusl/cancho/issues/534), found by cancho-forge (`spike/RESULTS.md` S3, S5, gaps G1 and G9)
and the "A bound chosen at deployment" row of §9. **Status: design only, no code.** What follows measures what the compiler
does today, finds that half of the issue is answerable by a handle that already exists, and stops at a choice that is the
author's (the issue says "shape is the author's"). Nothing here changes what any program means.

The issue asks two things. **(1)** `narrow` takes a literal, so a program cannot be given its workers directory or
workspace when it is deployed; the only alternative is to hold `Exec("")`/`Fs("")` and check paths in program code, and the
report then says `exec("")`. **(2)** A library function cannot be generic in the prefix, so a reusable file layer cannot be
narrowed by its caller. A second asker for (1) exists, so the bar of two (`CONTRIBUTING.md`) is met for `Exec`:
cancho-tools' MCP server holds `Exec` narrowed to its tools' directory "baked in at build, since `narrow` takes a literal"
(ROADMAP, cancho-tools#17), and `examples/tls_echo_fixed` refuses `--dir` for the same reason.

**Measured first** (this checkout, Linux x86-64, `cancho check` and `cancho authority`; nothing run on macOS):

| program | result |
|---|---|
| `narrow(fs, arg(g, 1))` | `error: narrow takes a literal, so the refinement can be checked where it is written` |
| `fn f(fs: &f Fs(""), ..)` called with a `Fs("/tmp/conf")` | `error: expected "", found "/tmp/conf"` (the issue's, reproduced) |
| `fn f(fs: &f Fs("/tmp/conf"), ..)` called with a `Fs("/tmp")` | `error: expected "/tmp/conf", found "/tmp"` (so it is invariance, not only "no narrower into wider") |
| `fn f(fs: &f Fs("/tmp"), ..)` called with a `Fs("/tmp")` | accepted; a library fixes its prefix in its own text, as the issue says |
| a library `fn f(dir: &d Dir, path: &p [byte]) -> [dir_read] int` over `std.dirs.open_file`, handed a `Dir` that `main` opened with `open_dir(fs, arg(g, 1))`, edition 6 | accepted and built; run on a directory named at the command line it opened `x` (exit 0), and on a missing directory it answered `ENOENT` (exit 2). Its report: `dir_read`, `fs_read("")`, `args` |

*Why the type is invariant (an argument, not a measurement).* Every operation takes its bound from the capability's type
at the call (`Expr::PathOp`, `OpenFile`, `ExecSpawn`, `PathCovers` and the others carry `prefix: String`, and a capability
is leaf-free: it is not a value at run time, `defs.rs` `leaf_free`). A callee written for `Fs("/tmp")` that was handed an
`Fs("/tmp/conf")` would therefore check against `/tmp`, which is a **widening**. Subtyping on the prefix is unsound until
the bound travels with the value, and that is the whole of what (2) asks for.

**What already answers the `Fs` half.** `Dir` (`directory-handles.md`, edition 6) *is* a bound chosen at run time. It is a
descriptor, opened once from a path under the held `Fs`, and everything beneath it is a single component resolved from
the descriptor, with no `..` and no link followed. A library over `&Dir` is generic in the directory, its row is
`dir_read`/`dir_write` and names no path, and it can create, append, rename, remove and sync (slice 2 there), which is
what a journal does. What it does not give: a **file** as the bound (`Fs("/tmp/app.log")`), and a report that names the
directory (`open_dir` spends `fs_read("")` when the path is not known to `main`, as `directory-handles.md` §3 already says
about `cancho-tools`). Whether it serves cancho-forge's journal is for cancho-forge to say; it was not tried there.

**What answers nothing: `Exec`.** There is no handle for "the programs in this directory". `exec_spawn_in` takes a `Dir`
only as the child's working directory, and the program's path is still checked against the capability's literal. §9 has
this as "Spawning beneath a `Dir`" with the reason it waits.

**The options**, from the smallest:

| | What it is | Answers | What it costs |
|---|---|---|---|
| **(a)** Nothing for `Fs`, name the pattern | Document `Dir` as the answer to (1) and (2) for `Fs`, with the limits above, and close that half of the issue | `Fs` (1) and (2), for a directory | Nothing is built. The report still says `fs_read("")` at the open |
| **(b)** `Exec` beneath a `Dir` | `exec_spawn_under(&Exec, &Dir, name, ...)`: the program is one component named in a run-time directory, the child starts in it (the `addfchdir` of §4.10 already exists; its availability on macOS is the part CI runs and this checkout did not), and a new pathless label (`dir_exec`) says "the programs of a directory chosen at run time". Edition 9 | `Exec` (1), the cancho-tools and cancho-forge case | The unmeasured parts: whether the component walk gives the same guarantee for a program (a link as the *last* component is followed by `exec`; `Dir` refuses it with `O_NOFOLLOW` for a read), and what `Exec`'s own bound is (`Exec("")`, so the report says `exec("")` beside `dir_exec`, or a new capability) |
| **(c)** The bound travels with the value | A capability carries its bound at run time: `narrow` takes a `&[byte]`, checks it against the static bound with the same emitted path check (§4.13), refuses by a value, and the operations check the static bound **and** the runtime one. The type stays the outer bound, so the row stays exact and the report can say "deployment-bound, within `p`". Prefix subtyping then becomes sound | `Exec` and `Fs`, a file as well as a directory, (1) and (2) | The decision below. Capabilities stop being zero-sized, which `leaf_free`, the `Split` layout, borrows, `narrow`'s tuples and `release` all assume; `prefix` appears on 140 lines of the two Rust code generators and 76 of the compiler in cancho (counted, not estimated); **and the bytes need an owner**: a borrowed `&[byte]` cannot live inside a capability (a `Stdio` cannot carry a borrow, §9), so either the bound is a copy on the heap and the capability must be freed (a new linear obligation on a type that is `release`d today), or it is restricted to a slice that lives as long as the program (`Args`) |

**Recommendation.** Take (a) (this section; `AGENTS.md` once it is accepted) and build (b) as the next slice: it reuses the one
run-time-bounded handle the language already has, with its ownership solved (`res`, closed), it is the row §9 already
lists as waiting, and it needs no change to the zero-sized capability. Do **not** build (c) until a program asks for what
(a) and (b) cannot express (a single file, or a bound that is a prefix and not a directory), because (c) changes the
representation every capability has had since §8.1 of `linearity-and-effects.md`, and the lifetime question has no answer
in the documents.

**What is genuinely the author's to decide** (so this section stops here): (i) whether "a directory, opened once" is an
acceptable shape for "a bound chosen at deployment", or whether the bound must be a *prefix* the type can show; (ii) if (b),
whether a program found through `Dir` may be a link (the `exec` of a link is the program the link names), and whether
`Exec` keeps a literal bound beside the handle or a new capability replaces it; (iii) if (c), who owns the bytes. None of
these is answered by `filesystem.md`, `authority.md` or `directory-handles.md`.

**What is not measured.** macOS (the `Dir` measurement above is Linux only; `directory-handles.md` says Darwin is written
and run by CI); WebAssembly; cancho-forge's journal against `Dir` (not tried); the cost of (c) beyond the line counts.

---

## 5. Alternatives considered

| Alternative | Why not |
|---|---|
| `fork` and `exec` as builtins | `fork` copies the parent's heap and every thread's locks into a child that runs one thread, and everything between `fork` and `exec` must be async-signal-safe. A language whose backend calls `malloc` cannot promise that. `posix_spawn` is the operation both askers mean, and it does the descriptor and signal setup (§4.4 to §4.6) in the kernel's own order |
| Keep it `Ffi("libc")` | The status quo. `reach.md` §5 and `under-a-grant.md` §4 are the argument: `Ffi` is the one capability whose label does not bound what it authorises |
| One run-to-completion builtin (`exec_run(..., input, output) -> code`) | Serves the MCP server and not #237, which must kill a child at a moment it chooses and serve sockets meanwhile. It is the right *library* function, and `std.process.capture` is it (slice 3, §7.1: the spawn stays with the caller), built on the handles |
| A list of programs as the bound (`Exec("seek,write")`) | `Signals` uses a set because signals are a closed list. Programs are files, and the natural unit a deployment grants is a directory (`/opt/cancho-tools/bin`). A prefix that is exactly one file already expresses "this one program" |
| The child's ends as `File` | A `File` is a regular file: no `Again`, no readiness, and a write to a closed pipe raises `SIGPIPE`. A child's stream is a peer, and cancho already has a handle for a peer's byte stream. `Stdio::File` keeps redirecting to a real file |
| The parent's ends as `Conn` | The same machinery, but `conn_read` in a row would read as *network*. A distinct `Pipe` keeps the row honest about what the program talks to, at the cost of four small builtins over one backend path |
| `PATH` lookup (`execvp`) | §4.1: the program that runs would depend on an environment the row does not name |
| Inherit the environment | §4.3 |
| `Stdio::Inherit` | §4.4: a child would print for a parent that released `Io` |
| `Stdio::Merge` (`2>&1`) for standard error | A fourth variant of `Stdio` stops every edition-7 program that matches `Stdio` exhaustively (the process probe does) from compiling, so it is an edition, not an addition. It also gives the caller one stream where a tool that reports failures on standard error wants two. Measured (`scripts/process_cwd_stderr_measure.py`): with both on one socket the order the child wrote is the order read, 50 of 50 runs of 500 alternating lines on each kernel, so it would be exact; that is what is given up, and §7.2 reads a third channel instead. Open (§9) |
| Spawn beneath a `Dir` (`execveat`) | Would close §2's symbolic-link gap the way [`directory-handles.md`](directory-handles.md) did for files. Linux has `execveat` and macOS has nothing equivalent, and `posix_spawn` takes only a path. Open (§9) |

---

## 6. Underneath

Both targets have `posix_spawn`, `posix_spawn_file_actions_*` and
`posix_spawnattr_*`, all non-variadic, so neither backend calls a
variadic C function for the spawn ([`filesystem.md`](filesystem.md)
§2.2). The `posix_spawn_file_actions_t` and `posix_spawnattr_t` objects
are backend-internal stack buffers, sized per target the way `stat_layout`
sizes `struct stat` ([`directory-listing.md`](directory-listing.md)), and
initialised and destroyed by the libc functions. The `argv` and `envp`
pointer arrays are built in a stack or heap buffer from the `\0`-separated
slices and never reach the program, the way `getaddrinfo`'s result never
does ([`net.md`](net.md) §4.1).

| step | Linux | macOS |
|---|---|---|
| a channel | `socketpair(AF_UNIX, SOCK_STREAM \| SOCK_CLOEXEC)` | `socketpair` then `fcntl(FD_CLOEXEC)` and `SO_NOSIGPIPE` |
| the child's 0, 1, 2 | `posix_spawn_file_actions_adddup2`, `_addopen("/dev/null")` | the same |
| the child's working directory (§4.10) | `posix_spawn_file_actions_addfchdir_np` (glibc 2.29), before the `closefrom` | the same (macOS 10.15) |
| everything else closed | `posix_spawn_file_actions_addclosefrom_np(3)` (glibc 2.34), over close-on-exec everywhere (slice 0) | `POSIX_SPAWN_CLOEXEC_DEFAULT`, over the same (§4.5) |
| signals | `POSIX_SPAWN_SETSIGMASK`, `POSIX_SPAWN_SETSIGDEF` | the same |
| exit readiness | `pidfd_open` (via `syscall`, since glibc only wraps it from 2.36) | `kqueue` `EVFILT_PROC`, `NOTE_EXIT` |
| reap | `waitpid(pid, &status, 0)` | the same |

---

## 7. Slices

| Slice | What | Edition |
|---|---|---|
| **0** | Close-on-exec on every descriptor the backends open (§4.5), with a conformance test that asks a child what it inherited. **Built** | every edition (a tightening) |
| **1** | `Exec`, the `exec`, `child_signal`, `pipe_read`, `pipe_write` labels, `Split`'s ninth field, `pipe_open`, `exec_spawn`, `child_wait`, `child_kill`, the `Pipe` operations; both backends, both targets. **Built** (§8) | 7 |
| **2** | `poller_add_pipe`, `poller_add_child`. **Built** (§4.8, §8) | 7 |
| **3** | `std.process`: ~~`Args` (the `\0` builder) and `run(heap, exec, path, args, input, most, timeout)`~~ `Argv` (the `\0` builder), `channels`, and `capture(heap, clock, child, to_child, from_child, input, most, timeout)`, a bounded capture with a deadline, built on slices 1 and 2. **Corrected (#275):** neither name nor signature could be built; §7.1. **Built** (§8) | 7 |
| **4** | cancho-tools#10: the MCP server, as a cancho program holding `Exec` narrowed to the tools' directory ~~and nothing that writes~~ and no `Fs`, `Net` or `Ffi`. **Built** (cancho-tools#17, §8) | 7 |

| **5** | The two prerequisites of a `run` tool in cancho-tools: `exec_spawn_in` (the working directory, §4.10) and `std.process.capture_both` (standard error beside standard output, §7.2). **Built** (§8) | 7 |
| **6** | Letting go of a child (§4.11, #524) and signalling its group (§4.12, #533): `exec_spawn_detached`, `child_pid`, `child_release`, `child_kill_group`, and `capture` killing the group. **Built** (§8) | 9 |

### 7.1 Slice 3: `std.process`

**Two of §7's promises could not be built, measured with the compiler of
slice 2.**

* *`run` cannot take the `Exec`.* A function's parameter names one prefix,
  and nothing is polymorphic over it (effect-polymorphism.md). A library
  `run` would have to take `&Exec("")`, and a program holding
  `Exec("/usr/bin")` cannot pass it: `expected "", found "/usr/bin"`. The
  MCP server's whole point (§3.3) is holding `Exec` narrowed to the tools'
  directory, so a `run` it can call does not exist. **The spawn stays with
  the caller**, under the caller's own `Exec`, which is where the report
  should see it, and the library does everything after it.
* *`Args` is taken, and so is `arg`.* `` `Args` is a built-in type and cannot be
  redeclared``: it is the argument capability every `Split` holds. The
  builder is `Argv`, and adding to it is `add`, since `arg` is the builtin
  that reads one argument.

**The surface.**

```cancho
pub res struct Argv { .. }                       // a `\0`-separated list, in a buffer.Buffer
pub fn argv[&h](heap: &!h Heap, capacity: int) -> [heap] Argv
pub fn add[&h, &s](heap: &!h Heap, list: Argv, one: &s [byte]) -> [heap] (Argv, int)
pub fn list[&a](list: &a Argv) -> [] &a [byte]  // what `exec_spawn` takes
pub fn drop[&h](heap: &!h Heap, list: Argv) -> [heap] int

pub enum Channels { Ok(Pipe, ChildEnd, Pipe, ChildEnd), Failed(int) }
pub fn channels() -> [] Channels                 // to the child, from the child
pub fn close(to_child: Pipe, from_child: Pipe) -> [] int

pub enum Ran { Code(int), Signaled(int), TimedOut, TooMuch, Failed(int) }
pub fn capture[&h, &c, &i](heap: &!h Heap, clock: &c Clock, child: Child,
        to_child: Pipe, from_child: Pipe, input: &i [byte], most: int, timeout: int)
    -> [heap, clock, poll] (buffer.Buffer, Ran)
```

The MCP server's call (§3.3), with it:

```cancho
match process.channels() {
    Channels::Failed(e) => { ... }
    Channels::Ok(to_child, child_in, from_child, child_out) => {
        match exec_spawn(exec, path, process.list(argv), "",
                         Stdio::Pipe(child_in), Stdio::Pipe(child_out), Stdio::Null) {
            Spawned::Failed(e) => { process.close(to_child, from_child); ... }
            Spawned::Ok(child) => {
                let (answer, ran) = process.capture(heap, clock, child, to_child, from_child,
                                                    request, 1048576, 10000);
                ...
            }
        }
    }
}
```

`capture`'s row is `[heap, clock, poll]`: owning the `Child` and both
`Pipe`s discharges `child_signal`, `pipe_read` and `pipe_write` (§3.2;
checked: a function that owns them and kills, reads and closes declares
none of the three), and rows are exact, so `poll` is there because
`poller_wait` is. Nothing in `std.process` names `exec`: a program's report
says `exec("p")` exactly where it spawns.

**What `capture` does,** and the measurement behind each choice
(`scripts/process_run_measure.py`, run on each kernel). All of it
on one `Poller`: the output channel (readable, token 1), the input channel
(writable, token 2, until the input is written) and the child (token 3).

* *Input and output are interleaved, never write-all-then-read.* A channel
  holds **8,192 bytes on macOS 26.2 and 180,224 on Linux 6.8** before
  `EAGAIN` (an `AF_UNIX` socket pair, default buffers). Writing all the
  input before reading any output, to `cat`, finishes at 16 KiB and
  deadlocks at 32 KiB on macOS, and at 256 KiB and 512 KiB on Linux: the
  child blocks writing output nobody reads, and the parent blocks writing
  input nobody reads. A loop that passed every Linux test would deadlock on
  macOS at a size no Linux test tried, so both channels are non-blocking and
  both are on the poller from the start. The input end is closed once the
  input is written (the child's end of file), at once for empty input, and
  as soon as a write answers `Failed` (a child that stopped reading is not an
  error; its exit status says what it thought).
* *The child's exit ends the capture; the end of the stream does not.* Once
  the poller reports the child, the output is read until `Again` or `End`,
  and the capture stops. A write to a socket has reached the reader's queue
  when it returns, so everything the child wrote is there when it exits:
  for a child writing 1 byte, 64 KiB and 1 MiB and exiting, the bytes
  captured this way were all of them in **600 runs of 600 on each kernel**
  (and the end of the stream had already arrived in every one). Waiting for
  the end instead would wait for any process that inherited the end:
  `sh -c 'sleep 2 & echo hi'` exits at once and its output ends **2.0 s**
  later, on both. A grandchild's output after the child has exited is not
  the child's, and is not captured.
* *A deadline is a `Clock`, not a timeout per wait.* `poller_wait`'s timeout
  restarts at every wake, so `capture` reads `clock_ms` once at the start
  and waits each time for what is left. When nothing is left, the child is
  killed (`KILL`, which cannot be caught or blocked), reaped, and the answer
  is `TimedOut` with what was captured so far.
* *`most` is a bound on what is kept, and exceeding it ends the child.* The
  buffer never grows past `most` bytes. The first byte beyond it kills the
  child, which is reaped, and the answer is `TooMuch` with the first `most`
  bytes. An output bound that kept reading and discarded would still let a
  child run unbounded in time; with a deadline that is bounded too, but a
  tool that is producing more than its caller will read has already failed.
* *Every path reaps.* `child_wait` is the only way out of `capture` that
  ends the `Child`, so linearity says this. `Code(n)` and `Signaled(bit)`
  are `Exited`'s; `TimedOut` and `TooMuch` say why `capture` killed it, rather
  than a `Signaled(256)` that could have come from anywhere.
* *A child the poller cannot watch is killed, not waited on blind.* If
  `poller_new` or `poller_add_child` fails (`EMFILE` §4.8, `ENOSYS` on Linux
  before 5.3), there is no way to wait for the child and the deadline at
  once, and a blocking `child_wait` could outlast any deadline. The child is
  killed and reaped and the answer is `Failed(errno)`. A Linux before 5.3
  therefore cannot `capture`; it can still use the handles.
* *Standard error is the caller's.* `capture` reads one stream: the MCP
  server returns a tool's standard output unchanged with its exit code
  (cancho-tools#10), and #237 drives its children through the handles. A
  caller passes `Stdio::Null` or a `File` for the third stream. **Extended
  (§7.2):** `capture_both` reads it too, for a caller that needs it; `capture`
  itself is unchanged.

**`add` refuses an argument containing a `\0`, as an outcome, not a trap.**
Such an argument would arrive as two, and the second could be anything,
`--root` included. `exec_spawn` traps on an unterminated list because that
is the program's own mistake (§4.2); an argument is data, and the MCP
server's arguments come from a model. So `add` answers `22` (`EINVAL`) and
adds nothing, `0` when it adds, and the caller decides.

**Not in slice 3:** an environment (the MCP server passes none, and #237
builds its own list with `Argv` and calls `exec_spawn` itself); ~~capturing
standard error~~ (built in §7.2); a `capture` for a child started with `Stdio::Null` input
(open `channels` and pass empty input; it costs one socket pair).

**How it was planned to be checked** (§8 has what was built and found;
both backends, both targets): `cat` fed and read 1 MiB, past both kernels'
deadlock sizes; `most` exceeded is `TooMuch` with exactly `most` bytes and
the child reaped; `sleep 30` with a deadline of 200 ms is `TimedOut` within
the deadline and a margin; `sleep 2 & echo hi` answers `hi` well before 2
s; a child that never reads its input still exits `Code(0)`; an argument
with a `\0` is refused and nothing is spawned with it; a caller whose row
leaves out `clock` is refused (`effect-not-declared`; a test, since reject
fixtures are checked without `--std`). Mutants, as for the
slices before: the interleaving (write all, then read), the drain after the
exit, the deadline taken per wait, the bound off by one, and the kill on
each path.

### 7.2 `capture_both`: standard error beside standard output

A tool that runs a program for a model (cancho-tools' future `run`) must
return both what the program printed and what it complained about, and they
are not one stream: an exit code with an empty `stdout` and a reason on
`stderr` is the common failure. `capture` read one channel and left the third
stream to the caller. Both channels are now captured, bounded and under the
deadline, by `capture_both`.

```cancho
pub enum ChannelsWithErrors { Ok(Pipe, ChildEnd, Pipe, ChildEnd, Pipe, ChildEnd), Failed(int) }
pub fn channels_with_errors() -> [] ChannelsWithErrors   // input, output, errors: parent's end then child's
pub fn capture_both[&h, &c, &i](heap: &!h Heap, clock: &c Clock, child: Child,
        to_child: Pipe, from_child: Pipe, from_errors: Pipe, input: &i [byte],
        most: int, most_errors: int, timeout: int)
    -> [heap, clock, poll] (buffer.Buffer, buffer.Buffer, Ran)   // output, errors
```

The caller passes `Stdio::Pipe(child_errors)` as the third stream. `capture`
is now a thin call of the same loop with no errors channel; nothing about
its behaviour changed (its tests and mutants are unchanged and pass).

**What it does, and what was measured**
(`scripts/process_cwd_stderr_measure.py`, part B):

* *The errors channel is read on the same poller, never after the output.* A
  channel holds **8,192 bytes on macOS and 180,224 on Linux**. A parent that
  reads the output to its end and then the errors deadlocks against a child
  that writes more than that to its errors first: measured, 8,192 bytes of
  errors finish and 16,384 deadlock on macOS, 131,072 finish and 262,144
  deadlock on Linux. On one poll set a child writing 1 MiB to each of its two
  streams completes on both. So the third channel is registered beside the
  other two from the start (token 4), non-blocking, and drained whenever
  readable. `a_flood_of_errors_does_not_stall_the_output` is a megabyte of
  errors before any output.
* *The exit still ends the capture, and both are drained to `Again` after
  it.* For a child writing 1 byte, 64 KiB and 1 MiB to each stream and
  exiting, all of both streams were captured in **200 of 200 runs per size on
  each kernel**, with both ends of stream already present in every one. Waiting
  for the end of either would wait for any process that inherited it (§7.1).
* *Each channel has its own bound.* `most` for the output, `most_errors` for
  the errors: what is kept is at most `most + most_errors` bytes of content,
  whatever the child writes. The first byte past either bound ends the child
  (`KILL`), and the answer is `TooMuch` with both buffers as they were: the one
  that overran holds exactly its bound. `TooMuch` does not say which; a caller
  compares the sizes with its bounds (exactly the bound is not an overrun: four
  bytes of output under a bound of four is `Code(0)` in
  `errors_past_their_bound_end_the_child`, four of errors under three is not).
* *One deadline for both.* It is the `Clock`'s, as in §7.1: a deadline that
  passes kills the child, reaps it and answers `TimedOut` with **both**
  buffers as read so far (`a_deadline_returns_both_channels_so_far`: `a` on
  one and `b` on the other, then `sleep 30` under 300 ms).
* *A channel that has ended is closed, and nothing is spun on.* A child that
  closes all three of its streams and sleeps for a second costs the
  capture no processor time by `wait4`'s account (bound 400 ms), as for
  `capture`. Every channel is also given back when the deadline ends the
  capture: sixty captures under a limit of 40 descriptors all complete.
* *The order between the two streams is not kept.* Two channels are two
  queues; a caller that needs the interleaving reads one channel (the
  alternative of §5, and measured there: one socket keeps it exactly).

## 8. What it is checked by

What follows is slice 1 as built and measured; the poller (§4.8) is
slice 2's, and is checked there.

* **Refusals** (`tests/reject/`), each with its rule tag:
  `exec_without_capability` (`capability-misused`), `exec_widened`
  (`Exec("/a/b")` cannot become `Exec("/a")`) and `exec_sibling_prefix`
  (`/opt/x` cannot become `/opt/xevil`) (`capability-not-narrowable`),
  `exec_effect_undeclared` (`effect-not-declared`), `child_unwaited` and
  `child_end_kept` (`linear-value-unconsumed`), `child_taken_apart`
  (`linear-value-taken-apart`), `exec_is_edition_seven` (`unknown-name`).
  One accepted program, `tests/accept/process_spawn.cho`, prints what
  `/bin/echo` wrote down a channel.
* **Conformance tests over real processes**
  (`tests/conformance/processes.rs`). Each case runs on both backends, and
  the two must print the same:
  * the child's argument list (an empty argument, a space inside one) and
    environment are exactly what was passed, and none of the test's own
    environment arrives;
  * a channel carries the parent's bytes to `cat` and back;
  * the child holds 0, 1 and 2 and nothing else, while the parent holds
    eight more channel descriptors of its own and one it inherited
    *without* close-on-exec;
  * `KILL` by its bit is `Signaled(256)`;
  * `TERM` ends a child although the parent holds `TERM` claimed;
  * a bit naming no signal is `EINVAL`;
  * a missing program is `Spawned::Failed(ENOENT)`, and `exit 3` is
    `Code(3)`;
  * writing to a reaped child is `Sent::Failed(EPIPE)`, and the parent
    lives;
  * a child writes to `Stdio::Null` stdout and stderr as to any stream;
  * a path outside the bound, `..`, a sibling of the bound, `LD_PRELOAD`,
    `DYLD_INSERT_LIBRARIES`, and an argument or environment list not
    ending in `\0` each *trap*: `SIGILL` or `SIGTRAP`, not merely a death
    by signal;
  * the authority report says `exec("/bin")` and is bounded.
* **Mutants.** 22 of them, eleven a backend, each run against the process
  tests:
  * the path check, the loader refusal, the unterminated-list trap,
    `closefrom`, the signal mask and default flags, the parent's close of
    the ends it gave, the exit-code decode, the kill bit, `Null` opened
    read-only, and `argv[0]`: all 20 are killed. The two closes are
    killed by a hang: a parent still holding the child's end of its output
    never sees that output end.
  * The signal decode masked with `0x3f` instead of `0x7f` survives, and
    is equivalent: the two masks differ only for signal numbers from 64,
    and no signal a child can be ended by is numbered that high on either
    target.

  The first run left four survivors. Each had the same cause: the tests
  did not look at the thing the check changes.
  * Without its trap, an unterminated list makes `memchr` find nothing,
    and the walk past the end crashes. The test asked only that the
    program die by a signal, so it now asks for the trap's signal.
  * Without `closefrom`, nothing changed, because slice 0 had already made
    every descriptor the parent opens close-on-exec. Only a descriptor
    inherited without close-on-exec tells them apart, so the test now
    starts the probe holding one.
  * `Null` opened read-only went unnoticed until a child wrote to it.

  The equivalent mutant was killed on one backend in each of two runs.
  That led to a flaky case: "writing to a child that has ended" took the
  end of the child's output as proof that its input was closed too. Linux
  releases a dying process's descriptors through deferred work, in no
  promised order, and about 1 run in 10 wrote to a channel the child still
  held. The case now reaps the child before it writes, since reaping comes
  after every descriptor is released. It passed 60 runs in a row.
* **Letting go (§4.11).** Fixtures: `tests/accept/process_release.cho`;
  `tests/reject/child_released_then_used.cho` (`linear-use-after-move`),
  `child_release_undeclared.cho` and `exec_spawn_detached_undeclared.cho`
  (`effect-not-declared`), `child_release_is_edition_nine.cho`
  (`unknown-name`), `exec_spawn_detached_on_wasi.cho`
  (`unsupported-on-target`). Conformance tests over real processes, both
  backends (`tests/conformance/process_release.rs`, driver
  `tests/programs/process_release.cho`): a detached child is alive after its
  parent exited, leads its group and session, and a group signal reaches it;
  of a plain and a detached child released together only the detached one
  survives its parent's group being killed; fifty released children leave no
  zombie while the parent lives, and a child it kept is still waited for;
  releasing one that has already ended answers `0`; fifty releases leave the
  descriptor count as it was (Linux); a detached start outside the prefix
  traps; the report carries `exec_detached`, and `tests/accept/process_spawn.cho`'s
  does not. Mutants: `scripts/process_release_mutants.py`.
* **The bound check under `-O2`.** The first `Exec` program built with the
  LLVM backend exited through the wrong arm: `checked_path` loaded a
  byte past the bound for every index of the path, and the optimiser used
  that poison to delete the path. It was fixed for `Fs` and `Net` as well,
  in #273, which this slice carries (`filesystem.md` §4).
* **Measured on macOS (slice 1).** The first macOS CI run of slice 1 (#274)
  passed `CLOEXEC_DEFAULT`, `SO_NOSIGPIPE` on the socket pair and Darwin's
  signal numbers. It failed one test, and the test was wrong: `ls /dev/fd`
  lists five entries there to Linux's four, and the status line after the
  listing, `== code 0`, had been read as a descriptor.

### Slice 2

* **Conformance tests** (`processes.rs`, the waiter), each on both backends,
  the two to print the same:
  * a channel and a child watched together: while the child sleeps nothing is
    reported, then its output arrives, the channel ends, and the child's exit
    is reported;
  * a child that ended *before* it was registered is still reported;
  * `poller_wait`'s deadline passes with the child running, a kill follows,
    and the exit is reported;
  * sixty children come and go under a limit of 24 descriptors and the next
    is still watchable: reaping gives the `pidfd` back;
  * a kernel that refuses `pidfd_open` (a `seccomp` filter answering `ENOSYS`,
    installed before the waiter starts) is told so by `poller_add_child`,
    and the child is still reaped.
* **Refusals:** `poller_add_child_is_edition_seven` (`not-a-function`) and
  `poller_add_child_effect_undeclared` (`effect-not-declared`).
* **Emission, from any host:** Linux asks for `syscall` (the `pidfd`),
  `epoll_ctl` and `closefrom`, Darwin for `kevent` and none of the three; all
  four triples are accepted by `clang`, and Cranelift builds the Mach-O object.
* **Mutants.** Thirteen, one site each, run against the process tests with
  a limit of five minutes (`scripts/process_mutants.py`; Linux, aarch64,
  kernel 6.8, where every one but the dispatch is on the path taken): the
  `pidfd` not closed, `pidfd_open` asked for pid 1, the refusal's `errno` not
  negated, the `pidfd` watched for `EPOLLOUT`, a missing `pidfd` not checked,
  and `poller_add_pipe` taking the modify path, each on both backends; and
  the `pidfd` read from the `Child`'s low half (Cranelift). **All 13 are
  killed.** Seven fail a test: the two closes leave the sixty-child test
  without a descriptor, and the refusal's `errno` and the missing check are
  caught by both the `seccomp` test and the `EMFILE` one. Six are killed by
  a hang, which a mutant that watches the wrong thing can only be: pid 1's
  `pidfd`, the pid taken as a descriptor, `EPOLLOUT` on a `pidfd`, and a
  channel never added (`EPOLL_CTL_MOD` answers `ENOENT`) are never reported,
  and the waiter waits.
  The first two had been run in an earlier session and killed; that run was
  stopped there, and its script was not kept. This one is in the repository
  and runs all thirteen.
* **Measured on macOS (slice 2).** The first macOS CI run passed the tests
  of live children (output and exit; a deadline, then a kill) and failed the
  two that register a child that had already ended, with `ESRCH` (§4.8). The
  second run, with a fix sending two changes, reported that child with token
  `0` (§4.8 says why). With one change, all the process tests pass on macOS
  26.2 (arm64), and the four poller tests passed 40 runs of 40 there.
* **`EMFILE`** (§4.8): `a_child_with_no_descriptor_to_spare_says_emfile`
  reaches it on Linux and checks that Darwin, which watches the pid, needs no
  descriptor.

### Slice 3

* **`std/process.cho`** (`docs/standard-library.md`): `Argv`, `add`, `list`,
  `count`, `drop`; `Channels`, `channels`, `close`; `Ran` and `capture`, as
  §7.1 gives them. `capture`'s row is `[heap, clock, poll]`, as §7.1 said:
  it calls `drain`, which declares `pipe_read`, and owning the `Pipe`
  discharges it.
* **Conformance tests** (`tests/conformance/capture.rs`, one driver,
  `tests/programs/process_capture.cho`, both backends printing the same):
  * 1 MiB through `cat`, every byte back in order (1,048,576 bytes, 5 to 8
    ms on macOS, where 32 KiB written first deadlocks);
  * exactly `most` bytes is `Code(0)`; one more is `TooMuch` with `most`
    kept, and a child that lingers after its output is killed, not waited
    for;
  * `sleep 30` under a 200 ms deadline is `TimedOut` (203 ms on macOS);
  * `sleep 2 & echo hi` gives `hi` in 4 ms, not 2 s;
  * a child that reads none of 1 MiB of input exits with its own status;
  * a child that closes both streams and sleeps for a second costs the
    capture no processor time (about 0 ms by `wait4`, against a bound of
    400 ms): an ended channel or a gone reader is closed, not spun on;
  * with `pidfd_open` refused (Linux, the `seccomp` stand-in of slice 2),
    `capture` answers `Failed(38)` and kills `sleep 30` rather than wait
    for it;
  * `add` refuses an entry containing a `\0` with `22`, the list runs
    without it;
  * a caller whose row leaves out `clock` is refused, `effect-not-declared`.
* **Mutants.** Eleven (`scripts/process_mutants.py --capture`, Linux
  aarch64): the input written blocking, no drain after the exit, the
  deadline taken per wait, the bound off by one, no kill on each of the
  three paths that need one, the input end never closed, an ended output
  not closed, a failed write not ending the input, and a `\0` let through.
  **The first run killed 7 and left 4**, and each survivor was a test that
  did not look at what the choice changes:
  * *No kill past the bound* survived because closing the output ends a
    child that is still writing (`SIGPIPE`). The child now lingers after its
    output, and only the kill ends it in time.
  * *No kill when it cannot be watched* survived because nothing reached
    that path, and descriptor limits cannot: the most `capture` holds at
    once (four, in `channels`) comes before the poller. The `seccomp` filter
    reaches it, so the test is Linux's.
  * *An ended output not closed* and *a failed write not ending the input*
    each leave a channel that is always ready on the poller, and the loop
    spins until the child exits: the right answer, at the cost of a
    processor. Only the processor time shows it, so a test now measures it.

  With the four sharper tests, **11 of 11 are killed**.
* **Found while building it.** `arg` is a builtin as well as `Args` a type,
  so the builder's operation is `add` (§7.1). And a qualified call is
  resolved against local bindings: with `let max = 3;` in scope,
  `math.max(max, 2)` is refused as "`max` is a local binding". That is the
  compiler's, not this slice's, and is reported separately; `std.process`'s
  driver names its local otherwise.

### Slice 4

Built in cancho-tools (#17); its design and measurements are that
repository's `docs/mcp.md`. What it says about this document's work:

* **The design held.** The server calls `std.process.capture` once per tool
  call and nothing else of slices 1 to 3 beyond `exec_spawn`, which it
  makes under its own `Exec("<bin>")`, as §7.1 decided. Its derived
  authority is bounded: `exec("<bin>")`, `child_signal`, `pipe_read`,
  `pipe_write`, `poll`, `clock`, the console and `args`, and no `Fs`, `Net`
  or `Ffi`. The row's ~~"nothing that writes"~~ is corrected above: the server
  writes its answers to standard output, and writes no file.
* **The bound is a literal at build time.** `narrow` takes one, so the
  server's source names `/opt/cancho-tools/bin` and a build script bakes
  another directory in, as cancho-tools' D14 variant bakes `--root` -- the
  open question of §9's "a bound chosen at deployment", met for `Exec`.
* **A missing flush is invisible to batch tests.** A stdio server's answers
  sit in libc's buffer until `flush_out`; tests that send every request and
  close the input see them at exit anyway. Only a client that waits for each
  answer with its input open catches it, so the server's tests include one.
* **The qualified-call shadowing** (`let name = ...; tools.name(k)` is refused)
  was met twice more there; it is reported against the compiler.

### Slice 5

The working directory (§4.10) and `capture_both` (§7.2): the two things a
`run` tool in cancho-tools needs beneath it. Measured on macOS 26.2 (aarch64)
and Linux 7.0 with glibc 2.43 (x86-64), by
`scripts/process_cwd_stderr_measure.py`; the Linux tests and mutants ran on
the x86-64 host, the macOS tests on the Mac.

* **Code.** `Builtin::ExecSpawnIn` and an `in_dir` flag on `Expr::ExecSpawn`;
  `exec_spawn` in both backends takes the flag and adds
  `posix_spawn_file_actions_addfchdir_np` (declared in the LLVM backend's
  `emit.rs`) between the three streams and the `closefrom`.
  `std/process.cho`: `ChannelsWithErrors`, `channels_with_errors`,
  `capture_both`, and the shared loop `gather`, which `capture` now calls.
* **Refusals.** Two fixtures under `tests/reject/`:
  `exec_spawn_in_owned_dir` (`type-mismatch`) and `exec_spawn_in_arity`
  (`arity-mismatch`). No new rule.
* **Conformance tests**, both backends, both targets. `processes.rs`
  (driver `tests/programs/process_cwd.cho`): the child's `pwd` is the `Dir`'s and
  without a `Dir` the parent's, and a second plain child afterwards shows the
  parent has not moved and `dir_close` still answers `0`; `./tool` is found
  beneath the `Dir` and is `ENOENT` without one; the descriptors the child
  lists are equal with and without the `Dir`; a directory that does not open
  starts nothing; a directory without search permission is `Spawned::Failed(13)`;
  a relative program path under `Exec("/bin")` traps. `capture.rs` (driver
  `tests/programs/process_capture.cho`, new modes): output and errors kept apart
  with the exit status; a megabyte of errors before any output; each bound its
  own (an exact bound is no overrun, one byte past is) and the child killed;
  the deadline returns both buffers; no processor spent on an ended errors
  channel (`wait4`); sixty timed-out captures under a limit of 40 descriptors.
* **Mutants** (`scripts/process_mutants.py`, Linux x86-64): **15 new, 15 killed**, and the eleven of §7.1 re-run against the refactored loop (`--capture`: 17 of 17 killed); the thirteen of slice 2 were not re-run in full (the first seven were, and were killed), since that code did not change.
  The spawn: no chdir, the chdir after the `closefrom`, the chdir to descriptor
  0, the `Dir` closed after the spawn, the wrong argument taken for the `Dir`
  (both backends, nine in all). `capture_both`: the errors never read, read under
  the output's token, bounded by the output's bound, an overrun not ending the
  child, an ended channel not closed, the channel not consumed at the end. Two
  things the first run taught:
  * *A mutant I wrote was equivalent* (`0 * poller_add_pipe(...)` still
    registers; it only dropped the check of the answer). It is replaced by
    one that registers under the wrong token.
  * *The errors drain after the exit has no mutant, on purpose.* Skipping it
    survives, and so would skipping the output's. It is redundant by
    construction: the poller is level-triggered and a child's last write
    precedes its exit, so a channel still holding bytes is in the batch that
    reports the exit, and the event loop has read it before the drain is
    reached. It stays as a guard; no test can tell it from its absence, which
    is the honest reason it is not in the list.
  * *The last, "left open at the finish", is killed by the compiler*: `errors`
    is `res`, so a path that does not consume it does not compile (§4.7's
    argument for `Child`). The 60-capture test is what checks the paths that
    do consume it all do so before returning.
* **Not mutated:** the trap on a refused `addfchdir_np` (`ENOMEM`), which no
  program can provoke (§4.10); the macOS side of the ordering (a `closefrom` is
  Linux's), which the Mac's tests exercise (the same `pwd`, relative-path and
  descriptor cases pass there) but a mutant run on Linux does not.
* **Found.** §4.1 said a path may be relative to the working directory; under
  a narrowed `Exec` it traps (corrected in place, with a test). A removed
  directory is not refused: the child starts in it (measured on both).
  `Stdio::Merge` is an edition (a new variant breaks every exhaustive match of
  `Stdio`), so it was not built.

### Slice 6

Letting go (§4.11) was checked in its own PR (#525); this is what #533 (§4.12) added, measured on Linux 6.18 x86-64 in a
container with no init to adopt orphans, so a killed grandchild stays a zombie (`Z`) and the tests call a process dead
when it is gone or `Z`. macOS: the code is the same call with the pid negated, and was not run.

* **Code.** `Builtin::ChildKillGroup` (edition 9, row `child_signal`, region count 1, refused on WASI);
  `child_kill(args, group)` in both backends negates the pid and answers `EINVAL` with no call for a pid of 1 or less;
  `std/process.cho` is edition 9 and `capture` sends the group `KILL` before the pid's. The compiler in cancho: code 138
  in `codes.cho`, `netcode.cho` and `exprs.cho`, `cancho_child_kill_group` in `runtime.ll`, and the generated tables.
* **Refusals.** `tests/reject/child_kill_group_is_edition_nine` (`not-a-function`), `child_kill_group_undeclared`
  (`effect-not-declared`), `child_kill_group_on_wasi` (`unsupported-on-target`). No new rule. `tests/accept/process_group_kill.cho`.
* **Conformance** (`process_group.rs`, driver `tests/programs/process_group.cho`, both backends): the S7 scenario run
  for real: a detached worker `sh -c 'sleep 77 & echo $!; wait'`, a 300 ms deadline through `capture`, and the grandchild
  (pid printed by the worker) gone or `Z` after the driver ends; the same worker started plain, whose grandchild is
  still `S` (the control: without it the first test could pass for any reason); the answers of `child_kill_group` for a
  detached child (`0`, then `Signaled(256)`), a plain child (`3`, and the program goes on), and a signal that is none
  (`22`); a worker already ended and not waited for, with a grandchild still running, whose group is still reached; and
  the authority report (`child_signal`, `exec("/bin/sleep")`, no `exec("/bin")`, no `kill`).
* **Mutants**, each run to a red conformance test and then reverted. (1) `capture` without the group kill: the S7 test
  fails, `the grandchild survived: Some("S")`. (2) Cranelift with the pid not negated: three tests fail (the plain child
  answers `0` instead of `3`; both grandchildren survive). (3) The Rust LLVM backend (`CANCHO_COMPILER=rust`), the same
  mutation: the same three fail. (4) The compiler in cancho's `runtime.ll` with the pid not negated: the same three fail
  on the `llvm` backend. Not mutated: the `pid > 1` guard, which no program can reach (§4.12).
* **Found.** `docs/wasm.md`'s builtin counts (162, 70 refused) were already stale since #524 (165 and 73) and are 166 and
  74 now, corrected there in place. `--backend llvm` builds through the compiler in cancho by default, so a test run
  without `CANCHO_COMPILER=rust` never exercises the Rust LLVM backend's code: the third mutant needed the variable.

### Slice 6, §4.13: asking first

What #535 added, measured on Linux 6.18 x86-64. macOS and WebAssembly under `wasmtime` were not run.

* **Code.** `Builtin::ExecCovers` and `Builtin::FsCovers` (edition 9, no row, `exec_covers` refused on WASI), lowered
  as `Expr::PathCovers { prefix, exec, args }`. In `cancho-codegen` and `cancho-codegen-llvm` `checked_path` is now
  `path_escapes` plus a trap, and the predicate is `path_escapes` negated. The compiler in cancho: calls 14 and 15 of
  `capability_call`, `cancho_path_covers` in `runtime.ll` (and `cancho_checked_path` calls it), `atoms.push_bound`,
  `filecode.capability_which`, the regenerated tables.
* **Refusals.** `exec_covers_is_edition_nine` and `fs_covers_is_edition_nine` (`not-a-function`),
  `exec_covers_on_wasi` (`unsupported-on-target`), `exec_covers_not_an_exec` and `fs_covers_not_an_fs`
  (`capability-misused`), `exec_covers_arity` (`arity-mismatch`). No new rule. `tests/accept/path_covers_asks_first.cho`.
* **Conformance** (`path_covers.rs`, driver `tests/programs/path_covers.cho` built per bound by Cranelift, the Rust LLVM
  backend and the compiler in cancho): for four bounds (a directory, one program, a directory ending in `/`, none) a
  table of paths, each asked of `exec_covers` and `fs_covers` and **held against** what `exec_spawn` and `open_read`
  really do with it, a value or a signal with nothing printed. The cases include a path outside, `..` at the start, in
  the middle and inside a name, the prefix-sibling `/bin2`, `/binx`, the exact program, the directory with and
  without its slash, a relative path, 4095 and 4096 bytes, and a NUL after the bound; a supervisor that asks and
  survives (`refused`, `journaled`) beside the same path handed straight to `exec_spawn` (killed); and the authority
  report (`io_write` and no `exec` or `fs_*` label).
* **Mutants**, each run to a red conformance test and then reverted. Cranelift: the boundary check removed
  (`/bin2` accepted: three tests fail), `..` not refused (five fail). The Rust LLVM backend: the same two (three and five).
  The compiler in cancho's `runtime.ll`: the same two (three and five). **Drift between the two uses of the rule,** the
  one the design exists to prevent: the predicate alone ignoring the length cap (`the_longest_path` fails, the one test
  with a path of 4096 bytes), and the predicate alone not negated (seven fail).
* **Found.** The compiler in cancho trapped on a NUL after the bound where the Rust code generators return a value
  (§4.13), now aligned. `exprs.cho` was at 1,998 lines of its 2,000, so the dozen `is_word` ifs moved to
  `filecode.capability_which` first.

## 9. Open

| Question | Why it waits |
|---|---|
| Giving a child the console | §4.4. The exact shape needs the parent to lend `&!Io` for the life of the child, and a `Stdio` value cannot carry a borrow. Neither asker needs it |
| Spawning beneath a `Dir` | §5's last row. Linux only, so it needs a decision about a capability that exists on one target |
| ~~The working directory~~ | **Built (§4.10):** `exec_spawn_in`. Open still: a working directory that is not a `Dir` (a path), which needs the `Fs` rule's check and has no asker |
| `Stdio::Merge` | §5. Needs an edition; `capture_both` serves the asker who wanted standard error |
| A bound chosen at deployment | `narrow` takes a literal, so a server whose tools' directory is chosen when it is installed holds `Exec("")` or a literal it was built with. `Fs` has the same question (`authority.md` §2.3, `lines.cho`), and it should be answered for both at once. **Measured and analysed in §4.14 (#534), not built:** for `Fs` a `Dir` already is a run-time bound and a library over `&Dir` is generic in it; `Exec` has no such handle. Waits for the author's choice among §4.14's options (a), (b) and (c) |
| Children of a parent that dies | §4.7. `PR_SET_PDEATHSIG` exists on Linux only. §4.11 is the opposite request, and a detached child is meant to outlive its parent. **Narrowed by §4.12 (#533), not closed:** while the supervisor lives it can stop a worker *and* what the worker started, with the `Child` it holds and no `/bin/kill`. After the supervisor **crashes** nothing changes: the `Child` died with it, an orphaned worker's group is a number in a journal, and stopping it is the next row. Not measured: a supervisor that traps with workers running (the issue's "related, not measured") |
| Signalling a pid the program holds only as a number | §4.11, §4.12. A `kill(pid)` builtin needs a capability of its own, since `child_signal` is about a `Child`; today `/bin/kill` through a narrowed `Exec` does it, and says so in the report. **Narrowed (#533):** for a worker the program still holds, `child_kill_group` does it with no `Exec` and no number. What stays open is the orphan after a crash and the pid-reuse hazard that comes with a recorded number (record it with something that tells runs apart, §4.11); it needs a capability or a handle that survives the process, and no program has asked for more than the issue does |
| `child_kill_group` reaching a descendant that left the group | §4.12. A subprocess that calls `setsid` or `setpgid` is outside the group, and no group signal reaches it. A `cgroup` (Linux) or a job object would; neither exists on both targets, and no asker has needed it |
| A spawn that returns the refusal as a value (`Spawned::Refused`, or a checked variant) | §4.13 (#535). The predicate closes the supervisor's need and the checked variant needs a new `Spawned` or a new builtin per spawn, and answers no `Fs` operation; no program has asked beyond the issue |
| `exec_spawn_detached` with a working directory | §4.11. No program has asked: `exec_spawn_in` does not detach, and the combination is the same node with two flags |
| One reaper thread for all released children | §4.11. A thread per running released child costs about 16 KB (measured); a single reaper needs a table shared between threads, which no program needs yet |
| Mapping `exec("p")` onto `lex-os`'s `exec` level | The same bridge `agent-toolbox.md` §2.5 found missing for `net_out`. The report carries what the bridge needs |
