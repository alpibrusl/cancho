# `unsafe` and global state, audited (#587)

> **Status: audited at `972099e` (main when it started); the fixes it made are in the commit that adds this file.** Prompted
> by the PHP security release of 2026-10-21 (CVE-2026-109618 and CVE-2026-109619): both are memory-lifetime bugs in a
> long-lived process, (1) an out-of-memory failure that interrupts a request while internal data is half-made and cleanup then
> trusts it, and (2) per-request state that survives the memory behind it and is used by a later request on the same worker.
> We do not run PHP, so this is not an exposure. It is a check for the same two classes in our own code, and for the `unsafe`
> and the globals that could carry them. Every number below was produced by the command or the test next to it.

## 1. The answer, first

| | |
|---|---|
| `unsafe` in the compiler and the code generators | **0**, and now enforced: `#![forbid(unsafe_code)]` in all eight crates (§2) |
| `unsafe` anywhere | 14 occurrences (the issue's number), all in the conformance tests, each a libc call or a `pre_exec` (§2) |
| Globals in the Rust compiler | **2**: `COUNTER` (names a temporary directory) and `FLAG` (the `--compiler` named); neither can show a later compilation an earlier one (§3) |
| Globals in a generated program | **9** kinds, each per process; what a long-lived program carries from one unit of work to the next is the region chunk stack, the descriptor epoch table, the UDP peer ring and the signal claim (§4, §5) |
| Failed allocation | every allocation the language makes **traps** (`SIGILL`) on a null `malloc` or a failed `pthread_create`; swept 19 limits × 3 drivers × 3 code generators: only ever *finished with the right answer* or *trapped* (§6) |
| After a trap | the process is gone; what is left is on disk and in the pipe, and standard output that was still buffered is **lost** (§6) |

**Findings** (five; each fixed here, or filed):

| | finding | |
|---|---|---|
| F1 | `cancho run`, `test` and `satisfy` built into `temp_dir()/cancho-<command>-<pid>` with `create_dir_all`, then **ran what they had built**: another user who made that directory first (a pid is guessable) controlled the executable between its writing and its running | **fixed** (§3.3) |
| F2 | the LLVM backend wrote `cancho-llvm-<pid>-<n>.ll` into the shared temporary directory through whatever link was at that name | **fixed** (§3.2) |
| F3 | the LLVM object carried that temporary name, so **the same program built twice differed** (Cranelift's did not) | **fixed** (§3.2) |
| F4 | `conn_attach` spent a ticket with a plain load, compare and store: two threads could redeem one ticket | **reproduced and fixed** (#593, §5): 4 to 35 doubly redeemed tickets in 300 rounds on x86-64 before; none after, with a mutant that brings them back |
| F5 | the UDP peer ring and its ticket counter are plain memory: two receiving threads can share a ticket and an entry | filed, [#594](https://github.com/alpibrusl/cancho/issues/594); read from the code, not reproduced |

No refusal is involved in any of them, so no rule tag is added.

## 2. Every `unsafe`

`git grep -n unsafe origin/main -- 'crates/*.rs'` at the start: 15 lines (the issue counted 14; the fifteenth,
`signals.rs:158`, landed since). One is a comment (`cancho-ir/src/capabilities.rs:45`, on what the language rules out). The
rest:

| where | what | the invariant it relies on, and where it is enforced |
|---|---|---|
| `mathfn.rs:9` `unsafe extern "C"` | libm's `sin`, `cos`, `exp`, ... declared `safe fn` | pure functions of a `f64`; no `unsafe` block is needed to call them. A test only |
| `conn_peer.rs:260`, `:270` | `setsockopt(SO_LINGER)` to make a socket reset | `// SAFETY:` in the file: `linger` outlives the call, `len` is its size, the descriptor is open (the stream is owned by the caller) |
| `conn_peer.rs:575`, `:598` | `dup2` in a `pre_exec` | async-signal-safe, between `fork` and `exec`, allocates nothing; the descriptor is the listener's, open in the parent |
| `signals.rs:18`, `:158` | `kill(2)` on the test's own child | two integers; the pid is a child not yet waited for |
| `processes.rs:743`, `:764`, `:770` | `prctl` with a seccomp filter in a `pre_exec`; `unsafe impl Sync` for a wrapper of the filter | the filter is a `static` never written (`// SAFETY:`); `prctl` is async-signal-safe |
| `capture.rs:138`, `:154` | `wait4` with a `rusage` | the driver is this test's own child, not yet waited for; `Usage` is at least as large as `struct rusage` on both targets |
| `differential.rs:173`, `:182` | `getrlimit`/`setrlimit` in a `pre_exec` to turn core dumps off | async-signal-safe, touches only the child's limits |

None is in a crate a user's program is compiled by; all are libc calls that Rust's standard library has no safe form for, in
test code. **Enforced from here on:** `#![forbid(unsafe_code)]` is the first item of the root of `cancho`, `cancho-codegen`,
`cancho-codegen-llvm`, `cancho-id`, `cancho-ir`, `cancho-syntax`, `cancho-types` and `cancho-vcs`. The integration tests are
separate crates and are not covered; a new `unsafe` there needs its `// SAFETY:` line, which every one above already has or is
a `safe fn` declaration.

## 3. Globals in the Rust compiler

`git grep -nE 'static mut|thread_local!|lazy_static|OnceLock|OnceCell' origin/main -- 'crates/*.rs'`: 9 lines. One is the
compiler's; eight are test fixtures that build a driver once per test binary (`BUILT: OnceLock<...>` in
`capture.rs`, `process_group.rs`, `process_release.rs`, `processes.rs` ×3, `signals.rs`, `spawn_detached_under.rs`; their value
is a path to an executable, written once, read many times). Beside them, two `AtomicU64`/`AtomicU32` counters name scratch
files (`cancho-codegen-llvm::COUNTER`, three in `cancho-vcs/tests`), a `Mutex<Vec<String>>` collects a test server's lines
(`tls_echo.rs`), and two `AtomicUsize`/`AtomicBool` count connections in tests. There is no `static mut`, no `thread_local!`
and no `lazy_static` in the repository.

| item | holds | can a second run in the same process see the first? |
|---|---|---|
| `cancho::selfhost_build::FLAG` (`OnceLock<PathBuf>`) | the compiler `--compiler` named | `set_flag` is called once, from `main`; a second call is ignored (`let _ = FLAG.set(flag)`), so a library caller who set two different values would silently get the first. Not reachable from the command line; noted, not changed |
| `cancho_codegen_llvm::COUNTER` (`AtomicU64`) | a number, to make a temporary directory's name unique | it was visible **in the output** (F3, below); it is not now |
| the test fixtures | an executable built once | by design; the driver is a pure function of its arguments |

### 3.1 The code generators keep nothing between compilations

`cancho-codegen` has no `static` that is written and `cancho-codegen-llvm` has only `COUNTER`. Measured:
`two_compilations_in_one_process_write_the_same_object` (`cancho-codegen`) compiles a program, another program with regions
in it, and the first again, and the first and third objects are byte-identical.

### 3.2 F2 and F3: the LLVM backend's temporary files

`run_clang` wrote `temp_dir()/cancho-llvm-<pid>-<n>.ll` and `.o`. Two defects, both measured:

* **F3, the same program built twice differed.** `cancho build p.cho --backend llvm --compiler rust` twice gave binaries that
  `cmp` called different at byte 14,143; `strings` found `cancho-llvm-14557-0.ll` in the object: `clang` records the input's
  path in the file symbol. Cranelift's two builds were identical. After the fix: identical.
* **F2, a predictable name in a shared directory.** `std::fs::write` follows a link, so a link another user left at
  `/tmp/cancho-llvm-<pid>-0.ll` was written through. Now each compilation makes a directory of its own with `mkdir`
  semantics (`DirBuilder`, mode `0700`, refused when anything is at the name, a link included), writes `module.ll` in it, runs
  `clang` with that directory as its working directory and relative names (so the object carries `module.ll` and nothing that
  varies), and removes the directory.

Tests: `two_compilations_in_one_process_write_the_same_object_and_leave_nothing` (three compilations, the first and third
byte-identical, no `cancho-llvm-<pid>-*` left behind) failed before the fix on the first assertion, and
`the_private_directory_is_refused_where_anything_already_is` (a link to a directory, an existing directory and a fresh name,
checking that the link's target is untouched). Mutant: `create` replaced by `create_dir_all` fails the second.

### 3.3 F1: the directory `run`, `test` and `satisfy` run from

`cancho run p.cho` builds the program into `temp_dir()/cancho-run-<pid>` and runs it; `test` and `satisfy` do the same with
the runner they synthesise. The directory was made with `create_dir_all`, which succeeds when the directory is already there
and when the name is a link to one. A process id is a guess of a few thousand values, and the temporary directory is shared:
another user who made `/tmp/cancho-run-<guess>` first (world-writable, or owned by them) kept the right to delete and replace
entries in it, so between the build writing the executable and the command running it they could swap it, and the swapped
program ran as the user who typed `cancho run`. Now `scratch::private_temp_dir` makes `<prefix>-<pid>-<n>` for the first free
`n` (a name that is taken, by anything, is skipped, never used), mode `0700`. Tests in `scratch.rs`: a taken name (a link, a
directory, a file) is skipped and the link's target untouched; two calls give two directories; a full range is an error and not
the last name. Mutant: treating `AlreadyExists` as success fails all three.

Not changed: the tests' own `cancho-dit-<pid>` (aarch64 only, in a `#[test]`) and `cancho-lock-origin-<pid>` (`cancho-vcs`
test); they are test scratch and run as the developer.

## 4. Globals in a generated program

A generated program is one process. The symbols below are what its object file defines writable (`Linkage::Local` in
Cranelift, `internal global` in the LLVM text); everything the language calls a `static` is emitted **non-writable**
(`declare_data(.., writable = false)`, `emit.rs`; read-only data in LLVM), so a program cannot change one.

| symbol | what | scope | initial | written by | read by |
|---|---|---|---|---|---|
| `lexs_argc`, `lexs_argv` | the process's arguments | process | 0 / null | `main`, once, before any cancho code | `arg`, `arg_count` |
| `lexs_fd_epoch` | 65,536 × `i32`, a counter per descriptor number | process | 0 (bss) | `conn_detach`, `conn_attach` (plain stores) | `conn_attach` |
| `lexs_udp_peers` | 65,536 × 32-byte entries and the count of tickets issued | process | 0 (bss) | `udp_recv_from` (plain stores) | `udp_send_to`, `udp_attach` |
| `lexs_signal_state` | the mask of signals a live `SignalWatch` holds; the number of threads not joined | process | 0 (bss) | `signals_watch`/`release`; `spawn`/`join` (**atomic** add) | the same |
| `cancho_region_key` | `pthread_key_t` + 1, or 0 for "no key" | process (the key); **per thread** (the stack it names) | 0 | `main`, once, before any cancho code | `region` entry and exit |
| `cancho_hw_cache`, `cancho_hw_sha_cache` (LLVM) | 0 unknown, 1 no, 2 yes: does the CPU have AES/PMULL, SHA | process | 0 | the probe, **atomic** monotonic store | the same, atomic monotonic load |
| `cancho_out_buf`, `cancho_in_buf` and their cursors (WASI) | the console's buffers | process | 0 | the console builtins | the same. WASI has no threads (`spawn` is refused there), so these are single-threaded |
| `@cancho_signal_state`, `@cancho_argc`, `@cancho_argv` (`examples/selfhost/runtime.ll`) | the same three, in the runtime the compiler in cancho links | process | 0 | as above | as above |

`cancho test` runs **one process per test** (`docs/testing.md` §3, §5), `cancho run` one process per run, and no generated code
runs inside the compiler's process (there is no JIT; `grep -rn 'cranelift_jit\|libloading\|dlopen' crates` finds nothing). So
"a second run in one process" is, for a program, **a second unit of work in a long-lived one** (a server answering requests),
and for the compiler a second compilation in one process (§3).

## 5. What a long-lived program carries from one unit of work to the next

**The region chunk stack (`cancho_region_key`).** A region's 64 KiB chunk is not freed when it ends; it is kept on a stack for
the thread's next region, which is exactly the shape of CVE-2026-109619. `docs/region-chunk-cache.md` §3.1 argues the three things
that make it safe, and this audit checked what could be checked:

* *No uninitialised memory is reachable.* `alloc_slice`'s fill is not optional and `alloc` stores its value; no other
  expression bumps (`Expr::AllocSlice { fill: Box<Expr> }` in `ir.rs` has no optional fill; the rest is `region-chunk-cache.md` §3.1, not re-derived here). A later region cannot read an
  earlier region's bytes.
* *The bytes are cleared anyway* when the chunk is given back (`cancho_region_give`: `[base, end)`, before the chunk is kept or
  freed), so the old bytes do not live longer than the region, and `FFI` that reads past a slice sees zeroes. The clearing is
  stored into the stack, so it is not a dead store (`region-chunk-cache.md` §8 read the object code). Not re-measured here.
* *The stack is per thread* (a `pthread` key; the destructor frees a thread's chunks), so no chunk is ever on two threads' stacks.
* *Out of memory is a trap.* `region_take` answers null when `malloc` does, and the call site traps (`body/memory.rs`,
  `trapz(base)`). **Measured by deleting it**: with the trap removed (Cranelift), the sweep of §6 ends `r` in `SIGSEGV` at 60,000
  KiB; with it, in the trap.

**The descriptor epoch table (`lexs_fd_epoch`).** The table is what keeps a ticket for a connection that has ended from
reaching the unrelated connection that now has the same descriptor number: the kernel hands out the lowest free number, so
the next `tcp_connect` after a `conn_close` gets the number just freed. **New test**,
`global_state.rs::a_ticket_does_not_reach_the_connection_that_took_its_descriptor_number`, on Cranelift and LLVM: detach a
connection (ticket 1), attach it back, close it, dial again, detach the new one (ticket 2); the program checks that the two
tickets name **the same descriptor number** (it did, so the case happened), then redeems ticket 1 and gets `EBADF`, and
redeems ticket 2 and gets its own connection. The previous test (`a_ticket_is_spent_when_it_is_redeemed`) stopped at
redeeming a ticket after its own connection was closed; it never had another connection take the number. **Mutant:** the
comparison of the ticket's epoch with the table's removed (Cranelift, `conn_attach`): the new test fails (score 2, the old ticket
attached the new connection).

**F4, the same table across threads (#593): reproduced, then fixed.** A ticket is an `int`, so it can be copied into two
threads, and `conn_attach` took no capability. The table was read, compared and written with plain loads and stores, so two
threads redeeming one ticket together could both pass the check. `tests/programs/ticket_race.cho` makes it observable: each
round dials, detaches (the ticket), and starts two threads that wait for the same millisecond on the clock and then call
`conn_attach` on that one ticket; a round is *doubled* when both were given the connection. Measured before the fix (x86-64,
4 cores; the first probe, three runs of 300 rounds each): 11, 11 and 35 doubled rounds on Cranelift; 4, 34 and 20 on LLVM
through the compiler in cancho; 23, 14 and 34 on LLVM through the Rust compiler. After the fix: 0 in 400 rounds, twice, per
code generator, and `conn_attach` spends the ticket with one compare-and-swap (`atomic_cas`, `cmpxchg`,
`__atomic_compare_exchange_n`) and `conn_detach` bumps with an atomic add, all sequentially consistent: of any number of
threads redeeming one ticket exactly one finds the epoch it checked. **Mutant:** the compare-and-swap replaced by the old
store (Cranelift) fails the test. The runtime of the compiler in cancho is `examples/selfhost/runtime/net.c`, regenerated with
`make.sh` into `net_linux.ll` and `net_darwin.ll` (clang writes `@lexs_*` as plain `global` now; the two lines were put back
to `common global`, which the program's own definition relies on). aarch64 is measured by CI only.

**The UDP peer ring (`lexs_udp_peers`).** Existing tests cover the single-threaded claims: a ticket older than the ring is
refused (`a_ticket_older_than_the_ring_is_refused`), a forged one sends nothing, a ticket is redeemed once, and only the socket
that heard the sender can use it. **Finding F5:** none is multi-threaded, and the counter and the entry are plain memory
([#594](https://github.com/alpibrusl/cancho/issues/594)). The two documents that describe the tickets
(`docs/udp.md` §4 and `docs/native-sockets.md` §10.3) now say they describe one thread.

**The signal claim (`lexs_signal_state`).** The thread count is `atomicrmw`/`atomic_rmw` on both generators
(`body/signals.rs`: "any thread may spawn"). The mask is written by the claim and the release; the claim is refused while a
thread is running (`docs/signals.md` §3), so two threads never write it at once. Not changed.

**`argc`/`argv` and the hardware caches.** Written once (`main`) or idempotently (a probe that always stores the same answer;
atomic, monotonic ordering is enough for a value that only moves from 0 to 1 or 2). Nothing to carry.

## 6. Allocation failure, and what a trap leaves

**Every allocation site traps on failure.** `check-cost.md` lists them: `box`/`box_slice` and a new arena chunk trap on a null
`malloc`; an arena bump traps when the chunk cannot spare the bytes (`region-limit.md`); `alloc_slice` traps on a negative count
or an overflowing size; `spawn` traps when `pthread_create` fails (`body/expr.rs`, `trapnz(failed)`) *before* it counts the
thread, so the count never includes a thread that was not made. A failed `pthread_key_create` is the one soft failure, by
design: the key stays 0 and every region is a `malloc` and a `free` as before. There is no allocation whose failure is a value
a program can ignore, which is what the first CVE needs.

**Measured: `tests/programs/memory_exhaustion.cho`** under `ulimit -v` at 19 limits from 1,000,000 KiB to 4,000 KiB, three
drivers, on Cranelift, on LLVM through the Rust compiler and on LLVM through the compiler in cancho
(`global_state.rs::a_failed_allocation_traps_and_never_leaves_a_half_made_value_in_use`):

| driver | what it does | result over the sweep |
|---|---|---|
| `g` | a `std.buffer` grown 4,096 times by 64 KiB (256 MiB), so every growth is an allocation, a copy and a free | finishes with 268,435,456 bytes, or traps; never a wrong size, never a `SIGSEGV` |
| `r` | regions nested 1,000 deep (1,000 live 64 KiB chunks) | finishes with 0, or traps |
| `b` | 32 `box_slice`s of 8 MiB, each read and freed | finishes with 496, or traps (on Cranelift; **on LLVM `-O2` the allocation is removed because its only use is one read of the fill, so it finishes under every limit**; held to the outcomes, not to a trap) |

Below the program's own needs the dynamic loader cannot map it at all (`error while loading shared libraries`, exit 127);
that is the program not running, and the test counts it apart. **Mutant:** the null check after `region_take` removed (Cranelift)
turns the `r` sweep into `SIGSEGV` at 60,000 KiB and the test fails.

**What a trap leaves.** The process ends by `SIGILL` (4); nothing runs after it (no `atexit`, no unwinding, no destructor), so
what is visible afterwards is what had already left the process:

* *Standard output that was still buffered is lost.* `write_bytes` is `fwrite` on the C stream; a line written before the trap
  is not in the pipe or the file (`global_state.rs::a_trap_ends_the_process_and_loses_what_standard_output_still_buffered`, on
  all three configurations; already stated by `agent-toolbox.md` L1 and `checked-output.md`, which is why `flush_out` exists).
  Standard error is unbuffered. A program that must not lose a record writes it with `flush_out` checked, or to a file it
  `file_sync`s.
* *A file opened for writing is as the last completed `write` left it.* The language has buffered file stream
  to lose (not measured here beyond the standard output case), so a record written in two writes can be seen with one.
* *Child processes are not stopped* by their parent's trap (`docs/processes.md` §4.7, §9: "Children of a parent that dies").
* *An exclusive-create file or a lock file stays.* The same.

None of these is state a *later run* mistakes for valid, with one exception worth saying aloud: a lock or exclusive-create
marker left by a trapped process is exactly that, and is the program's own protocol to recover from (stale-lock handling is not
the compiler's).

## 7. Not done

* F5 (#594): see its own section when it is done.
* The clearing of a released region chunk was not re-measured; `region-chunk-cache.md` §8 did.
* macOS: **the failed-allocation claim of §6 is measured on Linux x86-64 only.** `ulimit -v` is refused on macOS
  (`cannot modify limit: Invalid argument`, seen on CI in this PR's first run) and it has no other per-process address-space
  limit to set from a shell, so the sweep is `#[ignore]`d there with that reason. The other two measurements (the descriptor
  reuse and the trap) run on both. That a null `malloc` is a trap on macOS rests on the code, which is the same on both
  (`body/memory.rs` is not per-platform), and is not measured.
* Non-Linux `ETXTBSY`/`posix_spawn` behaviour; stack exhaustion (a deep recursion is a `SIGSEGV`, not a trap; not in this audit's
  scope, and not a state-sharing question).
* The on-disk caches (`$CANCHO_CACHE`, `docs/package-system.md`) are state across runs by design and were not audited here.
