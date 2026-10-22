# Directory handles: opening beneath a directory, following no links

Status: **slices 1, 2, 4, 5 and 6 built** (4 is edition 7, 5 and 6 are edition 9), both backends: open a directory, step into a child directory, open a
file in it for reading (slice 1, #250), and create, append to, rename, remove and sync beneath it (slice 2). Linux
measured; Darwin's flags and Apple AArch64's variadic call are written and run only by CI. Slice 3 (`cancho-tools`
on top) is built there (alpibrusl/cancho-tools#4); §3 corrects what it said the row would become. Issue #227, gap L6 of [`agent-toolbox.md`](agent-toolbox.md).

## 1. Why

A path is a string, and the kernel resolves it every time it is used. [`agent-toolbox.md`](agent-toolbox.md) A.4
measured what that means for a narrowed capability: under `Fs("/tmp/jailprobe")`, `/tmp/jailprobe/link.txt` -- a
symlink to `/tmp/outside/secret.txt` -- **read the outside file**. The prefix check is lexical, and a link is not
lexical. `cancho-tools` (#214) has the same hole one level up: its `--root` is checked lexically in the tool (D9), so
a link inside the root reaches outside it, for `write` and `replace` as well as for the readers. D9 documented the
escape and pinned it with a test (M8) that was written to flip the day a primitive existed.

The obvious fixes do not work:

* **`O_NOFOLLOW` on the open.** It refuses a link only as the *last* component; `root/linkdir/secret` goes through a
  linked directory untouched.
* **`realpath` and re-check the prefix.** Check-then-use: a link swapped in between the check and the open escapes.
  It is also a libc call a program could only make through `Ffi("libc")`, which makes the authority unbounded.
* **Refuse `..` lexically, as the tools already do.** Necessary and not sufficient: a link is not a `..`.

What does work is resolving the path **one component at a time, from a descriptor**: open the root once, then
`openat(dir, name, O_NOFOLLOW)` for each component, so no path string is ever resolved twice and no link is ever
followed. That is how WASI's preopened directories, Capsicum and Linux's `openat2(RESOLVE_BENEATH)` work. Measured
here with a C probe (Linux 6.18, `root/` holding `a/b/f.txt`, `link.txt -> ../../outside/s.txt` and
`dirlink -> ../outside`):

| path | per-component walk | `openat2(RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS)` |
|---|---|---|
| `a/b/f.txt` | opened | opened |
| `link.txt` | refused, `ELOOP` | refused, `ELOOP` |
| `dirlink/s.txt` | refused, `ENOTDIR` | refused, `ELOOP` |
| `../outside/s.txt` | **opened** | refused, `EXDEV` |

The last row is the lesson: `O_NOFOLLOW` does nothing about `..`, so a walk must refuse `..` (and `.`, and an empty
component) itself. Cost, 200,000 opens of `a/b/f.txt` and closes: `open` 1.60 µs, the walk 6.14 µs (three `openat`
and two `close`), `openat2` 1.83 µs. `openat2` is Linux-only (5.6+) and macOS's nearest, `O_NOFOLLOW_ANY`, is
macOS 11+ and has no `RESOLVE_BENEATH`; the walk is the one that works on every target this compiler builds for, and
4.5 µs a file is not what a tool that reads a file spends its time on.

## 2. The primitives

A **directory handle** is a new `res` type, `Dir`: one descriptor, opened on a directory, which must be closed. Four
builtins, edition 6:

```
open_dir(fs: &Fs(p), path: &[byte]) -> [fs_read(p)] DirOpened     // DirOpened::Ok(Dir) | DirOpened::Failed(int)
dir_enter(dir: &Dir, name: &[byte]) -> [dir_read] DirOpened
dir_open_read(dir: &Dir, name: &[byte]) -> [dir_read] Opened
dir_close(dir: Dir) -> [] int
```

* **`open_dir`** is the trust anchor. It checks `path` against the capability's prefix exactly as `open_read` does
  (`file-handles.md` §2.1) and opens it with `O_RDONLY | O_DIRECTORY`. Links *in this path* are followed: the caller
  named it, and it is what everything after it is beneath. A path that is not a directory is `Failed(ENOTDIR)`.
* **`dir_enter`** opens one child directory: `openat(dir, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW)`.
* **`dir_open_read`** opens one child file for reading: `openat(dir, name, O_RDONLY | O_NOFOLLOW)`, and answers the
  same `Opened` as `open_read`, so `file_read`, `file_size`, `file_close` and the rest work on the `File` unchanged.
  Like `open_read`, it opens a directory too, and the first `file_read` on it is `Failed(EISDIR)`.
* **`name` is one component.** It must be non-empty, must not be `.` or `..`, and must hold no `/` and no NUL byte;
  any of those is `Failed(EINVAL)` (22) and no call is made. That is the check the probe's last row says a walk cannot
  leave to the kernel. A missing name, a link (`ELOOP` on both kernels, or `ENOTDIR` when a link to a directory is
  entered on Linux) and a permission refusal are values, as a missing file is for `open_read`.
* **`dir_close`** answers `close`'s result, like `file_close`.

The flags are per target and spelled once in each backend, with the probe's values: Linux x86-64 `O_DIRECTORY`
`0o200000`, `O_NOFOLLOW` `0o400000`; Linux AArch64 `0o40000` and `0o100000`; Darwin `0x100000` and `0x100`.
`O_RDONLY` is zero everywhere. `openat` is variadic, but its variadic argument (`mode`) is read only with `O_CREAT`,
which none of these passes, so declaring it with its three fixed arguments puts every argument where the callee looks
on every ABI -- the reasoning `filesystem.md` §3 gives for `open(path, O_RDONLY)`.

**Labels.** `open_dir` performs `fs_read(p)`, the path label, because it spends a path under the capability. The
others perform `dir_read`: like `file_read` (`file-handles.md` §4.1), a label that names no path, because the path
was spent at `open_dir` and the handle is the authority. A tool whose row says `dir_read` and not `fs_read` can reach
only what is beneath the directories it opened.

**The walk is a library function, not a builtin.** `std.dirs.open_file(dir, "a/b/c.txt")` splits on `/`, refuses an
empty, `.` or `..` component and a leading `/`, enters each directory with `dir_enter` (closing each as it goes) and
opens the last component with `dir_open_read`. Each step is one primitive with one check, so neither backend emits a
loop over a path, and the policy (what a component may be) is readable in `std` rather than buried in two code
generators.

## 3. Slices

1. **Built (#250):** `Dir`, `DirOpened`, the four builtins above, `std.dirs.open_file` and `std.dirs.enter`, on both
   backends.
2. **Built: writing beneath a directory**, what `cancho-tools`' atomic write (a temporary, `fsync`, rename over the
   target, `fsync` the directory, a lock file appended to) needs, and nothing more. Five builtins, edition 6, each
   name with §2's one-component check:

   ```
   dir_open_new(dir: &Dir, name: &[byte]) -> [dir_write] Opened      // openat(O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0644)
   dir_open_append(dir: &Dir, name: &[byte]) -> [dir_write] Opened   // openat(O_WRONLY | O_CREAT | O_APPEND | O_NOFOLLOW, 0644)
   dir_rename(dir: &Dir, from: &[byte], to: &[byte]) -> [dir_write] Done   // renameat(dir, from, dir, to)
   dir_remove(dir: &Dir, name: &[byte]) -> [dir_write] Done          // unlinkat(dir, name, 0)
   dir_sync(dir: &Dir) -> [dir_write] Done                           // fsync(dir)
   ```

   * **`dir_write` is its own label**, discharged like `dir_read` by an owned `Fs` or `World`: a function that may
     only read beneath a `Dir` says `[dir_read]` and is refused the moment it changes anything.
   * **A link is never written through.** `O_EXCL` already refuses to create over any link, a dangling one included,
     and `O_NOFOLLOW` refuses to append through one; `unlinkat` removes the link itself, never its target; `renameat`
     replaces a link at `to` rather than following it. A rename is within one directory, which is what an atomic
     replacement is.
   * **`mode` is variadic.** `openat`'s `mode` is read only with `O_CREAT`, but these two pass it, and on Apple
     AArch64 a variadic argument travels on the stack. The LLVM backend declares `openat` variadic and calls it as
     one; Cranelift cannot declare a variadic callee, so on that target the call is shaped the way the callee reads it,
     nine integer parameters with `mode` the ninth, as `fcntl`'s already is (`native-sockets.md` §3). Every `openat`
     in a module goes through that one shape, slice 1's included, since a module holds one signature per symbol. The
     flags for both slices are one table in `cancho_ir::open_flags`.
3. **Built in `cancho-tools`** (alpibrusl/cancho-tools#4): every tool opens `--root` with `open_dir` and every path
   beneath it one component at a time (its `toolbox.place`, which is `std.dirs.open_file`'s walk plus one probe: when
   `dir_enter` answers `ENOTDIR`, the component is opened once more with `dir_open_read`, so a link to a directory is
   `ELOOP` on Linux too, not indistinguishable from a plain file). `write` and `replace` lock, create, rename, remove
   and sync beneath the parent `Dir`; M8's symlink test flipped to a refusal, under a new rule `path.symlink`.
   *(Corrected.)* This item said the row would say `dir_read` "instead of `fs_read("")`". It does not: `open_dir`
   spends `fs_read(p)`, and a reader without `--root` still opens by path, so every tool keeps `fs_read("")` and
   gains `dir_read`. What disappears is **`fs_write("")`**: the writers hold `dir_write` and no path write at all.
   Without `--root` there is nothing to be beneath, and a reader opens the path as given; `.` was not opened as a
   root, because a working directory the process may not read would have broken every relative open.
4. **Built: a rename that never replaces a name** (§5). `dir_rename` is `renameat`, and POSIX `renameat` silently
   replaces an existing destination. A tool that promises never to replace a name (`cancho-tools`' `move`) could only
   look first and rename second, under a lock only its own processes take, and the look-then-rename window is real:
   in `cancho-tools`' `scripts/move_race.py`, a process that takes no lock and creates the destination at a random
   moment during the move lost its file in 55 of 20,000 trials (0.28%), and in 100 of 100 when the rename was delayed
   30 ms. One builtin, **edition 7**, additive (`editions.md` §5: `dir_rename_new` is a name a program may already
   declare, so older editions do not have it):

   ```
   dir_rename_new(dir: &Dir, from: &[byte], to: &[byte]) -> [dir_write] Done
       // Linux: renameat2(dir, from, dir, to, RENAME_NOREPLACE)    macOS: renameatx_np(dir, from, dir, to, RENAME_EXCL)
   ```

   * **`to` exists: `Failed(EEXIST)` (17, on both targets)**, whatever it is -- a file, a directory (an empty one
     too, which `renameat` replaces), a link, a dangling link -- and its bytes, its inode and the source are
     untouched. The check and the rename are one call in the kernel, so there is no window to race.
   * **A filesystem that cannot do it is `Failed(ENOTSUP)`, never a replacing rename.** The builtin makes one call
     and has no fallback. Linux answers `EINVAL` for a flag the filesystem does not implement, and `EINVAL` is also
     what a name that is not one component gets (§2, no call made), so the two would be the same value to a program.
     The builtin therefore answers a kernel `EINVAL` as **`EOPNOTSUPP` (95) on Linux**; macOS answers **`ENOTSUP`
     (45)** itself. The name check's own `EINVAL` is not mapped. A program reads the pair as: `EEXIST` -- the name is
     taken; `EOPNOTSUPP`/`ENOTSUP` -- this filesystem cannot promise it, decide what to do; `EINVAL` -- the names
     were wrong. The kernel checks that the destination exists before it looks at the flag, so a *taken* name is
     `EEXIST` even on a filesystem without the flag, and only a free name there is `ENOTSUP`.
   * Names are §2's one component, each; `dir_write`; every other answer is `renameat`'s (`ENOENT` for a missing
     source, even onto a taken name).
   * **Requirements.** Linux kernel 3.15 and glibc 2.28 (`renameat2` has no wrapper before that; not measured against another libc). macOS 10.12 (`renameatx_np`). A libc without the symbol fails at link time, not at run time, so a
     program cannot silently get the replacing behaviour.

5. **Built: creating a directory** (#595). There was no builtin that makes one, so a tool that unpacks a tree (cancho-tools'
   `archive --op extract`) could only extract into directories that already existed, and no tool in the toolbox could
   make them. One builtin, **edition 9** (the latest open one, `editions.md` §6.4: `dir_mkdir` is a name a program may
   already declare):

   ```
   dir_mkdir(dir: &Dir, name: &[byte], mode: int) -> [dir_write] Done        // mkdirat(dir, name, mode)
   ```

   * **Never replaces, never follows.** `mkdirat` fails with `EEXIST` for a name that exists, whatever it is: a file, a
     directory, a link, a dangling link (it does not follow the link to see whether its target is missing). The caller
     decides whether an existing directory is acceptable; `dir_stat` tells it which it is.
   * **`mode` is the permission bits asked for, `0` to `0o777`.** The process umask applies, as it does to
     `dir_open_new` (so `0o755` under a umask of `0o022` is `0o755`, and `0o777` is `0o755`). A `mode` above `0o777`
     or below zero is `Failed(EINVAL)` with no call: setuid, setgid and sticky are not something a program asks for by
     passing a larger number, and `mkdir` ignores setuid and setgid on Linux anyway.
   * **One component**, §2's check on `name` (`EINVAL` with no call for empty, `.`, `..`, a `/`, a NUL, or longer than
     `NAME_MAX`); `dir_write`; every other answer is `mkdirat`'s (`ENOENT` if the directory itself was removed,
     `EACCES`, `ENOSPC`, `EROFS`).
   * **WASI: refused**, under the permission-bits family (§4 of `wasm.md`): `path_create_directory` takes no mode, so
     the argument would be silently dropped, as `dir_mode` would answer 0.
   * What it does **not** do: create a missing parent (`mkdir -p` is a loop of `dir_enter` and `dir_mkdir` in the
     caller, one component at a time, which is where the policy for an existing directory belongs), set the mode of a
     directory that exists (there is no `chmod`; a file's mode is set when it is made, item 6), or make a directory
     atomically with its contents.

6. **Built: creating a file with its mode** (#596). `dir_open_new` takes no mode, so a file made with it is `0666` less the
   umask: `archive --op extract` wrote every file as `0644` and a script or binary lost its executable bit (measured: a
   `0755` file in a GNU tar extracted as `0644` under a umask of `0o022`). A `chmod` would be a second, weaker mechanism (a
   window in which the file has the wrong bits) and a wider authority, so the mode is given at creation. A second builtin,
   **edition 9** (edition 7 is closed, and `dir_open_new_mode` is a name a program may declare):

   ```
   dir_open_new_mode(dir: &Dir, name: &[byte], mode: int) -> [dir_write] Opened   // openat(dir, name, O_CREAT|O_EXCL|O_WRONLY|O_NOFOLLOW, mode)
   ```

   * **`dir_open_new`, and atomic.** Everything §3 item 1 says of `dir_open_new` holds (exclusive, no link followed, a taken
     name or a dangling link is `EEXIST`, one component, close-on-exec); the file never exists with other bits than the
     ones asked for, the umask applying as always (`0o755` under `0o022` is `0o755`, `0o777` is `0o755`).
   * **`mode` is `0` to `0o777`.** Above that, or below zero, is `Failed(EINVAL)` with no call: setuid, setgid and sticky
     are not asked for by passing a larger number. `dir_open_new` itself is unchanged and keeps `0o644`.
   * **WASI: refused**, under the permission-bits family, as `dir_mkdir` is.

## 4. What it is checked by

Slice 6 (`tests/conformance/directory_open_new_mode.rs`, **both backends** and the compiler in cancho): a file is made with the mode asked for under a umask of `0o022` (`0o755` is `0o755`, `0o700` is `0o700`, `0o777` is `0o755`, `0o644` and `0` as asked) and holds the bytes written; a file, directory or dangling link in the way is `EEXIST` and untouched; `..`, an empty name and `a/b` are `EINVAL`; modes `0o1000`, `0o4755`, `0o1777` and `-1` are `EINVAL` and no file appears; `tests/reject/dir_open_new_mode_is_edition_nine.cho`, `dir_open_new_mode_not_declared.cho` and `dir_open_new_mode_on_wasi.cho`.

Slice 5 (`tests/conformance/directory_mkdir.rs`, **both backends**, the same expectations): a free name becomes a directory with
the mode asked for under a umask of `0o022` (`0o755` is `0o755`, `0o700` is `0o700`, `0o777` is `0o755`), and it can be entered; an
existing file, directory, link or dangling link is `EEXIST` and nothing is written outside; `..`, `.`, an empty name, `a/b` and a
NUL are `EINVAL` with no call; a mode of `0o1000`, `0o4755` and `-1` is `EINVAL` and no directory appears;
`tests/reject/dir_mkdir_is_edition_nine.cho` (`not-a-function` at edition 8), `dir_mkdir_not_declared.cho`
(`effect-not-declared`) and `dir_mkdir_on_wasi.cho`.

Slice 4 (`tests/conformance/directory_rename_new.rs`, **both backends**, the same expectations):

* A table against a tree the test builds: a free name moves, byte for byte and inode for inode; a taken name is
  `EEXIST` with its bytes and inode and the source untouched; a link or a dangling link at the destination is
  `EEXIST`, the link stays and nothing is written outside; an existing directory (empty too) is `EEXIST`; a missing
  source is `ENOENT`, even onto a taken name; `..`, an empty name, `a/b`, `.` and `../escaped` are `EINVAL` with no
  call.
* **A race.** The program renames 300 pairs `s<i>` to `d<i>` while a thread of the test creates `d<i>`, exclusively,
  with a head start of a few trials on two of every three (the third is an uncontested control). Exactly one side
  may win a name: rename `ok` means the rival's create was refused and the file holds the source's bytes; `EEXIST`
  means the rival's create succeeded, the file holds the rival's bytes and the source is still there. A replacing
  rename answers `ok` *and* lets the rival's create succeed, which the test names as the lost file. Measured here,
  of 300 trials: on macOS (APFS) the rival won 152 to 162; on Linux (aarch64; overlayfs, NFS v3, ntfs-3g) it won
  between 17 and 136, depending on the filesystem and the backend. **Corrected:** that was not true of every
  machine. On CI's Linux x86-64 runner the LLVM program won all 300 and the rival none (the first CI run of the
  slice failed on exactly that), and on x86-64 Linux with the test pinned to two cores the rival wins about one
  trial in 300 per backend (the first contested one): which side wins is scheduling, and the head start is weak
  where the program is faster than the rival. So the safety checks (no replaced file, one winner per name) hold
  in every trial, and the power check is retried: a round is 300 fresh pairs, up to 20 rounds until the rival has
  won a name, and the test fails only if it never does.
* **A filesystem without the flag**, when `CANCHO_RENAME_UNSUPPORTED_DIR` names a directory on one. CI's
  filesystems all support the flag, so without the variable that test prints that it has nothing to run on and
  passes **vacuously**; it was run, with the answers in §5, on NFS v3 and ntfs-3g (Linux) and ExFAT (macOS), both
  backends: `ENOTSUP` for a free name, `EEXIST` for a taken one, nothing moved.
* `tests/reject/dir_rename_new_is_edition_seven.cho` (`not-a-function` at edition 6) and
  `dir_rename_new_not_declared.cho` (`effect-not-declared`: `[dir_read]` is not `dir_write`).
* **Mutants: 10, all killed.** Flag 0 (a plain rename) in the shared table, and on each backend alone; the exchange
  flag in place of no-replace; the second name unchecked, on each backend; the builtin at edition 6; the builtin
  performing `dir_read`; and, on each backend alone, the kernel's `EINVAL` left unmapped, which only the Linux run
  on NFS v3 kills (`err 22` where `err 95` is expected). The first six were each killed by the table and by the race
  independently.

Slice 2:

* `tests/conformance/directory_writes.rs`, on **both backends**, against a tree with a link to a file outside and a
  dangling link whose target is outside: create once and then `EEXIST`; create through either link `EEXIST`, and
  nothing appears outside; append twice reads back twice, append through either link `ELOOP`, and the outside file is
  unchanged; rename within the directory, and `..`, an empty name and `../escaped` refused with `EINVAL` (nothing
  escapes); remove a file, remove a link (the link goes and its target stays), a missing name `ENOENT`; sync `Ok`; a
  created file is `0644`, which is the check that would catch a mis-shaped variadic call on Apple AArch64.
* `tests/reject/dir_write_not_declared.cho`, and `dir_remove` in the accept fixture's owned-`Fs` function, so `dir_write`'s
  discharge is checked.
* **Mutants: 14, all killed.** On each backend: create without `O_EXCL`, append that follows links, append without
  `O_APPEND`, create with mode 0, and a rename that checks its first name twice. In the IR: `World` or `Fs` not
  discharging `dir_write`, the writes performing `dir_read` instead, and the writes at edition 5.

Slice 1:

* `tests/conformance/directory_handles.rs`, on **both backends**, judged from outside the program, against a tree the
  test builds: a file two directories down opens and reads back; a link to a file outside is refused (`ELOOP`); a
  link to a directory outside is refused when entered; `..`, `.`, an empty name, a name with `/` and an absolute path
  are refused by `std.dirs.open_file` and, as single names, by the builtins (`EINVAL`) without a call; a missing name
  is `ENOENT`; `open_dir` of a file is `ENOTDIR`; a path outside the capability's prefix traps, as `open_read` does.
* `tests/accept/directory_handles.cho`, on the default backend in the corpus and on both by hand: the shape of a program,
  every `Dir` closed, and a function that owns an `Fs` stepping beneath a directory with the row `[]` (owning the
  capability that paid for the handle discharges `dir_read`, as it discharges `file_read`).
* `tests/reject/`: an unclosed `Dir` (`linear-value-unconsumed`), one taken apart by a pattern
  (`linear-value-taken-apart`, naming `dir_close`), a function that steps beneath a `Dir` with a row that does not
  say `dir_read` (`effect-not-declared`), and an edition-5 file that names `open_dir` (`not-a-function`).
* **Mutants: 22, all killed.** On each backend: a file open or a directory step that follows links, no `/` check, no
  NUL check, no `.` check, no `..` check, no `NAME_MAX` bound, and `open_dir` without `O_DIRECTORY` (seen in the exit
  status: `open_dir` of a file then answers `ENOTDIR` one step later). In the IR: `World` or `Fs` not discharging
  `dir_read`, the steps performing nothing, the builtins at edition 5, and a taken-apart `Dir` naming `file_close`.
  In `std.dirs`: a walk that hands the rest of the path to one `dir_open_read` instead of recursing. Two survived the
  first run -- `Fs` not discharging `dir_read`, and the steps performing nothing -- and the accept function and the
  `dir_read_not_declared` fixture above were added for them.

## 5. Slice 4 measured: which filesystems can refuse to replace

`scripts/rename_noreplace_probe.c` makes one call per case in a directory on the filesystem under test:
`renameat2(RENAME_NOREPLACE)` (the raw syscall) or `renameatx_np(RENAME_EXCL)` onto a **free** name, then onto a
**taken** one (does it answer `EEXIST`, do the destination's bytes survive, does the source stay), then a plain
`renameat` onto the taken name for contrast. `scripts/rename_noreplace_matrix.sh` and
`scripts/rename_noreplace_matrix2.sh` build the Linux filesystems (loop images, an NFS server on loopback, ntfs-3g).

| filesystem | free destination | taken destination | plain `renameat` over a taken one |
|---|---|---|---|
| Linux 6.8 (aarch64, glibc 2.39): tmpfs | moved | `EEXIST`, bytes kept | **replaced** |
| overlayfs (the container root) | moved | `EEXIST`, bytes kept | **replaced** |
| ext4, XFS, Btrfs, vfat (loop mounts) | moved | `EEXIST`, bytes kept | **replaced** |
| NFS v3 and v4.2 (client and server on this kernel) | **`EINVAL`** | `EEXIST`, bytes kept | **replaced** |
| ntfs-3g (FUSE) | **`EINVAL`** | `EEXIST`, bytes kept | **replaced** |
| macOS 26.2: APFS, HFS+, FAT32 (disk images) | moved | `EEXIST`, bytes kept | **replaced** |
| macOS 26.2: ExFAT (disk image) | **`ENOTSUP` (45)** | `EEXIST`, bytes kept | **replaced** |

Not measured: exFAT and f2fs on Linux (the mount failed in the container), a nested overlayfs (same), kernels other
than 6.8, SMB/CIFS, 9p, virtiofs, Linux on x86-64 (the syscall is the same, the table is one architecture's), and a
macOS that is not 26.2. For a filesystem not in the table the contract is the builtin's, not a measurement: it makes
one call, and whatever that answers is `EEXIST`, success, or an error a program can see; it is never a replace.

What the table says that the design leans on:

* **The kernel refuses a taken name before it looks at the flag.** NFS, ntfs-3g and ExFAT answer `EEXIST` for a taken
  destination and only a free one is unsupported, so the answer to "may I take this name" on such a filesystem is
  still the correct `EEXIST` for the case that matters, and `ENOTSUP` only says the move cannot be made atomically.
* **macOS differs from `renameat` only in that.** It follows no link at the last component either way (a link at the
  destination is a taken name, as on Linux; the conformance test checks both), the names are the same one component, and the unsupported
  answer is `ENOTSUP` natively where Linux says `EINVAL`. The CI runner's filesystem is APFS, which supports it; the
  test above runs on it on every CI run.
* **`EINVAL` is ambiguous on Linux**, which is why the builtin maps it (§3, slice 4). Measured on NFS v3 and
  ntfs-3g through the compiler, both backends: `err 95` for the free name, `err 17` for the taken one.
