# Finding a byte: `index_of_byte`

Status: **built**, edition 5, both backends.

## 1. Why

Every scan for one byte -- the end of a line, the next delimiter, the first byte of a needle -- was a loop of `text[i] == b`, one
bounds-checked load and one branch per byte. On Cranelift that loop runs at about 0.3 GB/s; clang vectorises it on LLVM, but only
inside the function that holds it. libc's `memchr` reads a word or a vector at a time on every target. `cancho-tools` (#214) wrote its
own Horspool search rather than use `std.bytes`' `find`, and its line splitter is that loop; `find` and `count_byte` were both that loop.

## 2. The primitive

```
index_of_byte(text: &t [byte], b: byte) -> [] int      // edition 5
```

Where `b` first occurs in `text`, or -1 if it does not. A search from an offset is a sub-slice, `index_of_byte(text[at..len(text)], b)`,
and answers relative to that sub-slice's start. There is nothing to check: `memchr` reads at most `len(text)` bytes from the slice's own
start. No effect row, no capability.

Both backends call `memchr` and turn its null into -1. Cranelift passes the byte widened to 64 bits, not as C's 32-bit `int`: the callee
reads the low 32 bits either way, and 64 bits is the width a program that declares `memchr` itself gives it (`int` is 64 bits at
cancho's crossing, `reach.md` §3). With 32 bits the two declarations disagreed and Cranelift refused the module as a compiler bug
(found writing `tests/accept/index_of_byte_beside_own_memchr.cho`). LLVM declares `memchr` only when the program does not.

`std.bytes` is now edition 5 (editions are per file and additive, so every importer is unaffected):

* `find` looks for the needle's first byte with `index_of_byte` and compares only there;
* `count_byte` makes one `index_of_byte` per occurrence, and answers 0 for a `b` outside 0..255 rather than building a byte from it.

## 3. What it is worth

64 MiB of 61-byte text lines built in memory, then five passes of the operation; minimum of five runs, same machine. Building the text
is in every number (about 0.24 s on Cranelift).

| operation | Cranelift before | after | LLVM before | after |
|---|---|---|---|---|
| `bytes.find`, needle's first byte once per line | 2.82 s | **0.61 s** | 0.72 s | **0.15 s** |
| `bytes.count_byte` of newlines | 1.04 s | **0.31 s** | 0.31 s | **0.13 s** |
| `bytes.find`, needle's first byte a space (every 5 bytes) | 2.98 s | **1.24 s** | **0.46 s** | 0.66 s |

**The last row is a regression on LLVM**, and it is kept. When the first byte is common, `find` makes a `memchr` call every few bytes,
and that costs more than the plain loop clang vectorises. Three other shapes were measured and none removed it: comparing the rest of
the needle inline instead of through `equal` (0.66 s), looking 32 bytes ahead inline before calling `memchr` (0.60 s, at the cost of
the rare case going from 0.15 s to 0.46 s), and going inline only after a short skip (0.66 s). Text in which the needle's first byte is
rare is the common case for a search tool, and that case is four to five times faster on both backends; a needle that begins with a
space is better searched for without it.

## 4. What it is checked by

* `tests/accept/index_of_byte.cho`, on **both backends** (`tests/conformance/backends.rs`). Each answer is checked against a byte loop:
  * the first, last and a middle byte, an absent byte, an empty slice;
  * a sub-slice that answers from its own start and does not see past its end;
  * 0 and 255 at every position of a 300-byte slice;
  * `bytes.find` with false starts, the needle at the very end, a needle longer than the text, an empty needle and an empty text;
  * `bytes.count_byte`, including -1 and 353, which are not bytes.
* `tests/accept/index_of_byte_beside_own_memchr.cho`, on both backends: a program that declares `memchr` itself.
* `tests/reject/index_of_byte_is_edition_five.cho`: an edition-4 file does not see the name.

**Mutants: twelve, eleven killed.** Killed, on each backend: no null check (the absent byte answers a garbage offset), the answer
and -1 swapped; on Cranelift, searching one byte short; the builtin at edition 4 (the reject fixture); and in `std.bytes`, `find`
without its empty-needle guard, `find` searching for a start past the last one that fits, `find` never comparing, `count_byte` without
its range guard, and `count_byte` skipping a byte after each match. **Survived, and equivalent:** LLVM sign-extending the byte instead
of zero-extending it -- `memchr` converts its argument to `unsigned char` (C11 7.24.5.1), so the two calls are the same call.
