# Removing a directory and making and reading a link: `dir_rmdir`, `dir_symlink`, `dir_readlink`

Status: **all three builtins built, §8** (slice 1 `dir_rmdir`, slice 2 `dir_symlink` and `dir_readlink`); slice 3, the consumers, is not. Written before the code, as `CONTRIBUTING.md` asks. It answers #684 (directory removal) and #685 (symlink and readlink),
the two primitives that `docs/vcs-native.md` §7 found to stop seven of the fifteen unported `cancho vcs` commands (`status`, `diff`, `record`, `switch`, `restore`,
`revert`, `apply`). Every claim is marked **[measured]** (run for this document, `docs/assets/directory-links/`), **[read]** (taken from the named file, not run) or
**[choice]** (a decision this document proposes). The owner took the recommendation of each open decision in §7 (D1, D2, D3) on 2026-11-10; §8 records what that meant for slice 1.

## 1. Why, and the askers

`cancho vcs` V1 does three things that a cancho program cannot do today **[read]**: `remove_path` removes the directories a deleted file leaves empty
(`cancho-vcs/src/repo.rs:410`), `scan` reads a link's target (`repo.rs:296`), and `materialize` makes a link (`repo.rs:375`). Without them the compiler in cancho
refuses a working copy that holds a link (`vcsscan.cho:12`, status 30) and cannot finish `record`, `switch`, `restore`, `revert` or `apply`.

**The two-askers rule (`docs/standard-library.md` §5) is not met for either primitive, and this document says so.** The asker is `cancho vcs` (one program, seven
commands). A second asker was looked for and **not found**: `cancho-tools`' `archive --op extract` extracts tar, whose entries can be links, but the document did not
read that program for it; `cancho-forge` reads links defensively and never makes one (`docs/vcs-native.md` §4). The issues #684 and #685 keep saying "one asker". The owner
decides whether the removal of Rust is reason enough (§7, D1).

## 2. What the kernel does **[measured]**

`docs/assets/directory-links/probe.c`, output in `probe-linux.txt` (Linux 6.18, x86-64, one machine). Darwin was **not measured**; where this document gives a Darwin
value it says so.

| call | answer |
|---|---|
| `unlinkat(dir, "empty", AT_REMOVEDIR)` | 0 |
| the same on a directory with a file in it | -1, `ENOTEMPTY` (39) |
| on a missing name | -1, `ENOENT` (2) |
| on a link to a directory, a dangling link, a regular file | -1, `ENOTDIR` (20) each: **it never follows a link and never removes one** |
| `symlinkat("", dir, name)` | -1, `ENOENT` |
| `symlinkat(target, dir, "taken")` and onto a dangling link | -1, `EEXIST` (17) both: it never replaces and does not follow |
| a 4,095-byte target / a 4,096-byte target | 0 / -1, `ENAMETOOLONG` (36). **Linux's limit; Darwin's is lower:** the first run of slice 2 on a macOS runner refused a 4,095-byte target with `ENAMETOOLONG` (not measured: where its limit is) |
| `readlinkat` of a 7-byte target into 8 / 7 / 4 bytes | 7 / 7 / **4**: it cuts the target silently and does not say so |
| `readlinkat` of a regular file or a directory | -1, `EINVAL` (22) |
| `readlinkat` of a missing name; into 0 bytes | -1, `ENOENT`; -1, `EINVAL` |
| `openat(.., O_NOFOLLOW)` on a link (what `dir_enter` and `dir_open_read` do) | -1, `ELOOP` (40) |

Two of these shape the design: a removal that cannot remove a link (so `dir_rmdir` cannot be used to delete what `dir_remove` should), and a read that **cuts
without saying so** (so `dir_readlink` needs a rule of its own, §4).

## 3. `dir_rmdir`

```
dir_rmdir(dir: &Dir, name: &[byte]) -> [dir_write] Done        // unlinkat(dir, name, AT_REMOVEDIR)
```

* **The shape of `dir_remove`, with one flag.** `dir_remove` is `unlinkat(dir, name, 0)` (`dirs.c:101` **[read]**); this is the same call with `AT_REMOVEDIR`
  (`0x200` on Linux **[measured]**; `0x80` on Darwin **[read]**, from the system header, not run). Everything `directory-handles.md` §2 says of a name holds: one
  component, `EINVAL` with no call for an empty name, `.`, `..`, a `/`, a NUL or one longer than `NAME_MAX`.
* **Only an empty directory, and only a directory.** A non-empty one is the kernel's `ENOTEMPTY` (a value, 39 on Linux; Darwin's number **not measured**). A link, a file
  and a missing name are `ENOTDIR` or `ENOENT` as §2 shows: the caller who wants a link gone calls `dir_remove`.
* **No recursive form.** `rm -r` is a loop of `dir_list`, `dir_enter`, `dir_remove` and `dir_rmdir` in the caller (`std.dirs` is where a helper would live), one
  component at a time, which is where the policy for what to do with a link belongs. `remove_path` of the Rust V1 is that loop upward: remove the file, then each parent
  that is empty, stopping at the first that is not.
* **Label `dir_write`**, like `dir_remove` and `dir_mkdir`: it changes what is beneath the directory the program holds, and reaches nothing else.

## 4. `dir_symlink` and `dir_readlink`

```
dir_symlink(dir: &Dir, name: &[byte], target: &[byte]) -> [dir_write] Done     // symlinkat(target, dir, name)
dir_readlink(dir: &Dir, name: &[byte], out: &![byte]) -> [dir_read] Linked     // readlinkat(dir, name, out, len(out))
```

**`dir_symlink`.**

* `name` is checked as everywhere (one component). **`target` is not a component:** it is the text of the link, any bytes but NUL, up to 4,095 bytes **[measured]**; an
  empty one or a NUL is `EINVAL` with no call (the kernel's answer for empty is `ENOENT` **[measured]**, which would say a file is missing when it is the argument that
  is wrong, so it is refused first, as `dir_mkdir` refuses a mode before the call). **Corrected in place (slice 2):** this said 4,096 bytes or more is `ENAMETOOLONG`, the kernel's
  answer. All three implementations copy the target into a buffer of 4,096 bytes to give `symlinkat` its terminator, so a target of 4,096 or more is `EINVAL` with no call, like an
  empty one; the longest target is still 4,095 bytes, as measured.
* It **never replaces and never follows**: a name that exists, whatever it is, a dangling link included, is `EEXIST` **[measured]**.
* **A link can point anywhere**, including outside everything the program holds. The link is made; no access is granted by making it. What is checked is the *use*:
  `dir_enter` and `dir_open_read` open with `O_NOFOLLOW` and refuse a link (`ELOOP`, §2), so a handle never leaves the directory through one.
* **Where this is not true, and the decision it forces (§7, D2).** A path-based operation (`open_read(fs, "/prefix/dir/link/file")`) checks the path **as text** against the
  prefix (`filesystem.md` §4) and the kernel then follows the link in it. Today a program cannot make a link, so it cannot plant one; with `dir_symlink` a program that holds
  both a `Dir` and an `Fs` can make a link inside the prefix that leads out of it and then read through the `Fs`. That is a new way to widen an `Fs` that no builtin gives
  today. It is **not** new for a link that already exists (a program given a directory holding one reads through it the same way), and it was **not measured** whether the
  kernel's following can be refused by the existing path operations (`O_NOFOLLOW` on the last component only).

**`dir_readlink`.**

* The target goes **into the caller's buffer** (`&![byte]`), as `dir_next` puts a name there (`directory-listing.md` §3): the callee allocates nothing, and the caller
  chooses the bound.
* **A cut is refused, not returned.** The kernel cuts silently **[measured]**: a 7-byte target into 4 bytes answers 4. An answer equal to the buffer's length may be a
  target of exactly that length or a longer one, and a caller cannot tell. The rule, taken from `dir_next` ("`ENAMETOOLONG` rather than a cut name"): **if the kernel
  fills the buffer, the answer is `Failed(ENAMETOOLONG)`.** A caller needs a buffer one byte longer than the target; 4,096 always suffices, since no target is longer than 4,095
  **[measured]**. A zero-length buffer is `EINVAL` with no call, as the kernel says **[measured]**.
* **The answer is `Done::Ok(n)`** (the target is the first `n` bytes of the buffer; `n >= 1`, since an empty target cannot be made) **or `Done::Failed(errno)`**
  (`ENOENT`, `EINVAL` for a file or a directory **[measured]**, `ENAMETOOLONG` as above, `EACCES`). **Corrected in place (slice 2):** this proposed a new prelude enum `Linked`, as
  `DirStat` is. `Done` has the shape (`Ok(int)` and `Failed(int)`, the three numbers `dirrt_triple` already answers) and `dir_mode` already answers a number in it, so no type, no `Split`
  change and no edition-wide table row was needed.
* **Label `dir_read`**, like `dir_stat`. It reads the text of a link and follows nothing: the target it answers is data, and a program that opens it opens it by a path
  its own `Fs` allows or not.

**What neither does.** No `lstat`-style call is added: `dir_stat` already answers a link as a link (`AT_SYMLINK_NOFOLLOW`, `directory-listing.md` §3.5). No `realpath`: see §6.

## 5. Where they go, and what each costs **[read]**

**Edition: the latest open one (12 today)** **[choice]**, for the reason `editions.md` §6.4 gives for every builtin added since edition 7: they are names a program may declare
for itself, and the earlier editions are closed. No `Split` field changes, so no new edition is needed (`editions.md` §6.2). To confirm against the file before code.

**In the compiler in cancho** (`dir_mkdir` is the pattern): `examples/selfhost/runtime/dirs.c` (a function each, as `cancho_dir_mkdir` is), the regenerated `dirs_linux.ll`,
`dirs_darwin.ll` and `runtime.cho` (`runtime/make.sh`), `codes.cho` (the call numbers: 139 and 140 are `dir_mkdir` and `dir_open_new_mode`), `netcode.cho` (the runtime
function each calls), and the type of `Linked`.

**In Rust, the part that has no way around it.** `examples/selfhost/tables.cho` is **generated** from `cancho-ir` by `selfhost_tables` (`cancho-ir/src/tests/selfhost_tables.rs`,
`UPDATE_SELFHOST_TABLES=1`) **[read]**: the builtin's name, edition, label and signature are read from `builtin.rs` and `builtin_signature.rs`. So a new builtin is, today, a
registration in `cancho-ir` (the enum variant, its name, its edition at `builtin.rs:1498`'s kind of table, its label, its signature) whether or not the Rust compiler ever
lowers it; for `dir_mkdir` that is 7 sites in `cancho-ir`, plus `target.rs`'s WASI table, plus the two code generators (the LLVM one and the Cranelift one, each with a `dirs.rs` and an `expr.rs`) **[read, counted by grep]**.
Two ways, and the decision is §7 D3:

1. *Register only, lower nothing in Rust.* A program using the builtin builds with the compiler in cancho (the default) and is refused or fails with `CANCHO_COMPILER=rust`.
   About a dozen lines of data in `cancho-ir`, no new Rust logic. **Not measured:** what the Rust compiler does with a registered builtin it has no lowering for.
2. *Stop generating the table from Rust first* (`#487`: "checked-in artifacts are the source of truth"): `tables.cho` becomes the source and the Rust test goes. Larger, and
   it is work the removal needs anyway, but it is not this note's.

**Not in Rust: the WebAssembly table.** WebAssembly is retired with the Rust compiler (`removing-rust.md` D2, amended 2026-11-10), so the three builtins are **not** given a WASI
lowering. `target.rs` refuses a builtin a target cannot do; whether a builtin with no row is refused or accepted on WASI is **not measured**, and slice 1 reads it.

## 6. What this does not decide

* **`realpath`.** `docs/vcs-native.md` §7 item 4: the Rust `vcs` calls `fs::canonicalize` in three places. That is a different primitive (it resolves links in a path
  the program names, which `dir_enter` is built never to do) with its own authority question, and it is **not designed here**. #686 is where it belongs.
* **`exit` at a point and the process id** (#687, #688): §7 of `vcs-native.md` says the first is probably not needed and the second is cosmetic.
* **Recursion and `mkdir -p` style helpers.** They are `std.dirs` functions, and a slice of their own once these three builtins exist.

## 7. Decisions for the owner

* **D1. Build these on one asker?** The rule asks two; there is one, and removing Rust is the reason. *Recommendation:* yes, with the document saying so in the commit,
  because the alternative is to keep `crates/cancho-vcs` alive for two syscalls.
* **D2. The label of `dir_symlink`.** *(a)* `dir_write` (§4): the simplest, and it lets a program that holds a `Dir` and an `Fs` plant a link that leaves the prefix, with
  nothing in `cancho authority` to say it could. *(b)* A label of its own, `dir_symlink`, so a report shows who can make a link; it costs a label (the authority tables, the
  budget, `capability-budget.md`) and the measurement of what a program needs it for. *Recommendation:* (b). The point of the authority report is that a link a program
  can make is not hidden under a label that also means "write a file".
* **D3. The Rust side.** Option 1 or option 2 of §5. *Recommendation:* option 1 for these three, so nothing waits, and the work of option 2 under #487; with a test that
  pins what the Rust compiler does with the three (refuses, in the best case) so the difference is recorded and not found.
* **D4. The names.** `dir_rmdir`, `dir_symlink`, `dir_readlink` follow `dir_remove`, `dir_mkdir`, `dir_stat`. Nothing else is proposed.

## 8. Slices and what each is checked by **[choice]**

1. **`dir_rmdir`.** The smallest: the shape of `dir_remove`. The fixture `tests/commands/directory_rmdir.cho` (a command test as the others in `tests/commands/`): an empty
   directory goes; a non-empty one is `ENOTEMPTY` and stays; a link, a file and a missing name give §2's answers and change nothing; `..`, `.`, an empty name, `a/b` and a NUL are
   `EINVAL` with no call; the authority row says `dir_write` and not `fs_*`. Reject fixtures `dir_rmdir_is_edition_NN.cho` and `dir_rmdir_not_declared.cho`. **Mutant:** the
   flag dropped (it then answers `EISDIR`/`EPERM` on Linux, not measured here), and the kernel's check on the name removed.
2. **`dir_symlink` and `dir_readlink`.** Make-then-read for targets of 1, 7, 255, 4,095 bytes; a taken name and a dangling link; 4,096 and empty and NUL; the buffer of exactly the
   target's length is `ENAMETOOLONG` and one longer is the target; a file and a directory are `EINVAL`; and `dir_enter` and `dir_open_read` still refuse the link they made
   (`ELOOP`). The decision of D2 pinned by an authority fixture. **Mutant:** the fill check dropped (a cut target comes back as if whole).
3. **The consumers.** `vcsscan.cho` stops answering status 30 and reads the target; `vcsrepo`'s `materialize` and `remove_path`; `a_binary_file_is_a_snapshot_and_a_symlink_is_a_link`
   of the pinned tests, checked against a store the Rust command wrote.

Every slice is a pull request with the gate of `docs/gate.md` (`scripts/gate.sh`), and the three of `CLAUDE.md` while they still stand.

**Slice 1, built and measured (2026-11-10, Linux x86-64).** `dir_rmdir` is edition 12, label `dir_write`, `Done`. In the compiler in cancho: `runtime/dirs.c` (`cancho_dir_rmdir`, `AT_REMOVEDIR` by `#if DARWIN`, the `.ll` regenerated: 512 in the Linux text and 128 in the Darwin text, and nothing else in either changed), call number 145 in `codes.cho`, `netcode.cho` at each site that names `dir_remove`, and the generated tables. The same program (`tests/programs/directory_rmdir_probe.cho`) gives the **same transcript** built by the compiler in cancho and by the Rust compiler: an empty directory goes; a directory with a file in it is `ENOTEMPTY` and keeps the file, and goes once it is empty; a missing name is `ENOENT`; a file, a link to a directory and a dangling link are `ENOTDIR`, none is removed or followed; `..`, `.`, an empty name, `a/b` and a 256-byte name are `EINVAL` with no call (`a/b` is still there); a 255-byte name reaches the kernel (`ENOENT`). `tests/commands/directory_rmdir.cho` pins it, and `tests/reject/dir_rmdir_is_edition_twelve.cho` and `dir_rmdir_not_declared.cho` the edition and the label. **Mutant:** pointing `dir_rmdir` at `dir_remove`'s runtime function (the flag dropped) fails the test. **Not run:** the second mutant (the name check removed), Darwin, and the NUL case (an argument cannot hold one).

**D3 as built, and where it differed from §5.** Registering the builtin in `cancho-ir` and generating `tables.cho` was done as §5 says. "Lower nothing in Rust" was **not** possible: the Cranelift code generator's `match` over builtins is exhaustive, so a registered builtin with no arm is a compile error, and the arm that would have answered it was an `unreachable!`, a panic that a program could reach. Both Rust backends therefore lower `dir_rmdir` (about twenty lines each, `dir_remove` with the flag, and `cancho_ir::at_removedir(darwin)` for the number). Registering also moved four numbers that tests pin: `Builtin::ALL` is 185 and the WASI-supported count 94 (`os_tables.rs`, `docs/wasm.md`), and `tests/builtins.golden` has the new name.

**Slice 2, built and measured (2026-11-10, Linux x86-64).** `dir_symlink(dir, name, target) -> [dir_symlink] Done` and `dir_readlink(dir, name, buffer) -> [dir_read] Done`, edition 12.
**D2 as built:** a label of its own, `dir_symlink`, in `WORLD_PLAIN_LABELS`, in what a `Dir` discharges (so a function that holds a `&Dir` may declare it, as it may `dir_write`), and in the
WASI import table (`path_symlink`; `dir_read` gains `path_readlink`). `dir_readlink` performs `dir_read`. In the compiler in cancho: `dirs.c` (`cancho_dir_symlink`,
`cancho_dir_readlink`), codes 146 and 147, `netcode.cho`, the `.ll` and `runtime.cho`, and the generated tables. In Rust, both backends lower them (`cancho_ir::LINK_MAX`, and the fill check
done after the shared helper, in five instructions). **Three implementations give one transcript:** the compiler in cancho, the Rust compiler with the LLVM backend and with the Cranelift
backend, on `tests/programs/directory_links_probe.cho` and `tests/commands/directory_links.sh`. It pins §4: targets of 1, 7, 255 and 1,000 bytes are made as links of those lengths and read back
(the shell's `readlink` agrees), and a 4,095-byte one is either made and read back whole or `ENAMETOOLONG` (Linux makes it; **the first CI run on macOS did not, and the test claimed 4,095 for every
kernel: corrected, §2**); a buffer of exactly the target's length is `ENAMETOOLONG` and one byte longer is the target, and so are a cut by 3 and a buffer of 1; an empty buffer is `EINVAL`;
a taken name, a file, a directory and a dangling link are `EEXIST` and untouched, and the dangling link was not followed; an empty target and a 4,096-byte one are `EINVAL` and no link is made;
`..`, `.`, an empty name and `a/b` are `EINVAL` for both calls; a file and a directory are `EINVAL` to read and a missing name is `ENOENT`. The label is pinned by
`tests/reject/dir_symlink_not_declared.cho` (a row of `[dir_write]` does not make a link: `effect-not-declared`), `dir_readlink_not_declared.cho` and `dir_symlink_is_edition_twelve.cho`.
**Mutant:** the fill check dropped in `dirs.c` (the comparison made unsatisfiable) fails the test, and the files were restored byte for byte. **Not run:** the mutant of the target's
NUL check, Darwin, and a NUL in a target or a name (an argument cannot hold one). **Not checked yet:** that `dir_enter` and `dir_open_read` still refuse the link a program made (`ELOOP`,
§2); they are unchanged and §2 measured the kernel, but no test of this slice makes the link and then enters it.

**Slice 3a, the scan (2026-11-11, Linux x86-64).** `vcsscan.cho` (edition 12 now, for `dir_readlink`) makes a link a tree line of mode `120000` whose blob is the target text, with the
Rust `scan`'s refusals: a target that is not UTF-8 is `path-unsupported` (31), and `vcsobj.check_path` applies to the link's path. Status 30 (`symlink`) is no longer answered. A buffer of 4,096
bytes is never filled (a target is at most 4,095), so the fill check of `dir_readlink` cannot refuse a real link. `examples/selfhost/vcsstatus_check.sh` replaces its one "a link is refused" case
with twelve states of a working copy with links, compared byte for byte with the Rust `cancho vcs status`: a new link, recorded and clean, a changed target, a link to a directory, a dangling link,
a relative target, a link below a directory, a 500-byte target, a link replaced by a file and back, a file replaced by a link, a link to a directory replaced by a directory, a deleted dangling link,
`.canchoignore` skipping a link, and a target that is not UTF-8 (`path-unsupported` from both). All 31 checks of the script pass. **Mutant:** the mode written as `100644` fails nine of them.
**Not done in 3a:** `materialize` and `remove_path` (the writes of `switch`, `restore`, `revert`, `apply` and `record`'s deletions): slice 3b, built since in `vcsworktree.cho` ([`vcs-worktree.md`](vcs-worktree.md)); nothing calls them yet. `vcsstatus_check.sh` is not part of `scripts/gate.sh` or CI; it was run by hand.

## 9. Not measured

Darwin: the value of `AT_REMOVEDIR`, `ENOTEMPTY`, `ELOOP` and the behaviour of every row of §2. A second kernel. What the Rust compiler does with a registered builtin it does not lower (§5).
Whether a path-based `Fs` operation can be made to refuse a link planted inside its prefix (§4). Time: no cost was measured, because none of these is on a hot path.
