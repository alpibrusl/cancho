# Processes: running a program, with an authority that says which

> **Status: slices 0 to 2 built (§4.5, §4.8, §8); slices 3 and 4 designed, not built.**
> Written before the code, the way
> [`filesystem.md`](filesystem.md), [`net.md`](net.md) and
> [`signals.md`](signals.md) were. When building it disagrees with this
> document, this document is corrected in place.
>
> Two askers ([`CONTRIBUTING.md`](../CONTRIBUTING.md)):
> [#237](https://github.com/alpibrusl/lex-sys/issues/237) part (a), a test
> that starts a service, kills it and restarts it; and
> [lexsys-tools#10](https://github.com/alpibrusl/lexsys-tools/issues/10),
> an MCP server that runs each tool as its own process so each keeps its
> own authority. [`editions.md`](editions.md) §6.4 has expected an `Exec`
> since edition 2.

---

## 1. Why

lex-sys has no way to start a process. A program that needs one declares
`fork`, `execve` and `waitpid` through `Ffi("libc")`, and
[`reach.md`](reach.md) §5 is what that costs: `Ffi("libc")` is every
authority at once. The report says `UNBOUNDED`, and a supervisor holding
`lex-os`'s grant (`filesystem`, `network`, `exec`) cannot decide the third
dimension at all ([`under-a-grant.md`](under-a-grant.md) §2: *"`exec`: No"*).

So the gap is the same one `Net` closed for the network: the authority
has no name of its own. This document gives it one, and most of the work
is saying exactly what that name does and does not bound (§2), because
running a program is unlike every capability lex-sys has so far.

### 1.1 What the two askers need

| | #237 (a test harness) | lexsys-tools#10 (an MCP server) |
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

1. **Which programs may be started.** `Exec("/opt/lexsys-tools/bin")` may
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
  `exec("/opt/lexsys-tools/bin")` composes with the reports of what lies
  there. When those are lex-sys programs, each carries its own (`lex-sys
  authority`, or, for lexsys-tools, `<tool> introspect`), and the closure
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
| `Child` | a started process, not yet reaped: `res`, one leaf (the pid) | `child_wait` |
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
| `child_wait` | `(Child) -> Exited` | none (ending a handle is not using one) |
| `child_kill` | `(&Child, signal: int) -> int` | `child_signal` |
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

```lex-sys
edition 7;

// A sketch: the request is written and the answer read in the elided part.
fn call[&h, &x, &q](heap: &!h Heap, exec: &x Exec("/opt/lexsys-tools/bin"), request: &q [byte])
    -> [heap, exec("/opt/lexsys-tools/bin"), pipe_read, pipe_write] (buffer.Buffer, int) {
    var answer = buffer.empty(heap, 4096);
    var status = 0 - 1;
    match pipe_open() {
        Piped::Failed(e) => { }
        Piped::Ok(to_child, child_in) => {
            match pipe_open() {
                Piped::Failed(e) => { pipe_close(to_child); child_end_close(child_in); }
                Piped::Ok(from_child, child_out) => {
                    // `\0`-separated (§4.2); a real caller builds this with `std.process.Args`.
                    let args = "--root\0/work\0--create\0--stdin\0notes.txt\0";
                    match exec_spawn(exec, "/opt/lexsys-tools/bin/write", args, "",
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

`std.process` (§7, slice 3) wraps this into `run(heap, exec, path, args,
input, most, timeout)`, which is what the MCP server and a test actually
call.

---

## 4. Semantics

### 4.1 The path, and the bound

* `path` is the program to run, **absolute or relative to the working
  directory, never looked up in `PATH`**. A program that has no
  environment (lex-sys programs have none) has no `PATH` to search, and a
  search would make the program that runs depend on something the row
  does not name.
* It is checked against the capability's prefix with `Fs`'s rule
  ([`filesystem.md`](filesystem.md) §1.1): a path prefix extends at a `/`
  or not at all, exactly the prefix is allowed (one program), and `""`
  contains everything. A path containing a `..` component is refused, as
  `Fs` refuses it (§4.1 there). Both are **traps**, for `filesystem.md`
  §4's reason: a path outside the bound is a program doing what its type
  said it would not, not an outcome to handle.
* A missing program, one that is not executable, or one the kernel cannot
  load is an outcome: `Spawned::Failed(errno)`, with the parent's
  `ChildEnd`s and `File`s closed by the builtin, so a failed spawn leaks
  nothing.
* `argv[0]` is `path` as given. `args` is the rest.

`narrow(exec, "/opt/lexsys-tools/bin")` is the same `narrow` `Fs` has, at
compile time, against a literal.

### 4.2 Arguments are `\0`-separated bytes

`args` is one `&[byte]`: each argument followed by a `\0`. `""` is no
arguments. This is C's own encoding, so the builtin builds the `argv`
array from it without copying a byte, and it needs nothing the language
lacks: a slice of slices would need a region per element. An argument
cannot contain `\0` in any encoding a process can receive, so the
separator excludes nothing. A malformed list (not empty, and not ending
in `\0`) is a trap, like an out-of-range index: it is a program bug, not
a kernel outcome. `std.process.Args` builds one in a buffer.

### 4.3 The environment is exactly what is passed

`env` is the same encoding, each entry `NAME=value`. The child's
environment is **exactly** `env`: nothing is inherited, and `""` is an
empty environment. A lex-sys program cannot read its own environment, so
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
Measured, in a lex-sys program, by reading `/proc/self/fdinfo`: a `File`
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
> in `lex_sys_ir::open_flags` and `SocketOs`.
>
> **Two windows remain, and both are stated rather than closed.** Where
> Darwin has no flag, `fcntl` follows the call that made the descriptor,
> so an `exec` on another thread between the two would still inherit it;
> and `fopen`'s own descriptor is not close-on-exec for the instant before
> its `fclose`, on both targets. Neither is reachable from a lex-sys
> program until slice 1 (only a foreign `exec` can start a process, and a
> lex-sys program has one thread per `spawn` it wrote), and slice 1 closes
> the first one for spawns it makes itself: `posix_spawn` on Darwin can be
> asked to close every descriptor but the three it is given
> (`POSIX_SPAWN_CLOEXEC_DEFAULT`).
>
> **Built (slice 1), and stronger than this section first said.** A child
> started with `exec_spawn` holds exactly its three streams on both kernels,
> whatever the parent holds: macOS with `POSIX_SPAWN_CLOEXEC_DEFAULT`, Linux
> with `posix_spawn_file_actions_addclosefrom_np(3)` (glibc 2.34 and later)
> after the three `dup2`s. Close-on-exec alone could not promise that:
> it covers what a lex-sys program opens, and a descriptor the program
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
  slice 2, and in C on macOS 26.2: 200 children of 200, each confirmed a
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

---

## 5. Alternatives considered

| Alternative | Why not |
|---|---|
| `fork` and `exec` as builtins | `fork` copies the parent's heap and every thread's locks into a child that runs one thread, and everything between `fork` and `exec` must be async-signal-safe. A language whose backend calls `malloc` cannot promise that. `posix_spawn` is the operation both askers mean, and it does the descriptor and signal setup (§4.4 to §4.6) in the kernel's own order |
| Keep it `Ffi("libc")` | The status quo. `reach.md` §5 and `under-a-grant.md` §4 are the argument: `Ffi` is the one capability whose label does not bound what it authorises |
| One run-to-completion builtin (`exec_run(..., input, output) -> code`) | Serves the MCP server and not #237, which must kill a child at a moment it chooses and serve sockets meanwhile. It is the right *library* function, and `std.process.run` is it (slice 3), built on the handles |
| A list of programs as the bound (`Exec("seek,write")`) | `Signals` uses a set because signals are a closed list. Programs are files, and the natural unit a deployment grants is a directory (`/opt/lexsys-tools/bin`). A prefix that is exactly one file already expresses "this one program" |
| The child's ends as `File` | A `File` is a regular file: no `Again`, no readiness, and a write to a closed pipe raises `SIGPIPE`. A child's stream is a peer, and lex-sys already has a handle for a peer's byte stream. `Stdio::File` keeps redirecting to a real file |
| The parent's ends as `Conn` | The same machinery, but `conn_read` in a row would read as *network*. A distinct `Pipe` keeps the row honest about what the program talks to, at the cost of four small builtins over one backend path |
| `PATH` lookup (`execvp`) | §4.1: the program that runs would depend on an environment the row does not name |
| Inherit the environment | §4.3 |
| `Stdio::Inherit` | §4.4: a child would print for a parent that released `Io` |
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
| **3** | `std.process`: `Args` (the `\0` builder) and `run(heap, exec, path, args, input, most, timeout)`, a bounded capture with a deadline, built on slices 1 and 2 | 7 |
| **4** | lexsys-tools#10: the MCP server, as a lex-sys program holding `Exec` narrowed to the tools' directory and nothing that writes | — |

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
  One accepted program, `tests/accept/process_spawn.ls`, prints what
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

## 9. Open

| Question | Why it waits |
|---|---|
| Giving a child the console | §4.4. The exact shape needs the parent to lend `&!Io` for the life of the child, and a `Stdio` value cannot carry a borrow. Neither asker needs it |
| Spawning beneath a `Dir` | §5's last row. Linux only, so it needs a decision about a capability that exists on one target |
| The working directory | The child starts in the parent's. A `Dir` to start in (`posix_spawn_file_actions_addfchdir_np`, glibc 2.29 and macOS 10.15) is the shape, and nothing asks yet |
| A bound chosen at deployment | `narrow` takes a literal, so a server whose tools' directory is chosen when it is installed holds `Exec("")` or a literal it was built with. `Fs` has the same question (`authority.md` §2.3, `lines.ls`), and it should be answered for both at once |
| Children of a parent that dies | §4.7. `PR_SET_PDEATHSIG` exists on Linux only |
| Mapping `exec("p")` onto `lex-os`'s `exec` level | The same bridge `agent-toolbox.md` §2.5 found missing for `net_out`. The report carries what the bridge needs |
