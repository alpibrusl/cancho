# Zero-filled heap slices: `calloc`

Status: **built**, both backends. No new syntax, no new edition: the same `box_slice(h, n, fill)`, emitted differently when `fill`
is a constant zero.

## 1. Why

`box_slice` is one `malloc` and then a loop that writes `fill` into every element ([`boxed-slices.md`](boxed-slices.md) §3),
because the language has no uninitialised memory. When the fill is zero, the loop is work the allocator would have done for free:
`calloc` hands back zeroed memory, and for a large block -- which glibc takes straight from `mmap` -- the kernel supplies zero
pages that are not even made resident until something touches them.

Measured on `lexsys-tools`' `jsonq` before this change: `std.json` parses into a tape the caller provides, sized by
`json.tape_len` at 24 bytes per byte of source, and the zero fill of that tape was **43% of the instructions** (callgrind,
`memset`), while most of the tape is never written (a node is at least one byte of source, so the bound is loose).

## 2. The rule

A `box_slice` whose fill is, in the program's text, `0`, `false`, `0.0` or `byte_of(0)` -- every bit zero -- is
`calloc(bytes, 1)` and no fill loop (`lex_sys_ir::is_zero_fill`, one function both backends call). Everything else keeps
`malloc` and the loop, including:

* a zero **computed at run time** (`box_slice(h, n, f(x))`): the decision is made from the text, so both backends make it the
  same way and it never depends on a value;
* **`-0.0`**: its sign bit is set, so it is not all-zero bits;
* a **struct** fill, even of zeros: not needed by any program yet, and one more case to get wrong.

The size is computed and checked exactly as before (`slice_bytes`: a negative count or an overflowing product traps before
any allocation), and a null answer traps as `malloc`'s does. On LLVM, `calloc` is declared only when the program does not
declare it itself, like every libc name a program may use (`llvm-backend.md`).

## 3. What it is worth

`lexsys-tools`' `jsonq -p /0` on a 16 MiB JSON array, minimum of five runs, same machine:

| | before | after | `jq -c .[0]` |
|---|---|---|---|
| time | 0.46 s | **0.15 s** | 0.94 s |
| peak resident memory | 427,540 KB | **88,888 KB** | -- |

The memory column is the point: the tape is 24 times the document, and now only the part the parser writes is ever resident.

## 4. What it is checked by

`tests/conformance/zeroed_slices.rs`, on **both backends**:

* every zero kind reads back zero -- **in memory just freed full of nines**, so a zero fill that skipped the loop without asking for
  zeroed memory would be seen; a non-zero `int` and `byte` fill, a zero computed at run time and `-0.0` read back as themselves;
* on Linux, a 512 MiB zero-filled slice with one element written leaves the process's peak resident memory (`VmHWM`, read from
  `/proc` while it waits) under 64 MiB; filled, it would be 512 MiB.

**Mutants: eight, seven killed.** Killed: no fill counts as zero; every `int` counts; every float counts (the `-0.0` case);
`byte_of` of anything counts (the `byte_of(5)` case); Cranelift calls `malloc` for a zero fill and skips the loop (the freed-nines
case); LLVM calls `malloc` for a zero fill; Cranelift fills anyway after `calloc` (the residency case). **Survived, and
equivalent:** LLVM filling anyway after `calloc` -- clang at `-O2` deletes zero stores into memory `calloc` has just zeroed, so
the binary is the same; the residency test, which still passes with the mutant, is the evidence.
