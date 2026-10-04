# Directory handles: opening beneath a directory, following no links

Status: **slices 1 and 2 built**, edition 6, both backends: open a directory, step into a child directory, open a
file in it for reading (slice 1, #250), and create, append to, rename, remove and sync beneath it (slice 2). Linux
measured; Darwin's flags and Apple AArch64's variadic call are written and run only by CI. Slice 3 (`lexsys-tools`
on top) is below, not built. Issue #227, gap L6 of [`agent-toolbox.md`](agent-toolbox.md).

## 1. Why

A path is a string, and the kernel resolves it every time it is used. [`agent-toolbox.md`](agent-toolbox.md) A.4
measured what that means for a narrowed capability: under `Fs("/tmp/jailprobe")`, `/tmp/jailprobe/link.txt` -- a
symlink to `/tmp/outside/secret.txt` -- **read the outside file**. The prefix check is lexical, and a link is not
lexical. `lexsys-tools` (#214) has the same hole one level up: its `--root` is checked lexically in the tool (D9), so
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
2. **Built: writing beneath a directory**, what `lexsys-tools`' atomic write (a temporary, `fsync`, rename over the
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
     flags for both slices are one table in `lex_sys_ir::open_flags`.
3. **`lexsys-tools`:** every tool opens `--root` (or `.`) with `open_dir` and every path beneath it with `std.dirs`;
   its row says `dir_read` (and slice 2's label) instead of `fs_read("")`; M8's symlink test flips.

## 4. What it is checked by

Slice 2:

* `tests/conformance/directory_writes.rs`, on **both backends**, against a tree with a link to a file outside and a
  dangling link whose target is outside: create once and then `EEXIST`; create through either link `EEXIST`, and
  nothing appears outside; append twice reads back twice, append through either link `ELOOP`, and the outside file is
  unchanged; rename within the directory, and `..`, an empty name and `../escaped` refused with `EINVAL` (nothing
  escapes); remove a file, remove a link (the link goes and its target stays), a missing name `ENOENT`; sync `Ok`; a
  created file is `0644`, which is the check that would catch a mis-shaped variadic call on Apple AArch64.
* `tests/reject/dir_write_not_declared.ls`, and `dir_remove` in the accept fixture's owned-`Fs` function, so `dir_write`'s
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
* `tests/accept/directory_handles.ls`, on the default backend in the corpus and on both by hand: the shape of a program,
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
