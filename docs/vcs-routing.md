# Routing `cancho vcs` through the command line in cancho: `init`, `log`, `show`, `branch`, `status`

Status: **design; nothing here is built.** Written before the code, as `CONTRIBUTING.md` asks. It is the step 2 of the order in `docs/vcs-native.md` §7 ("route `log`, `show`, `status`,
`branch` and `init` from `cli.cho` so the repository form exists as a command"), and part of #680. Claims are marked **[measured]** (run for this document, 2026-11-11, Linux x86-64),
**[read]** (taken from the named file, not run) or **[choice]** (a decision this document proposes). Decisions for the owner are in §6.

## 1. What a person gets today **[measured]**

The command line in cancho (`examples/selfhost/cli.cho`, built from `cancho.files` by the compiler in cancho) and the Rust `cancho`, run in the same repository: one the Rust command made with
`vcs init` and one `vcs record`.

| `vcs …` | the Rust command | the command line in cancho |
|---|---|---|
| `log` | the document of changes (`{"changes": […]}`) | ``nothing published yet at `.cancho-vcs` ``, exit 0: **the manifest log, in a repository** |
| `log --limit 1` | the document, one change | ``unknown option `--limit` `` and the whole usage, exit 2 |
| `status`, `show`, `init`, `branch` | their documents | the generic usage of `fmt`, `print`, `ingest`, exit 2 |

So the 15 commands are not "missing": the five this document is about are *unreachable*, and one of them, `log`, answers **wrongly and with success**. A script that runs `cancho vcs log` in a
repository and checks the exit status passes on the cancho-written command and reads the wrong thing.

## 2. What exists **[read]**

* **The libraries, written and checked against a store the Rust command wrote** (`docs/vcs-native.md` §6): `vcsrepo.log_command`, `show_command`, `status_command` (documents, byte for byte),
  `vcsstore.init`, `vcsstore.read_ref`, `vcsstore.update_ref` (compare-and-swap), `vcsrepo.resolve` (revisions: `HEAD`, `@`, ids and prefixes, branches and tags, `~N`). `vcsstorectl.cho` is a
  test command over them ("Not a user command", its header), which prints `refused TAG` and not the Rust refusal document.
* **What `branch` needs and the store does not have:** `list_refs` and `delete_ref` (`cancho-vcs/src/refs.rs:167`, `:186`). `vcsstore.cho` has `read_ref` and `update_ref` only.
* **`cli.cho` routes the registry side of `vcs`** (`log` as the manifest log, `lock`, `fetch`, `resolve`, `publish`, `sync`) in `cli.cho:57-72`, and nothing of the repository side.
* **A walk upward from the current directory already exists in cancho:** `cliproject.find_project` looks for `cancho.toml` in the current directory and the ones above it with the whole-file
  `Fs("")` the command line holds, the directory from `/bin/pwd` run through `Exec` (`cliproject.cho:24`, `:79`). **This answers the one item `docs/vcs-native.md` §7 left unverified** (whether the
  `Fs` the command line holds reaches the directories above the current one): it does.
* **How the Rust command routes** (`vcs_cli.rs:93`): `log` is the manifest log when `--store` is named or **no repository is in reach**; every other repository command is `vcs_repo::dispatch`.
  Discovery is `Repo::open`: canonicalize the start (`--repo` or `.`), look for `.cancho/vcs/format` there and in each parent, refuse an unknown format, and refuse `not-a-repository` at the top
  (`cancho-vcs/src/repo.rs:110`). Every command accepts `--output json`, `--repo` and `--dry-run`; an unknown option is a usage error (`crates/cancho/src/vcs_args.rs:68`).
* **The output**: one pretty JSON document on stdout, keys sorted, `{"ok": true, "command": …, …}` or `{"ok": false, "command": …, "refused": {"tag", "message", …}}`; a refusal exits 1, an
  environment failure 3 (`vcs_args.rs:78`, `main.rs:221`). `docs/vcs-cli.md` §4 says "text is a rendering of the same result"; the code prints the document in both cases, so **the document is what
  parity means**.
* **The oracle after Rust is checked in:** `crates/cancho/tests/vcs_pins/` holds 38 documents (init, status, log, show, branch, 5 refusals, …) from the Rust command, and `crates/cancho/tests/vcs_repo.rs` compares the
  Rust command with them with the root normalised (`normalise`, line 86).

## 3. The bootstrap rule decides the order **[read]**

`docs/seed.md` §5: *the compiler's sources may use a builtin only when the committed seed already compiles it; a change that needs a newer compiler than the seed is two pull requests: the compiler
change, then the reseed, and only then the code that uses it.* Adding a module to `cancho.files` puts it in the compiler's own sources.

* The seed is **older than #760**: it does not know `dir_rmdir`, `dir_symlink` or `dir_readlink` (edition 12). `vcsscan.cho` now reads links with `dir_readlink` (#761).
* **`vcsrepo.cho` imports `vcsscan.cho`** (`vcsrepo.cho:21`: for `tag` and for `status_command`). So *any* routing that imports `vcsrepo` (`log` and `show` included) pulls `dir_readlink` into the
  compiler's sources, and the seed chain (check 1 of `seed.md` §5) fails.
* Two ways out: wait for a reseed, or **keep the modules `log` and `show` need free of `vcsscan`** (a split of `vcsrepo.cho`, §5 R0). The reseed is not free: three seeds of about 1.9 MB each go
  into the history (`seed/`: 1,914,753, 1,920,732 and 1,919,655 bytes), and the macOS seed has to be written on a Mac: `.github/workflows/reseed.yml` runs on the two Linux targets only
  (**[read]**; whether a macOS runner could be added is not looked at).

## 4. The decisions

**D1. The routing is the Rust command's. [choice, from §2]** `vcs log` is the manifest log when `--store` is named or no repository is in reach, the repository log otherwise; `init`, `status`,
`show`, `branch` (and later `diff`, `record`, …) go to the repository commands. Unknown subcommands keep the cancho command line's usage error.

**D2. Discovery, and the one difference it has. [choice]** The start is `--repo PATH` or the current directory (`cliproject.current_directory`); the walk upward is `cliproject`'s `ancestor`
(a path is cut at its last `/`, so no `..` is ever written: an `Fs` traps on one); `.cancho/vcs/format` is read with the `Fs("")`; a format other than `cancho-vcs 1` and the top of the tree are the two
refusals of `Repo::open`, with its messages. **The difference:** the Rust command *canonicalizes* the start (it resolves links) and the command line in cancho has no `realpath` (`vcs-native.md` §7
item 4, #686). The current directory comes from `/bin/pwd` with no environment, which is the physical one, so a command run with no `--repo` agrees. A `--repo PATH` that holds a link is taken as written,
and the `root` that `init` and the refusals print differs from the Rust command's then. The tests normalise `root` as `crates/cancho/tests/vcs_repo.rs` does; the difference is said in the sentence of `--repo`, and not hidden.

**D3. One new module for the repository side, and none of it in `clivcs.cho`. [choice]** `clivcs.cho` is the manifest side (`publish`, `log --store`); the repository side is `clivcsrepo.cho`:
the option parser (the value and flag sets of `crates/cancho/src/vcs_repo.rs:30-110`, `--output`, `--repo`, `--dry-run` on all, "unknown option" and "`--output` is `json`, the only form" as usage errors with the Rust
words), discovery (D2), and the envelope (D4). `cli.cho` gets the routing of D1 in a few lines. The libraries are not rewritten.

**D4. The envelope is written once. [choice]** `{"ok": true, "command": C, …}` and `{"ok": false, "command": C, "refused": {…}}`, pretty, keys sorted, as `jsonwrite.cho` already prints the documents the
libraries answer (`vcs-native.md` §6: byte for byte as `serde_json`); exit 0, 1 for a refusal, 3 for an environment failure.

**D5. Refusals become documents. [choice]** `vcsstorectl` prints `refused TAG`; the Rust command prints the tag, a **message** and the details (a `current` change for `ref-moved`, the candidates for an
ambiguous prefix). The pins hold the messages (`refuse_not_a_repository.json`: ``no `.cancho/vcs` at or above `.` ``), so parity is the Rust sentence, and the compiler in cancho's "the rule's sentence"
decision (`gate.md` G3) does not apply to `vcs`: these are the command's refusals, not the checker's rules. Each slice lists the tags its commands can answer, by reading the `cmd_*` function, and
holds each with a pin or a fixture; **the number of tags is not counted here** (the Rust `vcs` code carries 41 `Refusal::new` sites).

**D6. The oracle is the pins and a committed store. [choice]** `crates/cancho/tests/vcs_repo.rs` builds a repository by running the Rust `record` and compares the commands with the pins. After the Rust code goes there
is no `record` to run. So: a **store committed under `tests/vcs_fixtures/`**, written once by the Rust command with fixed `--time` and `--author` (a store is content-addressed and its documents hold no
clock, so it is reproducible), and a command test (`tests/commands/vcs_repo.sh`, compared with the pins, `<root>` normalised) that runs `init`, `log`, `show`, `status` and `branch` on copies of it. It
needs no Rust binary at test time, which is what makes it a gate stage and not a script run by hand (the fate of `vcsstatus_check.sh`, which is not in `scripts/gate.sh` or CI). The existing
differential scripts stay as the check while the Rust command exists.

**D7. What this does not decide. [choice]** `record` and its gate (the `driver` refactor), `switch`, `restore`, `revert`, `apply` (they need `materialize` and `remove_path`, slice 3b of
`directory-links.md`), `diff`, `blame`, `verify`, `unlock`, the git bridge; `log --path|--intent|--session|--issue|--author` and `show --patch` (`vcs-native.md` §6 lists them as not done); the
crash test; and `canonicalize` (D2).

## 5. Slices

| # | Slice | Needs | Done when |
|---|---|---|---|
| R0 | Split `vcsrepo.cho`: `status_command`, `head_info` and the `tag` that calls `vcsscan.tag` move to `vcsstatus.cho`; `log`, `show` and `resolve` stay and import no `vcsscan` | nothing | `vcsrepo_check.sh` and `vcslog_check.sh` pass as before; `vcsrepo.cho` imports nothing that uses edition 12 |
| R1 | `clivcsrepo.cho` and the routing of D1 for **`init`, `log`, `show`**; the envelope, the documents of refusals; discovery; the fixture and the command test of D6 for these three | R0 | the pins `init`, `init_again`, `log`, `show`, `refuse_not_a_repository` and the refusals these commands can answer equal the output; `cancho vcs log` in a repository is the repository log; with no repository and `--store` it is the manifest log as now; Rust parity script green; the standalone `cancho` builds (stage 2 = stage 3) |
| R2 | `branch` (list, create, delete, `--from`, `--expect`, `--dry-run`): `vcsstore.list_refs` and `delete_ref` | R1 | `branch_list`, `branch_create`, `branch_delete` pinned; `ref-moved` and the checked-out-branch refusal; the crash and race tests are **not** this slice |
| R3 | **Reseed**: the seed rebuilt from `main` (it then knows #760's builtins), on the three targets | owner: a Mac for `aarch64-apple-darwin` | `seed/` changed alone; check 2 of `seed.md` §5 green |
| R4 | `status` through `vcsstatus.cho` (imports `vcsscan`, which uses `dir_readlink`) | R3, and #761 merged | `status_clean`, `status_dirty`, `status_one_left` pinned; the link cases of `vcsstatus_check.sh` as a command test on the fixture |
| R5 | `restore PATH... [--from REV] [--dry-run]` through `vcsrestore.cho` (§7): the work lock, `rel_path`, the tree diff, `vcsworktree.materialize` / `remove_path` | R4 | goldens made by the Rust command for the cases of §7, each also checked on the working copy it leaves; `lock-held`; the four mutants of §7 killed |

Every slice is a pull request with the gate of `docs/gate.md`, a Rust-parity check while the Rust command exists, and a mutant where a check is new (an envelope key, a refusal message, the routing
rule, the order of a document).

**R0, built and measured (2026-11-11, Linux x86-64).** `status_command`, `head_info`, `head_parts`, `HeadInfo` and a `tag` that adds `vcsscan`'s tags move to `vcsstatus.cho` (171 lines; `vcsrepo.cho`
goes from 1,119 to 976). The six helpers the two share (`failed`, `member`, `quoted`, `emit_files`, `tree_id_of`, `tree_text`) are `pub` in `vcsrepo.cho` and called as `vcsrepo.NAME` from the new module
(`log` and `show` use them too, so there was nothing to move). `vcsrepo.tag` chains to `vcsstore.tag` and `vcsstorectl` takes `vcsstatus.tag`, which answers what `vcsrepo.tag` did before. **What it was
for, measured with the compiler built from the committed seed (the one that does not know `dir_readlink`):** a program importing `vcsrepo` builds and runs (`bad-revision`); the same program importing
`vcsstatus` is refused (`vcsscan.cho:189`, `dir_readlink` "is not a function in this program"), and the compiler of today builds it. So `init`, `log` and `show` can enter `cancho.files` without a reseed, and
`status` cannot. The four differential scripts against the Rust `cancho vcs` pass as before the move: `vcsrepo_check` 31 checks, `vcslog_check` 51, `vcsstatus_check` 31, `vcsstore_check` 26. **Not changed:**
no behaviour, no `.expected`, no pin; `cancho.files` (nothing here is in it yet).

**R1, built and measured (2026-11-11, Linux x86-64).** `clivcsrepo.cho` (the options, discovery, the envelope and the refusal documents) and the routing in `cli.cho` (D1: `init` and `show` always, `log` unless `--store` is
named or no repository is in reach). `cancho.files` gained five modules, `clivcsrepo`, `vcsrepo`, `vcsstore`, `vcsobj` and `vcschange`, and **not** `vcsscan` or `vcsstatus` (R0 did that). **The chain built from the committed
seed reproduces itself with it** (stage 2 = stage 3), so R1 needed no reseed, as §3 said. `tests/commands/vcs_repo.sh` (in `script_tests.cho`) holds `init`, `log` and `show` against the pins of `crates/cancho/tests/vcs_pins/` and
the 14 goldens of `tests/vcs_fixtures/golden/` (made by the Rust command with `tests/vcs_fixtures/make.sh`), on copies of `tests/vcs_fixtures/three_changes`, byte for byte, with the root made `<root>`. **A false alarm, said
because it cost an afternoon of reading:** the first chain from the seed appeared to fail (stage 2 differed from stage 3 by 2.6 MB). The IR the seed-built stage 1 wrote was byte-identical to a correct compiler's; the cause was an
edit of `clivcsrepo.cho` made while the chain was running, so stage 2 and stage 3 compiled different files. A run with nothing touched reproduced itself. **Not the Rust command's:** `--repo` is not resolved through links (D2); the
refusal messages for the tags whose Rust sentence carries what only the Rust store knows (`object-corrupt`, the OS's words for `io`) are the tag's own sentence; `log --path|--intent|--session|--issue|--author` and `show --patch`
are refused with exit 2; an invalid `--limit` is refused before the revision is resolved, where the Rust command resolves first.

**R2, built and measured (2026-11-11, Linux x86-64).** `branch list`, `create NAME [--from REV]` and `delete NAME --expect REV`, with `--dry-run`. In the store: `vcsstore.list_refs` (the refs under `refs/PREFIX`, sorted by the bytes
of the whole name, as `refs.rs` does, so `a.b` comes before `a/b` and `a0`), `vcsstore.delete_ref` (the lock, the compare, a log line `OLD - TIME deleted`, then the file removed), and `vcsrepo.head_ref`. They do not touch
`update_ref`, which `create` uses as it is; `take_lock` and `add_log_line` are new and **`update_ref` still carries its own copies of both**: sharing them is a refactor of a function the crash and race tests will want
first. **A thing the Rust source did not say until a golden did:** `delete_ref`'s `ref-moved` is not `update_ref`'s: its message is `` `heads/NAME` is not at the expected change `` and it has no `expected` key, so the document
has two forms (`moved_refusal`'s `deleting`). The test (`vcs_repo.sh`, now 81 checks) takes the three pins (`branch_create`, `branch_list`, `branch_delete`) and 18 goldens: the empty repository, `create` from `HEAD`, again
(`created: false`), where it is elsewhere (`ref-moved`), `--dry-run` of both, a bad name, no name, an unknown word, `--from` a name that names nothing, a delete with no `--expect`, with the wrong one, of the checked-out branch and of
a branch that is not there, and the sorted list. It also reads the files: the ref holds the id and a newline, the lock is gone, the log holds `- ID T branch create` and then `ID - T deleted` (the time made `T`). **Interoperation,
checked by hand against the Rust command** (not part of the test, which cannot run it): a branch made by one is listed by the other, and deleted by the other, and the log reads the same. **Mutants, each on a scratch copy of the sources, each killed by the test:** the `--store` test of the routing (3 lines of the transcript differ), the comparison of the sorted insert (3 checks fail), the `--expect` test of `delete_ref` (the ref was deleted), and the `expected` key of the delete refusal (1 check fails). **Not covered:** `lock-held` as a document
(its Rust detail names the lock path and the pid; here it is the generic sentence), a crash between the log line and the removal, two writers racing a ref (the crash and race tests are their own slice), and `branch` in a repository
whose HEAD names a tag.

**R3, the three seeds written and measured (2026-11-11, `main` at 51524d20; Darwin on macOS 26.2, Apple silicon, the two Linux seeds by the `reseed` workflow, run 43050178956, which reproduced each from itself with `cmp`: x86-64 23,392,966 bytes of IR, aarch64 23,404,718).** `scripts/reseed.sh --cancho0 target/release/cancho` wrote `seed/aarch64-apple-darwin.ll.gz` (2,002,425 bytes, from 1,914,753) and its SHA-256 `ae2fd525…c01f4` (23,414,261 bytes of IR, from 22,615,893); the IR of stage 2 equals that of stage 3 (the fixed point); `scripts/reseed.sh --out reseed-out2`, from the new seed alone, wrote both files again and `cmp` finds them **identical**; `scripts/gate.sh` (fmt, files, seed, walk, tests: exit 0 with the new seed; two earlier runs, with the old one, stopped at the `backends` command test, which loses part of its output when run after `http_server_upload`, `http_server_stream`, `thread_state` and `api` and is not about `vcs`) builds stage 1, 2 and 3 from the new seed, and `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` pass with it. The seed now knows `dir_rmdir`, `dir_symlink` and `dir_readlink`, so check 2 of `seed.md` §5 holds for Darwin; R4 no longer waits for a seed.

**R4, built and measured (2026-11-12, Darwin, macOS 26.2, Apple silicon; `main` at 0a18f498).** `cancho vcs status` is `clivcsrepo`'s (`routes` answers it, `command` finds the repository, opens the working copy and the store and calls `vcsstatus.status_command`); `cancho.files` gained `vcsscan.cho` and `vcsstatus.cho`. **The chain built from the committed seed (the one R3 wrote) reproduces itself** (stage 2 = stage 3, `scripts/build-standalone.sh --seed`), so no reseed was needed beyond R3. `tests/commands/vcs_repo.sh` goes from 81 to **102 checks**, the 81 unchanged: the three pins (`status_dirty`, from a new repository; `status_clean` and `status_one_left`, from the fixture with its `main` set to the first and the second change, whose ids the pins hold), 13 goldens made by the Rust command with `tests/vcs_fixtures/make.sh` (the clean fixture, a deleted file, added files in a new directory, the execute bit, a link added, links to a file, to a name that is not there and to a directory, a file that became a link, a 500-byte target, ignored links, a file that became a directory, an empty repository, no repository), `status` from two directories down and with `--repo`. **A bug the test found, not the Rust command's:** a link target that is not UTF-8 answered `"tag": ""` (the module took the tag from `vcsstore.tag`, which does not know the scan's statuses); it now takes `vcsstatus.tag` and says `path-unsupported`, exit 1, which is also what the Rust command says (its message names the link, ``link target of `bad` ``; this one's is the tag's sentence, so that case is checked by tag and exit, not by bytes, as for the other refusals in D5). **Not covered:** `--dry-run` on `status` (accepted, changes nothing), HEAD naming a tag, a working copy of many thousands of files, Linux (the Darwin seed was the one used; the Linux seeds are written, `seed-fixed-point` is the check).

## 6. Decisions for the owner

* **O1. Split `vcsrepo.cho` (R0) so `init`, `log` and `show` do not wait for a reseed? Recommendation: yes.** It is a small move of code that already works, and it puts the wrong-and-successful `vcs log` of §1
  right two slices earlier. The cost is one more module, and that `status` (R4) still waited for R3 (done: R3 and R4 above).
* **O2. Who writes the macOS seed, and when?** R3 needs `aarch64-apple-darwin`, written on a Mac, and the Linux seeds can come from `reseed.yml`. Recommendation: do R3 once, after R2, so the three
  slices before it share one seed rather than needing one each.
* **O3. Commit the store of D6 (`tests/vcs_fixtures/`)? Recommendation: yes,** a few files, written once by the Rust command and checked by the pins; it is the only way the test outlives the Rust code.
* **O4. Is a `--repo` path with a link in it printed as written acceptable (D2)?** Recommendation: yes, said in the option's sentence, until #686 gets a `realpath` (its own design question).

## 7. Not measured

How much `clivcsrepo.cho` and the modules it pulls in add to the standalone build (`cancho.files` is 78 files and each stage takes 33 s, `standalone-cancho.md` D5), to the size of `cancho` (3.1 MB) and to
the seed's IR (22 MB). The cost of `status` through the command line (the library did 654 files in 0.105 s against the Rust command's 0.021 s, `vcs-native.md` §6). Darwin: nothing in this document
was run there. The tags the five commands can answer (D5). Whether a macOS runner can write the seed (§3).

## 7. R5, `restore`: the design **[read]** unless marked, **[choice]** where it is a decision (2026-11-12; nothing here is built or measured)

What `cmd_restore` does (`crates/cancho/src/vcs_repo.rs:572-604`) **[read]**: refuse `nothing-selected` with no path; resolve `--from` (default `HEAD`) to a change and take its tree; with no `--dry-run` take the **work lock**; scan the working copy; turn each path argument into a repository-relative one (`rel_path`, `vcs_args.rs:116`); for every difference between the working copy and that tree whose path is covered by an argument (`""` covers all, else the path or beneath it), record `{"path", "was": KIND}` and, unless `--dry-run`, `materialize` the tree's entry or `remove_path` the path (what the tree lacks). The answer is `{"from", "restored": [...], "dry_run"}`, `restored` in the order of the diff.

**D8. One module, `vcsrestore.cho`, calling what exists. [choice]** `vcsstatus` already scans and diffs against HEAD's tree (`vcsrepo.emit_files`); `vcsworktree` already writes (27 checks against Rust `restore`). New is the glue: (a) the tree of `--from` instead of HEAD's, (b) the covered-path filter, (c) reading each blob and calling `materialize` with its mode and bytes, `remove_path` for a deletion, (d) the lock. `clivcsrepo` routes `restore` (option `--from`, positional paths, `--dry-run`).

**D9. The work lock. [choice, needs the owner: O5]** Rust writes `PID TIMESTAMP` to `.cancho/vcs/lock` with `O_EXCL` and removes it on drop; a held lock refuses `lock-held` and the document carries `lock`, `pid` and `age_seconds` (the message says to run `cancho vcs unlock`). cancho has no `getpid` builtin: `vcsstore.take_lock` stamps `0 NOW` for refs, which are held for microseconds. For `restore` the lock is held while many files are written, and `unlock` prints the pid. Options: **(a)** stamp `0 NOW` as refs do (a pid of `0` in the stamp, whether the Rust `unlock` accepts it is **not measured**; the pid in `lock-held` is `0`); **(b)** add a `getpid` builtin (touches many pinned counts and needs a reseed: a slice of its own). Recommendation: (a), said in the header, with the document's `pid` as `null` when the stamp holds `0`.

**D10. `rel_path`. [choice]** The Rust function resolves the current directory through links (`canonicalize`), joins, collapses `..` and requires the result to lie under `repo.root` (`path-escapes` otherwise, `path-unsupported` for non-UTF-8). `clivcsrepo` already takes `--repo` as written (D2). The same rule here: join the argument to the current directory as `clivcsrepo.absolute` does and strip the root **as found by upward search**. A difference from Rust arises only when the current directory is reached through a link (the Rust prefix test then sees the physical path, this one the logical one); listed in the header and in a test, not hidden.

**D11. Order and partial failure. [choice]** As Rust: one change at a time in the order of the diff; a failure (`io`, exit 3) stops the command with the files written so far in place (the Rust command does not roll back either, **read**: the loop `?`s out). The temporary file of `materialize` is `NAME.cancho-tmp-0` (`vcs-worktree.md` W3): `status` skips it, so a crash leaves no phantom change.

**What the tests are. [choice, corrected 2026-11-13]** Goldens made by the Rust command on copies of the fixture, for the cases below. **The pins `restore`, `restore_again` and `restore_dry` cannot be used as the design first said:** they come from a repository that has had two `revert`s (their `from` is `935bbc…`), which cancho cannot make yet, so the same cases are made on the fixture instead (the documents have the same shape). The cases: `.` (everything, after the working copy was changed in each way `status` knows: file, mode, link, deleted, a file that became a directory), a path beneath a directory, `--from HEAD~1`, `--dry-run` (nothing written), a name that matches nothing (`restored: []`), no path (`nothing-selected`), a held lock (the fixture with `.cancho/vcs/lock` made; `lock-held` with its `pid` and `age_seconds`), `restore` from a subdirectory (paths relative to it). After each write case the **tree is compared with the Rust command's** entry by entry (`vcsworktree_check.sh`'s comparison), not only the JSON. **Mutants to kill:** the covered-path test dropped (everything is restored), the lock not taken, the order of the diff changed, `--dry-run` writing.

**Not in R5:** `switch` (needs HEAD movement and the dirty-path check, `move_working_copy`), `revert`, `apply`, `record`'s deletions; the crash window between the write and the rename; a restore of many thousands of files.

**O5. The pid in the work lock: `0` now, a `getpid` builtin later? Recommendation: `0` now.** A builtin is a reseed, and R5 is not a reason to make one.

**R5, built and measured (2026-11-13, Darwin, macOS 26.2, Apple silicon; `main` at ea65b43e).** `vcsrestore.cho` (295 lines), `restore` in `clivcsrepo.cho` (the paths, `--from`, `--dry-run`; `rel_path`; the `lock-held` document), `vcsstore.take_lock` made `pub`, and `cancho.files` gained `vcsrestore.cho` and `vcsworktree.cho` (`cli.cho`'s `run` gained the `dir_symlink` effect). The chain built from the committed seed reproduces itself (stage 2 = stage 3): **no reseed**, as `vcs-worktree.md` W1 said. `tests/commands/vcs_repo.sh` goes from 102 to **136 checks**, the 102 unchanged: 15 documents made by the Rust command (`make.sh`), and for each write case the working copy is compared with HEAD's, entry by entry (`diff -r`), or with the files the Rust command left (`--from HEAD~1`). They cover one file, `--dry-run` (nothing written), a file already HEAD's, `.` after every kind of change (deleted, mode, an added link, files in new directories), a file that became a directory (`io-error`, exit 3, where the Rust command stops too), a directory, `--from HEAD~1`, no path, a path nothing has, a path outside the repository, a bad `--from`, from a subdirectory (a path with `..`, and `.`), two paths, `--dry-run` with the lock held (it takes none), and the lock held: `lock-held` with the lock's path, the pid and the age (checked by key, since the age is the clock's), the lock left as it was, and a lock stamped `0` read as a null pid. After a restore the lock is gone. **Mutants, each killed (the test run against a scratch build):** the covered-path test dropped (5 checks fail), the lock not taken (4), `--dry-run` writing (2), the order of the walk swapped (4). **Found on the way, not R5's:** the command line said `io` for an I/O failure where the Rust command and `vcs-cli.md` say `io-error`; `clivcsrepo` now says `io-error` (the library `vcsstore.tag` is unchanged, so the differential scripts still compare what they compared). And `absolute` leaves a final `.` (`/a/b/.`), which `restore` strips. **Known differences:** `path-escapes` is refused before `--from` is resolved and before the lock is taken (Rust does both first); the lock's pid is `0` (O5), so `unlock` and `lock-held` show `null`, and the Rust `unlock` on such a lock is **not measured**; the temporary file of a write is `NAME.cancho-tmp-0` (`vcs-worktree.md` W3). **Not covered:** a link entry restored through the command (the fixture has none; `vcsworktree_check.sh` has them), HEAD naming a tag, a crash between the write and the rename, Linux (CI).

## 8. R6, `switch`: the design **[read]** unless marked, **[choice]** where it is a decision (2026-11-13; nothing here is built or measured)

What `cmd_switch` does (`crates/cancho/src/vcs_repo.rs:532-570`, `move_working_copy` at `:499`) **[read]**, and what the Rust command printed on the fixture **[measured]** (`cancho-darwin` binary, scratch copies of `three_changes`):

* Exactly one positional (`bad-argument`: "`switch` takes one branch or change", for none or two). Unless `--dry-run`, take the **work lock** first.
* The target is: with `--create`, a **new branch at HEAD's change** (`bad-revision` "there is no change to branch from yet" if HEAD has none; `check_ref_name`; `update_ref(heads/NAME, Expect::Absent, ..., "switch --create")` unless dry; a branch that exists is `ref-moved` with `"expected": "absent"`); else, unless `--detach`, an **existing branch** of that name; else `resolve(target)` and **detach** at it (`bad-revision` "names no change, branch or tag").
* If HEAD already is that target, answer `{"command","head","ok","switched": false}` (no `dry_run`, no `branch`, no `discarded` keys), nothing touched. Otherwise diff `HEAD's tree -> target's tree` (`moving`) and `dirty` = the paths `status` lists. A dirty path that is also in `moving` is **in the way**. Blocked = every dirty path (default), only those in the way (`--keep`), none (`--discard`); a non-empty blocked list is `dirty-working-copy` (exit 1) with `"paths"` (sorted). Unless `--dry-run`, for each change in `moving` (path order) `materialize` the new entry or `remove_path`, then **write HEAD** (`ref: refs/heads/NAME\n`, or `ID\n` detached). Answer `{"branch": NAME|null, "command", "discarded": [in-the-way paths with --discard, else []], "dry_run", "head", "ok", "switched": !dry}`.
* **Measured on the fixture:** `--keep` with a dirty file that `moving` does not touch proceeds and carries it (`discarded: []`); `--discard` overwrites only what `moving` names and leaves every other dirty file; `--dry-run` still refuses over a dirty working copy (the check comes before the write); a dry-run switch answers `switched: false` with `dry_run: true`, as it does when HEAD is already there (that one has no `dry_run` key).

**D12. One module, `vcsswitch.cho`, in the shape of `vcsrestore`. [choice]** `switch_command(heap, root, vcs, scratch, target, create, detach, keep, discard, dry, now)`. It reuses `vcsrepo.resolve/tree_id_of/tree_text`, `vcsstore.read_ref/update_ref/take_lock`, `vcsscan.scan`, `vcsworktree.materialize/remove_path`, and the walk of `vcsrestore` for applying a change (the walk `old -> new` is the same merge-walk with `old` the HEAD tree and no filter; `vcsrestore.apply` is made `pub`).

**D13. The dirty set and `in the way` by merge-walks, not a set. [choice]** `dirty` = the paths of the walk `HEAD's tree -> scan` and `moving` = the paths of the walk `HEAD's tree -> target's tree`, both in path order as lines `PATH\n`; `in the way` is their intersection by one merge pass; the refusal lists `dirty` (default) or `in the way` (`--keep`). No quadratic search.

**D14. Writing HEAD. [choice]** `vcsstore.put_whole` (private today, used by `init`) is made `pub` and writes `HEAD` through `HEAD.tmp-0` and a rename: the temporary file is in `.cancho/vcs` (not `.cancho/vcs/tmp` as Rust's `write_atomic`): **a known difference**, invisible to every command (the scan skips `.cancho`). A crash between the working-copy writes and the HEAD write leaves HEAD at the old place with a half-moved tree, exactly as in Rust; `status` shows it.

**D15. The refusal document needs a `paths` member. [choice]** `clivcsrepo` builds `dirty-working-copy` as it builds `lock-held`: tag, message, and the sorted `paths`. A new status code (32 `dirty-working-copy`) in `vcsrestore`'s/`vcsstatus`'s sense is **not** added to the libraries: the command line carries the list in `bytes`, the same way `lock-held` carries the lock text (status 15).

**D16. Order with the lock and the create. [choice]** As Rust: lock, then HEAD's tree, then the create (`update_ref`), then the dirty check. **A difference from Rust that is Rust's:** with `--create` and a dirty tree the branch is made *before* the dirty check could refuse; but `moving` is empty for a create (same change), so nothing can be in the way and the create never refuses for dirt. Kept.

**What the tests are. [choice]** Goldens by the Rust command on copies of the fixture; after each write case the **working copy and HEAD file are compared with the Rust command's**. Cases: `switch main` (no-op), `--create topic` (HEAD file, ref file), `--create` twice (`ref-moved`), `--create` bad name, `--create` in an empty repository (`bad-revision`), a branch at `HEAD~1` then switch to it (files change, HEAD is the branch), switch back, `--detach main`, `HEAD~1` (detached, `branch: null`), a change id, `nosuch`, no argument, two arguments, a dirty file blocking (default, with `paths`), the same with `--keep` where the dirty file is in the way (blocked) and where it is not (carried), `--discard` where it is in the way (`discarded`), `--dry-run` (nothing written, HEAD unchanged), an added and a removed file and a mode change between the two changes, from a subdirectory, the lock held (`lock-held`) and `--dry-run` with the lock held. **Mutants to kill:** HEAD not written, the dirty check dropped, `--keep` treated as `--discard`, `--dry-run` writing, the lock not taken, the in-the-way intersection returning everything dirty.

**Not in R6:** `revert`, `apply`, `record`'s deletions, `merge`; a switch of many thousands of files; a crash between the writes and the HEAD write.

**R6, built and measured (2026-11-13, Darwin, macOS 26.2, Apple silicon; `main` at a6998f08).** `vcsswitch.cho` (about 500 lines), `switch` in `clivcsrepo.cho` (the options `--create`, `--detach`, `--keep`, `--discard`, `--dry-run`; the `dirty-working-copy` document with `paths`; `ref-moved`, `bad-ref-name`, `lock-held` for `switch`), `vcsstore.put_whole`, `vcsrestore.apply` and `vcsrestore.tree_of` made `pub`, and `cancho.files` gained `vcsswitch.cho`. The chain built from the previous standalone compiler reproduces itself (stage 2 = stage 3): **no reseed**. A new committed fixture, `tests/vcs_fixtures/four_changes` (three_changes and a branch `wide` one change ahead that removes a file, adds files, one in a new directory, changes a mode and adds a link; HEAD on main), made by `make.sh` with the Rust command, and 35 new golden files (`switch_*`, `refuse_switch_*`, the HEAD file and the working-copy listing the Rust command left). `tests/commands/vcs_repo.sh` goes from 136 to **200 checks** (the 136 unchanged). Before the tests were written the command was compared with the Rust command on 35 cases (stdout, exit status, HEAD, the working copy and every ref, entry by entry): all equal, apart from the lock's path (the directory's name). The cases cover a no-op, `--create` (with `--dry-run`, over a branch that exists, a bad name, an empty repository), a branch, a change, `--detach`, a bad revision, no argument and two, a dirty file in the way and one not in the way with no flag, `--keep` and `--discard`, `--dry-run` over dirt, the lock held (and `--dry-run` with it held), and a switch from a subdirectory. **Mutants, each killed (the test run against a scratch build):** HEAD not written (11 checks fail), the dirty check dropped (1), `--keep` read as `--discard` (4), `--dry-run` writing (3), the lock not taken (3), the in-the-way intersection returning every dirty path (5). **A measurement that went wrong and was redone:** the first mutant run started its loop with a trailing `&` inside a background command, so the loop outlived the command and kept rewriting `vcsswitch.cho` while the foreground runs built from it (two builds then failed the stage 2 = stage 3 check, and the counts of the first run mixed mutants); all six were run again one at a time with nothing else running, and every build reproduced. **Known differences:** HEAD is written through `HEAD.tmp-0` in `.cancho/vcs` (D14); the lock's pid is `0` (O5), as for `restore`; `--keep`, `--discard` and the other flags are taken as `--NAME` only (no `--NAME=VALUE`). **Not covered:** a crash between the writes and the HEAD write, a tag as the target, a link replaced by a directory through `switch`, Linux (CI).

## 9. R7, `apply` (and `unlock`): the design **[measured]** against the Rust command unless marked **[choice]** (2026-11-13; nothing here is built)

**Why not `revert`.** `revert` ends in `commit_tree` (the gate, the intent, the new tree and change object): that is `record`, which has no native form yet, so `revert` waits for it. `apply FILE` and `unlock` need only
the lock, the working copy and one JSON file. The Rust command was run on a scratch repository for each line below.

* **E1. The file.** One positional, else `bad-argument` ``"`apply` takes a JSON file of edits"``. It is read from the **current directory**, not the repository root; unreadable: tag `io-error`, exit 3, ``cannot read FILE: ...``.
  A JSON array of objects with `path`, `old`, `new`, all strings; **extra keys are ignored**; anything else is `bad-argument` (exit 1) with the parser's sentence, which is serde's (`missing field \`new\` at line 1 column 27`).
  **[choice]** the tag and exit are the Rust command's, the sentence is `FILE: ` and the parser's own words; the golden test compares tag and exit for these, as D5 does.
* **E2. Paths.** Each `path` goes through `rel_path` (the cwd, `..` collapsed, outside the root is `path-escapes` with ``\`../x\` is outside the repository``), as `restore` does; the helper of `restore_in` is shared, not copied.
* **E3. Order and atomicity.** Edits are grouped by path, **paths sorted by bytes**, each path's edits in file order; the work lock is taken **before the file is read and also under `--dry-run`** (`restore` and
  `switch` do not take it when dry); every file is read and edited in memory before any is written, so a refusal in the second path leaves the first untouched (measured: `e.sh` is refused before `o.txt` is looked at).
  An empty list is `applied: []`, ok, with the lock taken.
* **E4. The anchor.** `old` empty: `anchor-not-found` ``an empty anchor in \`P\```. Otherwise `old` must occur **exactly once, counting non-overlapping matches from the left**: `aa` in `aaa` occurs once (measured: the file became
  `ba`), not twice, so it is *applied*. Not found: `anchor-not-found` ``the old text of an edit is not in \`P\```; two: `anchor-ambiguous` ``... occurs more than once in \`P\```. Edits of one path apply in order, each to the text the last one left.
* **E5. Reading and writing.** The file is read whole as UTF-8 (a missing one, a directory: `io-error`, exit 3, ``cannot read P: ...``). The Rust write is **in place** (`fs::write`, then the old permissions set again) and **follows a link**: through
  `l -> o.txt` it changes `o.txt` and leaves the link (measured). **[choice]** cancho writes with `vcsworktree.materialize` (a temporary file beside it, renamed over: whole or old), mode `0644`/`0755` from the owner's execute bit,
  and **refuses a link** with `io-error`. **Known differences, to be listed in the header:** a link; a mode other than those two (`0600` becomes `0644`); a file that is not valid UTF-8 is refused where Rust also refuses it (same).
* **E6. The document.** `{"applied": [paths sorted], "command": "apply", "dry_run": B, "ok": true}`; `--dry-run` writes nothing.
* **`unlock`, E7.** Lists `refs/**` for names `lock` or ending `.lock`, then the store's top `lock`, reads each as `PID SINCE`, removes it unless `--dry-run`; the document is
  `{"command","dry_run","ok","removed":[{"age_seconds","lock","pid"}]}`, `age_seconds` = now − SINCE (at least 0), `pid` and `age_seconds` null when the text does not parse. The pin has `removed: []`. **[choice]** order: sorted by path
  (Rust's is the directory's, unspecified), the top lock last as in Rust; a lock cancho made says `pid` 0 (D9).
* **Where.** A new `vcsapply.cho` (the anchor, the grouping, the document: pure, testable without a repository), the command in `clivcsrepo.cho` (the options, the file, `rel_path`, the lock), routed like `switch`.
  Tests: pins `apply`, `refuse_anchor_not_found`, `unlock`; goldens of the Rust command for E1–E6 on a copy of the fixture; mutants: the overlap rule (`aa`/`aaa`), the order of paths, the lock under `--dry-run`, the
  empty-anchor test, atomicity (a second file's refusal after a first one's success).
