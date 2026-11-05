# `cancho vcs` in the cancho-written compiler: what std has, what is missing

> **Status: slice 1 of #680 (the primitives audit); no code yet.** Written before the code (`CONTRIBUTING.md`).
> *(read)* is from the source or a design document; *(measured)* was run on 2026-11-05 on macOS arm64 with the
> compiler of `main` (c7435d6e). The rule behind it: new features are written in `examples/selfhost`, the Rust compiler is
> being deprecated, so **no new Rust**, and a missing primitive stops the work with an issue instead of a fallback to Rust.
> The Rust V1 (`crates/cancho-vcs`, `crates/cancho/src/vcs_*.rs`) and its pinned JSON (`crates/cancho/tests/vcs_pins/`)
> are the oracle. A Rust V2 (merge, `resolve`, `replay`, a gate that resolves modules outside the tree, deflate and delta
> packs; branch `claude/vcs-v2-db1c7e`, PR #682, closed unmerged) exists only as a **reference for algorithms and
> measurements**: its R3 numbers (82 historical merges, 700 random pairs, against `git merge-tree`) are the oracle for the
> native merge.

## 1. What the store, refs and merge need, against what exists

| need | exists? | where *(read)* |
|---|---|---|
| Open a directory beneath a path; step into a child without following links | yes | `open_dir`, `dir_enter`, `dir_open_read` (`directory-handles.md`) |
| Create a file exclusively (lock files, temp files) with a mode (`0o644`/`0o755`) | yes | `dir_open_new`, `dir_open_new_mode` (edition 9) |
| Write, append, positional read/write, truncate, size | yes | `file_write`, `file_pread`, `file_pwrite`, `file_truncate`, `file_size` (`file-writes.md`) |
| `fsync` a file and a directory | yes | `file_sync`, `dir_sync` |
| Atomic replace by rename; rename that refuses to replace | yes | `dir_rename`, `dir_rename_new` (edition 7) |
| Remove a file | yes | `dir_remove` (`unlinkat(dir, name, 0)`) |
| Make a directory | yes | `dir_mkdir` (edition 9) |
| List a directory, sorted bytewise; kind, size, mtime without opening; mode bits (the exec bit) | yes | `std.dirs.list`, `dir_stat`, `dir_mode`, `dir_own_mode` (`directory-listing.md` §3.5) |
| Advisory lock | yes | `file_lock` |
| The clock | yes | `clock_unix_ms` (needs `Clock`) |
| BLAKE3, SHA-256/384/512, HMAC, Ed25519 | yes | `examples/selfhost/blake3.cho`; `std.crypto` (`sha256`, `sha512`), `std.hmac`, `std.ed25519` |
| Parse JSON | yes | `std.json` (a tape; no writer) |
| Write JSON as `serde_json::to_vec_pretty` | yes, selfhost | `examples/selfhost/jsonwrite.cho` (the `vcs` files, `cli-in-cancho.md` §28) |
| Parse, check, identify, format a program held in memory | yes, but as whole-process modes | `driver.drive(world, mode, ..)` reads the program as a stream on standard input (`check_files.cho`, `ids.cho`, `clifmt.cho`) |
| Arguments | yes | the `Args` capability; the CLI is `selfhost.cli*`, `clivcs.cho` is the publish side |
| Run another program (the gate does not need one) | yes | `Exec` |
| **SHA-1** (git object ids, V1's `import-git`/`export-git`) | **no** | a library, `examples/selfhost/sha1.cho`; no compiler change |
| **zlib inflate** (reading git objects) | **no** | a library: `std.flate` (#691) |
| **zlib deflate** (git export writes stored blocks; the store may compress) | **no** | a library: `std.flate` (#691), which compresses; stored blocks first, LZ77 and Huffman when R4 asks |
| Histogram diff, anchored edits, diff3, declaration merge, canonical JSON of a change | **no** | libraries, the Rust V1/V2 as the algorithm |
| Remove a directory | **no** | `dir_remove` is `unlinkat(.., 0)`; an empty directory cannot be removed (`directory-handles.md` §2) |
| Create or read a symbolic link | **no** | none of `symlink`/`readlink` is a builtin (`Builtin` table, 185 entries, read) |
| The current directory as an absolute path | **no** | `cliproject.cho` runs `/bin/pwd` with `Exec` to get it |
| Stop the process at a chosen point | **no** | no `exit`/`abort` builtin (read) |
| The process id | **no** | none (read) |
| An environment variable | **no, by design** | `agent-toolbox.md` §2: "no environment, clock or tty unless handed" |

## 2. Measured

Throughput decides whether the V1 `import-git` of `cancho` (826.9 MiB of objects, `docs/vcs-cli.md` R4) is bearable in
cancho code. BLAKE3, `blake3sum` built from `examples/selfhost/blake3.cho` with the Rust compiler, 20 MB of random
bytes, standard input: **0.69 s wall, 0.40 s user (about 50 MB/s of hashing)**, which puts the 826.9 MiB at about 17 s. `std.crypto.sha256` over 20 MB in memory: **0.33 s
wall (about 60 MB/s)**.

**Slice (a), written and measured (2026-11-05, macOS aarch64, the Rust compiler's build of each program).**

| what | measured |
|---|---|
| `sha1.cho` over 20 MiB **in memory** (`sha1mem`, no I/O) | **0.087 s, about 240 MB/s** (best of 2 warm runs; the first run 0.32 s). OpenSSL's SHA-1 through Python takes 0.008 s (hardware instructions). The 826.9 MiB of the `cancho` import is about **3.6 s** of SHA-1 |
| reading standard input (`lexcore.slurp`) | **20 MB takes 0.275 s (about 73 MB/s)**; writing 20 MB with `buffer.write` is about as slow. Not investigated |
| compression | **`std.flate`** (#691, `docs/flate.md`): inflate about 410 MB/s of output, deflate within 0.9 to 4.3 % of zlib level 6 and about as fast as Python's `zlib` on 20 MB of text |

**Corrected in place (2026-11-05).** The first version of this table said SHA-1 ran at about 57 MB/s and inflate at about 44 MB/s. Those were the whole
run of a program that reads 20 MB (or 6 MB) from standard input and writes 20 MB, and the read alone is 0.275 s for 20 MB; the hash and the inflate
were 4 and 5 times faster than the table said. The first inflate written for this port (`puff.c`, a bit at a time, 215 MB/s) was also half as fast as
the one cancho-tools' `archive` already had (410 MB/s, a table for the short codes, plus a compressor): it was dropped, and `archive.flate` moved into
`std` as `std.flate`, with a zlib layer, because `cancho vcs` is its second asker (`docs/flate.md` §1).

Correctness of `sha1.cho`, against independent implementations: `hashlib` for 19 sizes from 0 to 1,000,003 bytes (every size around a block: 55, 56,
57, 63, 64, 65, 119, 120, 121, 127, 128, 129), and a table of the lengths 0 to 130, each whole and cut at every point, in `sha1_test.cho`, with the
FIPS 180-4 vectors and a git blob (`ce01362...`). A mutant (a round constant) fails all four tests. It passes under the compiler in cancho alone
(`CANCHO_COMPILER=auto`). `examples/selfhost/test.sh` runs the tests.

## 3. What the missing primitives block, and what does not

* **Not blocked, because it is a library:** SHA-1, inflate, deflate, the diffs and merges, the JSON of a change, the
  object store, trees, refs (a lock file is `dir_open_new`, the compare-and-swap is a read, a rename under it).
* **Blocked, with an issue each (section 4):** `rmdir` (V1's `remove_path` deletes the directories a delete leaves
  empty: the `restore` and `switch` pins), `symlink` and `readlink` (a tree entry of mode `120000` is a link: the
  `a_binary_file_is_a_snapshot_and_a_symlink_is_a_link` test, `scan`, `materialize`, git bridge), the absolute current
  directory (the `root` in `init`'s JSON and every pin; today `/bin/pwd` through `Exec`, which is a program run to
  read what the system could answer, against the rule of this work), `exit` at a chosen point (the crash test kills
  the process at every write: without it the only way out is to trap on purpose, which is a signal and not
  an exit code), and the process id (the `lock-held` message and `unlock`).
* **Deviations that need no primitive, said so the oracle is not misread:** the Rust CLI reads `CANCHO_AUTHOR` and
  `CANCHO_VCS_CRASH_AT` from the environment; cancho reads none, so the native CLI takes `--author` (it already does) and a
  hidden `--crash-at <n>`, and the tests of `vcs_repo.rs` pass them as arguments. Nothing else in a pin depends on the environment.
* **The gate is a refactor, not a primitive.** The compiler in cancho checks a program as a process that reads it from
  standard input (`driver.drive`). A gate inside `vcs record` needs the same check as a function over in-memory
  files; running `cancho check` as a child process of itself would work today and is the one thing here that is
  still a program started, so the refactor of `driver` into a callable module comes before the gate (slice 5).

## 4. Issues opened for the missing primitives

Per `docs/standard-library.md` §5 a feature needs two askers, counted by reading programs. For each, the askers found:

| primitive | asker 1 | asker 2 | verdict |
|---|---|---|---|
| directory removal (`dir_rmdir`) | `cancho vcs` `remove_path` | not found yet | one asker; the issue says so |
| `symlink`, `readlink` | `cancho vcs` (mode `120000`) | `cancho-forge` *reads* links defensively (`evidence_io.cho`) and never makes or reads one by name; not an asker | one asker |
| absolute current directory | `cancho vcs` (`root`) | `selfhost.cliproject` (runs `/bin/pwd`) | **two** |
| `exit(n)` / abort at a point | `cancho vcs` crash test | the design cites forge's `tests/conformance/crash_*`; not read | one, a second unverified |
| process id | `cancho vcs` locks | not found | one |

Issues: directory removal [#684](https://github.com/alpibrusl/cancho/issues/684), symlink/readlink [#685](https://github.com/alpibrusl/cancho/issues/685), absolute cwd [#686](https://github.com/alpibrusl/cancho/issues/686), exit at a point [#687](https://github.com/alpibrusl/cancho/issues/687), process id [#688](https://github.com/alpibrusl/cancho/issues/688).

## 5. What is built next, in order, as far as this allows

1. **This audit** (#683, merged).
2. **Slice (a), built:** `sha1.cho` (#690) and `std.flate` (inflate, deflate, gzip, zlib; #691), their tests and measurements (§2). Next: the object store, `init`, `record`, `log`, `show`, `status`, refs with
   compare-and-swap, against the V1 pins. Blocked for `status`/`restore` over directories and links until the issues above are answered.
3. Crash and race tests (blocked on `exit`, or on a deliberate trap).
4. The git bridge (SHA-1, inflate, pack idx v2, deltas; the exporter with stored blocks).
5. The gate (the `driver` refactor), then merge, `resolve`, `replay` and packs, with the Rust V2's R3 corpus as the oracle.
6. Mark the Rust `vcs` deprecated; update the selfhost parity lists.
