# Directory listing and file status: reading a directory beneath a handle

Status: **slices 1 and 2 built** (`dir_list`, `dir_next`, `dir_list_close`, `std.dirs.list`; `dir_stat`), edition 6,
and **§3.5 built** (`dir_mode`, `dir_own_mode`, edition 7, issue #243),
both backends; Linux measured, Darwin's `dirent` offsets run by CI's hostile listing and its `stat` offsets by CI's
status test. Slice 3 (the `lexsys-tools` `list` tool) is below, not built. Issue #222, gaps L2 (no listing) and L3 (no status without opening)
of [`agent-toolbox.md`](agent-toolbox.md). The issue asks for `fs_list` and `fs_stat` on a path under `Fs`; this
document puts the same capability on a `Dir` instead ([`directory-handles.md`](directory-handles.md), #227), §2 says
why, and that change of shape was decided by a person on the design's PR (§7).

## 1. Why

A tool cannot see the file tree. `fs_list(f, "/tmp")` and `fs_stat(f, "/tmp")` are `not-a-function`
(`agent-toolbox.md` A.2); `opendir` and `readdir` through `Ffi` compile, but the report becomes `bounded: false` and
`readdir` answers a `c_ptr` nothing can read. That blocks `list` (`ls`, `find`, `tree`), the toolbox's sixth tool, and
`seek` over a directory instead of named files. A program can learn a file's size only by opening it
(`file_size` is `lseek`), and cannot learn its kind or modification time at all.

`docs/file-writes.md` §8 named the obstacle: `readdir`'s `struct dirent` puts `d_name` at different offsets on
different targets, and a listing answers a variable number of names. Both are layout problems, and the second has
the same answer `file_read` already gives: the caller owns the buffer.

## 2. On a handle, not on a path

The issue's shape is `fs_list(fs: &Fs(p), path)`, performing `fs_read(p)`: a second way to spend a path. Since #227
there is a first one, `open_dir`, and everything beneath it is reached by handle, following no link. Listing and
status belong there:

* **Confinement for free.** A name a listing answers is one component of the directory it came from, so the natural
  next call, `dir_stat(dir, name)`, `dir_enter(dir, name)` or `dir_open_read(dir, name)`, can only reach a child. A
  path-based `fs_list` would hand back names a caller then glues onto a path string, which is the hole #227 closed.
* **No new path label.** The row of a lister is `fs_read(p)` (spent by `open_dir`) and `dir_read` (§3.3), the labels
  `lexsys-tools` already holds; a tool that lists gains no new kind of authority, and the report stays
  `bounded: true`.
* **One check, written once.** `dir_stat`'s name is checked exactly as `dir_enter`'s is (one component, not `.` or
  `..`, no `/`, no NUL, at most `NAME_MAX`; otherwise `EINVAL` with no call). A path-based `fs_stat` would need the
  prefix check *and* the link question again.

What the issue's gate asks for still holds, restated for this shape (§6): a hostile directory lists identically on
both backends, in bytewise order, in bounded memory; status never follows a link; the rows are exact.

## 3. The primitives

Two new `res`/value types and four builtins, edition 6, both backends:

```
dir_list(dir: &Dir) -> [dir_read] Listing                  // Listing::Ok(DirList) | Listing::Failed(int)
dir_next(list: &!DirList, name: &![byte]) -> [dir_read] Listed
                                                           // Listed::Name(int, int) | Listed::End | Listed::Failed(int)
dir_list_close(list: DirList) -> [] int
dir_stat(dir: &Dir, name: &[byte]) -> [dir_read] DirStat   // DirStat::Ok(int, int, int) | DirStat::Failed(int)
```

### 3.1 Listing

* **`dir_list`** starts a listing of `dir`: `fdopendir` on `openat(fd, ".", O_RDONLY | O_DIRECTORY)`, so the stream
  owns a descriptor of its own and closing it leaves `dir` open. *(Corrected while building: this said `dup(fd)`. A
  `dup` shares the file position, so two listings of one `Dir` read in turns would each see half the names; a second
  open of `.` beneath the handle does not, and is still beneath it. The conformance test reads two listings in
  turns.)* `DirList` is a `res`: it must be closed (`dir_list_close`, `closedir`), like `Dir` and `File`.
* **`dir_next`** answers the next entry: `Listed::Name(n, kind)` with the name's `n` bytes copied into the front of
  `name`, `Listed::End`, or `Listed::Failed(errno)`. `.` and `..` are never answered. If `name` is shorter than the
  entry, nothing is copied and the answer is `Failed(ENAMETOOLONG)` (36 on Linux, 63 on macOS); a buffer of
  `NAME_MAX` bytes (255) always suffices, and `std.dirs` passes one. `readdir` reports an error only through `errno`,
  so the step clears `errno` before the call and reads it when the answer is null.
* **`kind`** comes from `d_type`, with no extra call: `1` file, `2` directory, `3` symbolic link, `4` other (FIFO,
  socket, device), `0` unknown. Unknown is what a file system without `d_type` reports (some network and older file
  systems); a caller that needs to know asks `dir_stat`. The numbers are the language's, not the kernel's (`DT_REG` is
  8 on both targets, but a program should not depend on that).
* **Order is the kernel's**, which is no order. Sorting is `std.dirs`' (§4), because the gate asks for bytewise order
  and a sort over a variable number of names is not a builtin's job.
* **Memory** is the caller's: one name buffer and whatever the caller keeps. The stream itself is libc's `DIR`
  (about 32 KiB of buffer on glibc).

### 3.2 Status

**`dir_stat(dir, name)`** is `fstatat(fd, name, &st, AT_SYMLINK_NOFOLLOW)` on one checked component, answering
`DirStat::Ok(kind, size, mtime)` (kind numbered as in §3.1, never `0`; size in bytes; modification time in whole
seconds since the epoch) or `DirStat::Failed(errno)`. *(Renamed while building: this said `Status`, a name too many programs declare
for the prelude to take at an edition, as `editions.md` §7 found for `Conn`.)* It **never follows a link**: a link answers kind `3` and the
link's own size, a dangling link included. Following one is `dir_enter` or `dir_open_read`, which refuse links
anyway. Status is not opening: a FIFO answers kind `4` without the open that would block on it.

### 3.3 Labels

All four perform `dir_read` except `dir_list_close`, which performs nothing (as `dir_close` and `file_close`). Owning
an `Fs` or `World` discharges `dir_read`, as it already does for slice 1's steps. No new label: a listing reveals
names beneath a directory the program opened under `fs_read(p)`, which is what `dir_read` already means.

### 3.4 The layout table

The one design problem `file-writes.md` §8 named. Two structures, three targets, measured on Linux x86-64 with a C
probe (`offsetof` on glibc 2.x) and taken from the system headers for the others:

| | Linux x86-64 (measured) | Linux AArch64 | Darwin AArch64 |
|---|---|---|---|
| `dirent.d_type` | 18 (`u8`) | 18 | 20 |
| `dirent.d_name` | 19 | 19 | 21 |
| `stat` size | 144 | 128 | 144 |
| `st_mode` | 24 (`u32`) | 16 (`u32`) | 4 (`u16`) |
| `st_size` | 48 | 48 | 96 |
| `st_mtim.tv_sec` | 88 | 88 | 48 |
| `AT_SYMLINK_NOFOLLOW` | 0x100 | 0x100 | 0x20 |
| `ENAMETOOLONG` | 36 | 36 | 63 |

`DT_REG` 8, `DT_DIR` 4, `DT_LNK` 10 and `S_IFMT` `0o170000` with `S_IFREG` `0o100000`, `S_IFDIR` `0o40000`,
`S_IFLNK` `0o120000` are the same on all three. The table is spelled once, in `lex_sys_ir` beside `open_flags`
(`directory-handles.md` §3), and both backends read it. `d_name` is NUL-terminated on every target, so the name's
length is `strlen` from that offset, and `d_namlen` (Darwin only) is not needed. On Darwin AArch64, `readdir`,
`fdopendir` and `fstatat` are the 64-bit-inode symbols under their plain names (the `$INODE64` suffixes are x86-64
only). CI runs Linux x86-64 and Darwin AArch64; Linux AArch64's column is from `asm-generic/stat.h` and is run by no
CI job, as slice 1's flags were not.

The `stat` buffer is a stack slot of 144 bytes in the function that calls, read at the table's offsets; nothing in
the language sees `struct stat`.

### 3.5 Permission bits (#243)

```
dir_mode(dir: &Dir, name: &[byte]) -> [dir_read] Done      // Done::Ok(bits) | Done::Failed(int)
dir_own_mode(dir: &Dir) -> [dir_read] Done
```

The asker is lexsys-hooks: its production profile refuses to start when its data directory or a log in it can be read
or written by the group or by others, and lex-sys could not say a file's mode, so the service called libc's `statx`
through `Ffi("libc")` and its authority report was `bounded: false` for that alone. #243 asked for a path-based
`fs_stat`; §2's reasons put status on a handle, so the bits are two more steps beneath a `Dir`.

* **`dir_mode(dir, name)`** is `dir_stat`'s call: the same one-component check (`EINVAL`, no call), `fstatat` with
  `AT_SYMLINK_NOFOLLOW`, and it answers `st_mode & 0o7777`: the nine read/write/execute bits, set-user-id (`0o4000`),
  set-group-id (`0o2000`) and sticky (`0o1000`). A link answers its own bits, never its target's.
* **`dir_own_mode(dir)`** is `fstat` on the handle's descriptor: the directory that was opened, which no name beneath
  it can reach (`.` is refused). `fstat` needs no search permission on the directory and no path.
* **The answer is `Done`**, the prelude's integer-or-`errno` (`file-writes.md` §4), rather than a fourth field on
  `DirStat`: `DirStat::Ok(kind, size, mtime)` is matched by `lexsys-tools`' `list`, and a field added to a variant would
  break every match on it. Owner and group stay out, as §7 says, until a program asks.
* **Edition 7**, which is still being built ([`processes.md`](processes.md) slices 3 and 4): `dir_mode` is a name a
  program may already declare, so an edition-6 file does not see it (`tests/reject/dir_mode_is_edition_seven.ls`).
* **The offsets are §3.4's**: `st_mode`, 32 bits on Linux and 16 on Darwin, read only when the call succeeded; a
  failure's value is `0`. The permission bits are the same on every target.
* **Labels**: both perform `dir_read`, as `dir_stat` does (`tests/reject/dir_mode_not_declared.ls`).

Checked by `tests/conformance/directory_modes.rs`, on both backends: a directory made `0o710` and, beneath it, files
`0o600`, `0o644`, `0o400`, `0o4755` and `0o000`, directories `0o750` and `0o1777`, a link, a missing name (`ENOENT`)
and two names that are not one component (`EINVAL`), each printed by the probe and compared with what Rust's
`symlink_metadata` reports.

## 4. `std.dirs`: the sorted listing and the walk

The builtins answer one name at a time in the kernel's order; `std.dirs` turns that into what a tool wants:

```
pub fn list[&h, &d](heap: &!h Heap, dir: &d Dir, most: int) -> [heap, dir_read] Names
```

`Names` holds every name in one `std.buffer` and their offsets, **sorted bytewise** (a byte is compared as
unsigned, so `caf\xe9` sorts after `cafe`; no locale exists to be consulted), with each entry's kind. `most` caps the
entry count: past it the answer is truncated and says so, because a directory of 10^7 entries is a value the caller
must be able to refuse rather than a heap trap. Memory is the names plus 16 bytes an entry. The sort is a merge sort
over the offsets in `std` (there is no sort in `std` yet; this is its first asker, and it is written for this
use, not as a general `std.sort`).

`list`, the toolbox's tool, walks a tree with `dir_enter` on each directory entry, depth-first in sorted order, with a
depth cap; that walk is the tool's, not `std`'s, until a second program wants it.

## 5. Cost

Measured with a C probe on Linux 6.18, ext4, a directory of 100,000 empty files, three runs:

| | per entry |
|---|---|
| `readdir` (through `fdopendir`) | 246–284 ns |
| plus `fstatat(…, AT_SYMLINK_NOFOLLOW)` on each | 1.73–1.86 µs more |

So a listing that only needs names and kinds (all `ls` and most of `find`) costs a quarter of a microsecond an entry,
and status is opt-in at seven times that. 100,000 entries list in 25 ms; with status, 200 ms.

Built, the same directory through `dir_next` and written out a line a name: 29.9 ms on LLVM and 29.6 ms on Cranelift
(minimum of five), about 300 ns an entry with the output. Peak resident set is the same at 1,000 entries and at
100,000 (10.5 MB as Python's `RUSAGE_CHILDREN` reports it, the launcher included): one name buffer, whatever the
directory holds.

## 6. What it is checked by

* `tests/conformance/directory_listing.rs`, on **both backends**, against a tree the test builds and judged from
  outside the program (Rust's `read_dir` and `symlink_metadata`), names printed as hex so every byte survives:
  * a hostile directory: 100,000 plain files and a name with a newline, one that is not UTF-8 (`caf\xe9`, beside
    `cafe`; Linux only, since APFS refuses to create such a name with `EILSEQ`), a 255-byte name, a dangling link, a link to `..`, a FIFO and a subdirectory. `std.dirs.list` equals the
    entries sorted as bytes, with every kind equal to `lstat`'s; `dir_next` alone gives the same entries in the
    kernel's order, with `.` and `..` absent; the two backends agree byte for byte;
  * `dir_next` with a three-byte buffer on a longer name is `ENAMETOOLONG` and copies nothing; `std.dirs.list` with
    `most` 2 keeps two and says `truncated`; two listings of one `Dir`, read in turns, each see every name;
  * memory: §5's measurement (flat between 1,000 and 100,000 entries), not a test.
* `tests/accept/directory_listing.ls`: `/` listed by `dir_next` and by `std.dirs.list`, the two counts equal, no `.`
  or `..`, the sorted names strictly increasing; every `DirList` closed; an `Fs`-owning function listing with row `[]`.
* `tests/reject/`: an unclosed `DirList` (`linear-value-unconsumed`), one taken apart by a pattern
  (`linear-value-taken-apart`, naming `dir_list_close`), `dir_next` in a row that does not say `dir_read`
  (`effect-not-declared`), and `dir_list` at edition 5 (`not-a-function`).
* Slice 2: `dir_stat` on every entry of a hostile directory (20 files plus the cases above and a 12-byte file) equal to
  `lstat`'s kind, size and whole-second `mtime`, on both backends; the dangling link a link of 7 bytes, not `ENOENT`;
  the FIFO answered without blocking; `..`, `.`, `sub/x`, an empty name and a 256-byte name `EINVAL` with no call.
  The accept fixture checks `dir_stat` agrees with the listing on every kind of `/` the listing knew, and
  `tests/reject/dir_stat_not_declared.ls` that a status performs `dir_read`.
* **Mutants (slice 1): 19, all killed.** On each backend: `d_name` read one byte off; `d_type` read one byte off;
  `d_type` mapped wrong (a directory called a file); `.` and `..` both leaked; `..` alone leaked; the short-buffer check
  skipped. In `std.dirs`: unsorted, reversed, the cap ignored. In the IR: the steps performing nothing, the builtins at
  edition 5, a `DirList` that a pattern may take apart, and owning a `DirList` not discharging `dir_read`. The last
  survived the first run -- no fixture owned a listing outright -- and the accept fixture's `drain`, row `[]`, was added
  for it; the Cranelift `d_type` mutant first missed its target after `cargo fmt` rewrapped the line and was re-run.
* **Mutants (slice 2): 12, all killed.** On each backend: `fstatat` without `AT_SYMLINK_NOFOLLOW`; `st_mode`,
  `st_size` or `st_mtim` read at the wrong offset; a link's mode mapped to a file. In the IR: `dir_stat` performing
  nothing, and at edition 5. `dir_stat`'s name check is `dir_enter`'s own (`dir_call`), whose mutants #250 killed.

## 7. The decision, and what this does not do

**Decided: the handle shape.** The issue was written before `Dir` existed and asks for `fs_list`/`fs_stat` on paths.
This document proposed the handle-based shape of §2 instead, and a person chose it on the design's PR: same capability, one way to spend a path rather than two, and a
listing that cannot be turned into an escape. The "two askers" bar (`CONTRIBUTING.md`) is the issue's other open
question; D16 already answered it (`list`, and `lexsys-log`'s segment discovery in `file-writes.md` §8), and
this document assumes that answer.

**Slices.** 1: `dir_list`, `dir_next`, `dir_list_close` and `std.dirs.list`, both backends. 2: `dir_stat`. 3:
`lexsys-tools`' `list` tool and `seek` over a directory.

**Not done:**

* **No following stat.** `dir_stat` never follows a link; a tool that wants the target opens it with
  `dir_enter`/`dir_open_read`, which refuse links, or does not follow at all.
* **No sub-second times, no owner.** `list` needs kind, size and a time; `ls -l`'s owner has no asker. Nanoseconds are
  an added field when one appears. *(Corrected: this also said "no mode bits"; lexsys-hooks asked, and §3.5 answers
  them with `dir_mode` and `dir_own_mode`.)*
* **No `rewinddir`, no `telldir`.** A listing is read once; a second pass opens a second `DirList`.
* **No recursive walk in `std`** (§4).
