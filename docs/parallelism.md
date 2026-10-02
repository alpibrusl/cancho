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
| soundness is the existing aliasing rule: a reference crosses if the thread is joined before the region it belongs to ends | `threads.md` §3 |
| multi-field payloads and `Rc` were decided "not yet": nothing asked, and a general struct needs a compiler-built trampoline | `threads.md` §5 |
| no atomics, no channels, no shared mutable memory; `Rc` is not implemented | `grep` of `docs/` and `std/` |
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
| an owned `Net(..)` or `Clock` | **refused**, though the allowlist's own comment says `Net` has no fields |
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
| **T1** | give a thread **its own `Heap`**. The runtime is `malloc`, which is thread-safe, and a `Heap` carries no state, so the uniqueness is a type-system choice (`heap.md` §2): the question is how a second one may be *made* without making allocation authority unbounded. Candidates: `spawn` takes the parent's `&!` heap and gives the thread a derived one; or a `fork_heap` that is itself `[conc]`. Decide in this step, on the budget document (`budget.md`) first | a worker thread allocates a `Box`, the parent frees it after `join`, on both backends, with `the_heap_actually_frees`'s footprint check still passing |
| **T2** | admit `Net(..)` and `Clock` as payloads: both are zero- or one-leaf like `File` (the comment on `crosses_to_a_thread` says so) and the change is the allowlist, as `File`'s was | fixtures for each on both backends; the reject fixtures that pinned the refusal become the obsolete-pin case (`function-values.md`'s own precedent) |
| **T3** | **server workers**: N threads each running `http.server`'s loop on its own `SO_REUSEPORT` listener, the schema and the OpenAPI document shared read-only through `&`. Pre-register the criterion before building: at N = 2 and 3 on 4 cores, **at least 0.9x of N processes sharing a port** (measure the process numbers first, for the in-memory `users` service) | if it is below 0.9x, say so and keep processes: they are free today |
| **T4** | **shared read-only data** across threads (`&T` crossing to several threads at once): test it, document it. It already type-checks as one reference per thread (`tests/accept/spawn_thread_ids.ls` shares a borrowed capability); what is missing is a worked example | a reader pool over a shared `Box[[int]]` agrees with the single-thread answer |
| **T5** | **communication**: a bounded queue between threads needs an atomic load and store with ordering, which does not exist. Design only: what the primitive is, and why it is not `Rc` | **not started until a program asks** (the pool and the HTTP loop are one thread today and do not) |

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
2. **T1 on paper**: how a thread gets a `Heap`. It is the one blocker that is a design question and not a
   check-list, and T3 cannot start without it.
3. **T2** (the allowlist) and the T3 baseline (the in-memory `users` service as N processes), which cost an afternoon
   each and decide whether T3 is worth building.
4. **V2** and **V3** only after a column kernel exists to measure them on.
