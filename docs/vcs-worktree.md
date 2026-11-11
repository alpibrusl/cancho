# Writing the working copy from cancho: `materialize` and `remove_path` (slice 3b)

Status: **W1-W4 built and measured on Linux x86-64** (2026-11-11, §6); **the five check scripts and the kernel side were run on Darwin on 2026-11-11, §8**. Claims are marked **[read]** (from the source), **[measured]** (run) or **[choice]** (a decision). This is slice 3b of
[`directory-links.md`](directory-links.md) §8; the primitives it needs were built in slices 1 and 2, and `vcsscan.cho` (3a) already reads a working copy. What is missing is
the two functions that *write* one, which `switch`, `restore`, `revert`, `apply` and the deletions of `record` call in Rust (`vcs_repo.rs:524,598`, `vcs_record.rs:359`,
`vcs_git.rs:36`) **[read]**.

## 1. What the Rust functions do **[read]** (`crates/cancho-vcs/src/repo.rs:361-426`)

* `materialize(entry)`: `check_path`; read the blob; **clear obstacles** (every parent below the root that is not a directory is removed, a link included, then a directory
  at the path itself is removed with everything in it); a link entry makes the parent directories, removes the name and makes the link with the blob's text as target; any
  other entry is written whole to a temporary file in `.cancho/vcs/tmp` with mode `0755` or `0644`, then **renamed over the destination** (parents made first).
* `remove_path(path)`: `check_path`; remove the file (a missing one is fine, any other failure is a refusal `io`), then remove each parent directory that is empty, upward,
  stopping at the root or at the first that is not.

## 2. What cancho can do today **[read]**

`dir_mkdir`, `dir_open_new_mode` (create with the mode, `O_EXCL`, no link followed), `dir_rename` (**within one directory only**: `renameat(dir, from, dir, to)`),
`dir_remove`, `dir_rmdir` (empty directories only), `dir_symlink`, `dir_readlink`, `dir_list`, `dir_stat`, `dir_enter` (refuses a link, so a link planted in the path is
`ELOOP`/`ENOTDIR`, never followed). There is **no** recursive remove and **no** rename across directories.

## 3. Design **[choice]**

**W1. One new module, `examples/selfhost/vcsworktree.cho`** (edition 12), two public functions on a `&Dir` of the working-copy root, answers in `vcsstore`'s status
convention, as `(status, errno)` (0 ok, 10 io with the errno, 31 `path-unsupported` for `check_path`). The caller reads the blob first, so a blob that cannot be read
refuses before the working copy is touched, as in Rust:

```
materialize(heap, root: &Dir, mode: int, path, text)   // mode 0 file, 1 executable, 2 link; text is the blob (a link's target)
remove_path(heap, root: &Dir, path)
```

It is not in `cancho.files` (nothing routes to it yet), exactly as `vcsscan.cho` was in 3a, so no compiler, seed or release file changes, and no reseed.

**W2. Obstacles and parents, one component at a time.** Walk `path` with `dir_enter`; at each component `dir_stat` (no follow): a missing one is `dir_mkdir` `0755`; a
directory is entered; **anything else (file, link, dangling link) is `dir_remove`d and replaced by a directory**, as Rust does. At the leaf: a directory is removed
recursively by a private loop of `dir_list`, `dir_enter`, `dir_remove`, `dir_rmdir` (§2 of `directory-links.md` already puts this loop in the caller; it is **not** added to
`std.dirs`: one asker, the two-askers rule). The loop never follows a link and never leaves the directory it is in.

**W3. Writing a file: where the temporary file lives. Owner decision.** `dir_rename` cannot cross directories, so Rust's `.cancho/vcs/tmp` → destination is not available.
Options:

* **(a) Recommended, and built (the owner had not answered when the session was asked to finish what it could: change to (b) if you prefer it).** A temporary `NAME.cancho-tmp-0` **beside** the destination, written with `dir_open_new_mode` (so the mode is right from the first byte), then
  `dir_rename` over `NAME`. One file is whole or old, as in Rust. A crash can leave the temporary file in the working copy, where Rust's cannot (its is in `vcs/tmp`):
  `vcsscan.cho` therefore skips a name ending in `.cancho-tmp-0` (a **known difference** from the Rust scan for a user file so named, listed in the header and tested).
  `vcsstore.cho` already does this for objects (`.tmp-`).
* **(b)** Remove the destination, then create it at its final name. No scan change, no stray file, but there is a window in which the file is absent or partial; `status`
  shows it as changed and `restore` repairs it.

**W4. A link entry** is `dir_remove` of the name (if it is a file or link) then `dir_symlink`; a target that is not UTF-8 is not made (`path-unsupported`), as the scan
refuses one. **`remove_path`** is `dir_remove` (`ENOENT` is fine), then `dir_rmdir` of each parent while it succeeds; `ENOTEMPTY` ends the walk quietly, any other
errno on a parent is also quiet, as in Rust (`fs::remove_dir(...).is_err()` breaks the loop) **[read]**.

## 4. What it is checked by

Neither function is a command, so Rust's `Repo::materialize` has no command-line oracle of its own; its oracle is the commands that call it. `vcsstorectl.cho` gains
`materialize ROOT PATH` (the entry of HEAD's tree at `PATH`) and `remove-path ROOT PATH`. `examples/selfhost/vcsworktree_check.sh` makes a repository with the Rust command, then
for each case makes two copies of the working copy, runs **Rust `cancho vcs restore PATH`** in one and the controller in the other, and compares the two trees entry by entry
(kind, mode, size, bytes, link target; `find` and `stat`). Cases: a file changed, deleted and recreated; a new executable and one losing its bit; a file where a directory
was, and the reverse (with contents); a link where a file was and the reverse; a link in the middle of the path (a link to a directory replaced by a real one: the target
directory is **not** written through); a dangling link in the way; a deep path whose directories are all made; `remove_path` leaving empty parents, stopping at the first
non-empty one, leaving the root; a missing file; a path with `..` or an absolute one (`path-unsupported`, nothing touched). **Mutants:** the exec bit dropped; a link followed
when clearing; the empty-parent loop not stopping at the root (run against a scratch tree, never the real one).

## 5. Not decided here, not measured

* Which command calls these first. `restore` is the smallest and the oracle above; `switch` also needs `work_lock` and the dirty-path check (`move_working_copy`), slice 4.
* Darwin: **measured since, §8** (`AT_REMOVEDIR`, `ENOTEMPTY`, the rename-over behaviour); crash windows (no test kills the process between the write and the rename; `vcs-native.md` §6 has the
  harness for it); the cost of a switch of many thousands of files.

## 6. Built and measured (2026-11-11, Linux x86-64)

`vcsworktree.cho` (edition 12: `dir_symlink`), `vcsscan.skips` ignores a name ending `.cancho-tmp-0`, and `vcsstorectl.cho` has `restore ROOT PATH`.
`examples/selfhost/vcsworktree_check.sh` runs **27 checks**, all passing: 21 cases against Rust `cancho vcs restore` (tree equal entry by entry and both succeed or both
fail), 4 refusals (`..`, absolute, `.cancho/HEAD`, an empty component: exit 1 and the working copy untouched), a path nothing names, and the scan skipping a left-over
temporary file. The older scripts that build `vcsstorectl` still pass (`vcsstatus` 31, `vcsrepo` 31, `vcsstore` 26, `vcslog` 51), and `vcsscan.line_end` is public so
`vcsstorectl` does not copy it (the duplication guard caught the copy).
**Mutants, each killed:** the execute bit dropped (2 cases fail), a link entered as if it were a directory (1), the upward `dir_rmdir` removed (2), the recursive removal of a
directory in the way removed (1). **A hole found by the first mutant run and fixed:** the case "a link as a directory of the path" restored `d` first, which removes the link,
so no link was ever in the path of `d/f`; it now restores only `d/f` and `d/g/h`.
**Found in Rust, not a difference:** restoring a directory-with-contents where a file was (`f1`, `f1/inner`, `f1/deep/er/x`) makes Rust refuse mid-way (exit 3): `f1`
is a file once written, so the next `remove_file(f1/deep/er/x)` is `ENOTDIR`, which is not `NotFound`. cancho refuses at the same step (`io` 20) and leaves the same tree.
**Known differences, as the header of `vcsworktree.cho` says:** the temporary file's place (W3); a name over 242 bytes cannot have one (`io` 36 on Linux, **read, not run there**; on Darwin the kernel refuses the temporary name of a 243-byte one with `ENAMETOOLONG` = 63, **run at the kernel level only**, §8; whether `vcsworktree` says `io` 63 was not run); the
umask applies to the mode of a new file (the same under `022`, which is all the script runs); `remove_path` through a link as a directory is `io`.
**Not done:** a crash between the write and the rename; a command that calls them (slice 4); the script is not part of `scripts/gate.sh` or CI.

## 7. For the session on a Mac **[choice]**

Everything above ran on Linux x86-64 only. A session on Apple silicon has three jobs, in this order; none needs more than what is already in `main`.

1. **Run the five scripts there**, each as `sh examples/selfhost/NAME_check.sh target/release/cancho` after `cargo build --release -p cancho`: `vcsstatus` (31 checks),
   `vcsrepo` (31), `vcsstore` (26), `vcslog` (51) and `vcsworktree` (27). They use only `find`, `sort`, `cksum`, `readlink`, `cp -a`, `head -c` and `mktemp -d`; if one is
   missing a flag on macOS, fix the script, not the check. A failure there is the first measurement of Darwin for these modules.
2. **(Done, §8.) Measure what §9 of `directory-links.md` and §5 above list as not measured**: the value of `AT_REMOVEDIR` and `ENOTEMPTY` and the `ELOOP` of `dir_enter` on a link (the
   `directory_rmdir` and `directory_links` command tests pin them: run `scripts/gate.sh`), renaming over an existing file and over a link with `dir_rename`, and the
   longest name that can have a `NAME.cancho-tmp-0` beside it (255 minus 13 on Linux; **read, not run** on either kernel). Correct this document and `directory-links.md`
   in place where a claim is false.
3. **R3 of `vcs-routing.md`, the reseed**: `cargo build --release -p cancho`, then `scripts/reseed.sh --cancho0 target/release/cancho --out reseed-out`, which writes
   `seed/aarch64-apple-darwin.ll.gz`; run `scripts/reseed.sh --out reseed-out2` again from the seed alone and `cmp` the file with the first (`.github/workflows/reseed.yml` does
   exactly this on Linux, and can be dispatched for the two Linux seeds). One pull request that changes only `seed/`; check 2 of `seed.md` §5 must be green before anything that
   uses `dir_readlink` (R4: `status` through `vcsstatus.cho`) is routed.

## 8. Darwin (2026-11-11, macOS 26.2, Apple silicon, `main` at 51524d20)

**The five scripts, each `sh examples/selfhost/NAME_check.sh target/release/cancho`, no script changed:** `vcsstatus` 31 ok, `vcsrepo` 31, `vcsstore` 26, `vcslog` 51, `vcsworktree` 27, all exit 0 (the counts of §7). None needed a BSD flag fixed.
**`tests/commands/vcs_repo.sh`: 81 of 81 with the standalone `cancho` that `scripts/gate.sh` builds from the committed seed**, output identical to `vcs_repo.expected`. Run with the Rust `target/release/cancho` it
prints 4 `FAILS` lines (`log --path`, `show --patch` exit 0, not 2), because the Rust command implements them: the script tests the command line in cancho, which is what `$1` has to be.
**The kernel side** (`directory-links.md` §10, `probe.c`, `probe2.c`): `AT_REMOVEDIR` 0x80, `ENOTEMPTY` 66, `ENAMETOOLONG` 63, `ELOOP` 62; renaming a file over a file, over a link (to a file, to a directory, dangling) replaces it and follows
nothing; the longest name that can have a `NAME.cancho-tmp-0` beside it is **242 bytes** on APFS (run: 243 makes `NAME`, and the 13-byte suffix is refused with `ENAMETOOLONG`). **A claim of this document that was false on Darwin:** none
here; the ones that were are in `directory-links.md` §10 (`dir_enter` on a link is `ENOTDIR`, not `ELOOP`; a link target is at most 1,023 bytes, not 4,095).
