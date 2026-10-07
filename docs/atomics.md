# Atomics, and a channel built on them

> **Status: design, with the measurements it needed. No compiler change.** `threads.md` §4 and `parallelism.md` T5
> say atomics and channels are "not started until a program asks". A program now asks: a parallel CSV scan in
> `cancho-table` (a dynamic work counter between workers, and results delivered in order) and a possible network
> broker. The question that came with the request was *is there a hard constraint, or did nobody ask?* §1 answers
> it with evidence, and the answer is **nobody asked, plus four real constraints that shape the design but block
> nothing**. The most important thing this document found is not about atomics: **the checker does not stop two
> threads writing one value today** (§2, measured), and `threads.md` §3 said it did. That is corrected in place.
>
> What this decides (each with its trade-off in the section that makes it):
>
> | | decision | § |
> |---|---|---|
> | the type | one `res` type `Atomic`: a heap block of *n* 64-bit words, reached through `&Atomic`; no plain access exists | 3 |
> | operations | sequentially consistent only: `load`, `store`, `fetch_add` (wrapping), then `exchange` and `cas` | 4 |
> | effects | the existing label `conc`; creation is `[heap]`; no new capability | 5 |
> | blocking | a separate, later step (`wait`/`wake`), and the channel ships without it first | 6 |
> | the channel | a library over atomics, messages are `int`s; not a primitive | 6 |
> | the gate | litmus tests, a stress counter, nine lowering mutants and a model check, fixed before any code | 7 |
> | the cost | edition 8; four stages A0-A3, each with its own pass/fail line | 8 |
>
> Measured on: Apple M-series arm64 macOS (compiler built from `origin/main` at f7e7307) and an Intel i7-1260P
> x86-64 Linux box, 16 threads, cores 0-5 only, niced (the Linux compiler there is an older rev, 4c27593, built
> before editions 7; the programs used are edition 6). Every number says which. A claim that was read and not run
> says **read**, one that was neither says **assumed**.

---

## 1. Hard constraint, or nobody asked?

### 1.1 It is not a hard constraint: what already exists

| fact | how known |
|---|---|
| **The compiler already emits an atomic.** `spawn`/`join` keep a count of unjoined threads with one atomic add (`count_thread`, `cancho-codegen/src/body/signals.rs`; `cancho-codegen-llvm/src/body/signals.rs`). It has been in every threaded binary since #128 | **measured**: `cancho build tests/accept/spawn_join.cho --emit obj` disassembled: Cranelift on arm64 emits `ldaxr`/`stlxr`; LLVM `--target x86_64-unknown-linux-gnu` emits `lock xadd`, `--target aarch64-unknown-linux-gnu` emits `ldaxr`/`stlxr` |
| **Both backends have all five operations, sequentially consistent.** Cranelift 0.121.2: `atomic_load`, `atomic_store`, `atomic_rmw` (with `Add`, `Xchg` and the bitwise ops), `atomic_cas`, `fence`; its own documentation says each is "sequentially consistent and creates happens-before edges that order normal (non-atomic) loads and stores". LLVM: `load atomic`, `store atomic`, `atomicrmw`, `cmpxchg`, all `seq_cst` | Cranelift: **read** (`cranelift-codegen-meta` 0.121.2 `instructions.rs`, and the x64 and aarch64 `lower.isle`). LLVM: **measured** on a hand-written module, §4.2 |
| **A shared reference already crosses to several threads at once.** One `&r Ffi` is the payload of two spawns and read a third time by `main` after both joins (`tests/accept/spawn_thread_ids.cho`); `crosses_to_a_thread` admits any non-slice `Type::Ref`, so **`&Atomic` needs no change to the payload rule** | read, and `spawn_thread_ids.cho` is a conformance test |
| **A struct with a `Box[[byte]]` and an `Ffi` field crosses by `&r` to four threads, which read through it.** | **measured**, `benches/atomics/libatomic_counter.cho`, §1.3 |
| `Atomic`'s memory is one `malloc`, so it is aligned. The one atomic the compiler emits had a latent alignment bug (a zeroed global, `threads.md` §6, SIGBUS on aarch64 Linux); a `malloc`ed cell does not have it | read |

Nothing in the language, the ABI, either backend or either OS forbids an atomic. The wall `threads.md` §1 found for
`pthread_create` (a `void *` that cannot carry a region) does not apply: an atomic operation has no callback and no
foreign pointer.

### 1.2 What is real: four constraints that shape the design

These are not reasons to refuse atomics. Each changes what the design has to be.

1. **A borrow copies.** `borrow x as &r in { .. }` spills `x` to a buffer and points `r` at the buffer; a unique borrow
   writes it back at the close, **a shared one writes nothing back** (`ir.rs`, `Stmt::Borrow`, **read**). Put an
   atomic *value* in a local and borrow it shared, and each borrow gets its own copy: two threads would increment
   two cells, and the owner would never see either. So the cell cannot be the value; it has to live at a stable
   address, and the value that is borrowed is a handle to it (§3).
2. **A struct cannot name a region** (`aliasing.md` §3, route 2: "a type declaration takes no region parameters").
   So a per-worker struct cannot hold a `&r Atomic`, and `Atomic` is `res`, so one cannot be handed to each worker
   by value. Together with the one-leaf payload (`threads.md` §2), a worker gets **one** pointer. That is enough for
   the counter and the ordered slots (§9.2) because everything shared goes in one `res struct Shared` passed as
   `&r Shared`. It is **not** enough for a worker that needs both a shared `&r Shared` and its own `&!r Job`: that
   is `thread-payloads.md`'s tuple payload (T-P1), and this document does not pretend atomics replace it.
3. **`&!` does not mean unique, so the checker is no help with plain shared memory** (§2).
4. **A new type and new builtins cost an edition** (`editions.md`), and a hash check (§8).

### 1.3 "Write it in your program first" (`AGENTS.md` §7): possible for a counter, on Linux, and no further

`AGENTS.md` says a feature earns its way in when a program asks, and that you write it in your program first. For
atomics that is nearly impossible, and **how nearly is worth a measurement**: a program has no way to write an atomic
operation, because `__atomic_*` are compiler intrinsics, not libc functions. The one exception found:
`libatomic` (GCC's runtime, on Linux) *exports* `__atomic_fetch_add_8`. Only a `[byte]` slice crosses a foreign
boundary (the checker's own message: "the only references that cross a foreign boundary are a borrowed capability
and a `[byte]` slice"), and it crosses as a pointer **and a length**, which are the first two C arguments. So a
slice of 8 bytes is `(p, 8)`: an `__atomic_fetch_add_8(p, 8, order)`.

`benches/atomics/libatomic_counter.cho`: four threads each add 8 to one 8-byte heap cell a million times through a
shared `&r Shared`, with `Ffi("atomic")` and `-l atomic`. **Measured on x86-64 Linux, both backends: the answer is
exactly 32,000,000 on every run**, in 0.05-0.08 s for the four million contended increments (about 12-20 ns each,
thread start included). It works, so no hard constraint exists even today. It is not an answer: it is Linux only
(macOS has no `libatomic`), it needs `Ffi`, which `reach.md` says is every authority at once, it adds only `v = len`,
it cannot load, store or compare, and the "increment" is a call through the PLT. It is a stopgap for `cancho-table`'s
one counter and nothing more, and it is the reason a second asker cannot be satisfied by writing it first.

### 1.4 What the askers need

| asker | needs | atomics alone cover it? |
|---|---|---|
| `cancho-table`: workers claim chunks of a CSV from a shared counter | `fetch_add` on one cell | yes (A0) |
| `cancho-table`: results come back **in chunk order** | one `ready` word and one result word per chunk, a consumer that reads them in order | yes for a small result (a count, an aggregate), A0; **no** for rows (§9.2) |
| a broker: handlers fed by an acceptor | an MPSC queue, blocking, and a way to wake a thread that is blocked in the `Poller` | queue yes (A3); **waking a poller no**, §6.4 |
| `cancho-cache` (`thread-payloads.md` §4) | a shared store | no: needs shared mutable memory, not a word |

`cancho-table`'s repository holds no threaded code and no design for it today (checked: nothing in its `docs/` or
`README.md` mentions a thread or an atomic); the ask is the request that came with this document, not a program.
`AGENTS.md`'s bar of a second program is met in the sense that `cancho-cache` named the same absence in writing.

---

## 2. The hole: nothing stops two threads writing one value

> **Closed for `spawn` since this was written** (`aliasing.md` §6.1). `spawn` lends the `&!` it is given until
> `join`, so the program below is refused (`tests/reject/spawn_two_copies_of_unique.cho`); the join-first control is
> accepted (`tests/accept/spawn_unique_join_first.cho`). `benches/atomics/race.cho` and `sb.cho` no longer compile: they
> are kept as the record of what was measured. Everything below describes the checker *before* that change, and
> stands as the measurement. It also means §7.1's plain-access litmus test (`sb.cho`) cannot be rerun without a
> reference the checker still lets two threads share (§1.3's atomics are exactly that, once they exist).

`threads.md` §3 argues that no new soundness rule is needed because "the aliasing rule ... forbids a second writer
while a reference is live ... and a spawned thread joined before its region closes is just a second writer the
checker already refuses to admit exists". `aliasing.md` says the rule is not that: `&!` is **a lock on the binding**,
`&!r T` is `val` and copies, and two copies are two copies of one pointer.

Put the two together and it is a program the checker accepts (`benches/atomics/race.cho`):

```
borrow mut c as &!r in {
    let a = r;  let b = r;               // two copies of one pointer
    let ta = spawn(a, fa);  let tb = spawn(b, fb);   // each adds 1 to c.n, 200,000,000 times
    ...
}
```

**Measured**: it compiles on both backends and loses updates. The answer is 400,000,000 and the program printed `L`
(wrong) on every run, three runs of each backend on arm64 macOS and one of each on x86-64 Linux. The control that
joins the first thread before spawning the second prints `E` on both backends. So this is a data race, in a
language whose documents say it has none, reachable in four lines.

What this means for atomics:

* **It is not caused by atomics and atomics do not fix it.** A program that wants safe shared mutation of an *ordinary*
  value still cannot have it. `&!` copies are the root, `aliasing.md` routes 2 and 3.
* **An `Atomic` is sound without the aliasing rule** (§3.3). That is the point of making the cell opaque and every
  operation atomic: the type does not lean on the one rule that does not hold.
* **`aliasing.md` §6 named this**: "Parallel lanes ... the *correctness* argument, which today is nil ... Two lanes
  writing through aliasing references is the race a checker should catch, and that is when this stops being about
  speed". Threads arrived in #128; the condition is met and the third row of that table (provenance in signatures)
  is what a sound `&!` across threads needs. Not proposed here. A narrower fix that only `spawn` needs is possible
  (refuse to spawn with a `&!` that was copied: route 2 closed for the one call), and `thread-payloads.md` T-P3's
  disjoint split is the same family. Both are separate designs.
* **`threads.md` §3, `parallelism.md` §2 and `aliasing.md` §6 are corrected in place** by this change.

Why the hole is also a *test fixture* for this design: it is the cheapest way to show the litmus tests of §7 can fail
(plain accesses, no atomics): `benches/atomics/sb.cho` is the store-buffering test written with plain accesses through
that hole, and §7.1 reports what it measured.

---

## 3. The type

### 3.1 The decision

**One new `res` type, `Atomic`: a heap block of *n* 64-bit words, each readable and writable only through the atomic
operations.**

```
atomic_new(h: &!x Heap, n: int, fill: int) -> [heap] Atomic      // n >= 1, or a trap; every word starts at fill
atomic_free(h: &!x Heap, a: Atomic) -> [heap] int                // the only consumer; answers n
atomic_len(a: &r Atomic) -> [] int                               // the length never changes
atomic_load(a: &r Atomic, i: int) -> [conc] int                  // i is bounds-checked, like a slice index
atomic_store(a: &r Atomic, i: int, v: int) -> [conc] ()
atomic_fetch_add(a: &r Atomic, i: int, d: int) -> [conc] int     // answers the OLD value; wraps
```

(Illustrative signatures, as `threads.md` §2's were: builtins checked at the call site, like `box_slice`.) A cell is
`atomic_load(a, 0)`. `Atomic` is released like a `Box`: it is `res`, a program that never frees it does not compile,
and `atomic_free` is its only consumer (the same `unboxed_only` pattern `defs.rs` already has for `Box`).

### 3.2 Why this shape, and the alternatives

| | | verdict |
|---|---|---|
| **A** | `Atomic[int]` / `AtomicBool` as a value type, borrowed `&Atomic` | **rejected on measurement of the compiler, not taste**: a borrow spills the value and a shared borrow does not write back (§1.2.1), so every borrow would be its own cell. Fixing it means a rule "a type that is interior-mutable is borrowed by its own address", which needs every such local to live in memory, a new lowering for locals *and* for struct fields, and a write-back on a shared borrow. That is real compiler work in both backends for a type whose first user (a counter and some slots) does not need it |
| **B (chosen)** | a handle to heap memory | the handle is one pointer leaf; spilling it copies the *pointer*, so every borrow reaches the same cell. No change to `borrow`, to struct layout or to the spill. Cost: one `malloc`, one extra load per operation (the handle is read from the spill, then the cell), and a `Heap` to create it (so creation is `[heap]`, which is honest: it allocates) |
| C | `Atomic` as an *array* of words (chosen) against a scalar cell | the channel and the ordered slots both need many words, and one allocation of *n* words is cheaper than *n* handles (§9.2's chunk table would be thousands of `malloc`s). A scalar is `n = 1`. Cost: every call takes an index (`atomic_load(a, 0)`), and a bounds check (one compare, the same as a slice index). The bounds check is **not** optional: an out-of-range atomic store is memory corruption |
| D | `Atomic[T]` generic over `int`/`bool`/`byte` | **not now**. One width, 64 bits, `int`. A `bool` is 0 or 1, a flag is a word. A narrower atomic is a second layout and a second set of lowering arms for no asker. `Atomic[T]` can come later as an edition-gated widening (it is additive) |
| E | a plain `static` atomic (a process-wide counter) | **no**. `static` data is read-only (`compile-time-data.md`); a global mutable word is ambient authority by another name, which the capability design exists to refuse. A program that wants one makes it in `main` and passes it |

The trade-off to name: **B costs an indirection on every operation that a scalar-in-place design would not**, and
needs a `Heap` where a counter in a struct would not. In exchange nothing in the checker's borrow machinery or either
backend's layout changes. If a program shows the extra load matters, A is the thing to build, and §3.1's signatures
are unchanged by it: that is the reason to write the operations over `&Atomic` and not over `&Box[..]`.

False sharing is the library's problem, not the type's: a channel puts `head` and `tail` sixteen words (128 bytes)
apart (§6.3).

### 3.3 Why it is sound, and what it does to the rules

The argument does not use the aliasing rule (§2), which is the one that does not hold.

1. **No plain access exists.** `Atomic` has no fields, cannot be destructured, and has no `contents`: nothing reads
   or writes a word except the operations above. Every one is atomic, so **no data race on an `Atomic` is
   expressible** whoever holds a `&Atomic`, however many copies of it there are, on however many threads.
2. **No use after free.** `atomic_free` consumes the handle, and consuming needs the binding not to be borrowed. The
   borrow's block cannot close before `join` (the existing escape check: a `Thread[&r Atomic, R]` that outlived `r`
   is `reference-escapes-region`, `tests/reject/spawn_handle_escapes_borrow.cho`, an existing rule applied to one more
   type). So no thread is running when the cell is freed. Nothing here is new.
3. **No out-of-bounds.** The index is checked on every operation.
4. **No tear.** Cells are 8-byte aligned (they come from `malloc`, which gives at least 16), operations are 64-bit,
   and `MemFlags::trusted()` in Cranelift (which asserts alignment) is therefore true. A test makes it a checked fact:
   §7.3, mutant 9.
5. **`&Atomic` copying is harmless** by (1).

What it does to the existing rules:

* **It is the first interior-mutable type**: the first thing a function can change through a *shared* reference.
  The convention "a function that takes only `&` references cannot change what it was given" stops being true for any
  function that receives a `&Atomic`. Two places use that convention, and both were checked rather than assumed:
  * **The optimizer**: `is_pure` (`ir.rs`, `docs/purity.md`) calls a function pure if its row is empty and no
    parameter reaches a *unique* reference. A `&Atomic` is not unique, so condition 2 would **not** catch
    `fn get(a: &Atomic) -> [] int { atomic_load(a, 0) }`, and the constant folder's pure-call elimination could then
    merge two calls, turning `while get(a) == 0 { }` into a loop that never ends. **The row is what prevents it**:
    `atomic_load` carries `conc`, so the function's `performs` is not empty and `is_pure` is false. This is the
    reason §5 chooses a label and not `[]`. It is a gate (§7.3, mutant 7).
  * **The backends**: neither emits `noalias`, `readonly` or `invariant` for a reference (**read**: `grep` of
    `cancho-codegen-llvm/src`, and `aliasing.md` §5 for Cranelift), so no backend assumes a shared reference's
    pointee is stable during a borrow.
* **It does not touch uniqueness.** `&!` stays what `aliasing.md` says.
* **Linearity**: `Atomic` is `res`, so a `val` struct cannot hold one (the existing "a `val` type may not hold a
  `res`" rule), and a `res struct Shared` that holds one is shared as `&r Shared`, whose `s.cell` is a `&r Atomic`.
* **`Rc` still does not exist.** An atomic `Rc` needs a copyable pointer, and `sharing.md` §2 says none exists. This
  design adds no copyable pointer: a `&Atomic` is bounded by its region, like every reference.

### 3.4 Hash identities

`hash-stability.md`: a new *node kind* moves tags; a new *name* does not. `Atomic` and the builtins are names (like
`File` in #65, which "added three prelude type names ... no node kind, no tag moved"). **Expected: no existing hash
moves**, because the whole feature is edition 8, absent from older files. **Unknown until built** whether adding names
to the interner's `PRELUDE` list (`cancho-syntax/src/ast.rs`) shifts any `DefId` an existing encoding reads. The
gate for it is mechanical (§8, A0): the 35 golden fixtures of `crates/cancho-id/tests/golden.rs` unchanged, and
`cancho ids` over every file in `examples/` and `tests/accept/` byte-identical before and after. If either moves, the
change is withdrawn, because that is exactly the "rate" `hash-stability.md` measured.

---

## 4. Operations and memory ordering

### 4.1 Sequentially consistent only, first

**Decision: every operation is `seq_cst`, with no ordering parameter.** Reasons, with the numbers where there are some:

1. **One thing to get right and to test.** Under SC every litmus outcome is decided by "is there an interleaving?",
   which is exactly what §7 can enumerate. Weak orderings need a memory-model checker.
2. **Cranelift has no other kind.** Its atomic instructions take no ordering and are all documented SC (**read**). A
   relaxed or acquire-release API would have to lower to SC there (stronger is always correct) and to something
   weaker on LLVM. Then the two backends differ in *speed* and, worse, **the differential test between them loses
   its power**: a weak-ordering bug that LLVM's weak code exposes cannot be seen in Cranelift's strong code, and the
   repository's method (`differential.md`: two backends must agree) has nothing to compare.
3. **On the machines measured, SC costs almost nothing except one thing.** One thread, 200 million operations, C
   (`gcc 15.2 -O1` on x86-64 Linux; `clang -O1` on arm64 macOS), per operation:

   | | x86-64 Linux | arm64 macOS |
   |---|---:|---:|
   | store, seq_cst | **6.08 ns** | 0.81 ns |
   | store, release | 0.34 ns | 1.15 ns |
   | store, relaxed | 0.37 ns | 0.83 ns |
   | fetch_add, seq_cst | 6.39 ns | 5.73 ns |
   | fetch_add, relaxed | 6.49 ns | 5.28 ns |
   | load, seq_cst | 0.37 ns | 1.92 ns |

   So the only place weaker ordering buys anything *on these two machines* is an **x86 store** (an `xchg` instead of
   a `mov`: about 5.7 ns). Loads and read-modify-writes cost the same either way, and on arm64 nothing differs. (One
   thread, uncontended, a tight loop: this is the instruction's cost, not a contended queue's; the figures are
   not claimed for other CPUs, and `ldar`/`stlr` on other ARM cores may differ.) A channel does about two stores per
   message; 12 ns is far below the cost of the work a message carries.
4. **Weakening is additive.** `atomic_store_release` and friends can be added later without changing the meaning of
   anything written now, which is why SC-first loses nothing.

**What relaxed and acquire/release would add, and why not yet:** (a) an x86 store 18 times cheaper (above); (b)
counters and statistics that need no ordering at all (`fetch_add` relaxed on arm64 skips the barrier, saving
under 10% in the table above); (c) a fence. **Not yet**, because no asker has a measured hot path where (a) matters,
the differential argument in (2) is lost, and a wrong weak ordering is the kind of bug §7's tests find least reliably
(§7.1 on IRIW). The trigger to revisit: a program whose profile has a seq_cst store on its critical path above a
few percent. That is a measurement a program can make with A0 in hand.

**`fetch_add` wraps.** The language's `+` traps on overflow; `wrapping_add` is how a program asks for wraparound. An
atomic read-modify-write that trapped would need a compare-and-swap loop plus a trap, and a trap in the middle of
another thread's observation of the cell is a half-done state. So `atomic_fetch_add` is the wrapping operation, and
its name should say so in the documentation exactly as `wrapping_add`'s does (counters modulo 2^64 are the
intended use). A trapping add is a library loop over `cas` (A1). **Trade-off:** a program that forgets and adds
past 2^63 gets a negative number, not a trap.

### 4.2 How each lowers

| op | Cranelift (0.121.2) | LLVM |
|---|---|---|
| `load` | `atomic_load.i64`, aligned flags; **x64**: a plain `mov` ("x86-TSO ... without the need for any fence"); **aarch64**: `ldar` | `load atomic i64 ... seq_cst, align 8` -> **x64** `movq (%rdi), %rax`; **aarch64** `ldar` |
| `store` | `atomic_store.i64`; **x64**: `mov` then `mfence`; **aarch64**: `stlr` | `store atomic ... seq_cst` -> **x64** `xchgq`; **aarch64** `stlr` |
| `fetch_add` | `atomic_rmw.i64 add`; **x64**: `lock xadd`; **aarch64**: an `ldaxr`/`stlxr` loop, or the LSE instruction where the target allows | `atomicrmw add ... seq_cst` -> **x64** `lock xaddq`; **aarch64** `ldaxr`/`stlxr` loop |
| `exchange` | `atomic_rmw.i64 xchg` | `atomicrmw xchg` -> **x64** `xchgq`; **aarch64** `ldaxr`/`stlxr` |
| `cas` | `atomic_cas.i64`: answers the old value, **x64** `lock cmpxchg`, **aarch64** a loop | `cmpxchg ... seq_cst seq_cst` -> **x64** `lock cmpxchgq`; **aarch64** `ldaxr`/`cmp`/`stlxr` |

What was verified and what was assumed:

* **Measured**: the existing `fetch_add` the compiler already emits, on Cranelift (arm64 host: `ldaxr`/`stlxr`) and on
  LLVM (x86-64 and aarch64 targets: `lock xadd`, `ldaxr`/`stlxr`), by disassembling `spawn_join.cho`. The whole LLVM
  column, including the x64 and aarch64 instructions, by compiling a module of the five operations with `clang -O2`
  for both triples (baseline CPUs, no LSE).
* **Read, not run**: Cranelift's x64 column (the `lower.isle` rules and their comments) and aarch64's `ldar`/`stlr`
  rules. The Cranelift x86-64 object was not disassembled: `--target` needs LLVM (Cranelift emits for the host), and
  the Linux box's compiler is the older rev. A0's first task is to disassemble it.
* **Assumed**: that the LSE instruction forms are used when a target enables them (default off here), and that
  Cranelift's alias-analysis pass (redundant-load elimination) treats an `atomic_load` as a barrier. This is the
  one that matters: if it does not, two atomic loads of one cell in a loop could be merged. §7.3 mutant 8 is that test.
* A hazard visible in the table: on x86 an SC load and a *plain* load are **the same instruction** (`movq`). So a
  mutant that lowers `atomic_load` as a plain load is **invisible to a litmus test on x86 and invisible in the
  object code**; only the optimizer's behaviour distinguishes them. That is why §7.3 has an emission test and a
  spin-loop test and does not rely on litmus tests alone.
* `fetch_add` on aarch64 without LSE is a loop and **can fail to make progress under extreme contention** in theory
  (LL/SC); the stress test (§7.2) measures, it does not prove.

`cas` answers the **old value**, not a bool: success is `old == expected`. A tuple `(bool, int)` is available
(`tuples.md`) and costs nothing, but then the builtin has two results; one answer keeps the signature `-> [conc] int`
and the library writes `cas(..) == expected`.

---

## 5. Effects, authority and determinism

### 5.1 The label: reuse `conc`

**Decision: every operation on a `&Atomic` carries the existing `conc` label; creation and freeing carry `heap`; no new
label and no new capability.**

* **Why a label at all** (and not `[]`): §3.3. `[]` means pure to the optimizer; an atomic read is not.
* **Why `conc` and not a new `atomic` label.** `conc` is "concurrency entered the program's authority surface ... and
  no capability carries it" (`wasi_imports.rs`). An atomic is meaningless without a second thread and the pair is
  always used together. Reuse costs nothing in vocabulary and **gets a refusal for free**: `conc` is in
  `REFUSED_LABELS` for `wasm32-wasip1`, so a program that uses an atomic is refused on WASI with the located refusal
  threads already get (`tests/reject/spawn_on_wasi.cho`), with no new code. A single-threaded atomic on WASI would
  work but is pointless.
  The cost: `conc` now also tells a reader "shares mutable cells", and a supervisor cannot distinguish "may spawn"
  from "may share state". Nothing needs the difference today (a grant of threads without shared state has no
  meaning: a thread with no way to share is shared-nothing already). An `atomic` label is additive and can be split
  out in a later edition; the cost then is one migration of every library that returns `[conc]`.
* **No capability.** The authority to touch a cell is **holding a reference to it**, the same as for a `&!` buffer.
  A thread given no `&Atomic` cannot reach one; no operation names a cell by anything but the reference.

### 5.2 What `cancho authority` should say

Today a threaded program reports `performs conc`, `bounded: true`. **Unchanged, with one text change**: the line
should say what the label now means. Suggested wording for the report: `conc: runs threads, and may share cells between them`.
The JSON (`"effects": ["conc"]`) does not change, so no supervisor parses anything new. **Unknown**: whether `lex-os`
grants key on `conc` today and what an `Atomic`'s `[heap]` creation does to a grant that allows `conc` but not `heap`
(it would have to allow both). `lex-os` was not read for this document.

### 5.3 Determinism

Shared mutable state is what defeats a byte-stable output: which worker takes which chunk, and when, depends on the
scheduler. **The discipline, stated as a rule the library can obey and a test can check:**

> An atomic is for **coordination**, never for **content**. What a program emits must be a function of the *input*
> alone. Work is claimed in whatever order the scheduler gives; **results are merged in a fixed order** (by chunk
> index, not by completion time).

The ordered slots of §9.2 are that rule: any worker may compute chunk `k`, the consumer reads chunk 0, then 1, then
2, so the output is the same bytes for 1 worker and for 8. It is checkable (§8, A3: byte-identical output across
worker counts and 200 runs each). What is **not** deterministic and cannot be made so: the claim order, timing, and
which thread ran what. A program that prints a thread number or uses the *value* a `fetch_add` returned as data
(not as an index into slots) has opted out, and nothing will say so.

---

## 6. Blocking, and a channel as a library

### 6.1 A channel needs no kernel support to be correct

A bounded queue over atomics (§6.3) is correct without blocking: `try_send` and `try_recv` never wait, and a caller
that must wait retries with a backoff. So **the channel (A3) can ship before any wait/wake builtin (A2)**, and A2
becomes a measurement ("how much latency and CPU does the retry loop cost?") and not a prerequisite. This is the
order chosen below; the user's suggestion that blocking is needed *for* a bounded channel is true only for a
channel that must not burn CPU while empty.

What a retry loop needs that does not exist as a builtin: a **sleep**. **Read**: `grep` of `builtin.rs` finds no
sleep or yield; `spawn_parallel_sleep.cho` reaches `usleep` through `Ffi("libc")`. A `Poller` wait with a timeout and
no descriptors might serve as a sleep; **not tried**. A pure spin works and burns a core.

### 6.2 `wait`/`wake` (A2), what it costs

```
atomic_wait(a: &r Atomic, i: int, expected: int, timeout_ms: int) -> [conc] int
atomic_wake(a: &r Atomic, i: int, n: int) -> [conc] int           // wakes at most n waiters, answers how many
```

* **Linux**: `futex(FUTEX_WAIT_PRIVATE / FUTEX_WAKE_PRIVATE)` through `syscall(2)`, called by the compiler (as
  `pthread_create` is: the *program* never names it, `threads.md` §1). The syscall number differs per architecture
  (x86-64 202, aarch64 98; **from memory, to verify**). **A futex word is 32 bits.** The cell is 64. The wait
  compares the low 32 bits only (little-endian, same address): a value that differs from `expected` only in the high
  half would sleep when it should return. The contract: *states a waiter distinguishes differ in their low 32
  bits*; the channel's sequence words are chosen so (and wrap in the low half at 2^32 messages, which the library
  must tolerate).
* **macOS**: `__ulock_wait`/`__ulock_wake` (private but long-stable: used by the C++ and Rust runtimes; **assumed**,
  not checked here) or `os_sync_wait_on_address` (public, macOS 14.4 and later; **assumed**). Which one, and the
  minimum macOS the compiler's output may then require, is A2's first question; a private symbol in generated code
  is a policy decision, not a technical one.
* **A portable fallback** needs nothing from the kernel: sleep-with-backoff in the library (§6.1). It costs latency
  (the sleep quantum) and is the right answer if A2's measurement says the futex is not worth a private symbol.
* **Spurious wakeups are normal and the contract says so**: `atomic_wait` may return when the cell has not changed.
  It answers a code (`0` woken or spurious, `1` value already differed, `2` timed out); **the caller always
  re-checks**, in a loop, and a library that does not is wrong.
* **`EINTR`** is the same case: it returns as "re-check". Signals are blocked in the thread by the signal claim
  (`signals.md` §5: `pthread_sigmask`), so it should be rare; **not measured**.
* **No cancellation**, as `threads.md` §4 says of `join`. The only way to ask a blocked thread to stop is to **wake
  it with a shutdown word set**, and a timeout so a thread that is not woken still looks. A wait with no timeout and
  no waker is a deadlock the type system does not see.

### 6.3 The channel, as a library

Not a primitive: a `std` module (`std.chan`) once a second program needs it (`AGENTS.md` §7), written in
`cancho-table` first. **Messages are `int`s** (a pointer-width leaf, like a thread's payload): the int is an index, a
handle, a count. A message that is a buffer is an index into memory the *receiver's side already owns and lent to the
sender*; moving a `Box` through a queue needs `join`-style Box crossing (`parallelism.md` §8.6, "not expressible
yet").

A bounded multi-producer, single-consumer ring, after Vyukov's bounded queue (each slot has a sequence word; a
producer claims a position with a CAS, publishes with a store). **Sketch, not compiled; the syntax is illustrative:**

```
// one Atomic block: [0] head, [16] tail (128 bytes apart, against false sharing), [32] closed, then
// slots: slot k is two words at 48 + 2k: [seq, value].  seq starts at k.
res struct Chan { cells: Atomic, cap: int }

fn try_send[&r](c: &r Chan, v: int) -> [conc] bool {
    loop {
        let pos = atomic_load(c.cells, 0);
        let at  = 48 + 2 * (pos % c.cap);
        let seq = atomic_load(c.cells, at);
        if seq == pos {                                          // the slot is free for this lap
            if atomic_cas(c.cells, 0, pos, pos + 1) == pos {     // claimed
                atomic_store(c.cells, at + 1, v);
                atomic_store(c.cells, at, pos + 1);              // publish
                return true;
            }
        } else if seq < pos { return false; }                    // full (a lap behind)
        // else another producer is ahead: retry
    }
}

fn try_recv[&r](c: &r Chan) -> Recv {                            // Got(int) | Empty | Closed
    let pos = atomic_load(c.cells, 16);                          // one consumer: no CAS needed
    let at  = 48 + 2 * (pos % c.cap);
    if atomic_load(c.cells, at) == pos + 1 {
        let v = atomic_load(c.cells, at + 1);
        atomic_store(c.cells, at, pos + c.cap);                  // free the slot for the next lap
        atomic_store(c.cells, 16, pos + 1);
        return Got(v);
    }
    if atomic_load(c.cells, 32) == 1 { return Closed; }          // closed AND empty
    return Empty;
}
```

`send`/`recv` are `loop { try_*; backoff }` first and `atomic_wait` later. SPSC is the same without the CAS.

What this shows about the primitive: the channel needs `load`, `store`, `cas` and nothing else; **no fence, no
weaker ordering, no `fetch_add`**. Under SC the publish order (`value` before `seq`) is enough.

**What a channel library cannot do** (limits of the *design*, not bugs): carry an owned value (the paragraph before the sketch), carry more
than a word, select over several channels, or be waited on together with a socket (§6.4).

### 6.4 The broker's real problem is not the queue

A broker's handler threads block in a `Poller` on sockets. A producer thread that enqueues a message must **wake a
thread blocked in `poller_wait`**, and a futex on a cell does nothing for that thread: it is not waiting on the cell.
The usual answer is a descriptor the poller also watches (an `eventfd`, or a pipe: `processes.md` has `Pipe`) that
the producer writes to. **Not designed, not measured**; it is a use of `Net` and `Pipe`, not of atomics, and it is
named so that A2 is not mistaken for the broker's blocker.

---

## 7. The verification gate, fixed before the build

Everything here is written before any code, so a result cannot be argued into passing. Each item says how it fails.

### 7.1 Litmus tests

Each runs on **Cranelift and LLVM, on macOS arm64, Linux x86-64 and Linux aarch64** (the third is the Docker arm64
image the project already uses on the Mac, native, not emulated). A trial resets the cells, passes a start barrier
built **from `Atomic` itself** (a counter both threads increment and spin on), runs the two code sequences, and
records the pair.

| test | code (every access an `Atomic` operation) | forbidden under SC | trials |
|---|---|---|---:|
| **SB** store buffering | A: `x=1; r1=y`  B: `y=1; r2=x` | `r1 = 0 and r2 = 0` | 100 M |
| **MP** message passing | A: `data=1; flag=1`  B: `r1=flag; r2=data` | `r1 = 1 and r2 = 0` | 100 M |
| **MP-plain** | as MP but `data` is **non-atomic** memory written by A before `flag=1` and read by B after seeing `flag` (the channel's lent-buffer case, which depends on "creates happens-before edges that order normal loads and stores") | `r1 = 1 and r2 = 0` | 100 M |
| **LB** load buffering | A: `r1=x; y=1`  B: `r2=y; x=1` | `r1 = 1 and r2 = 1` | 100 M |
| **CoRR** coherence | A: `x=1`  B: `r1=x; r2=x` | `r1 = 1 and r2 = 0` | 100 M |
| **IRIW** | A: `x=1`  B: `y=1`  C: `r1=x; r2=y`  D: `r3=y; r4=x` | `r1=1,r2=0 and r3=1,r4=0` | 10 M |
| **2+2W** | A: `x=1; y=2`  B: `y=1; x=2` | final `x=1 and y=1` | 100 M |

**The pass line: zero forbidden outcomes in every cell of the matrix.** What zero means, honestly: 100 million trials
with none observed bound the per-trial probability by 3e-8 at 95% (the rule of three), and say nothing about
schedules the machine did not produce. It is evidence, not a proof, and the document will not call it one.

**Honest about IRIW**: x86-TSO and ARMv8 are both multi-copy atomic, so IRIW's forbidden outcome **cannot occur on
any of the three platforms even with weak lowering**; the test has near-zero power there and is kept for POWER and
RISC-V and as a compile-time sanity check, not as evidence. The same is true of LB on x86. The tests with real power
on these machines are **SB** (x86 and arm), **MP** and **MP-plain** (arm; x86 only through compiler reordering),
**CoRR** and **2+2W** (compiler).

**Power check, which is the point of the gate (the tests must be able to fail).** The whole matrix is re-run with the
accesses written **plain**, and it has to see the forbidden outcome. Measured so far, **with plain accesses through
the hole of §2** (`benches/atomics/sb.cho`: 3,000,000 trials per run, no barrier, three runs each):

| | Cranelift | LLVM |
|---|---|---|
| arm64 macOS | 87,632 / 92,939 / 94,205 | 166,509 / 202,293 / 236,141 |
| x86-64 Linux | 128,808 / **0** / 307,986 | 242,583 / 97,792 / 59,147 |

Later reruns on arm64 gave 185,344 (Cranelift) and 254,134 and 293,499 (LLVM). So SB's forbidden outcome shows on both machines and both backends, in 2-10% of trials, and with **no barrier it
is noisy**: one x86 Cranelift run saw none (the threads happened not to overlap). **That is the argument for the
barrier**, and for the pass line "the plain version shows at least one forbidden outcome in its first 10 million
trials on every cell, or the test is declared too weak and fixed before the atomic version counts". This table
measures the *race*, not any atomic: there are no atomics yet. It is what the gate will be compared against.

### 7.2 Stress: a counter

`threads` x `10 M` `fetch_add(1)` on one cell; the final value must be exactly `threads * 10 M`. 2, 4, 8 and 16
threads (cores 0-5 on the Linux box), repeated 50 times, both backends, all three platforms. Then the same with
`cas` in a retry loop (A1), and with the two cells on **the same and on different cache lines** (the stress finds
nothing about false sharing; the timing does, and is reported, not gated). **Pass: exact, every run.**
Control, measured: the same counter with plain adds loses updates (`race.cho`, 400 M expected, `L` on every run).

### 7.3 Mutants: each must be killed

A mutant is the lowering deliberately wrong in one way. Each is a change to the backend run against the suite;
**the suite must fail**. A mutant that survives means a test is missing, and the stage is not done.

| # | mutant | killed by | caveat |
|---|---|---|---|
| 1 | `atomic_load` lowered as a plain `load` | the **emission test** (the LLVM text must contain `load atomic`; the Cranelift IR/object must contain `atomic_load`/`ldar`) and the **spin test** (`while atomic_load(flag, 0) == 0 {}` must terminate; LLVM at `-O2` hoists a plain load and loops forever) | **invisible to litmus tests and to the object code on x86**, where both are `movq` (§4.2). The emission test is the only thing that sees it there |
| 2 | `atomic_store` as a plain `store` | **SB** (x86: `mov` instead of `xchg`/`mfence`; arm64 `str` instead of `stlr`) | |
| 3 | `fetch_add` as `load; add; store` | the stress counter | the loss rate is in `race.cho`'s control |
| 4 | `cas` that stores even when the compare fails | the CAS stress and a single-thread unit test | |
| 5 | `exchange` as a store (drops the old value) | a single-thread unit test: the old value is returned | |
| 6 | `seq_cst` weakened to `monotonic` in the LLVM text | **SB** on x86 and arm64 (plain-shaped stores) | detectable only if SB sees it, §7.1's power check |
| 7 | the folder treats an atomic call as pure (`performs` ignores `conc` on an atomic builtin) | the spin test through a `fn get(a: &Atomic) -> [conc] int` wrapper, and a `fold`/`purity` unit test | §3.3 |
| 8 | two `atomic_load`s of one cell merged by redundant-load elimination (Cranelift's alias analysis) | the spin test on Cranelift | the one **assumed** fact of §4.2 |
| 9 | a cell not 8-byte aligned (an offset allocation) | a unit test on `atomic_new` plus an aarch64 run (SIGBUS, `threads.md` §6) | |

### 7.4 A model check of the channel

SC-only makes an exhaustive check feasible, which is a reason for §4.1: with SC the behaviours of a program are
exactly its interleavings of atomic operations, so a depth-first search over them is *complete*, where `loom` must
also model weak orderings. The channel's functions (§6.3) are loops of atomic operations over a few cells; a model
checker (a throwaway script under `scripts/`) enumerates every interleaving of 1 and 2 producers and 1 consumer,
capacity 1 and 2, three messages each, and checks: no message lost or duplicated, per-producer order preserved,
occupancy never above `cap`, `Empty`/`Full` never returned wrongly, no state where every thread is blocked and not
finished.

Honest limits. It checks a **model**, and a model drifts from the code. Two mitigations, both part of the gate:
the model is **extracted mechanically from `std/chan.cho`** (the functions are restricted to straight-line atomic
operations, loops and arithmetic, so this is a small parser, not an interpreter), and the real compiled code runs
under a **test-only scheduling mode** that yields at every atomic operation with a seeded choice, replaying
10,000 seeds, whose observable results must be a subset of the model's. Whether the mechanical extraction is as small
as claimed is **unknown until A3**; if it is not, the fallback is a hand transliteration plus the replay check, and
the document says the check is weaker. The scheduling mode is a compiler flag, so it is itself A3 work.

---

## 8. Edition, and the stages

### 8.1 What it costs

**Edition 8.** `Atomic` and the builtins are additive names, absent from an older file (`editions.md`'s table of additive and refining changes: "the new
thing is absent in older editions. Exact, with no tool and no second meaning"), like `fork_clock` (edition 5) and
`exec` (edition 7). Costs, all one-off: the parser's range (`1..=7` and its message in `parser/items.rs`), the
`PRELUDE` names, the builtin table, both backends' arms, `docs/editions.md`'s list, `cancho introspect`. A file that
declares `edition 8` hashes differently from its edition-7 text (`editions.md` §6.3), which touches only the files
that adopt it: `std/chan.cho` and the programs that use it. **Rejected: shipping A0 inside edition 7**: edition 7 is
closed (`editions.md` §6.4, the freeze: new names go into the next edition) and a name added to it could shadow a user's own `Atomic`.

### 8.2 The stages

| stage | what | gate (can fail) |
|---|---|---|
| **A0** | the type, `atomic_new`/`free`/`len`, `load`, `store`, `fetch_add`; both backends; edition 8; the reject fixtures; the litmus harness itself (it needs only these three) | all of: (1) `tests/accept`: the work counter and the ordered slots of §9.2 agree on both backends; (2) **SB, MP, MP-plain, LB, CoRR, 2+2W zero forbidden, in 100 M trials, every cell of the matrix** (IRIW 10 M); (3) the **power check**: plain versions see the forbidden outcome in 10 M trials on every cell; (4) stress: 2/4/8/16 threads x 10 M exact, 50 runs; (5) mutants 1, 2, 3, 6, 7, 8, 9 killed; (6) **rejects**: a plain read or write of an `Atomic`'s word (none exists: a fixture that tries `unbox`/`contents`/destructuring), `atomic_free` while borrowed (`linear`), a `Thread[&r Atomic, R]` escaping its `borrow` (`reference-escapes-region`), a `val struct` holding one, an out-of-range index (trap); (7) the 35 golden hashes and `cancho ids` over `examples/` and `tests/accept/` **unchanged**; (8) `cancho authority` reports `conc`, and a WASI build is refused; (9) the Cranelift x86-64 object disassembled (§4.2's **read**). **Performance**: uncontended `fetch_add` within **1.3x of C's `__atomic_fetch_add` seq_cst** in the same loop, on both platforms; if not, say so and say whether the indirection of §3.2 is why |
| **A1** | `atomic_exchange`, `atomic_cas` (answers the old value) | CAS-increment stress, 2-16 threads x 10 M, exact; mutants 4 and 5 killed; **a lock-free stack or the ticket of §9.2 built on it passes its own model check** (§7.4) |
| **A2** | `atomic_wait`/`atomic_wake` | **only if A3's retry-loop measurement asks for it.** A ping-pong of 1 M handoffs between two threads over wait/wake **beats a sleep-backoff loop's latency** by a measured margin, never loses a wakeup in 100 M handoffs (a hang is the failure, a timeout is the detector), and survives 10 M spurious wakeups injected by a test mode. The macOS symbol question (§6.2) is decided and recorded before any code. If it is not decided, A2 is not built and the channel stays on backoff |
| **A3** | `std.chan` (or the package), written first in `cancho-table` | the §7.4 model check of the channel passes on all configurations, and the 10,000-seed replay agrees; 8 producers x 1 consumer, 100 M messages, none lost, none duplicated, per-producer order kept; **the parallel CSV scan's output is byte-identical to the single-thread scan's for 1, 2, 4 and 8 workers, 200 runs each**, and 8 workers are **at least 2.5x faster than one on at least 6 cores** (a measurement, reported with the core count as `parallelism.md` §3.4 does; below that, it is written up as the result and the design is questioned) |

A0-A1 need no second program (the counter is `cancho-table`'s). **A3 moves to `std` only when a second program wants
it** (the broker would be the second). The order A0 -> A1 -> A3 -> (A2 if measured) is deliberate: A2 is the only
stage that adds a kernel dependency, so it waits for a number.

---

## 9. What this does not solve, and what to do instead

### 9.1 Not solved

* **The hole of §2.** Plain shared mutation across threads remains unchecked. Atomics are *a safe way to share one
  word*, not a safe way to share data.
* **Shared mutable data structures** (a hash map two threads insert into, `cancho-cache`'s store). A word is not a
  table. That needs either a lock (`threads.md` §4's "Mutex-shaped capability", not designed) or an algorithm built
  from CAS, which is a *library* (and a hard one) and is not scheduled.
* **Messages that own memory.** `join` refuses a `Box` result, so a channel carries an int and the buffer it names
  belongs to someone who stayed put.
* **A worker that needs its own mutable state and a shared one.** One pointer per thread (§1.2.2). Per-worker scratch
  can come from `region`s opened inside the thread (a region's arena is private to the thread that opens it,
  `parallelism.md` §8.1; **not tried with a spawned thread here**). A worker that must *return* structured state to
  its parent needs `thread-payloads.md`'s tuple payload or a wider `join`.
* **Disjoint writes into one buffer** (`thread-payloads.md` T-P3): atomics do not make two `&!` slices safe.
* **Poller wake-up, select, cancellation, a thread pool, async.** §6.4, `threads.md` §4.
* **Ordering weaker than SC**, and **atomics narrower than a word**, and **atomic operations on non-`int` data**.
* **Deadlock and livelock freedom.** A wait loop that never ends is a program the type system accepts.
* **Windows**, and 32-bit targets (no 64-bit atomics guarantee). Not supported targets today.

### 9.2 What a user should do now

**Shared-nothing workers merged in order**, which needs no atomic and works today: `threads.md` §5 step 2, T0
(`parallelism.md` §4). Give each worker a *static* slice of the input, one `&!r Job` struct each (one function value
per spawn, `parallelism.md` §3.4), join the workers in order and merge. The cost is load imbalance: a slow chunk
holds one worker while the others finish. Over-partition (more, smaller jobs than workers) and spawn per job: thread
start is a `pthread_create`, whose cost is not recorded in this repository (`threads.md` §4 says measuring it is part
of §5 and it was not done); measure it before choosing the chunk size. **Processes** (`parallelism.md` §3.5) share
nothing and have been measured to scale (25,180 a second for three processes against 15,638 for one, `cancho-web`).
For the single work counter on Linux only, the libatomic stopgap of §1.3 works today.

**The CSV scan with A0, as a design to try** (not built; `cancho-table` writes it first):

```
res struct Shared { data: Box[[byte]], next: Atomic /* word 0: next chunk */, ready: Atomic /* word k: chunk k done */,
                    result: Atomic /* word k: its aggregate */, nchunks: int }

worker(s: &r Shared):    // the only payload: one pointer
    loop { k = atomic_fetch_add(s.next, 0, 1); if k >= s.nchunks { return 0 }
           v = scan(contents(s.data), chunk_bounds(k));     // scratch in a region opened here
           atomic_store(s.result, k, v); atomic_store(s.ready, k, 1) }

main: spawn N workers; for k in 0..nchunks { spin/backoff until atomic_load(s.ready, k) == 1;
                                              merge(atomic_load(s.result, k)) }; join all
```

No `cas`, no wait, no channel: **A0 alone covers a scan whose per-chunk result is a number.** The merge is in chunk
order, so §5.3's discipline holds. A scan whose result is *rows* needs a per-chunk output buffer written by one
worker and read by main: that is a unique buffer lent to a thread, which `thread-payloads.md` T-P3 blocks. Until that
exists, the rows version has no safe design, and the aggregate version is what to build.

---

## 10. Open questions

| question | why it is open |
|---|---|
| Does adding names to `PRELUDE` move any existing hash? | Expected not (§3.4); only building tells. A0's gate (7) decides, and withdraws the change if so |
| Does Cranelift's alias analysis treat `atomic_load` as a barrier? | **Assumed** (§4.2). Mutant 8 is the test; if it fails the lowering must wrap each atomic load in something the pass respects |
| Cranelift on x86-64: the exact instructions | **Read, not disassembled** (§4.2). A0's first task |
| macOS wait/wake: `__ulock_*` or `os_sync_*` | **Assumed**, not checked (§6.2). A2's first task, and a policy question (a private symbol) |
| What does `lex-os` do with `conc` and with `[heap]` at creation? | Not read (§5.2) |
| Does a per-thread `region` work inside a spawned thread? | `parallelism.md` §8.1 says the arena is private; **no fixture** |
| `pthread_create`'s cost | Not recorded anywhere here; it decides the chunk size of §9.2's shared-nothing fallback |
| Is `Atomic`'s extra indirection (§3.2) ever worth removing? | Only a program that measures it can say; A is the answer if so |
| Should the aliasing hole of §2 be closed for `spawn` first? | **Done**, `aliasing.md` §6.1. Atomics were unaffected by it |
| Is x86 SC-store cost (6 ns, §4.1) ever on a program's critical path? | Unmeasured outside the C microbenchmark; the trigger for weaker orderings |
| Could the work counter be a `Clock`-style capability, so a program must be *granted* shared state? | Considered and not taken (§5.1: the reference is the authority); revisit if `lex-os` wants to deny it |

## Files

| | |
|---|---|
| `benches/atomics/race.cho` | the data race of §2 that the checker accepts |
| `benches/atomics/sb.cho` | store buffering with plain accesses: what the litmus gate must be able to see (§7.1) |
| `benches/atomics/libatomic_counter.cho` | the Linux-only counter stopgap of §1.3 |
