# Moving bytes inside a slice: `copy_within`

Status: **built**, edition 5.

## 1. Why

`lexsys-cache`'s arena compaction slides every live record down over the garbage in place. With only indexing to do it, that is a loop
`data[to + i] = data[from + i]`, which the compiler cannot turn into a block move (the two ranges may overlap, and nothing says which way),
and it ran at about 0.5 GB/s: **114.7 ms for the worst compaction of a 64 MiB arena** (`lexsys-cache`, `bench/stall.py`). A byte loop is also
the wrong tool for a language that wants the safe operation to be the fast one: every program that moves a run of bytes (a buffer being
compacted, a parser shifting its unread tail to the front) writes it, and each can get the direction wrong.

## 2. The primitive

```
copy_within(buf: &!r [byte], dst: int, src: int, n: int) -> [] int      // edition 5; answers 0
```

Move `n` bytes from `buf[src..src + n]` to `buf[dst..dst + n]`, as `memmove` does: the ranges may overlap, and the result is as if the bytes had been
copied out first. It has the checks indexing has: it **traps** unless `0 <= dst`, `0 <= src`, `0 <= n`, `n <= len(buf) - dst` and `n <= len(buf) - src`.
The sums `dst + n` and `src + n` are never formed, so a count near 2^63 cannot wrap past the bound (`tests/conformance/traps.rs` tries it). A move of
nothing at the very end (`dst == len`, `n == 0`) is allowed. It has no effect row and needs no capability: it reads and writes only the slice it is given.

It is a builtin and not a library loop because only the backend can emit a block move: Cranelift calls `memmove` (declared as any libc import is);
LLVM declares and calls it, where clang turns it into the target's best move. It takes one slice, not two, because a unique reference and a shared one
to the same bytes cannot both be held, which is exactly what a move inside a buffer needs.

## 3. What it is checked by

* `tests/accept/copy_within.ls`, on both backends: forward and backward overlap (the case a front-to-back loop gets wrong), the whole slice onto itself, empty
  moves at both ends, a one-byte move, a move inside a sub-slice that touches nothing outside it.
* `tests/conformance/traps.rs`: nine ways of being out of range (negative `dst`, `src`, `n`; past the end by each; beyond the length; and a count that would overflow
  `dst + n` and `src + n`) each killed by a signal on **both backends**, and three calls at the edge that must succeed.
* `tests/conformance/backends.rs`: the two backends agree.
