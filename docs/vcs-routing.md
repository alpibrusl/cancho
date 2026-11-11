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

## 6. Decisions for the owner

* **O1. Split `vcsrepo.cho` (R0) so `init`, `log` and `show` do not wait for a reseed? Recommendation: yes.** It is a small move of code that already works, and it puts the wrong-and-successful `vcs log` of §1
  right two slices earlier. The cost is one more module, and that `status` (R4) still waits for R3.
* **O2. Who writes the macOS seed, and when?** R3 needs `aarch64-apple-darwin`, written on a Mac, and the Linux seeds can come from `reseed.yml`. Recommendation: do R3 once, after R2, so the three
  slices before it share one seed rather than needing one each.
* **O3. Commit the store of D6 (`tests/vcs_fixtures/`)? Recommendation: yes,** a few files, written once by the Rust command and checked by the pins; it is the only way the test outlives the Rust code.
* **O4. Is a `--repo` path with a link in it printed as written acceptable (D2)?** Recommendation: yes, said in the option's sentence, until #686 gets a `realpath` (its own design question).

## 7. Not measured

How much `clivcsrepo.cho` and the modules it pulls in add to the standalone build (`cancho.files` is 78 files and each stage takes 33 s, `standalone-cancho.md` D5), to the size of `cancho` (3.1 MB) and to
the seed's IR (22 MB). The cost of `status` through the command line (the library did 654 files in 0.105 s against the Rust command's 0.021 s, `vcs-native.md` §6). Darwin: nothing in this document
was run there. The tags the five commands can answer (D5). Whether a macOS runner can write the seed (§3).
