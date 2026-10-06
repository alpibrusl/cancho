# Checked output: `flush_out`

Status: **built**, edition 5. Gap L1 of [`agent-toolbox.md`](agent-toolbox.md) (§2.1, A.7); issue #215.

## 1. Why

`write_bytes` is `fwrite` on the C `stdout` stream, which is buffered. The count it answers is what the *buffer* took, not what reached
the file, and the last buffer's worth is written by the C runtime at exit, where nothing looks at the result.
[`bulk-io.md`](bulk-io.md) §3.3 said *"it wrote the bytes or the process is gone"*; [`agent-toolbox.md`](agent-toolbox.md) §2.1 measured
that it is false and corrected it in place. Reproduced on this commit's parent:

```text
$ ./wr               # io.write_all(i, "hello\n"); return what it answered
hello                # exit 6
$ ./wr >&-           # stdout closed: exit 6
$ ./wr > /dev/full   # strace: write(1, "hello\n", 6) = -1 ENOSPC, at exit; exit 6
```

So a program could not learn that its output was lost, and a tool whose contract is "the output is complete or the exit code says
otherwise" (`cancho-tools`, D2) could not be written. Once the buffer fills, a failing flush does surface inside `fwrite` as a short
count, so a program that checks every count catches *most* failures; never the last buffer's worth.

## 2. The primitive

```
flush_out[&i](io: &!i Io) -> [io_write] Done      // edition 5
```

Flush standard output and say whether everything written to it so far arrived: `Done::Ok(0)`, or `Done::Failed(errno)` -- `ENOSPC` for
a full device, `EBADF` for a closed descriptor, `EPIPE` for a reader that left (when `SIGPIPE` is ignored; otherwise the process is
killed first, as before). It is `fflush(stdout)`, the same stream `putchar` and `write_bytes` use, so it flushes exactly what they
buffered, **and then `ferror(stdout)`**, because `fflush` alone does not remember an earlier failure (below).

* **Its label is `io_write`**, the label of what it completes: it writes the buffered bytes, it does not gain a new power, and a program
  that writes and then flushes has the same row as one that only writes (`linearity-and-effects.md`: the label says what was done).
* **It answers `Done`**, the write side's result (`file-writes.md` §4), so the errno survives and a caller can tell a full disk from a
  closed pipe. `-1` would not.
* **A failure at any earlier point is reported, not only in the last buffer.** That is the property a program needs: one `flush_out` at
  the end. `fflush` does not give it. Measured with a C probe on glibc: after an `fwrite` of 100,000 bytes to `/dev/full` fails
  (`errno` 28), `ferror(stdout)` is 1 but the next `fflush` returns **0** -- the failed bytes were discarded, the buffer is empty, and
  there is nothing for it to fail on. So `flush_out` answers `Failed` when `fflush` fails (with its `errno`) **or** when the stream's
  error indicator is set; in the second case the original `errno` is gone and the answer is `EIO` (5), meaning "an earlier write to
  standard output failed". The indicator is not cleared, so every later `flush_out` keeps saying so. Checked (§3).
* **It is edition 5**, like every name added since `Net`: an edition-4 file that declares its own `flush_out` keeps it
  (`editions.md`).

What a program does with it: write, and before exiting call `flush_out`; on `Failed`, exit non-zero and say so on standard error
(unbuffered). `cancho-tools` does this in `toolbox.out`.

## 3. What it is checked by

`tests/conformance/checked_output.rs`, on **both backends**, judged from outside the program. The `/dev/full` cases run where the
device exists -- on Linux, where its absence fails the test rather than skipping it; macOS has none, so there the closed descriptor is
the failed write that is observed (the first CI run of this slice failed on darwin for exactly that reason, in the shell's redirect,
before the program ran):

* to `/dev/full`: a program that writes six bytes and flushes is answered `Failed(28)` (`ENOSPC`);
* to a closed standard output: `Failed(9)` (`EBADF`);
* to a pipe and to `/dev/null`: `Ok(0)`, and the bytes arrive;
* an earlier failure: a program that writes 100,000 bytes to `/dev/full` (so `fwrite` itself fails while the buffer drains), then writes
  nothing more, is answered `Failed(5)` by the flush, and again by a second flush;
* the authority report of a program that calls it: `io_write`, nothing else;
* an edition-4 file that names `flush_out` is refused as not a function, so the name is resolved by edition.

## 4. What it does not do

* **A trap still loses the buffer.** A trap is one instruction (`testing.md` §2) and runs no flush; a tool that must not lose output
  flushes before it can trap, or writes records small enough that losing the tail is detected by their absence (`cancho-tools`' `end`
  record). Flushing on trap would need a handler, which is a runtime this language does not have yet.
* **The exit path is unchanged.** A program that returns without calling `flush_out` still has its last buffer flushed by libc with the
  result ignored, as before. Making a failed exit flush change the status would change every existing program's meaning silently; the
  explicit call is the change that cannot surprise one.
* **`write_bytes` still answers the buffered count.** Its meaning is unchanged; `flush_out` is how the rest is learned.
