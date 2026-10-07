# Standard input

> **Status: settled.** The one I/O direction the language did not have.
>
> **§7 adds the bulk read** (`read_bytes`, edition 7): `getchar` is a libc call per byte,
> 72 MB/s on macOS, and §3.2's "no buffer-filling read" is corrected there.

---

## 1. What is missing

`examples/lines.cho` is the M3 acceptance criterion: a real command-line
tool that reads and writes files, counts and filters. It cannot be piped
into.

Nothing here can. A program can write to the console (`putchar`), read
and write files (`fs_read`, `fs_write`), and read its command line
(`arg`). It cannot read the one input every tool in a pipeline gets. So
`wordcount.cho` counts an *embedded* document, which is a demonstration
of counting rather than a program anyone would run.

---

## 2. It is not a seventh capability

The obvious move is a `Stdin` capability, an eighth field on `Split`,
and a break in every program that calls it. That is the wrong move, and
the language already says why.

`Fs` is **one** capability with **two** effect labels:

```
fn read [&f](fs: &f Fs(p), ...)  -> [fs_read(p)]  int
fn write[&f](fs: &f Fs(p), ...)  -> [fs_write(p)] int
```

The capability is what you *hold*. The labels are what you *did with
it*. Reading a file and writing one are the same authority used in two
directions, and the row says which — so a caller knows from the
signature, which is what `arguments.md` §2 means by the whole thing
being about **visibility** rather than containment.

The console is the same shape. `Io` is the capability; reading from it
and writing to it are two directions of one authority, and each gets a
label.

> `standard-error.md` applies this a third time and finds the rule holds
> without amendment: standard error is a third *stream* rather than a
> third direction, `err_write` is its label, and it is still not a new
> capability. §2.1 there says what that widens — a grant of `Io` reaches
> descriptor 2 now, where before it could not — and why the row is what
> keeps that honest.

### 2.1 Which means the existing label is renamed

`putchar`'s effect is spelled `io` today. Add reading and that becomes a
vocabulary with a hole in it: `[io]` would mean *write*, `[io_read]`
would mean read, and every reader would have to be told which way the
bare one points.

So the labels are **`io_read`** and **`io_write`**, and `putchar`'s row
changes from `[io]` to `[io_write]`.

This is not a drive-by rename. There is no way to add a read label
without deciding what the write label is called, and `[io]` alongside
`[io_read]` is a decision too — the worse one. Doing it now costs 155
occurrences across 53 files; doing it later costs more, because there
will be more programs by then, and the wart is in every signature that
prints in the meantime.

### 2.2 What `Io` discharges

Owning an `Io` outright discharges both labels, the way owning an `Fs`
discharges both of its. "Owning discharges, borrowing declares" is
unchanged: `main`'s row stays `[]`, and a function holding `&!i Io`
still writes down exactly what it did.

---

## 3. The operation

```
getchar[&i](io: &!i Io) -> [io_read] int
```

One byte in, mirroring `putchar`'s one byte out, behind the capability
that authorises it. `-1` at end of input.

### 3.1 Why `int` and `-1`

A byte is 0..255, so `-1` cannot be one and the sentinel is
unambiguous — which is exactly why C's `getchar` returns `int` and not
`char`.

It also matches `fs_read`, which returns `-1` when a file could not be
read: `filesystem.md` §2 calls that "an ordinary outcome, not a broken
promise", and the end of input is the same kind of thing.

An honest caveat: an enum — `End` and `Byte(byte)` — would be *better*.
A sentinel is a check you can forget; an exhaustive `match` is one you
cannot, and "a value the program decides what to do about" is
`sharing.md` §3's whole argument for `Gen`. The reason it is `int` here
is consistency with `fs_read` rather than conviction, and §6 keeps the
question open for both of them at once.

### 3.2 Why only one operation

There is no `read_line`, no `read_all`, no buffer-filling read.

> **Corrected (§7).** The buffer-filling read was asked for by a program that reads
> millions of lines from a pipe, and measured: `getchar` is 72 MB/s on macOS and 264 to 315 MB/s
> on Linux, against 2 to 5.5 GB/s for a bulk read. `read_bytes` is that read. The rest of this
> section stands: there is still no `read_line` and no `read_all`; those are policies.

`boxed-slices.md` §4 made this call already, for growing: *"there is no
`grow`, `push` or `realloc`; growing is allocate-copy-end, every part of
which was already expressible"*, so `examples/buffer/` writes it down
and the policy belongs to the program. A line is a policy too — where
it ends, what to do with a carriage return, what happens when it is
longer than a buffer — and none of those belong in a compiler.

`examples/tally.cho` is the demonstration: a real `wc` over standard
input, built from `getchar` and nothing else.

Writing it found two things worth having found, which is the argument
for building the example rather than asserting the feature:

* **The word-boundary policy was wrong twice.** The first versions
  omitted vertical tab and form feed. Nothing in the fixture contained
  either; running it against GNU `wc` over a real file did. A policy in
  the program is a policy you can get wrong — and also one you can fix
  without touching a compiler, which is the trade §3.2 is making.
* **It disagrees with `wc` in the C locale, on purpose.** Over this
  repository's README — 758 lines, 42158 bytes — lines and bytes agree
  exactly, and words agree under a UTF-8 locale. Under the C locale `wc`
  reports 98 fewer, because it decodes each multi-byte sequence and
  skips the ones the locale calls invalid: an em dash between two spaces
  is not a word to it, and is one to `tally`.

  That is `strings.md` §1 showing up in a program. A string here is
  **bytes, not an encoding**, so a run of non-blank bytes is a word
  whatever those bytes mean. Matching the C locale's answer would need a
  decoder, and nothing in this language has one.

---

## 4. What this is not

* **No promise about buffering.** `getchar` is libc's, so libc's
  buffering applies, and a byte costs a call — the same cost `putchar`
  has always had. A program that wants fewer calls reads into a buffer
  it owns, which is §3.2 and, since §7, one call per buffer.
* **No seek, no rewind, no `isatty`.** Standard input is a stream of
  bytes that ends. Anything that asks where it is in a file is a
  question about a file, and files have `Fs`. (§7.5 asks whether that
  still holds, and lists what a tool that wants to seek would need.)
* **No line discipline.** Terminals do their own; this reads what
  arrives.
* **No second stream.** There is one standard input. Standard *error*
  is a separate missing thing and is not this document.

---

## 5. The harness learns `//~ STDIN`

A fixture that reads input needs input to be tested with, and every
harness here feeds a program nothing. So the directive vocabulary gains
one:

```
//~ STDIN  the text fed to the program
//~ STDOUT what it must print
//~ EXIT   the status it must exit with
```

It generalises: the accept walker, the example walker and any later
fixture get it at once, because they all read directives from the same
header.

---

## 6. Open

| Question | Why it waits |
|---|---|
| An enum instead of `-1`, for `getchar` **and** `fs_read` | §3.1. It is the better design and it is a change to two operations, which makes it its own slice rather than a rider on this one |
| ~~A buffer-filling read~~ | **Built** (§7). A library could not be one: the only fast way to standard input the language had was `/dev/stdin` through `Fs`, which hides the read from the authority report |
| Standard error | A second stream, and a question about what `Io` is. Probably a third label |

---

## 7. The bulk read: `read_bytes`

```
read_bytes[&i, &b](io: &!i Io, into: &!b [byte]) -> [io_read] Read
```

Edition 7. `Read` is `file_read`'s own: `Got(n)`, `End`, `Failed(errno)`
([`file-handles.md`](file-handles.md) §3). It fills `into` from the front and
says how many bytes it put there.

### 7.1 Why it is a builtin and not `/dev/stdin`

[`cancho-table`](https://github.com/alpibrusl/cancho-table)'s reader design
(PR 25, `docs/readers.md` §3.4) measured the cost of standard input byte by
byte, on the 31.7 MB file of its benchmark and on this repository's
`benches/stdin_read.cho` (71.7 MB of JSON lines here):

| | macOS (arm64, load 15 to 22) | Linux x86-64 (`gram`, cores 0 to 5) |
|---|---|---|
| `getchar` into the chunk's room | **72 MB/s** (0.99 s) | **315 MB/s** (0.23 s) |
| **`read_bytes`**, 64 KiB at a time | **5,480 MB/s** (0.013 s), 76x | **2,070 MB/s** (0.035 s), 6.6x |
| `/dev/stdin` through `file_read` (not a way to read standard input) | 5,530 MB/s | 2,030 MB/s |
| the same three from a pipe (`cat FILE` into the program) | 74 / 3,990 / 5,040 MB/s | 267 / 2,040 / 2,090 MB/s |

Best of 3, wall time; the program counts the bytes and newlines in 64 KiB chunks and
keeps nothing (`benches/stdin_read.py` checks each mode's count against Python's
before timing it). `getchar` is a libc call per byte (14 ns on this Mac) and nothing in
the program; the bulk read is within noise of the file read on both machines, so the
read is no longer what the time is spent on.

The fast route the language already had, opening the path `/dev/stdin`, reaches
standard input through the *file system*: the authority report of the program that
does it says `fs_read("")` and not `io_read`. That is [`bulk-io.md`](bulk-io.md) §2
again, from the other side: the cheap thing to grant was the expensive thing to
audit, and a capability language that charges less authority-visibility for speed is
teaching the wrong lesson. `read_bytes` is the same read behind the same capability
and the same label, so a faster program is not a quieter one
(`conformance/stdin_bulk.rs`: the row has `io_read` and no `fs_*`).

### 7.2 What it answers

| Answer | When |
|---|---|
| `Got(n)`, `1 <= n <= len(into)` | bytes arrived. `n < len(into)` means the input ended (or failed) while the buffer was being filled: `read_bytes` keeps reading until the buffer is full or something stops it |
| `Got(0)` | `into` was empty. Nothing was asked for, so nothing ended |
| `End` | the end of input, with a buffer to fill |
| `Failed(errno)` | the read failed with no byte read: `9` (`EBADF`) for a closed descriptor, `21` (`EISDIR`) when standard input is a directory (both measured, both backends) |

A failure that follows some bytes is not lost and not mixed in: the call answers
`Got(n)` for the bytes, and the next call reads again and reports it if it persists.
`Failed` is reported once; the stream's error and end-of-file indicators are cleared
after each call, so a program that carries on after `Failed` is judged by what the
next read finds, not by what an earlier one left behind (glibc and macOS differ on
whether an end-of-file on a stream is sticky, and this removes the difference;
a terminal was not tried).

### 7.3 It is `fread`, not `read(0)`, and what that costs

`read_bytes` is `fread` on the C `stdin` stream, the stream `getchar` reads. So the two
mix: a program may take a header with `getchar` and the rest with `read_bytes`, and no
byte `getchar`'s buffer already holds is skipped (`it_shares_the_stream_with_getchar`
reads 5 bytes with `getchar` and the rest in 4 KiB, 7-byte and 1 MB buffers on both
backends). `read(0)` would be a system call fewer and would silently drop up to 4 KiB.

The price is **latency**: `fread` returns when the buffer is full or the input ends,
not when *some* bytes have arrived, so a tool that must act on each line of a slow
producer (`tail -f log | tool`) sees nothing until 64 KiB have arrived with a 64 KiB
buffer. Such a tool should keep `getchar`, or ask for a small buffer. A `read_some`
that returns what is there is a different answer to `Got(n)`'s meaning and nothing has
asked for it (§7.5).

### 7.4 Both backends, and WASI

Cranelift and LLVM both call `fread` (`stdin` on glibc, `__stdinp` on macOS, as
`flush_out` finds `stdout`) and read `errno` before `ferror`. On `wasm32-wasip1` the
console is not libc's stdio (`wasm.md` W2): `read_bytes` first hands out what
`getchar`'s own 4 KiB buffer holds, then calls `fd_read` straight into the caller's
buffer until it is full or the input ends, and imports nothing `getchar` did not
(`a_wasm32_bulk_read_never_calls_libc_stdio`). Run under `wasmtime`, the program of
`tests/programs/stdin_read_bytes.cho` counts the same bytes, lines and hash as the
native one for buffers of 0, 7 and 65,536 bytes and for a `getchar` header followed by `read_bytes`, on a closed descriptor and on a directory (the failures differ: wasmtime hands a program with no standard input an empty one, so the closed descriptor is `End` there).

### 7.5 What is not here

| | |
|---|---|
| **A query for what standard input is** (a pipe, a file, a terminal; `fstat`/`isatty`) | **Not built, and not cheap as asked.** The question is useful for a tool that wants to choose a strategy, but the use named for it, *seeking*, needs more than the answer: there is no way to hold a `File` for descriptor 0 (a `File` comes from `open_read` of a path), so `file_pread` cannot be pointed at standard input, and `lseek` has no builtin. A kind query alone would let a tool learn that it could seek and then be unable to. The slice that closes it is two builtins under `io_read`, `stdin_kind(io) -> int` and a positional read (`stdin_pread(io, at, into) -> Read`, `EBADF`/`ESPIPE` when it is a pipe), on three backends (WASI has `fd_fdstat_get` and `fd_pread`). The authority argument of §7.1 applies unchanged: it has to be behind `Io`, not `Fs("")` and `/dev/stdin` |
| **A read that returns what has arrived** | §7.3. Nothing has asked |
| **`std.io.read_into`** | A wrapper like `std.io.write_all`. `std.io` is an edition-1 file and cannot name an edition-7 builtin without declaring the edition, which would move the identity of every function in it (`editions.md` §6.3); `std.io` gets a second module or an edition marker in a slice of its own |
| **Standard input as a `File`** | Same reason as the seek row |

## 8. The suite

| Fixture | Rule | § |
|---|---|---|
| `getchar_without_capability.cho` | Reading the console needs an `Io` | 2 |
| `io_read_undeclared.cho` | An effect performed is an effect declared | 2 |
| `io_write_does_not_cover_io_read.cho` | The two labels are distinct, and a row saying one does not permit the other | 2.1 |

The third is the one that matters. It is what makes the rename a real
distinction rather than a spelling: a function declaring `[io_write]`
may not call `getchar`, and the refusal names the label it is missing.

| Accepting | Shows |
|---|---|
| `stdin_roundtrip.cho` | `getchar` to end of input, a row carrying both labels, and the first fixture with a `//~ STDIN` |
| `examples/tally.cho` | A real `wc` over standard input: the program that could not be written before, and the one that found the two things in §3.2 |
| `tests/programs/stdin_read_bytes.cho`, `conformance/stdin_bulk.rs` | §7: every byte once and in order for buffers of 0, 1, 7, 4,096, 65,536 and 1 MiB over inputs of 0 bytes to 24 MiB, input that arrives in pieces (3 to 500 writes, 2 ms apart), mixed with `getchar`, a closed descriptor (`9`), a directory (`21`) and `/dev/null`; the authority row; on both backends |
| `tests/reject/read_bytes_is_edition_seven.cho` | `read_bytes` is not a name before edition 7 |
