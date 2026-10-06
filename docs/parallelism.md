# Parallelism: threads and vectorization -- what exists, what was measured, and the order to do it in

> **Status: strategy, with its measurements.** Nothing in the compiler changes with this document. What it adds:
> one program that pins what threads can already carry (`tests/accept/spawn_struct_ref.ls`, with a
> backend-agreement test), corrections in place to three older documents, and the experiments behind every number
> here (`benches/parallel/`). The question it answers came from
> `lexsys-web`'s database service, which reached its single core's limit (a 4-vCPU machine, one used), and from
> asking what a DuckDB-shaped engine would need.
>
> Measured on one machine: Intel Xeon 2.1 GHz, **4 vCPUs** (a Firecracker VM, 1 thread per core as reported),
> Linux 6.18, clang 18, DuckDB 1.5.6. Times are the minimum of 5-9 runs. A claim not measured says so.

## 1. The question, and the short answer

*Can lex-sys have threads, and can it vectorize, or does the design forbid them?*

* **Neither is forbidden.** Threads exist (`spawn`/`join`, real `pthread`s, both backends). Vectorization exists on the
  default backend for any loop that cannot trap.
* **The two real limits are different in kind.** Threads are limited by a *narrow* payload rule and by there being no
  way to give a second thread its own `Heap` (section 4). Vectorization is limited by the language's own guarantee:
  a checked `+` is an observable side effect, so a loop that uses it cannot be reordered across lanes (section 3).
* **Both have cheap first steps.** Section 5 orders them, each with a gate that can fail.

## 2. What exists (read from the source and the documents)

| fact | where |
|---|---|
| `spawn(payload, body)` and `join`: a real `pthread_create`/`pthread_join`, both backends. The payload and the result are each **one pointer-width leaf**: `int`, `bool`, `byte`, `c_ptr`, a captureless function value, **any non-slice reference**, or an owned `File`/`Io`/`Ffi`/`Fs`/`Args`/`Heap` | `docs/threads.md`; `crosses_to_a_thread` in `crates/lex-sys-ir/src/lower/conc.rs` |
| a reference crosses if the thread is joined before the region it belongs to ends (the escape check). **Corrected: this does not make `&!` sound across threads.** `&!` copies, two copies of one pointer can be spawned, and two threads then write one value unchecked (measured: `benches/atomics/race.ls`) | `threads.md` §3, `atomics.md` §2 |
| multi-field payloads and `Rc` were decided "not yet": nothing asked, and a general struct needs a compiler-built trampoline | `threads.md` §5 |
| no program-visible atomics, no channels, no checked shared mutable memory (the compiler's own unjoined-thread counter is an atomic, `threads.md` §6); `Rc` is not implemented. Atomics and a channel are designed in `atomics.md` | `grep` of `docs/` and `std/` |
| `--backend llvm` is the default and compiles with `clang -c -O2 -target <triple>`: **no `-march`**, the architecture's baseline (SSE2 on x86-64) | `crates/lex-sys-codegen-llvm/src/lib.rs`; `llvm-backend.md` |
| Cranelift has no auto-vectorizer; LLVM does, and does not reorder a loop whose arithmetic can trap | `backend-limits.md` §1.4; `check-cost.md`; `gpu.md` §2 |
| integers are `int` (64-bit, trapping) and `byte`; narrower and unsigned widths are deferred | `strings.md` §2; `bitwise.md` §1 |
| `Heap` is borrowed **uniquely** ("an allocator has state"), and `split` hands out exactly one | `heap.md` §2 |

## 3. What was measured

### 3.1 Vectorization: the default backend does it, for loops that cannot trap

`benches/reduce_checked.ls` and `reduce_wrapping.ls` (a million `int`s, 200 rounds; 8 MB), against the same loop in C:

| | time | SIMD instructions in the executable |
|---|---:|---:|
| C, `clang -O2` (`benches/reduce.c`) | 66-69 ms | 52 |
| lex-sys, **LLVM**, wrapping | **64-67 ms** | 54 |
| lex-sys, **LLVM**, checked (`+`) | 93-95 ms | 0 |
| lex-sys, Cranelift, wrapping | 148 ms | 0 |
| lex-sys, Cranelift, checked | 255 ms | 0 |

With the default backend a wrapping loop **is** C's loop, vectorized; a checked one is 1.4x C and scalar. The figures
in `gpu.md` §2 (checked 4.15x C, wrapping 2.27x) are Cranelift's, and `backend-limits.md` §1.4's "it will stay scalar"
is true of Cranelift; both are corrected in place by this change, because the default has moved.

That kernel is **memory-bound**: 8 MB per round is past the cache, and 64 ms for 1.6 GB is what the memory
delivers. Where the data stays in cache (the same work over 128 KB, 12,207 rounds):

| | time |
|---|---:|
| LLVM, wrapping | **29 ms** |
| LLVM, checked | 76 ms (2.6x the wrapping loop) |

So the trap costs 1.4x on data that has to come from memory and 2.6x on data that does not. Which of the two a
columnar engine mostly sees is a property of its blocking, not of the compiler.

### 3.2 Keeping the guarantee and vectorizing anyway: two attempts, both failed

Both keep every overflow a trap. Both were measured, because both looked likely to work.

| idea | cache-resident | memory-bound | why not |
|---|---:|---:|---|
| **sum the low and high 32 bits separately** (`reduce_limbs.ls`): two accumulators that cannot overflow in a block, one checked step per block | 79 ms (checked: 76) | 102 ms (checked: 95) | no gain: 29 SIMD instructions, but the 64-bit shifts and masks cost what the checks did |
| **prove a block cannot overflow, then sum it unchecked** (`reduce_proved_abs.ls`: largest absolute value; `reduce_proved.ls`: max and min by comparisons) | | 231 ms and 425 ms (checked: 95) | the abs pass has a checked `0 - x`, which is itself a trap; the comparison pass **does** vectorize (148 SIMD instructions) and is slower than scalar: SSE2 has no 64-bit compare |

In C (`proved.c`, 128 KB, 12,207 rounds) the same idea, to see what a better target buys:

| | `clang -O2` | `clang -O2 -mavx2` |
|---|---:|---:|
| checked, per element | 73 ms | 71 ms |
| wrapping | 25 ms | **14 ms** |
| proof pass, then wrapping | 166 ms | **63 ms** |

AVX2 turns the proof idea from 2.3x slower than checked into 1.13x faster, and it is still **4.5x slower than the
wrapping loop**: the proof pass costs more than the sum it licenses. The idea is dead *as a per-query pass*. What
makes it worth keeping is where a columnar engine already has the proof: **per-block minimum and maximum, kept
when the data was written** (DuckDB keeps exactly this as segment statistics). Then the check is O(blocks), the sum
is the vectorized wrapping loop, and the language's guarantee costs nothing at query time (section 5, V3).

### 3.3 A scan against DuckDB, on one core

50 million `int64` (400 MB), the same values in both (`x[i] = ((i * 2654435761) >> 7) & 1023`), in memory, the answers
equal (25,575,001,640 and 25,537,111). lex-sys is one loop per query on one core; DuckDB is a query engine and its
time includes planning and its pipeline:

| | `sum(x)` | `count where x > 500` |
|---|---:|---:|
| lex-sys, LLVM, checked, 1 core | 77 ms | 69 ms |
| lex-sys, LLVM, wrapping, 1 core | **42 ms** | 73 ms |
| DuckDB, 1 thread | 90 ms | 108 ms |
| DuckDB, 2 threads | 46 ms | 56 ms |
| DuckDB, 4 threads | **24.5 ms** | **40 ms** |

On one core a hand-written loop is level with DuckDB's single thread on these two primitives, and DuckDB's lead is its
threads (3.7x at 4 for the sum). That is **one primitive**, with the plan, the SQL, the storage format, compression,
joins and group-by all outside it: it says the single-core inner loop is not where a lex-sys engine would lose, and
nothing about the rest.

### 3.4 Threads: what already crosses, and how well they scale

Probes (each compiled and, where it passes, run on both backends; `tests/accept/spawn_struct_ref.ls` pins the second and the third):

| payload | result |
|---|---|
| **a unique reference to a struct** (`&!r Job` with ints) | accepted, runs |
| **a unique reference to a struct that owns a `Box[[int]]`** | accepted, runs, the thread reads and writes through it |
| two workers at once | accepted only with **one function value per spawn**: `work` taken once is instantiated at the first borrow's region and the second reference "does not outlive" it |
| a reference to a **slice** | refused (a pointer and a length: two leaves) |
| an owned `Net(..)` or `Clock` | **refused when measured**, though the allowlist's own comment says `Net` has no fields; **admitted since T2** (below), with `tests/accept/spawn_owned_net.ls` and `spawn_owned_clock.ls` |
| two threads allocating | **no way to write it**: there is one `Heap`, borrowed uniquely, and `split` gives out one |

So a multi-field payload by reference **works today**; `threads.md` §5 put it behind a trampoline, and that was
needed only for an *owned* struct. Scaling (`gen_threads.py`: P threads, each summing its own 128 KB slice 12,207 times,
the wrapping loop above; the work is P times one thread's, so perfect scaling keeps the time constant; cores 0-3):

| threads | 1 | 2 | 4 |
|---|---:|---:|---:|
| wall time | 29 ms | 42 ms | 41 ms |
| throughput relative to one thread | 1.0x | **1.4x** | **2.8x** |

Real parallelism, but short of linear, and the 2-thread figure is worse per thread than the 4-thread one, which I
cannot explain from here (a 4-vCPU VM; two vCPUs may share a physical core). Not investigated further.

### 3.5 What the pool work already said about multiple cores

Three blocking copies of `users_pg` sharing a port (`reuseport`, so processes, not threads) read 25,180 a second
against 15,638 for one (`lexsys-web`, `docs/benchmarks.md`). Processes already get lex-sys past one core for a
service that shares nothing; they cannot share memory, and each holds its own database connections.

## 4. Strategy: threads

**Principles.** Shared-nothing first. A worker owns what it touches; the soundness argument stays the existing
aliasing rule and needs no new one. No `Rc`, no atomics until a program asks for one (`AGENTS.md` §7: a feature earns
its way in when a program asks, and the asker here is a server that wants more than one core).

| step | what | gate (can fail) |
|---|---|---|
| **T0** | **no compiler change**: data-parallel kernels on today's threads, a job per `&!` struct (section 3.4). Put the pattern in `docs/threads.md`'s status, with the one-function-value-per-spawn rule and why | the thread-scaling program (`gen_threads.py`) stays at least 2.8x at 4 threads on both backends; a conformance test runs two workers on both |
| **T1** | give a thread **its own `Heap`**: **built** (section 8.6): `fork_heap(&!Heap) -> Heap`, edition 4, row `[heap]`; sound while `Heap` is stateless (section 8.3) | `tests/accept/fork_heap_workers.ls` on both backends (two forked heaps, two workers, a box the fork allocates freed by the parent); three rejects; 100 million allocate-and-free rounds in two threads at a flat 10.1 MB resident, on Cranelift |
| **T2** | **built**: `Net(..)` and `Clock` admitted as payloads: both are zero-leaf like `Io` (`leaf_free`), and the change is two names in `crosses_to_a_thread`'s allowlist, as `File`'s was. A thread now dials a refused port and reads a clock, on both backends | done: two fixtures, two backend-agreement tests; no reject fixture had pinned the refusal, so none became an obsolete pin |
| **T3** | **server workers**: N threads each running `http.server`'s loop on its own `SO_REUSEPORT` listener, the schema and the OpenAPI document shared read-only through `&`. **Baseline measured** (`copies_users.sh`, the in-memory `users` service on a stateless workload, an invalid `POST /users`: parse, validation, error, no store; copy *i* on core *i*, the load generator on cores 2-3): **one process 94,904 a second** (median of 5, 93.5k-100.0k), **two processes sharing a port 159,176** (139.9k-163.0k), 1.68x. N = 3 cannot be measured on this machine without the load generator sharing a core | **two threads at least 0.9x of two processes: 143,000 a second or more**; if it is below, say so and keep processes: they are free today. **Built and measured (section 9): met**, 149,380 against 141,208 a second for two processes in the same session, with ranges that overlap |
| **T4** | **shared read-only data** across threads (`&T` crossing to several threads at once): test it, document it. It already type-checks as one reference per thread (`tests/accept/spawn_thread_ids.ls` shares a borrowed capability); what is missing is a worked example | a reader pool over a shared `Box[[int]]` agrees with the single-thread answer |
| **T5** | **communication**: a bounded queue between threads needs an atomic load and store with ordering, which does not exist. Design only: what the primitive is, and why it is not `Rc` | **designed in [`atomics.md`](atomics.md)** (asked by a parallel CSV scan in `lexsys-table`); not built; stages A0-A3 each have a gate fixed beforehand. Hard constraint or nobody asked: nobody asked, `atomics.md` §1 |

**Non-goals.** Shared mutable memory, thread-local storage, an async runtime, and anything that gives up the
single-owner rule. If a use needs them, the answer is processes (section 3.5) until a measured reason says otherwise.

**What T0-T3 would buy `lexsys-web`.** The database service reached 64,000 reads a second in one process on one core. On
four cores and with workers, the ceiling moves to the database, which is where it belongs; whether it does is
exactly what T3's criterion measures.

## 5. Strategy: vectorization

The default backend already vectorizes everything that cannot trap, and that is a large class. The work is therefore
not "add a vectorizer"; it is *where is the guarantee paid for, and can the engine avoid paying it where it does not
need to*.

| step | what | gate (can fail) |
|---|---|---|
| **V0** | correct the documents that described Cranelift's numbers as the language's (`gpu.md` §2, `backend-limits.md` §1.4) and record LLVM's: **this change does it** | a reader of `gpu.md` §2.3 is told the default backend's figures and where the old ones came from |
| **V1** | **wrapping where the bits are the intent, in library kernels**: hashing, checksums, bitmaps, counters that are reduced modulo something. They are explicit (`wrapping_add`), they vectorize today, and they are not a loophole because the intent is stated | a `std` kernel (a byte-count or a bitmap popcount over a slice) at **0.9x or better of C** on both a memory-bound and a cache-resident input, SIMD count above zero |
| **V2** | **a target flag**: `--cpu <name>` (or `native`) for the LLVM backend. Baseline SSE2 has no 64-bit compare; AVX2 does (section 3.2: 166 ms to 63 ms for the same idea). Opt-in, because a binary built for AVX2 does not run everywhere | re-run section 3.2 with the flag; **if the wrapping loop is not at least 1.5x faster than baseline on a cache-resident input, drop it** (C says 1.8x: 25 to 14 ms) |
| **V3** | **statistics instead of proofs**: a column engine keeps minimum and maximum per block when it writes the block; a checked sum then reads the statistics (O(blocks)), sums each block with the vectorized wrapping loop, and combines the block sums with checked `+`. The language's guarantee is kept in full, and costs nothing per row | on the 50-million-row scan of section 3.3, a **checked** `sum` within **1.25x of the wrapping one** on one core (77 ms checked and 42 ms wrapping today); if the statistics are not maintained cheaply enough on write, the engine is not worth building |
| **V4** | **narrower element types** (`int32`, `int16`, `uint8`) for columns: a 32-bit column summed into a 64-bit accumulator **cannot overflow** (so it is exact, checked or not, and vectorizes), and it halves the memory a bandwidth-bound scan reads (section 3.1's 8 MB kernel is bandwidth-bound). This is a language change, and the largest one here: `strings.md` §2 and `bitwise.md` defer unsigned and narrower widths on purpose | an `int32` column sum at **at least 1.7x** the `int64` one on the memory-bound input (half the bytes, the same instructions); a design document first |
| **V5** | explicit SIMD types | **not started**. Cranelift has `v128` and LLVM has vectors; if V1-V4 leave a real kernel short of C, name it, and then design |

**What not to do.** The two attempts in section 3.2 each *looked* right and cost more than the check; the next idea
gets the same treatment before it is built, and the experiments are checked in so that is cheap.

## 6. A column engine, in that order

The question that started this section was "could lex-sys be something like DuckDB". Read against section 3.3: the
inner loop is not the obstacle on one core, the threads are (T0-T3), and the language's guarantee is manageable
**if** the engine keeps statistics (V3). What is *not* there, and not designed: a SQL front end, a planner, a storage
format (compression, a persistent file), joins and group-by with hash tables that scale, a spill path. A first
deliverable that does not need any of them: **a library of typed column kernels (`sum`, `count where`, `min`, `max`,
`filter` over `int` columns, with block statistics), queried through generated functions the way `pgen` generates
them from SQL**, single-threaded, with the target of section 3.3's numbers and V3's gate. If that holds, threads
(T3) are the next multiplier; if it does not, the document says why and the engine does not get built.

## 7. What to do first

1. Merge this document, the fixture and the corrections (V0, T0's pattern).
2. ~~**T1**: how a thread gets a `Heap`~~ (decided in section 8, built in 8.6). T3 is next.
3. ~~**T2** (the allowlist)~~ (done), ~~**T3**~~ (section 9), and the T3 baseline (the in-memory `users` service as N processes), which cost an afternoon
   each and decide whether T3 is worth building.
4. **V2** and **V3** only after a column kernel exists to measure them on.

## 8. T1: a thread's own `Heap` -- the decision

### 8.1 What is true today

* **A `Box` is a `malloc`, and a `Heap` is a type-level token.** `box` lowers to one `malloc` and `unbox` to one `free`
  on both backends (`crates/lex-sys-codegen/src/body/memory.rs`, `lex-sys-codegen-llvm/src/body/memory.rs`); `Heap` has no fields
  and no leaves (`leaf_free`), and there is no budget in the runtime (`budget.md`: it "does not belong here"). A region's
  arena is one `malloc` plus a bump pointer kept in the function's own locals, so it is private to the thread that opens it.
  glibc's `malloc` and `free` are thread-safe, and a block may be freed by a thread other than the one that allocated it.
* **`Heap` is unique anyway**, "an allocator has state, and the honest type for shared mutable state is the one that says
  only one reference reaches it" (`heap.md` §2) -- true of the *type's intent*, not of today's runtime -- and `split` hands out one.
* **A struct holding a `Heap` crosses to a thread and the thread allocates through it** (`tests/accept/spawn_heap_in_struct.ls`, both backends):
  a `res struct Worker { heap: Heap, out: int }` passed as `&!r Worker`, the worker calling `box_slice(w.heap, ..)` and
  `unbox_slice(w.heap, ..)`. Its row is `[heap]` (only *owning* the capability discharges the label, as for `Io`); `main`'s is
  `[conc]` because `main` owns the heap it moved into the struct. So the carrying side works; **what is missing is a second
  `Heap` to carry**.

### 8.2 The options

| | idea | verdict |
|---|---|---|
| **A** | a builtin **`fork_heap(h: &!x Heap) -> [heap] Heap`**: a new owned `Heap` made from a unique borrow of one | **chosen**: nothing is amplified (the parent already holds the authority, and the label is the same `heap`); the child is an ordinary owned capability, moved into a worker struct or a payload |
| B | make `Heap` copyable | rejected: it deletes the uniqueness the type was designed to state, for every future allocator, to fix a problem a fork solves |
| C | `split` hands out *N* heaps | rejected: `Split` is a fixed record; the number of workers is a run-time decision |
| D | workers use regions only | rejected: the library surface (`http.server.open`, `pg.pool`, `schema`) takes a `Heap`; a worker could not call any of it |
| E | `spawn` gives the thread a heap implicitly | rejected: an authority the program text does not show; the capability language exists to show it |

### 8.3 Why A is sound, and the one condition it depends on

A `Box` freed through any `Heap` is correct while `Heap` has no state of its own: `malloc` and `free` need no heap value to agree
on which allocation they are speaking of. So a forked `Heap` and its parent are interchangeable for memory safety, a box a worker
builds can be freed by the parent after `join`, and no new linearity rule is needed (the child is `res`, so it is moved, and
used once, by the existing rules).

**The condition**: `Heap` must stay stateless. If it ever gets state -- a per-heap arena, a budget, a different allocator for a
`no_std` target -- then `fork_heap` has to *split* that state, and `unbox` has to be tied to the allocator that made the box (a
type-level obligation `Box[T]` does not carry today). This design says so rather than hiding it: the footprint test of
`the_heap_actually_frees` (eight million 2 KiB boxes, one at a time) must keep passing with boxes freed by a different heap than
allocated them, and a change that makes `Heap` stateful has to start by failing it.

What this does **not** claim: that forked heaps are *faster* (glibc already keeps per-thread arenas; not measured here), or that
allocation under contention scales (T3 measures the service, not the allocator).

### 8.4 The gate (T1), and what building it touches

A fixture `tests/accept/fork_heap_workers.ls`, on both backends, and its rejects:

* **accepts**: the parent forks two heaps and moves each into a worker struct; two threads allocate and free boxes in a loop
  (a million iterations each); one worker also builds a box that outlives its thread (stored in its struct), which the parent frees
  with *its own* heap after `join`; the same run's resident size stays flat (the footprint check);
* **rejects**: using a forked heap after moving it into a thread (`linear-use-after-move`, existing); forking from a shared
  reference (`&Heap`: the builtin's signature wants `&!`); and releasing the parent heap and then forking.

Expected cost, **not yet measured**: the value has no leaves, so neither backend should emit code for the call; the work is the
builtin's signature and row in `lex-sys-ir/src/builtin.rs`, the lowering arm, and the fixture. `[heap]` stays the only label, so
`lex-sys authority` reports nothing new and a `lex-os` grant needs no new field.

### 8.5 After T1

T3's workers hold `{ heap, listener, poller, schema, ... }` in one struct each, passed by `&!`, with `Net` and `Clock` crossing
as owned payloads (T2, built) for the parts the struct cannot hold. Whether a *library* (`http.server`) can be driven from a
thread through a struct field of its `Server` type is the first thing T3 has to find out, and may need its own change.

### 8.6 Built, and what building it found

`fork_heap` is `Builtin::ForkHeap` (`lex-sys-ir/src/builtin.rs`), checked at the call site like `box` (`expect_heap`: the argument must be a uniquely borrowed `Heap`), effect `heap`, edition 4 (the same edition as `spawn`, so no older file's own `fork_heap` is shadowed). Its value has no leaves: both backends emit nothing for the call, one arm each next to `split`'s.

The gate of 8.4, as built:

* **accepts** `tests/accept/fork_heap_workers.ls`, agreeing on both backends: the parent forks two heaps, moves one into each worker struct, the two threads allocate and free boxes in a loop, and a box the *forked* heap allocates is freed by the *parent's* heap. Peak resident size on Cranelift is 10.1 MB at 2 million rounds and the same 10.1 MB at 100 million (50 million in each thread, 1.47 s), so nothing leaks.
* **rejects**: `fork_heap_from_shared_borrow` (`capability-misused`), `fork_heap_after_release` and `fork_heap_child_reused` (`linear-use-after-move`, the existing rule; nothing specific to `fork_heap` was added).

Three corrections to what 8.4 expected:

1. **The loop is not a footprint test on LLVM.** The same binary built with LLVM runs the two million rounds in 4 ms: `clang -O2` removes a `malloc`/`free` pair whose result it can see through. The Cranelift build does the real traffic (1.47 s for the 100 million). The footprint claim above is therefore a Cranelift measurement; the LLVM build proves the program type-checks, links and agrees on the answer.
2. **"A box built in a thread and freed by the parent after `join`" is not expressible yet**, and 8.4 should not have listed it. `join` refuses a `Box` result ("cannot cross back from a real thread yet"), and a `res` field cannot be moved out through a `&!` reference, so a worker has no way to hand a box back. What the fixture proves instead is the other direction of 8.3's claim, a box made by the fork and freed by the parent. Handing a box back is a separate, small piece of work for T3 if a worker needs it (a worker's reply is more likely to be bytes in a buffer the parent already owns).
3. **A compiler bug turned up on the way and is fixed**: `join(a) + join(b)` failed on the default (LLVM) backend with "the compiler failed to generate code" because the scalar-kind inference had no arm for the `Joined` node. It now answers the thread's result type; `tests/accept/spawn_join_operands.ls` pins it on both backends. Cranelift was never affected.

Not claimed: that a forked heap is faster than sharing one, which is T3's measurement.

## 9. T3: two threads running `http.server`

### 9.1 What it took

`examples/users_threads/users_threads.ls` in `lexsys-web` is `users` with its `main` replaced; the application, the schema and the whole server loop (`run`) are unchanged. Main opens two `SO_REUSEPORT` listeners (it holds the only `Net`), and each worker is a struct `Lane { heap, clock, listener, status }` handed to a thread by `&!`, which calls `run(w.heap, w.clock, w.listener)` and builds its own `Poller`, `Server`, store, schema and buffers inside the thread. So the first question of 8.5 has a plain answer: **a library can be driven from a thread through struct fields**, because `run` takes its capabilities as borrows, and the `Server` itself never needs to live in the struct (the thread makes it). No change to `http.server` was needed.

What the language needed was one more builtin. A thread needs a `Heap` (T1, `fork_heap`) **and a `Clock`** of its own, since `server.wait` takes `&Clock` and a `Clock` is `res`. `split` gives one, and a thread cannot borrow the parent's across `spawn` (only one `&!` struct reference crosses). So `fork_clock(&Clock) -> Clock`, edition 5, makes another from a shared borrow of the first. Its argument for soundness is `fork_heap`'s: a clock reads the monotonic time and nothing else, the parent already holds the authority, the label is unchanged, and no program's `lex-os` grant or `lex-sys authority` report changes. It is also where the capability discipline gives a little: **authority was conserved before (one holder of each capability), and for `Heap` and `Clock` it no longer is**. `Net` is deliberately not forkable: the listeners are made by the one thread that holds it, and workers receive listeners, not the right to make them. `tests/accept/fork_clock_workers.ls` (both backends) and `tests/reject/fork_clock_after_release.ls` pin it.

### 9.2 Measured

Same workload and machine as the baseline (invalid `POST /users`, 16 connections, 4 `kload` threads; server threads or processes on cores 0 and 1, the load generator on 2 and 3), the two variants **interleaved in three rounds of five runs each**, so the machine's drift hits both:

| | runs (requests a second, sorted) | median of 15 |
|---|---|---|
| two processes, `SO_REUSEPORT` | 115,972 125,328 133,872 136,180 138,096 140,388 140,436 141,208 141,264 147,292 152,204 154,896 159,708 160,972 173,404 | **141,208** |
| two threads, one process | 112,228 125,560 129,176 130,500 132,984 133,004 137,844 149,380 149,428 160,212 160,280 161,224 162,892 164,344 165,356 | **149,380** |

Both threads are busy (about 1,500 and 1,700 ticks of CPU each over a run), so the load is spread and not carried by one. The ratio of the medians is 1.06, the criterion was at least 0.9 and at least 143,000 a second, and **both are met**; but the ranges overlap almost completely and the machine was slower than when the baseline was taken (processes then 159,176, today 141,208), so the honest reading is *threads are indistinguishable from processes here, and not worse*. The first single block of five gave the threads 161,432; the pooled median is the number to quote.

### 9.3 What this does and does not say

* It says a typed, linear, capability-checked server can use several cores **in one process** with the same code, no `unsafe` and no shared mutable state, at the throughput of the process-per-core arrangement it replaces.
* It does not say threads *beat* processes. For a stateless service they should not: both are share-nothing. The reason to have threads is what processes cannot do, which is **share a store**, and that needs T4 and T5 (shared read-only data, then communication), neither built.
* Three cores cannot be measured on this 4-core machine without the load generator sharing a core, so scaling past two is unmeasured.
* The in-memory store (`POST` that succeeds) is per-thread today, so a `GET` may land on a worker that has not seen the `POST`. This example only demonstrates the stateless path; sharing the store is exactly T4/T5.
