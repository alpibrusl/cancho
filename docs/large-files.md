# Files larger than memory: `file_size`, `file_read_at`, and whether to map

> **Status: design. Nothing is built, and no measurement here is of cancho yet.**
>
> `file-handles.md` §5 stopped at a handle that is read to its end and closed: *"not seek,
> not append, not truncate, not directories, not metadata … every other verb can arrive when a
> program asks for it."* `cancho-gpu` ([cancho#251](https://github.com/alpibrusl/cancho/issues/251),
> slice 9) is that program: an inference runtime opens model weights of several gigabytes, and
> reads *part* of them, in an order the file does not dictate. `ds4`-class engines go further
> and stream tensors from SSD during a forward pass.

## 1. What is missing, from the code

Today (`filesystem.md` §3, `file-handles.md`):

| need | what exists | why it is not enough |
|---|---|---|
| read a whole file | `fs_read(fs, path, into)` | the whole file must fit one slice, in memory, at once |
| read a file of unknown size | `open_read` / `file_read` / `file_close` | strictly sequential; reading the last tensor means reading all before it |
| know a file's size | nothing | the caller must guess a buffer, or read to the end first |
| read a region | nothing | the verb is `seek` or `pread`, both absent |

A GGUF or safetensors file is a small header (tensor names, shapes, offsets) followed by
tensors at stated offsets. A runtime reads the header, then touches tensors in layer order,
possibly more than once, possibly never. With sequential reads the only way to reach layer 30
is to read layers 0 to 29 into memory.

## 2. The smallest verbs that answer it

```
file_size(f: &File) -> [file_read] int                                  // bytes, or a negative error code
file_read_at(f: &File, offset: int, into: &!r [byte]) -> [file_read] int // fills `into` from `offset`; bytes read
```

* **Positioned reads, not `seek`.** `file_read_at` is `pread`: it carries its offset and does
  not move a position the handle holds. That keeps `File` free of mutable state a second
  borrower could disturb, makes concurrent reads from several threads of one handle safe (the
  reason the verb is `pread` and not `lseek` + `read`), and removes the whole class of
  "forgot to seek back" errors. `seek` is *not* proposed.
* **Same label, same authority.** Both perform `file_read` (`file-handles.md` §4.1): the path
  was spent at `open_read`, and the report of a program that switched from whole-file reads to
  positioned ones does not widen. A reader of `cancho authority` sees no new row.
* **Same refusal discipline.** An offset that is negative, or past the end, is a returned
  error, never a trap and never a short read dressed as success: the result is the count
  actually read, `0` exactly at the end, a negative code on error. A count that does not fill
  `into` is not an error, as for `file_read`. The rule tag for the refusal (`file_read_at`
  with a negative offset on a constant) is the build-time case; the dynamic cases are
  `Read`-style results (`file-handles.md` §2), so nothing reaches a panic.
* **Sizes are `int`.** `int` is 64-bit, so a 100 GB offset is an ordinary number. What is not
  yet established is the largest slice the heap will give (§5, L0).

This is enough for the first working runtime: read the header, keep the offsets, read each
tensor into a buffer it owns when it is wanted, and drop it when it is not. Peak memory is
what the program chooses, not what the file weighs.

## 3. Should it also map? A real question, not a foregone yes

`mmap` is what `ds4`-class engines and `llama.cpp` use for weights: the kernel pages tensors
in on demand, evicts under pressure, and two processes share one copy. It is attractive for the
same reason it is hard here.

* **It breaks "no input reaches a panic".** A file truncated while mapped delivers `SIGBUS` on
  the next touch of a vanished page. That is *the file's* doing, not the program's, but it is a
  signal at an array index that the language otherwise promises is checked. A language whose
  claim is that safe code cannot crash on a bad index must either state this exception or not
  offer the verb. The honest options: (a) no mapping, `pread` only; (b) mapping, with the hazard
  written in `defined-behaviour.md` as the single named exception and a label that makes it
  visible; (c) mapping that `MAP_PRIVATE`s and copies on first touch, which removes the
  sharing that was the point.
* **It is a new kind of slice.** A mapped region is a `&r [byte]` the program did not allocate,
  with a lifetime tied to a `res` that must be unmapped. That fits the checker's existing model
  (`res Mapping` closed with `unmap`, as `File` with `file_close`, a forgotten one a compile
  error), but the slice must be **read-only** in its type: a `&!` view of a `PROT_READ` page is
  a segfault, not a trap.
* **It buys what `pread` does not, and no more.** Sharing between processes and zero-copy
  access. Single-process use with an explicit buffer pays one copy per tensor load, which the
  memory-bandwidth arithmetic (a few GB/s for a copy, against an SSD's ~1 to 7 GB/s) says is
  not where a streaming runtime's time goes. *That is a claim to test (§5, L3), not to assume.*

**Recommendation:** build `file_size` and `file_read_at` first, and decide on mapping only
with a measurement of what the copy costs in `cancho-gpu`'s runtime. If the copy is not the
cost, the exception in (b) is not worth buying. If it is, (b) is the design, as its own
document, with the label `fs_map(p)` so that the authority report distinguishes it.

## 4. Not here

* **Writes.** `file-writes.md` is a separate story and the runtime does not write weights.
* **Async or direct I/O (`io_uring`, `O_DIRECT`).** A streaming engine will want them; they are
  a performance design with their own capability story, and `pread` from a second thread
  (thread-payloads.md) is the baseline they would be measured against.
* **Metadata beyond size** (mtime, mode). Nothing asked.

## 5. Plan, each step with its own gate

| step | what | gate |
|---|---|---|
| L0 | **measured, §5.1**: the largest `box_slice` the heap gives, and what a request that cannot be met does | done: the answer is a silent `SIGILL`, which makes a fallible allocation a prerequisite (§6) |
| L1 | `file_size`, `file_read_at`, both backends; `open_read` unchanged | an 8 GiB sparse file: size is exact, a read at 6 GiB returns the bytes written there, a read past the end returns `0`, a negative offset returns the error, none traps (and a mutant that `lseek`s instead is caught by a two-handle interleaving test) |
| L2 | `cancho-gpu` reads a GGUF-shaped fixture tensor by tensor | memory high-water mark is the largest tensor plus the program's own, measured, against the file's size |
| L3 | the question of §3: time `cancho-gpu` loading with `file_read_at` against the same loop over a mapped file from C | the copy is, or is not, more than 10% of load time. Only "is" opens the mapping design |

### 5.1 L0, measured

One machine, so the *thresholds* are this machine's and only the *behaviour* is general: Linux 6.18 (Firecracker), 15 GiB of RAM, no swap,
`vm.overcommit_memory = 0` (heuristic), the Cranelift backend, `box_slice(heap, n, byte_of(0))` (so `calloc`, `zeroed-slices.md`), `calloc` observed with an
`LD_PRELOAD` that logs any request of 256 MiB or more.

| request (zero-filled bytes) | what `calloc` did | what the program did |
|---|---|---|
| 1 GiB, touched one byte per 4 KiB page | returned | ran; peak resident 1,025 MiB (what was touched), 4.1 s |
| 4 GiB, the same | returned | ran; peak resident 4,097 MiB, 12.5 s |
| 4 GiB, non-zero fill, every byte written | `malloc`, returned | ran; peak resident 4,097 MiB, 5.5 s |
| 16 GiB (more than the 15 GiB of RAM) | **returned `NULL`** | **killed by `SIGILL` in 0.01 s: no message, no rule tag, exit status 132** |
| 1 PiB (2^50) | returned `NULL` | the same `SIGILL` |

**What this says.**

* A request the kernel will not grant is a **trap**, as `zeroed-slices.md` §2 says ("a null answer traps as `malloc`'s does") and as `boxed-slices.md` says of
  every allocation. That is a *consistent* rule, and for a weight loader it is the wrong one: a runtime that wants to *try* 40 GiB and fall back to reading
  tensors on demand cannot, because the first refusal ends the process with nothing to tell the user what was asked for. **A fallible allocation (§6) is a
  prerequisite of any loader that sizes itself to the machine**, and it needs no new capability: `Heap` already authorises it.
* Large untouched zero-filled slices cost nothing resident, as `zeroed-slices.md` §1 measured, so a runtime may `box_slice` a whole tensor table lazily; the
  cost is paid at first touch (about 3 to 4 s per GiB here, which is this VM's page-fault cost and not the language's).
* **The method needed a correction.** The first probes read a value back and reported success for every size up to 2^63 - 1 bytes, because the compiler
  removed the allocation altogether (no `calloc` call was made, which the `LD_PRELOAD` showed) when its only observers were a store and a load. A probe has to
  keep the memory live (a loop over it, or a size the compiler cannot see). The same removal means a program whose slice is never really used does not
  exercise the refusal at all, so a test of §6's fallible allocation must make the memory observable too. One size, `2^63 - 1`, with a touching loop, made
  no `calloc` call and ran for ten minutes instead of trapping; it was **not** run down here and is recorded as unexplained, not as a bug.

## 6. Open

| Question | Why it waits |
|---|---|
| **A fallible allocation** (`try_box_slice(heap, n, fill)` answering a value the program can test, or a `Result`-shaped `Opened` of its own) | **new, from L0 (§5.1).** Today a refused allocation is a `SIGILL` with no message. It is the prerequisite for a loader that sizes itself to the machine and falls back to `file_read_at`; it wants its own short design (what it returns without `Result`, which is `std`: `file-handles.md` §4 hit that wall) |
| Whether `fs_read` should be redefined over `file_read_at` | it is the same verb at offset 0 with a size; do it only if the duplicated code is the cost |
| Direct I/O and alignment | `[byte]` has no alignment guarantee a `O_DIRECT` read needs; `layout.md` and `zeroed-slices.md` are where it would start |
| A hint verb (`file_advise`) | a kernel read-ahead hint is cheap and pure; nothing has measured the need |
