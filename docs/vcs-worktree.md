# Writing the working copy from cancho: `materialize` and `remove_path` (slice 3b)

Status: **design, nothing built** (2026-11-11). Claims are marked **[read]** (from the source), **[measured]** (run) or **[choice]** (a decision). This is slice 3b of
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
convention (0 ok, 10 io with the errno, `path-unsupported` for `check_path`):

```
materialize(heap, root: &Dir, vcs: &Dir, mode: int, path, blob_id)   // mode 0 file, 1 executable, 2 link, as in vcsobj
remove_path(root: &Dir, path)
```

It is not in `cancho.files` (nothing routes to it yet), exactly as `vcsscan.cho` was in 3a, so no compiler, seed or release file changes, and no reseed.

**W2. Obstacles and parents, one component at a time.** Walk `path` with `dir_enter`; at each component `dir_stat` (no follow): a missing one is `dir_mkdir` `0755`; a
directory is entered; **anything else (file, link, dangling link) is `dir_remove`d and replaced by a directory**, as Rust does. At the leaf: a directory is removed
recursively by a private loop of `dir_list`, `dir_enter`, `dir_remove`, `dir_rmdir` (§2 of `directory-links.md` already puts this loop in the caller; it is **not** added to
`std.dirs`: one asker, the two-askers rule). The loop never follows a link and never leaves the directory it is in.

**W3. Writing a file: where the temporary file lives. Owner decision.** `dir_rename` cannot cross directories, so Rust's `.cancho/vcs/tmp` → destination is not available.
Options:

* **(a) Recommended.** A temporary `NAME.cancho-tmp-0` **beside** the destination, written with `dir_open_new_mode` (so the mode is right from the first byte), then
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
* Darwin (`AT_REMOVEDIR`, `ENOTEMPTY`, the rename-over behaviour); crash windows (no test kills the process between the write and the rename; `vcs-native.md` §6 has the
  harness for it); the cost of a switch of many thousands of files.
