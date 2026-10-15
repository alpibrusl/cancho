# The region chunk limit, named (#514)

> **Status: built, both backends.** A region is one 65,536-byte chunk
> (`linearity-and-effects.md` §6, `region-chunk-cache.md`). An allocation that does not fit used to be a bare
> `SIGILL` with nothing on the screen, learned from a crash on one request in production (alpibrusl/cancho-code#58:
> a JSON tape of about 96 KB). Now a size the compiler can see is refused at `cancho check` with the rule
> `region-chunk-exceeded`, and one it cannot see traps at run time with a line on standard error that names the cause.

Related: [`defined-behaviour.md`](defined-behaviour.md) §1 (a trap is deterministic and has no message of its own),
[`region-chunk-cache.md`](region-chunk-cache.md), [`boxed-slices.md`](boxed-slices.md), [`agent-errors.md`](agent-errors.md)
§3 (rule tags), [`hkdf.md`](hkdf.md) §2 and [`tls-assurance.md`](tls-assurance.md) (earlier victims of the same limit).

---

## 1. The limit, measured

`alloc_slice[r](n, fill)` asks the region for `n * stride` bytes. The bump pointer starts at the chunk's base and is not
aligned or padded, so **one allocation may be the whole chunk, 65,536 bytes, and no more.** Measured with a one-region
program on both backends (cranelift and llvm, aarch64-apple-darwin, exit status of the built program):

| element | n | bytes | cranelift | llvm |
|---|---|---|---|---|
| `int` | 8,191 | 65,528 | exit 0 | exit 0 |
| `int` | **8,192** | **65,536** | exit 0 | exit 0 |
| `int` | 8,193 | 65,544 | 132 (SIGILL) | 132 |
| `byte` | 65,535 | 65,535 | exit 0 | exit 0 |
| `byte` | **65,536** | **65,536** | exit 0 | exit 0 |
| `byte` | 65,537 | 65,537 | 132 | 132 |

So the largest `int` slice is **8,192** elements and the largest `byte` slice is **65,536**. The issue's figures agree
(8,000 ints, 64,000 bytes, ran; 12,000, 96,000, trapped). The limit is the *sum* of what a region allocates: a second
allocation in the same region has what the first left. The number is `cancho_ir::REGION_CHUNK_BYTES`, read by the checker and
both backends, so they cannot disagree on it.

There is no third backend to agree with: the repository has Cranelift and LLVM (and LLVM for `wasm32-wasip1`); there is no
interpreter. The wasm build was not run for this change (no toolchain in the environment); it takes the LLVM path.

## 2. At check time: a constant size is refused

When the count of an `alloc_slice` is a literal, or an expression the lowering already folds to one (`8000 + 193`), and
`count * element size` is over 65,536, `cancho check` refuses it with rule **`region-chunk-exceeded`**, located at the
allocation. The message gives the size, the limit and the remedy:

```
`alloc_slice` of 8193 `int` is 65544 bytes, and a region holds 65536 bytes in all (one chunk); allocate data this big on
the heap with `box_slice` instead (AGENTS.md)
```

Element size is bytes, not elements: `byte` is 1, `f32` 4, `int`/`bool`/`float` 8 (`bool` takes a full leaf in a slice),
and a tuple the sum of its leaves at 8 each. **The check is a lower bound and never over-refuses**: for a struct or enum
element, whose size is counted from its declaration, it says nothing, and the run-time trap of §3 is what catches it. A
count held in a variable (`let n = 12000; alloc_slice[r](n, 0)`) is not folded by the compiler today, so it is §3's case
too; teaching the folder to follow `let` is a separate change, not made here.

A `static` item's compile-time arena has no chunk and is not checked.

## 3. At run time: the trap names itself

An allocation the chunk cannot hold, found at run time, writes one line to standard error (fd 2) and then traps exactly as
before, so the status is still `SIGILL`, exit 132 from a shell:

```
cancho: trap: region chunk exhausted: a region holds 65536 bytes in all; allocate big data on the heap with box_slice (AGENTS.md)
```

This is the one trap with a message. Every other trap (overflow, a bounds check, `trap()`) is still a bare `SIGILL`
(`defined-behaviour.md` §1, `testing.md` §2): there was no mechanism for a message to reuse, so this adds the smallest one,
a `write(2, ...)` on the failing path only. The text is `cancho_ir::REGION_EXHAUSTED_MESSAGE` and both backends emit it, so
a program says the same thing on either. The cost on the path that fits is nil: the branch to the trap was already there, and
the failing block is marked cold in Cranelift.

It covers the other way to run out as well, many small allocations that together pass 65,536 bytes: the message says a
region holds that many bytes *in all*.

## 4. Not done: a heap-backed chunk

The alternative to refusing is to let a region grow, falling back to a `malloc` for the allocation that does not fit and
freeing it with the region. That is a language decision, not a bug fix, and it is **not made here**: it would make `region`
no longer the O(1), no-traversal release `linearity-and-effects.md` §6 trades expressiveness for (the region would need a
list of its overflow blocks), would make the cost of an allocation depend on its size, and would hide the heap use from the
authority report, which today shows a program with no `Heap` cannot allocate on the heap at all. `box_slice` already is the
explicit form of it, with the heap capability in view.

## 5. Tests

- `tests/reject/region_slice_over_chunk.cho`, `region_byte_slice_over_chunk.cho`, `region_slice_over_chunk_folded.cho`:
  the rule for ints, for bytes, and for a folded count.
- `tests/accept/region_slice_at_chunk.cho`: 8,192 ints and 65,536 bytes are exactly the limit and run.
- `crates/cancho/tests/conformance/region_limit.rs`: a count the compiler cannot see, built and run on both backends: one
  element over traps with SIGILL and the message on stderr, exactly the chunk runs and prints nothing.
- Mutants (§6).

## 6. Mutants

Each was applied to the implementation, the gate run, and the mutation reverted.

| mutation | red in |
|---|---|
| refusal at `bytes >= chunk` (off by one, too strict) | `region_slice_at_chunk` accept fixture: the exact chunk is refused |
| refusal at `bytes > chunk + 1` (off by one, too lax) | `region_byte_slice_over_chunk` (65,537 bytes, one over) |
| the count must be a literal in the source, not a folded one | `region_slice_over_chunk_folded` |
| an `int` taken as 1 byte | `region_slice_over_chunk` (ints) |
| the Cranelift message written to fd 1 instead of 2 | `region_limit`, past the chunk (cranelift) |
| the LLVM message written with length 0 | `region_limit`, past the chunk (llvm) |
