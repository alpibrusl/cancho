# Threads that carry more than one value

> **Status: design. `threads.md` §2's single-leaf slice is built; this is what it left, and
> the second program that needs it.**
>
> `threads.md` restricts `spawn`'s payload and `join`'s result to **one pointer-width leaf**
> (`int`, `bool`, `c_ptr`, a captureless function value, one non-slice reference, or one owned
> zero-or-one-leaf capability), and says why a general struct is refused: *"not a missing feature
> but a missing prerequisite — this language has no compiler-synthesised trampoline function
> yet."* `cancho-gpu` ([cancho#251](https://github.com/alpibrusl/cancho/issues/251)) and
> `cancho-cache` (`docs/design.md` §2: *"a cache that shares one store across threads needs the
> communication primitive that cancho does not have"*) are the programs that now ask.

## 1. What each asker needs

| program | what a thread must be given | what it must hand back |
|---|---|---|
| `cancho-gpu` runtime | a region of the weights (`[byte]` view, offset, length), a range of rows of an activation matrix, a handle to a capability (`File`) | a count or a status; the result is *written into* a buffer it was lent |
| `cancho-cache` | the shared store, and a queue of connections | nothing until shutdown |
| the examples' pools (`threads.md` §4) | a job description: a few ints and a slice | an int |

The shape is the same everywhere: **a handful of leaves, one of them a slice**, and a result
that is mostly an acknowledgement because the real output went into lent memory. That is a
narrower need than "arbitrary structs".

## 2. Why a slice is the hard leaf

`threads.md` §3's argument for soundness is that `spawn`'s borrowed `payload` cannot outlive
`join`, using the region the borrow opened. That is unchanged for a slice, and a `&r [byte]`
payload is the same proof as a `&r int`. What is missing is only the **ABI**: a slice is
pointer *and* length, and `pthread_create` hands the start routine one pointer. So the compiler
has to build a small record on the spawner's stack, pass its address, and have the entry
point unpack it: the trampoline `threads.md` names. Nothing about the language is unsound; it
is a codegen slice.

## 3. The proposal: a tuple payload, packed by the compiler

```
let t = spawn(body, (xs[lo..hi], row0, row1));   // payload: any tuple of the allowed leaves
...
let n = join(t);                                  // result: still one leaf
```

* **`payload` may be a tuple** of leaves from §2's list, plus **slices** (`&r [T]` and
  `&!r [T]`), whose region is tracked exactly as the single non-slice reference is today.
  Nested tuples and structs follow only when a program needs them; the allowlist grows by one
  shape at a time.
* **A unique slice is the point, and it is where this document found a real problem.**
  Disjoint row ranges of one output buffer, lent to different threads, is the data-parallel
  pattern. `threads.md` §3 makes a *moved* unique reference sound: the spawner holds no
  reference to the value until `join`. That covers one thread per buffer and nothing more. To
  give two threads two halves, the spawner must derive two sub-slices from one borrow, and
  **`slicing.md` §4 says that is exactly what the language does not police**: `&!` is a lock on
  the binding, not Rust's no-aliasing invariant, so `xs[0..6]` and `xs[3..9]` are two live
  `&!` views of overlapping memory by design, and `split_at` "is not a safety feature here".
  Single-threaded that is sound (one buffer, one write-back, writes alias instead of racing).
  **Across threads it is a data race, and nothing in the current checker refuses it.** A tuple
  payload that accepted `&!r [T]` leaves would therefore be *unsound*, and this document does
  not propose it as it stood in its first draft.

  What would make it sound is a new rule, not a wider allowlist: a **disjointness-tracked split**
  (`split_at(xs, k)` returning two views the checker records as non-overlapping for the
  borrow's lifetime, and only such views may cross to a thread). That is the one place where
  Rust's `split_at_mut` is a safety feature here, and it reopens `slicing.md` §7's deferred
  question on a stronger footing than convenience. It is its own design (do two views of one
  split cross independently? is a split of a split tracked?) and T-P3 below is blocked on it.
  Until then, **shared** slices cross freely (readers cannot race) and a unique slice crosses
  only as the whole borrow, to one thread.
* **The result stays one leaf.** Results travel through lent memory; widening `join` is not
  needed by any asker, and removing the need is the cheaper design.
* **A rule tag per refusal**: a payload leaf not in the list keeps `ThreadPayloadType`; a unique
  slice whose region overlaps another live borrow keeps the aliasing rule's tag; a tuple whose
  elements escape the region is the escape check of `threads.md` §3.

## 4. What this still does not give `cancho-cache`

Sharing a *store* across threads needs two things that are not a wider payload: a way for
two threads to touch one mutable region at all (`threads.md` §4, T4/T5), and an ordered
communication primitive (a bounded queue needs an atomic load and store with ordering). The
payload slice lets a main thread lend disjoint pieces of a buffer to workers and take them
back at `join`; it does not let two running threads exchange messages. That is the
**communication primitive** of `parallelism.md` T5, and it is **not** proposed here. The cache's
one-core-per-process design (`cancho-cache` §5) stays correct in the meantime.

`cancho-gpu` does not need it: a matrix product is fork-join over disjoint output rows, which
the payload slice and `join` express completely.

## 5. Plan, each step with its own gate

| step | what | gate |
|---|---|---|
| T-P1 | the trampoline and tuple payloads of leaves, no slices; both backends | `tests/accept`: a tuple of three ints and an `Io` crosses and returns; a tuple with an unlisted leaf is refused with `ThreadPayloadType`; the existing thread fixtures (`tests/accept/spawn_*.cho`) unchanged |
| T-P2 | shared slices in the tuple | a four-thread sum over disjoint views of one `[int]` matches the sequential sum; a payload view that escapes `join` is refused (`spawn_handle_escapes_borrow`'s sibling) |
| T-P3 | unique slices through the disjoint split of §3, **its own design first** | four threads write four disjoint quarters of one buffer, result byte-identical to one thread's; **two threads given overlapping `&!` views are refused at compile time (today they would not be: this is the test that shows the rule is new)**; a mutant that drops the disjointness tracking is killed |
| T-P4 | `cancho-gpu` matrix product over rows | a measured speedup on the interpreter's `gemm` at 128³ against the one-thread time **on a machine with that many cores**, reported with the core count; the claim is parallel efficiency, and a number below ~70% at four threads is written up as the result |

## 6. Open

| Question | Why it waits |
|---|---|
| The disjoint split | needed by T-P3; `slicing.md` §7 leaves `split_at` open as a convenience, and this is the argument for promoting it. It does not exist as a checked guarantee today |
| Thread count and pools | `threads.md` §4 says a pool is `spawn`/`join` in a loop; with cheap payloads this becomes measurable, not before |
| `join` returning a tuple | only if a program shows a result that cannot be lent memory |
