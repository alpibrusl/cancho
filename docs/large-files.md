# Files larger than memory: `file_size`, `file_read_at`, and whether to map

> **Status: design. Nothing is built, and no measurement here is of lex-sys yet.**
>
> `file-handles.md` §5 stopped at a handle that is read to its end and closed: *"not seek,
> not append, not truncate, not directories, not metadata … every other verb can arrive when a
> program asks for it."* `lexsys-gpu` ([lex-sys#251](https://github.com/alpibrusl/lex-sys/issues/251),
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
  positioned ones does not widen. A reader of `lex-sys authority` sees no new row.
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
with a measurement of what the copy costs in `lexsys-gpu`'s runtime. If the copy is not the
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
| L0 | **measure first**: the largest `box_slice` the heap allows, and what a 4 GiB and a 40 GiB request do (refusal with a rule, not an out-of-memory kill) | recorded in this document; if the answer is a trap, that is a finding to fix before any file verb |
| L1 | `file_size`, `file_read_at`, both backends; `open_read` unchanged | an 8 GiB sparse file: size is exact, a read at 6 GiB returns the bytes written there, a read past the end returns `0`, a negative offset returns the error, none traps (and a mutant that `lseek`s instead is caught by a two-handle interleaving test) |
| L2 | `lexsys-gpu` reads a GGUF-shaped fixture tensor by tensor | memory high-water mark is the largest tensor plus the program's own, measured, against the file's size |
| L3 | the question of §3: time `lexsys-gpu` loading with `file_read_at` against the same loop over a mapped file from C | the copy is, or is not, more than 10% of load time. Only "is" opens the mapping design |

## 6. Open

| Question | Why it waits |
|---|---|
| Whether `fs_read` should be redefined over `file_read_at` | it is the same verb at offset 0 with a size; do it only if the duplicated code is the cost |
| Direct I/O and alignment | `[byte]` has no alignment guarantee a `O_DIRECT` read needs; `layout.md` and `zeroed-slices.md` are where it would start |
| A hint verb (`file_advise`) | a kernel read-ahead hint is cheap and pure; nothing has measured the need |
