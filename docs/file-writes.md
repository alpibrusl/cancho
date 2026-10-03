# File writes: handles that append, sync, and rename

> **Status: design only; nothing is built.** `file-handles.md` gave a
> program a file it can *read* without knowing its size. This is the other
> half: a file it can **append to, update in place, make durable, and
> replace atomically**. The asker is a durable log
> (`lexsys-log`, the engine under a job queue and an audit trail), and it
> is the first program that cannot be written at all today rather than
> written badly: `fs_write` rewrites a whole file from byte zero, and a
> log is the opposite of that.
>
> Two claims are measured here (§1, §4, §6) on one machine and one
> filesystem, and the places where that is not enough are named.

---

## 1. What a log needs, and what a program has today

`filesystem.md` §3 and `file-handles.md` §5 give the whole write side
as it stands:

| a log wants to | exists today |
|---|---|
| append a record to the end of a file | no. `fs_write` truncates and rewrites |
| write a record, then know it reached the disk | no sync of any kind |
| read the record at byte offset *n* | no. `file_read` is sequential from the start, no seek |
| overwrite 8 bytes at offset *n* (an index, a header) | no |
| cut a half-written record off the tail after a crash | no truncate |
| publish a new file under an old name atomically | no rename |
| delete a sealed segment | no remove |
| refuse to run twice over the same directory | no lock |
| learn what segments exist after a restart | no directory listing |

`file-handles.md` §5 says *"every other verb can arrive when a program
asks for it — which is how `vec.set`, `vec.swap` and the bit operators
arrived."* This is that asking, and it asks for a coherent set, because
each row above is useless without its neighbours: an append without a sync
is a cache, and a sync without a rename is half a checkpoint.

### 1.1 What the sync costs, measured

The one number that decides how the engine is built is the price of the
guarantee. On this machine (ext4 on a virtual disk, one writer, 128-byte
records, 2,000 of them):

| | per record |
|---|---|
| `write` only | **0.4 µs** |
| `write` + `fdatasync` after each | **193 µs** |
| `write` + `fsync` after each | **217 µs** |
| `write`, `fdatasync` every 100 records | **4.2 µs** |

Three readings, and the third is the one that matters:

* **Durable is ~500× the price of written.** A primitive that hid that,
  by syncing implicitly, would make every append look like a database
  commit. So `file_write` never syncs and `file_sync` is a separate,
  visible call (§5).
* **`fsync` against `fdatasync` is 11% here** (217 against 193). That is
  not worth a second name in the first slice; §5 says what the second would
  cost.
* **Batching recovers 46×** (193 µs to 4.2 µs a record) for a hundred
  records of added latency. That is group commit, and it is the engine's
  job, not the primitive's. The primitive's job is to *allow* it: one
  `file_sync` covering many `file_write`s.

This is one disk. The *ratio* is the claim; the absolute figures move with
the device, and a laptop SSD with a volatile cache will show a much smaller
one that is, for the reason in §6, partly an illusion.

---

## 2. Why not `Ffi("libc")` and be done

It works today, and it is what the networking stack did before
`native-sockets.md`: a program declares `extern fn pwrite`, `extern fn
fsync`, `extern fn rename`, and holds an `Ffi("libc")`. This is the
alternative this document has to beat, and `native-sockets.md` §2 already
beat it on the same grounds, which apply unchanged:

* **A descriptor is an `int`, and an `int` is forgeable.** An `extern fn
  fsync(fd: int)` accepts any number, including a socket, a closed
  descriptor already reused, or `2`. The linear `File` is the thing that
  makes "this is the descriptor I opened, exactly once, and `close` ends
  it" a property of the *types*.
* **`Ffi("libc")` is the whole of libc.** A program that fsyncs through it
  can also `unlink` anything. The report says `ffi("libc")`, which is true
  and tells a supervisor nothing. A log that is supposed to touch
  `/var/lib/log` and one socket should say exactly that, and `lex-sys
  authority` already prints `fs_write("/var/lib/log")` for the path
  operations. `under-a-grant.md` found the filesystem dimension is the one
  a `lex-os` grant can enforce, so this is the dimension worth keeping
  precise.
* **The variadic trap is waiting.** `filesystem.md` §2.2: `open` is
  variadic, a variadic argument is passed on the stack on Apple ARM64, and
  an `extern fn open(path, flags, mode)` declared with three fixed
  arguments created a file with an unreadable mode on one CI target and not
  on the other. A log needs `open` with `O_CREAT` and `O_APPEND`, and
  `fcntl`, both variadic. Doing it from `extern fn` means every program
  rediscovers this. §3 shows the builtins can avoid it entirely.

The cost of the alternative is the cost of the engine's authority report
reading `ffi("libc")`. That is the reason, and it is the same reason as
every earlier stage.

---

## 3. Opening: three flags nobody has to know

The first design question is how to open a file for appending without
calling a variadic function and without guessing platform constants.

`O_CREAT`, `O_APPEND`, `O_TRUNC` and `O_EXCL` are **different numbers** on
Linux and macOS, and `open(path, O_CREAT, mode)` is variadic. The existing
backend avoided both with `creat(path, mode)` for writing and
`open(path, 0)` for reading (`filesystem.md` §2.2). `creat` means exactly
`O_WRONLY|O_CREAT|O_TRUNC`, so it gives the truncating case but not the
three a log needs: create-or-append, exclusive create, and read-write on
an existing file.

C's `fopen` has a mode string for each, **and is not variadic**. The
backend can call it, take the descriptor, and discard the `FILE`:

```
FILE *f = fopen(path, "ab");   // or "wb", "wbx", "r+b"
int fd  = dup(fileno(f));      // the open file description is shared
fclose(f);                     // frees the FILE; fd stays open
```

That `fclose` closes the original descriptor and the `dup` survives, so
nothing leaks a `FILE`. It costs two extra syscalls at *open*, not per
write, so the hot path is untouched. Measured on this machine, with
`strace` and `fcntl(F_GETFL)` on the descriptor after the `fclose`:

| mode string | what the kernel saw | access | `O_APPEND` | created with |
|---|---|---|---|---|
| `"ab"` | `O_WRONLY\|O_CREAT\|O_APPEND, 0666` | write | yes | `0666 & ~umask` (`0644` at umask 022) |
| `"wb"` | `O_WRONLY\|O_CREAT\|O_TRUNC, 0666` | write | no | the same |
| `"r+b"` | `O_RDWR, 0666` | read-write | no | (fails if missing) |
| `"a+b"` | `O_RDWR\|O_CREAT\|O_APPEND, 0666` | read-write | yes | the same |
| `"wbx"` | `O_WRONLY\|O_CREAT\|O_EXCL\|O_TRUNC, 0666` | write | no | **`EEXIST` if present**, refused |

After `dup` and `fclose` the descriptor keeps `O_RDWR|O_APPEND` and
accepts a `write`. Three facts follow, and one of them is a limit:

* **The mode is `0666` masked by the umask.** The existing `creat(...,
  420)` is `0644`. The new opens are *more permissive by default* than
  `fs_write`, and a program that wants `0600` for a segment file cannot ask.
  Slice 1 accepts that; §8 lists it.
* **No `O_CLOEXEC`.** The descriptor is inherited across `exec` (measured:
  `FD_CLOEXEC` clear). lex-sys has no `exec` builtin, so this matters only
  to a program that reaches `fork`/`exec` through `Ffi`, and it is the
  same as `open_read` today.
* **Per-target flag constants are avoided entirely**, which is what the
  rule from `filesystem.md` §2.2 asked for: *the backend does not call
  variadic C functions*, and here it need not guess a flag either.

`fopen` is not the only way. The LLVM backend can emit a correct varargs
call, and each target's `O_*` values could live in the backend, as the
Poller's `epoll`/`kqueue` split already does. The Cranelift backend cannot
express a variadic signature, and the two backends are held to the same
conformance suite. `fopen` is the one answer that is the same on both, so
it is the recommendation, and the per-target flag table is the fallback if
a reason against `fopen` turns up.

### 3.1 One `File`, or a `WFile`

A handle opened with `"ab"` rejects `file_read` with `EBADF`, and a handle
opened with `open_read` rejects `file_write` the same way. The mode is
enforced by the **kernel**, not by the type.

The alternative is a second prelude type, so that `file_write` cannot be
*called* on a read handle at all. That is the more precise answer and the
language's habit (a `Conn` is not a `Listener`), but those are different
*kinds* of descriptor and a file opened `r` versus `a` is not. It would
double every operation (`file_close` against `wfile_close`), and it would
spend two more ordinary names (§4.2 of `file-handles.md`: *"a builtin's
name is reserved against every program"*). The recommendation is **one
`File`**, with the failure surfaced as an errno in the result, the same way
every other I/O failure here is. Open question in §8.

---

## 4. The operations

Names follow `file-handles.md` §4.2: the subject, then the verb, and none
of the three ordinary names (`write`, `read`, `sync`) that a program using
`Ffi("libc")` may already declare. Zero fixtures, examples or packages in
this repository use any of the names below (checked with `grep` over every
`.ls` and `.rs` file).

```
open_append[&c,&a](fs: &c Fs(p), path: &a [byte]) -> [fs_write(p)] Opened   // "ab":  create or append
open_write [&c,&a](fs, path)                      -> [fs_write(p)] Opened   // "wb":  create or truncate
open_new   [&c,&a](fs, path)                      -> [fs_write(p)] Opened   // "wbx": create, EEXIST if present
open_rw    [&c,&a](fs, path)                      -> [fs_read(p), fs_write(p)] Opened   // "r+b": existing, both

file_write [&f,&b](file: &!f File, bytes: &b [byte])                  -> [file_write] Done
file_pwrite[&f,&b](file: &!f File, at: int, bytes: &b [byte])         -> [file_write] Done
file_pread [&f,&b](file: &!f File, at: int, into: &!b [byte])         -> [file_read]  Read
file_sync  [&f](file: &!f File)                                        -> [file_write] Done
file_truncate[&f](file: &!f File, len: int)                            -> [file_write] Done
file_size  [&f](file: &!f File)                                        -> [file_read]  Done
```

`Opened` is the existing prelude enum, so a program that opens through the
new names uses the arms it already knows. The two additions are one prelude
enum for write-side results:

```
enum Done {
    Ok(int),     // bytes written, the size, or 0
    Failed(int)  // errno
}
```

Why a new enum and not `Read`: `Read` has an `End` arm that a write cannot
reach, and `file-handles.md` §3's rule is that a sentinel is how `getchar`
and `fs_read` came to disagree. Why not `-1`: `fsync` failing with `EIO`
and failing with `ENOSPC` need different handling (§5), so the errno has to
survive. `Done` is the second name this document asks the prelude to
reserve; zero declarations of it exist in the tree, and it is an open
question (§8) whether the name is too ordinary.

### 4.1 `file_write` is one `write(2)`, as `file_read` is one `read(2)`

It returns how many bytes the call accepted, which can be fewer than asked
(an interrupted call, a full disk partway). A loop that writes the rest is
`std.fs.write_all`, the same shape `bulk-io.md` took for the console and
`print_nat/write_all` took for std. The primitive does no looping because
the loop's *policy* (what to do on `EINTR`, whether to retry on `ENOSPC`)
is the program's.

### 4.2 `file_pwrite` and `file_pread` take an offset and leave the cursor

A record store reads *the record at offset n*, and an index file updates
eight bytes in place. Doing that with a cursor means `seek` and a race
between two readers sharing a handle. `pread`/`pwrite` carry the offset and
touch no state, so the same handle serves any number of readers.

One limit, **measured and not a bug in the design**: `pwrite` on a handle
opened for *append* does not write at the offset. On this machine, a file
holding `AAAA` opened `"a+b"` and given `pwrite(fd, "BB", 2, 0)` reads back
as `AAAABB`, not `BBAA`. That is documented Linux behaviour. It is why
`open_rw` (no `O_APPEND`) exists as a separate open: an in-place update
needs a handle that is *not* an appender. §3 lists the modes for that
reason.

### 4.3 `file_truncate` and `file_size`

A crash can leave a record half-written at the tail. Recovery scans the
tail, finds the last whole record, and cuts the rest off with
`file_truncate`. Without it the only recovery is to copy the good part to a
new file.

`file_size` could be `fstat`, but `struct stat`'s `st_size` is at a
different offset on different targets (48 on Linux, checked with
`offsetof` here; 96 on macOS, from the headers and **not checked**). It is
instead two `lseek` calls (`SEEK_END` is 2 on both) that read the end and
put the cursor back, so a handle opened `r+b` is not left at the end. Two
syscalls is not measured to matter here and a log keeps its own length.

---

## 5. Sync: the primitive, and what it promises

```
file_sync(file) -> [file_write] Done
```

It is `fsync(2)`: when it answers `Ok`, the data **and the metadata needed
to read it back** (the length) are on stable storage, as far as the kernel
and the device report. The first slice does not ship `fdatasync`; §1.1
puts the gain at 11% and a second name costs a second reservation.

### 5.1 A failed sync is not retryable

If `fsync` fails with `EIO`, the kernel may already have *dropped* the
dirty pages and cleared the error, so a second `fsync` on the same handle
can succeed over data that was never written. This is a well-known
behaviour (PostgreSQL's 2018 "fsyncgate" is the usual citation), and **it
is not measured here**: a test that provoked it would need a failing
block device. The consequence for the design is stated plainly instead:

> `Done::Failed` from `file_sync` means *the file's contents are unknown*.
> The documented response is to stop and recover from the log, never to
> retry.

That is policy for `lexsys-log`, and it is why `Done` carries the errno:
`ENOSPC` on a write is an ordinary condition a program may wait out,
`EIO` on a sync is not.

### 5.2 A directory is synced through the same call

A new file, a rename, a removal is a change to a **directory**, and is not
durable until the directory is synced. That needs no new builtin: `open(2)`
on a directory with `O_RDONLY` succeeds and `fsync` on that descriptor
returns 0 (measured here: `open("/tmp/fw", O_RDONLY)` gave a descriptor,
`fsync` gave 0). So a program does `open_read(fs, dir)`, `file_sync`,
`file_close`.

The authority question is what that function's row says. `file_sync`
carries `file_write`, the conservative label: it asks the device to
persist, and labelling it `file_read` would let a program that holds only
a read capability claim a write. But `file-handles.md` §4.1's rule is that
*owning a `File` discharges its label*, so a function that opens the
directory itself and syncs it may report only `[fs_read("/data")]`. Syncing
a descriptor opened read-only writes no new content, so that is
defensible, and it is also **a prediction about the checker that is not
tested here**. Open question 6 asks for a conformance test of the actual
report before this is relied on.

---

## 6. What `file_sync` does not promise, by platform

**Linux is the target this is designed for and measured on.** It is where
`lex-os` runs its Firecracker box and the only place the numbers in §1.1
were taken.

**On macOS, `fsync` does not flush the drive's own cache.** Durability
there needs `fcntl(fd, F_FULLFSYNC)`, which is variadic, the exact thing §3
avoids, so it needs the per-target path in §3's last paragraph, and it is
**not** in slice 1. Until it is, `file_sync` on macOS means "handed to the
drive", and the document that ships it says so. The rule from `lex-os` is
*refuse, don't downgrade*: a design that cannot make the guarantee on a
target should say so at the target, not pretend. Whether `file_sync`
refuses to compile for a non-Linux target or carries a documented weaker
meaning is open question 4 in §8. A reviewer on a Mac is the person to
answer it, and this machine cannot.

**A volatile cache can make §1.1 flatter than it should be.** A device that
acknowledges a flush it has not performed makes `fsync` fast and the
numbers an illusion. The 193 µs here is consistent with a real flush on a
virtual disk, and is not proof of one. The correctness of `lexsys-log`
cannot rest on this table, and its crash tests (not this document's
concern) do not.

---

## 7. Path operations: rename, remove, lock

These take an `Fs(p)` like `open_*` and are checked the way `filesystem.md`
§4 checks every path: against the prefix, with `..` refused, at run time.

```
fs_rename[&c,&a,&b](fs: &c Fs(p), from: &a [byte], to: &b [byte]) -> [fs_write(p)] Done
fs_remove[&c,&a](fs: &c Fs(p), path: &a [byte])                  -> [fs_write(p)] Done
file_lock[&f](file: &!f File)                                    -> [file_write] Done
```

* **`fs_rename` checks both paths**, and both must be inside the prefix; a
  rename that moves a file out of the granted directory is a write outside
  it. Atomic replacement of an existing destination is POSIX's guarantee
  within one filesystem. Across filesystems it fails with `EXDEV` and the
  errno is returned, not retried. That guarantee is not measured here; it
  is what the standard says and what every database relies on. Durability
  still needs §5.2's directory sync, and `lexsys-log` is the program that
  must do it.
* **`fs_remove` is `unlink`.** It does not remove directories.
* **`file_lock` is `flock(fd, LOCK_EX|LOCK_NB)`**: a non-blocking, exclusive,
  advisory lock released when the process ends, however it ends. That last
  clause is the reason it is better than a lock *file*: a crashed process
  leaves a stale lock file and a human to remove it, and leaves nothing
  behind with `flock`. A database refuses to open a directory a second
  process is already writing; this is the primitive for that. `flock`
  is not variadic; `LOCK_EX` is 2 and `LOCK_NB` is 4 in the Linux headers
  (read from `/usr/include/x86_64-linux-gnu/sys/file.h` here), and the BSD
  headers use the same values, **which is unconfirmed on a Mac** and is a
  line for the macOS CI to check before this is trusted there.

Advisory means a process that does not call `flock` is not stopped. It
protects against two copies of *this* program, not against hostility, which
is `Fs`'s own stated scope (`filesystem.md` §2.1: honest mistakes, not a
sandbox).

---

## 8. What this does not do

* **No directory listing.** `readdir`'s `struct dirent` has `d_name` at
  different offsets on different targets, and a listing returns a variable
  number of names, a bigger design than anything above. A log does not
  need it in its first version: a `CURRENT` file naming the live segments,
  replaced with `fs_rename` and a directory sync, is how LevelDB finds its
  files without listing a directory. `fs_list` waits for a program that
  cannot do that.
* **No `mmap`, no `O_DIRECT`, no `sendfile`.** All are performance
  features whose cost is not yet measured to matter; the log's
  throughput gate decides, not this document.
* **No mode argument.** New files are `0666 & ~umask` (§3). A secret
  written this way is readable under a lax umask. A `mode` parameter is
  cheap to add (`creat` takes one) and is held back only so slice 1 is one
  decision smaller.
* **No `O_CLOEXEC`** (§3).
* **No durability on a non-Linux target** (§6).

## 9. Open questions

1. **Is `Done` too ordinary a prelude name?** Zero declarations in this
   repository, but `file-handles.md` §4.2 priced exactly this mistake.
   `Written`, `Fs` results and `FileResult` are the alternatives.
2. **One `File` or a `WFile`?** §3.1 recommends one. The cost is that the
   read/write mismatch is a runtime `EBADF`, not a type error.
3. **`fopen` or per-target flag constants** for the opens (§3)? `fopen` is
   the same on both backends; the table is the fallback.
4. **Refuse or weaken `file_sync` off Linux** (§6)?
5. **A `mode` argument now or later** (§8)?
6. **What does the authority report say for a read-opened handle that is
   synced** (§5.2)? A test, not an argument, settles it.

## 10. Slices, and how each is checked

The same rules as every earlier stage: both backends, one conformance
suite, no source file over 2,000 lines (`fs.rs` is 254 and
`filesystem.rs` is 304, so there is room), every refusal with a rule tag.

1. **Handles.** `open_append`, `open_write`, `open_new`, `open_rw`,
   `file_write`, `file_pwrite`, `file_pread`, `file_sync`,
   `file_truncate`, `file_size`; `std.fs` gets `write_all` and
   `into_result`. This is enough to write and recover a log.
2. **Path operations.** `fs_rename`, `fs_remove`, `file_lock`. This is
   enough to rotate and seal a segment and to own a directory.
3. **Listing, `mode`, `fdatasync`, `F_FULLFSYNC`**, each only when a program
   asks and a measurement says it is worth the name.

What each slice's conformance tests check **from outside the program**,
for the reason `filesystem.md` §2.2 gave (*a read that happens to succeed
is not evidence about a mode*):

* the bytes of the file after `append`, `pwrite` and `truncate`, read by
  the test and not by the program;
* the open flags and the mode of a created file, from the *kernel* side
  (`strace`, or `fcntl` on the child's descriptor), including that
  `open_append` of an existing file does not truncate it and `open_new` of
  one refuses with `EEXIST`;
* that `file_sync` makes an `fsync` call appear under `strace`, because
  "sync returned `Ok`" proves nothing about whether it called anything;
* `file_pwrite` on an `open_append` handle appends (§4.2), written as a test
  so the behaviour is a known fact and not a surprise;
* the **authority report** of an `fs_write("/d")` program against the
  handle program over the same directory, and that the prefix survives
  (`file-handles.md` §4's rule: *a program must not look more powerful for
  having been written better*);
* for `file_lock`, a second process refused while the first lives and
  admitted after it is killed, which is the property that justifies the
  primitive.

What no test at this layer can check is that an acknowledged sync survives
a power cut. That is the log's crash test, in `lexsys-log`: truncate or
corrupt the file at every byte offset and require recovery to yield a valid
prefix. This document supplies the primitives that test needs and the
policy (§5.1) it must follow.
