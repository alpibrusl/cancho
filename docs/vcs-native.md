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

## 6. Slice (b): the store, in pieces (2026-11-05)

The format V1 writes is small and exact (`crates/cancho-vcs`, read for this section): an object file `objects/<kind>/<2 hex>/<62 hex>` holds the bytes and
nothing else, named by BLAKE3 of the kind, a 0 byte and the bytes; a tree is lines `MODE BLOB PATH`; a change is one line of JSON in a fixed field order
(`tree, parents, author, time, tz`, then `intent`, `decls`, `edits`, `attestations`, `git` when present); a ref is a file of one id, with `HEAD`
(`ref: refs/heads/main`), `format` (`cancho-vcs 1`) and an append-only `refs/heads/NAME.log`. The pieces, each its own PR, each checked against stores the Rust
command wrote:

| piece | what | state |
|---|---|---|
| b1 | `vcsobj.cho`: the object id, `is_id`, `check_path`, the text of a tree (check, find, walk) | **built**, below |
| b2 | `vcschange.cho`: the change object (JSON in V1's field order, written and read) | **built**, below; refs, `HEAD` and `refs/*.log` move to b3 |
| b3 | `vcsstore.cho`: **built**, below: refs, `HEAD`, the log; the store on disk: write an object atomically (temporary name, `file_sync`, `dir_rename`, `dir_sync`), read one and re-hash it; the ref compare-and-swap under a lock file; `init` | next; needs the primitives that exist, not the five of the issues |
| b4a | `vcsrepo.cho`: HEAD and the resolution of a revision (`HEAD`, `@`, id, branch, tag, prefix, `~N`) | **built**, below |
| b4b | `vcsrepo.cho`: the history of a change; the documents of `log` and `show` (sorted-key pretty JSON), and the refusal document of `bad-revision` | **built**, below; without the `log` filters and `show --patch` |
| b4c | `status`: the working copy scanned against HEAD's tree | next; over links it needs #685, over removed directories #684 |
| b4d | `record` (snapshot of the working copy to a tree and a change, no gate yet) | next |

Not in (b): `decls` (the declaration diff of a `.cho` file) and the gate, which need `ids` and `check` as functions over in-memory files (the `driver`
refactor of §3, slice d), so a `record` that touches a `.cho` file does not reproduce V1's pins yet; the pins with only other files do.

**b1, measured.** `vcsobj.cho` against the Rust command: a repository of **387 objects** (the `std/` and `docs/` of this tree, a 70,000-byte random file, an
empty file and a one-byte file, 7,073,083 bytes) was recorded by V1; every object's name is the id `vcsobj.cho` computes from its kind and bytes (**0
mismatches**), and the only tree in it (385 lines) is accepted by `tree_status`. `vcsobj_test.cho` pins the three blobs, the tree and the change of a
three-file repository, the paths `check_path` refuses and the trees `tree_status` refuses (a short line, a mode, an id, a path, a duplicate, a file that is
also a directory, an order), every prefix and every changed byte of a tree (a trap was found there and fixed: `tree_find` sliced a short line). Five mutants
(the kind separator, the `..` test, the order, the link mode, the first-component test) each fail it. One difference from V1, on purpose, in the file's header:
an unsorted tree is `object-corrupt` here and sorted silently there.

**b2, measured.** `vcschange.cho` writes a change as `change.rs` does (serde_json: fixed field order, no spaces, `intent` parts, `decls`, `edits`, `attestations`
left out when empty, serde's escapes, UTF-8 as it is) and reads one with the checks of `Change::load`. Against the Rust command: three changes it wrote
(a model and a session; an issue and a prompt with quotes, a backslash, a tab, a newline and non-ASCII letters, two declarations and an attestation; an
anchored edit; two authors, one with a non-ASCII name) are reproduced **byte for byte from their values**, their names are the BLAKE3 ids `vcsobj.cho`
computes from them, and each field reads back. Every prefix and every changed byte of the three gives a status or an answer, never a trap. Seven mutants (the order of
two intent parts, a key, the parent bound, the `\r` before a newline, the empty-is-absent rule, the sign of a time) each fail the tests; one survived at first
(the `\r\n` subject) and a test was added. Not here: the `git` part of a change (the bridge's) and the decoded form of a string (strings are answered as written,
which is what `log` and `show` print).

**b3, measured.** `vcsstore.cho` is `object.rs`/`refs.rs` on a `Dir` handle (edition 9: `dir_enter`, `dir_mkdir`, `dir_open_new_mode`, `dir_rename_new`, `file_sync`,
`dir_sync`, `dir_open_append`, `dir_remove`), with no path spent and no program run: `init`, `put`, `get` (hashed again: `object-corrupt`, `object-missing`, `bad-id`),
`read_ref`, and `update_ref` (compare-and-swap under `NAME.lock`, the `NAME.log` line, `ref-moved` naming the winner, `lock-held` that is never broken). The tests are a
script, `examples/selfhost/vcsstore_check.sh`, because a `cancho test` is given a heap and an io and no file system; it builds `vcsstorectl.cho` and uses the Rust command
as the oracle: **26 checks, all passing** (also with the compiler in cancho alone). The Rust command recorded a store of **387 objects** (std and docs of this tree, a 70 KB random
file, an empty one, a one-byte one): `get` reads every one back byte for byte; `put` of every one into a fresh store makes an object directory that is `diff -r`-identical
(names, bytes, mode 644); `init` makes the Rust layout and the same `HEAD` and `format` bytes; the Rust `vcs log` still reads the store after cancho moved its ref; ten
racing writers on one ref give **one winner and nine refusals** (six runs). Five mutants of `vcsstore.cho` (the re-hash, the expectation, the lock, the rename that
refuses to replace, the dash of a first log line) were tried: the first three fail the checks, the fourth is equivalent (two writers of one object write the same bytes) and the
fifth survived until a check of the first log line was added.

Differences from the Rust command, on purpose (header of the file): the temporary file of an object is made beside the object, not in `tmp/`; the lock holds `0 TIME` because there
is no process id (#688); no crash failpoints (#687), so the kill-at-every-write test of V1 is not reproduced here. **Not measured:** the speed of `put`/`get` (the path is
`open`/`write`/`fsync`/`rename`, so a store of many small objects is bound by `fsync`, as the Rust one is: 94 s for the 826.9 MiB import).

**b4a, measured.** `vcsrepo.cho` resolves a revision as `repo.rs` does. `examples/selfhost/vcsrepo_check.sh` records six changes with the Rust command, a branch (`side`) and a tag (`v1`)
at the third (V1 has no `tag` command: a tag is a ref file), and compares, for 26 revisions (`HEAD`, `@`, `HEAD~1` to `~99`, `main`, `heads/main`, `main~2`, `side`, `tags/v1`, `v1`, `v1~1`, the
full id, prefixes of 8, 6 and 5 digits, a name that does not exist, empty, `HEAD~`, `HEAD~x`, `HEAD~-1`, `..`, `heads/..`, and an ambiguous prefix), the change the Rust `show REV` names
(or the refusal tag) with what `vcsrepo.resolve` answers: **31 checks, all passing**, also with the compiler in cancho alone; an empty repository has no `HEAD` change in both. The check
first asserts that the branch and the tag are not at HEAD, so a comparison of two refusals cannot pass for the wrong reason (an earlier version of the script used a `vcs tag` command that
does not exist and passed vacuously). Four mutants (the prefix length, the `~` count, the order of `heads/` and `tags/`, the `@` alias) fail it; the fifth (status 19 for 18) is the same refusal
tag, which is all a command prints. The prefix search reads one fan-out directory; the ambiguous case was fabricated with two empty files named like changes.

**b4b, measured.** `vcsrepo.cho` walks the history as `repo.rs` does (the greatest `(time, id)` of what has been reached, a parent only once its child was taken) and prints the `log` and `show`
documents as `serde_json::to_string_pretty` prints them: keys in byte order, `[]` for an empty array, a refusal as `{command, ok: false, refused: {candidates?, message, tag}}`. `examples/selfhost/vcslog_check.sh`
builds five repositories with the Rust command and compares standard output **and exit status, byte for byte**, with `vcsstorectl log|show`: **51 checks, all passing**, also with the compiler in cancho alone.
The repositories: (A) four changes with declarations, an anchored edit, a gate attestation, a model, a session, an issue, a prompt with quotes, a backslash, a tab, a newline and non-ASCII letters, an empty
model, two authors, a mode change, a deletion and an edit; (B) a history that forks, read from the branch, from `HEAD~n` and with `--limit`; (C) one change that adds about 400 files and a 70 KB one (`show` is 7,561
lines); (D) an empty repository; (E) a history **with a merge**, imported by the Rust command from a git repository (git only made the fixture: times 1, 3, 5, 9, 10), because only a change with two parents
makes the order of the history depend on `(time, id)`: a chain and a fork never have more than one change waiting. The refusals compared are an unknown name, an ambiguous prefix with its candidates, `HEAD~99` and `HEAD~x`.
Six mutants (the order of the frontier, a state name, the `--limit 0` rule, an intent key, the indentation of an entry, the order of a tree diff) each fail it; the frontier one passed until scenario E was added.
`log --limit 0` lists one change, as the Rust command does (it stops after pushing a row).

One bug found by the check and fixed: `show` parsed the change into its scratch space twice but read it once, and answered from zeros (a trap). It showed as exit 132 on every `show` of a change with a tree, and
a debugging harness of mine had also trapped on a relative path through `..`, which an `Fs` traps on (the checks pass absolute paths). **Not done:** `log --path|--intent|--session|--issue|--author`, `show --patch`, the
refusals other than `bad-revision` as documents (they are printed as `refused TAG`); the history keeps its set of reached changes as a list (quadratic; not measured at scale).
