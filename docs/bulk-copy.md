# Copying between slices: `copy_into`

Status: **built**, edition 5, both backends.

## 1. Why

Moving bytes from one slice into another had one spelling: a loop of `dst[i] = src[i]`. Every program that assembles output writes
it, and `std.buffer` wrote it twice (`reserve`'s copy into the bigger box, and `append`). On Cranelift the loop is a load, a store,
two bounds checks and a branch per byte; clang vectorises it on LLVM, but not to a block move. `lexsys-tools` (#214) wrote its own
copy for the same reason, and its output path pays for it on every line it prints.

[`memory-moves.md`](memory-moves.md)'s `copy_within` already lowers to `memmove`, but it moves bytes **inside one slice**: it cannot
take a buffer's contents and a line read from a file, which live in different allocations.

## 2. The primitive

```
copy_into(dst: &!d [byte], src: &s [byte]) -> [] int      // edition 5; answers len(src)
```

Copy all of `src` onto the front of `dst`. It has the check indexing has: it **traps** unless `len(src) <= len(dst)`. A copy to an
offset is a sub-slice of the destination, `copy_into(out[at..len(out)], line)`, and a copy of part of the source is a sub-slice of
it; the sub-slices carry their own checks, so the builtin needs only the one. It answers `len(src)`, which is what a caller adds to
its cursor. It has no effect row and needs no capability.

**`memmove`, not `memcpy`.** The two slices may be views of one buffer: the checker accepts `copy_into(xs[2..8], xs[0..5])`, a
unique and a shared view of the same allocation in one call (`tests/accept/copy_into.ls` does it, in both directions). `memcpy` is
undefined on overlap; `memmove` is defined, and on glibc for non-overlapping ranges it costs the same.

Cranelift checks and calls `memmove` as `copy_within` does; LLVM checks and calls the `memmove` it already declares (only when the
program does not declare `memmove` itself, `llvm-backend.md`).

`std.buffer` is now edition 5 (editions are per file and additive, so every importer is unaffected) and both of its copies are a
`copy_into`.

## 3. What it is worth

640 MiB through `buffer.append`, 4 KiB at a time, into one 1 MiB buffer emptied every 256 appends (so the copy is all there is to
measure), minimum of five runs, same machine:

| backend | byte loop (before) | `copy_into` (after) |
|---|---|---|
| Cranelift | 2.00 s | **0.026 s** |
| LLVM | 0.47 s | **0.026 s** |

That is 0.33 GB/s and 1.4 GB/s before, against about 25 GB/s after, on both backends: the copy is libc's, so the backend no longer
matters.

## 4. What it is checked by

* `tests/accept/copy_into.ls`, on **both backends** (`tests/conformance/backends.rs`): between two slices, with the destination's
  tail untouched; exactly full; nothing into something and nothing into nothing; overlapping views of one buffer in both directions
  (the forward case is the one a front-to-back loop smears); and `std.buffer` growing through forty appends and several doublings,
  every byte read back.
* `tests/conformance/traps.rs`: three sources that do not fit (one byte too many, anything into an empty view, a longer slice)
  killed by a signal on **both backends**, and three calls at the edge that must succeed (exactly full, nothing into nothing, one
  byte into the last).
* `tests/reject/copy_into_is_edition_five.ls`: an edition-4 file does not see the name.

**Mutants: twelve, eleven killed.** Killed, on each backend: no length check, `>=` for `>` (the exactly-full case), answering 0,
copying the destination's length instead of the source's; and the builtin at edition 4 (the reject fixture), `append` copying to
the front of the buffer, `reserve` copying nothing. **Survived, and equivalent here but not by contract:** Cranelift calling
`memcpy` instead of `memmove`. On x86-64 glibc 2.39 `memcpy` resolves to the same overlap-safe routine as `memmove`; a C probe
copying every power of two from 8 bytes to 16 MiB onto itself three bytes higher found the two identical. No test can tell them
apart on this machine, which is why §2 rests the choice on the contract rather than on a test.
