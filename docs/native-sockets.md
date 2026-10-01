# Native sockets: servers without `Ffi("libc")`

> **Status: design settled; slice 1 built (§10).** Stage 1 of removing C from the
> path between a lex-sys program and the kernel. §7 is the whole
> roadmap to a toolchain with no C in it; this document is only the
> first of its four stages, and it is the one with an asker — the
> server in [`server.md`](server.md) and the two repositories that
> will be built on it (`lex-sys-web`, `lex-sys-schema`).
>
> **What it corrects before it builds.** The first sketch of this stage
> was `net_read(fd: int, buf)` / `net_write(fd: int, data)`, fixed
> signatures with no capability, because that is how `listen` and
> `accept` already work ([`listen.md`](listen.md) §6). §2 shows why that
> is the wrong shape for a *read* and a *write*, and §3 replaces it with
> typed handles, the way [`file-handles.md`](file-handles.md) did for
> files.

---

## 1. What the C costs, measured

`examples/api` — the server — declares four `extern fn`s of its own
(`poll`, `signal`, `time`, `send`) and imports eight more from
`packages/net-sockets` (`socket`, `setsockopt`, `bind`, `listen`,
`accept`, `read`, `write`, `close`). Twelve libc symbols, all behind
`Ffi("libc")`.

That is two costs, and the second is the one that matters.

**It is C on the critical path.** The program is lex-sys, the toolchain
that links it is Rust and (for the LLVM backend) `clang`, and every
syscall goes through a libc this project did not write. Removing that
is §7's whole subject.

**The authority report says nothing.** `lex-sys authority` on the server
prints `ffi("libc")`, and `ffi("libc")` means *"may call any function
in libc"* — `unlink`, `execve`, `system`. The program does not do those
things, and the report cannot say so. A server whose row reads
`net_in("8080"), conn_read, conn_write` is a server whose reviewer can
tell what it can reach; one whose row reads `ffi("libc")` is one that
has to be read line by line. This is [`authority.md`](authority.md)'s
reason for existing, applied to the one program class where the answer
matters most — the one that takes input from strangers.

So the target is not "fewer externs". It is **a server whose authority
row is exact.**

## 2. Why fixed-signature fd builtins are the wrong shape

`listen(fd: int, backlog: int)` and `accept(fd: int)` take no
capability: *"the fd already proves the authority `bind` checked"*
(`listen.md` §6.1). That argument has a hole, and for those two
builtins the hole is harmless. For a read and a write it is not.

**An `int` proves nothing.** `bind` returns an `int`, and `int` is
forgeable: a program can write `3`. `accept(3)` on a descriptor that is
not a socket answers an error; `listen(3, 5)` likewise. But
`net_write(1, data)` is `write(2)` to standard output **without an
`Io`**, and `net_read(0, buf)` is a read of standard input without one.
Every guarantee [`standard-input.md`](standard-input.md) and
[`standard-error.md`](standard-error.md) spent a milestone on — *"a
function that does not declare `io_write` cannot print"* — would hold
for every program except the ones that happened to import a socket.

[`file-handles.md`](file-handles.md) §4.1 already settled this question
for files, and the answer there is the answer here: **the handle is the
capability.** A `File` has no literal form (`defs.rs`'s
`is_capability`: *"the number would be someone else's open file"*), it
is one leaf, it is consumed exactly once by `close`, and it is minted by
exactly one builtin that checks an authority once. Nothing in the type
system needs to change; sockets are the second user of a mechanism that
already exists.

## 3. The design

Three new prelude types, all `res`, none with a literal form, all one
leaf (a descriptor), all `edition 5;` only (a change that *adds* is
exact, and costs old files nothing — [`editions.md`](editions.md)
header; `File`'s own name collision, §4.2 there, is why they are not in
every program's prelude).

| type | what it is | consumed by |
|---|---|---|
| `Listener` | a bound, listening socket | `listener_close` |
| `Conn` | one established connection, either direction | `conn_close` |
| `Poller` | a set of handles the kernel is watching (§4) | `poller_close` |

### 3.1 The builtins

Each builtin's result is a **prelude enum**, never a sentinel and never
`std.result` — `file-handles.md` §2.1: a builtin's signature is fixed in
the compiler and `std` is opt-in.

| builtin | signature | row |
|---|---|---|
| `tcp_listen` | `(net, port, backlog, flags) -> Listening` | `net_in(bound)` |
| `tcp_connect` | `(net, host, port) -> Dialed` | `net_out(bound)` |
| `tcp_accept` | `(&!Listener) -> Accepted` | `conn_accept` |
| `conn_read` | `(&!Conn, &![byte]) -> Received` | `conn_read` |
| `conn_write` | `(&!Conn, &[byte]) -> Sent` | `conn_write` |
| `conn_nonblocking` / `listener_nonblocking` | `(&!H) -> int` | `[]` |
| `conn_close` / `listener_close` | `(H) -> int` | `[]` |
| `conn_raw_fd` | `(&Ffi(..), &Conn) -> int` | `ffi(..)` (§6) |

```
enum Listening { Ok(Listener), Failed(int) }          // errno
enum Dialed    { Ok(Conn),     Failed(int) }
enum Accepted  { Ok(Conn), Again, Failed(int) }
enum Received  { Data(int), End, Again, Failed(int) }
enum Sent      { Wrote(int),    Again, Failed(int) }
```

`Again` is its own constructor, not `Failed(EAGAIN)`: the value of
`EAGAIN` is 11 on Linux and 35 on macOS, and a program that compares an
`errno` against a number it had to look up per platform has a sentinel
with a constructor around it (`file-handles.md` §3). `Data(0)` is not reachable, as `Got(0)` is not (a read into an empty
buffer is `Failed(EINVAL)`, §10); `Wrote(0)` is, for an empty write.

**Rows follow the file rule** (`file-handles.md` §4.1): the authority is
spent where the handle is minted — `tcp_listen` performs
`net_in("8080")`, `tcp_connect` performs `net_out("api.internal:443")` —
and the operations on the handle carry a *path-free* label named after
the capability and the direction. Owning a `Conn` discharges
`conn_read`/`conn_write`; borrowing one declares them. The authority
report of a program that moved from `Ffi("libc")` to handles over the
same port must keep the port in its row; a conformance test says so, the
one `bulk-io.md` §3.2 asked for and `file-handles.md` §4 wrote.

**`tcp_listen` folds four calls into one** — `socket`,
`setsockopt(SO_REUSEADDR)`, `bind`, `listen` — as `bind` already folds
three. Whether a program may also ask for `SO_REUSEPORT` (the api
server's multi-process mode) is the one option worth an argument; it
becomes a fourth `int` of flags rather than a second builtin.

**Writes never wait.** The contract of `conn_write` on a non-blocking
`Conn` is *"takes what the kernel has room for, answers how much, or
`Again`"*, on every platform. How that is achieved is the runtime's
business and is the reason this belongs in the compiler rather than in a
library: `MSG_DONTWAIT` is honoured on Linux and **ignored by macOS** on a
blocking socket ([`server.md`](server.md) §3, §7 — found by CI, not by
reasoning), so a portable program written against `send(2)` has to know
that. Against `conn_write` it does not.

**`SIGPIPE` stops being the program's problem.** Writing to a peer that
has gone must answer `Failed(EPIPE)`, never kill the process.
`MSG_NOSIGNAL` on Linux and `SO_NOSIGPIPE` on macOS do it per socket,
with no process-wide signal disposition — so `signal(SIGPIPE, SIG_IGN)`,
an `extern` and a magic number in the server today, disappears.

**`conn_nonblocking` is explicit and one-way.** A `Conn` is blocking
when it arrives, which is what `examples/serve` and `collect` want;
registering a handle with a `Poller` does *not* flip it silently, which
would be an action at a distance. A non-blocking listener matters for
correctness, not speed: between `poll` saying a listener is readable and
`accept` being called, the pending connection can reset, and a blocking
`accept` then waits for a different client.

## 4. Readiness: the `Poller`

A server needs to wait on many handles at once. The obvious builtin is
`poll(&![int] fds, timeout)`, which is `poll(2)` — and which has §2's
problem exactly: it names descriptors by number. It reads nothing, but
`poll` over `[0]` tells a program, without `Io`, whether standard input
has data, and over `[3..1024]` tells it which descriptors the process
has open. That is a small leak, and it is the kind this project has
refused every time it came up.

So a `Poller` owns its registrations, and the program names handles by
**tokens it chose**:

```
poller_new()                                 -> Polling      // Ok(Poller) | Failed
poller_add_listener(&!Poller, &Listener, token) -> int
poller_add_conn(&!Poller, &Conn, token, events) -> int      // events: 1 read, 2 write
poller_modify(&!Poller, &Conn, token, events)   -> int
poller_remove(&!Poller, &Conn)                  -> int
poller_wait(&!Poller, &![int] out, timeout_ms)  -> int      // pairs (token, events) written to `out`
```

Linux backs it with `epoll`, macOS with `kqueue`. That is a cost to
state plainly: `epoll_event` is 12 packed bytes on x86-64 and 16 on
arm64, and `kevent` is 32; the compiler writes those structs, the
program never sees them, and the program-visible surface is two ints
per event. It is also the scalable answer — `poll(2)` is O(registered
handles) per wait, and the api server's whole loop is currently a scan
of its own record array — so it is a performance change as well as a
safety one, and gets benchmarked as one (§8).

The kernel drops a registration when the descriptor closes, so a
`Conn` closed while registered cannot leave a stale entry that a *new*
connection with the same number inherits. A token is the program's own,
and the rule is the program's too: a token it has retired can still
arrive in the same wait that closed it.

A `Poller` row is `poll` — path-free, argument-free, and for the reason
`heap` and `args` have none: there is nothing to narrow. It observes
handles the program already holds.

## 5. Time

The idle timeout is the one place the server asks the clock, and today it
does it with `time(NULL)` — wall-clock seconds, which jumps when the
host's clock is corrected and would close every connection in the
process at once if it jumped forward. A timeout wants a *monotonic*
clock.

That needs a capability, or any program reads the time without saying
so, and "it is not secret" has not been the project's test for
authority. Proposal: a `Clock` capability carried by a third `Split`
declaration. `PRELUDE_SPLIT_NET` is the second: an edition-5 file's
`split()` would answer one with a `clock` field, so no file written
before it is made to consume a field it never heard of — the move `Net`
made (`ir.rs`'s note on `PRELUDE_NET`). One builtin:

```
clock_ms(&Clock) -> [clock] int     // monotonic milliseconds from an arbitrary origin
```

`poller_wait`'s timeout is the other half: it says *how long to sleep*
and the clock says *what time it is*, and the server needs both. This is
the one piece of §3–§5 that is a new capability rather than a new use
of an existing mechanism, and §9 asks about it.

## 6. The escape hatch, and why it is safe

TLS needs a descriptor: `SSL_set_fd(ssl, fd)` takes an `int`
(`examples/tls_client`). So `conn_raw_fd` hands one over — **and costs an
`Ffi` capability to call**. A program holding `Ffi("libc")` can already
call `write(2)` on any number; giving it the number a `Conn` wraps is
not a widening, and a program without `Ffi` cannot reach the hatch. The
descriptor stays owned by the `Conn`; the raw `int` is a loan and
`conn_close` still ends it.

There is no inverse (`conn_from_fd`). That one *would* mint authority
from a forgeable number, and nothing asks for it.

## 7. Where this sits: the four stages

| | stage | removes | status |
|---|---|---|---|
| 1 | **Handles** (this document) | `Ffi("libc")` from any program that only talks to the network | designing |
| 2 | **A libc-free Linux runtime** | libc from the *builtins'* implementation: `_start`, `read`/`write`/`socket`/… as raw syscalls, `errno` as a negative return, `getaddrinfo` as a DNS client | not started |
| 3 | **Port the frontend** | Rust from lexing, parsing, checking, lowering — `lex-sys-syntax` (5.3k lines), `-ir` (12.3k), `-types` (0.7k), `-id` (1.9k) | not started |
| 4 | **Own backend and linker** | Cranelift/LLVM/`clang`/`ld`; for macOS, an arm64 code-signing step | not started |

Stage 1 is **not** C-removal in the compiler: the builtins still call
libc. What it removes is the *program's* dependence on libc, which is the
precondition for stage 2. Until no program names a libc symbol,
replacing libc underneath is a breaking change; after it, a
backend-internal one — the same ordering that let `file_read` change
from `read(2)` to anything without touching `sort.ls`.

Two honest limits, so nobody plans past them. **macOS cannot leave
libSystem**: Apple does not guarantee its syscall numbers, and a binary
that bypasses libSystem is not one the platform supports — so "no C" means
"no C **on Linux**" and, on macOS, "no C *that this project wrote or
links beyond libSystem*". And `getaddrinfo`, which `connect.md` §10 uses
for name resolution, is a large body of C (`/etc/hosts`, `nsswitch`,
resolver configuration): a native replacement is a real DNS client, which
is why stage 2 is measured in months and not in builtins.

[`self-hosting.md`](self-hosting.md) said *"not yet — no asker"*, and was
right when it said it. There is an asker now; that document gets a
paragraph pointing here when this one is built, not before.

## 8. What building it must prove

1. **Mutation checks on every row.** Each of `tcp_accept`, `conn_read`,
   `conn_write`, `poller_wait` is mutated (return the wrong constructor,
   drop the `Again` case, ignore the count) and a test must fail — the
   discipline that exposed uniform-body tests and never-partial sends in
   [`server.md`](server.md).
2. **The forgery test.** A program without `Io` that tries to write to
   descriptor 1 has no spelling for it: `Conn { }` is rejected, no
   builtin takes an `int` descriptor, and `conn_raw_fd` is rejected
   without an `Ffi`. A positive test per refusal, with rule tags.
3. **Platform parity.** The `Again` contract is a test over a real
   socket pair on both Linux and macOS CI; the 1 ms `SO_SNDTIMEO`
   workaround in `examples/api` is the *evidence* the contract needs, and
   is deleted only once the test that replaces it passes on macOS.
4. **Both backends.** Cranelift and LLVM, byte-for-byte equal output on
   the conformance programs. One known trap: `fcntl` and `ioctl` are
   variadic, and on Apple arm64 a variadic call is **not** the same ABI as
   a fixed one. LLVM expresses the difference; whether Cranelift does is
   unmeasured, and if it does not, non-blocking mode on macOS goes through
   a path that avoids them — to be decided by the measurement, not here.
5. **No regression, measured.** `examples/api` migrated to handles runs
   the same `benches/server` loads; the ~120,000 requests a second may
   not fall, and the `Poller`'s effect at 1,024 connections is reported
   against `poll(2)` whichever way it points.
6. **The authority report test** of §3: the port survives the migration.

## 9. Slices, and what is open

1. **Handles and the read/write path** — `Listener`, `Conn`, `tcp_listen`,
   `tcp_accept`, `conn_read`, `conn_write`, `*_nonblocking`, `*_close`,
   `conn_raw_fd`; `examples/serve` and `examples/collect` migrated, their
   `Ffi("libc")` gone. (They need no `Poller`.)
2. **`tcp_connect`** — and `packages/net-connect` / `net-sockets`
   reduced to what is left, which may be nothing.
3. **`Poller`** — and `examples/api` migrated; the benchmark in §8.5.
4. **`Clock`** (decided: yes).

| Question | Settled |
|---|---|
| `Clock` capability, or a per-registration deadline in the `Poller`? | **`Clock`** (§5). A deadline the kernel reports as an event is less general, and the web layer will want the time for logs and `Date:` headers anyway; `poller_wait`'s timeout stays |
| `SO_REUSEPORT` | **A flags argument on `tcp_listen`** (bit 1), not a second builtin |
| The old int-returning `bind`/`listen`/`accept`/`connect` | **Kept**, documented as superseded; they cannot be removed without breaking files that check today |
| Peer address on `accept` | **Not in `Accepted`.** A later `conn_peer(&Conn, &![byte]) -> int` is additive and costs nothing now; no asker until an access log exists |
| Packages depending on `std` (#63) | **Yes**, separately, after slice 1 |
| UDP and Unix sockets | No asker. `Conn` is deliberately not named `TcpConn` — if a second transport arrives it should find the name free — but nothing here is built for it |

## 10. What slice 1 built, and what building it corrected

Built: `Listener`, `Conn`, the five answers (`Listening`, `Accepted`,
`Received`, `Sent`, and `Accepted`'s `Again`), `tcp_listen`, `tcp_accept`,
`conn_read`, `conn_write`, `conn_nonblocking`, `listener_nonblocking`,
`conn_close`, `listener_close`, all edition 5, on both backends. **Not yet
built:** `tcp_connect` (slice 2), `Poller` (slice 3), `Clock` (slice 4),
`conn_raw_fd`. Nine conformance tests over real sockets run every program on both
backends (`conformance/sockets.rs`), plus six corpus fixtures that pin the
refusals (a forged, dismantled or leaked handle; an undeclared
`conn_read`; the builtins at edition 4) and the edition-1 name freedom.

Four things the design did not know:

1. **`bind` has been wrong on macOS since it was written.** It passes
   `SOL_SOCKET = 1` and `SO_REUSEADDR = 2` -- Linux's numbers; Darwin's are
   `0xffff` and `4`. On macOS the `setsockopt` quietly did nothing and
   nobody saw it, because a failed `setsockopt` is ignored and a bind to a
   fresh port needs no reuse. `tcp_listen` takes its constants from
   `lex_sys_ir::SocketOs`, one table both backends read; the old `bind` is
   untouched, as §9 decided. A program that needs `SO_REUSEADDR` on macOS
   should use `tcp_listen`.
2. **A blocking `recv` of zero bytes waits for data.** `conn_read` into an
   empty buffer first reached the kernel, hung, and would have reported
   end-of-stream had it returned. It now never reaches the kernel and
   answers `Failed(EINVAL)`.
3. **`accept` hands a connection its listener's `O_NONBLOCK` on Darwin and
   not on Linux.** A `Conn` arrives *blocking* on both -- Darwin clears the
   flag after `accept` -- so a program's first read means the same thing on
   either.
4. **`fcntl` is variadic, and Cranelift cannot say so.** LLVM declares it
   `i32 (i32, i32, ...)` and the call is correct everywhere. Cranelift's
   modules refuse two signatures for one symbol, so it declares one,
   always with a third argument; on **Apple arm64 only** that one has nine
   integer parameters -- the first eight in registers, the ninth in the
   first stack slot, which is where `va_arg` reads a variadic argument.
   Verified by reasoning and by the macOS CI job, not on a Mac.

The non-blocking switches answer `0` or the `errno` (not the `-1` of
`fcntl`): a sentinel is how `fs_read` came to disagree with `getchar`.
