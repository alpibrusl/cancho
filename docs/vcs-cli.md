# `cancho vcs` as a complete version-control CLI, agents first

> **Status: slice V1 (a local repository, section 9) is built; V2 onward is design.** Written before the code (`CONTRIBUTING.md`). A claim marked
> *(measured)* was measured on the date given, in the repository named; a claim marked *(read)* was read
> from the source; one marked *(designed)* is a proposal that a slice below must measure before it stands.
> The first draft of this document assumed that a typed-AST store could stand in for git on a code
> repository. Section 2 measures that assumption and it fails; the design below is what the measurement left.

## 1. What is asked, and what exists

Asked: `cancho vcs` should be a complete command-line tool for the whole code-development cycle, the
equivalent of git, with an agent as the first-class user: start work, change files, see the change,
record it with the reason, branch, merge, review, publish, fetch, go back, find who changed what and why.

Exists *(read, 2026-11-05)*: `crates/cancho-vcs` (1,341 lines of library) and four CLI files in
`crates/cancho/src` (`vcs_cli.rs` 1,114, `vcs_sync.rs` 355, `vcs_dir.rs` 189, `vcs_origin.rs` 158):
typed `Operation`s with BLAKE3 identity and an edition tag, the always-valid gate (`gate::check_candidate`),
a loose-file op log, a hash-chained Ed25519-sealable attestation log, a content-addressed blob store for
one file's source, and a per-file `Manifest`. The commands are `vcs publish` (one store per file, `--requires`
locks between them), `vcs log`, `vcs sync` (an `ids.txt` index in `.cancho/`) and a fetch of a locked store
through **a `git` checkout** (`vcs_origin.rs`, `vcs_dir.rs`). So the present system is a *package publisher*
that sits on top of git. It has no working copy, no commit, no branch, no merge, no remote, no history of its
own. `docs/vcs.md` section 8 lists merge, merge sessions, issues and predicate branches as not started.

## 2. The measurement that changes the design

If the store versions only `.cho` declarations, how much of a real change does it see? Two repositories
written in cancho, the last 150 non-merge commits of each, files read from `git show --name-only`
*(measured 2026-11-05)*:

| repository | commits touching only `.cho` | touching `.cho` and other files | touching no `.cho` |
|---|---|---|---|
| `cancho-forge` | **3** (2%) | 87 | 44 |
| `cancho` | **1** (1%) | 106 | 43 |

In `cancho-forge`, 496 of the 1,084 files those commits touched are `.cho`; the rest are `sh`, `md`, `txt`,
`json`, `tsv`, `pem`, `py`, the manifest `cancho.toml`, CI. The tracked tree is 335 files, 182 of them `.cho`.

**Consequence.** A store of declarations cannot replace git on a code repository: 98% of the commits
carry something it cannot hold, and the change that matters (a source file, its test, the script that runs
it, the document that states it) is one change. The unit of version control has to be the **whole tree**.
What cancho adds is not a different unit but a **different reading of the part that is cancho**: for a `.cho`
file the diff, the merge and the blame are by declaration, and a commit is refused unless the tree it makes
still checks. Everything below follows from this one choice.

## 3. The model *(designed)*

Four kinds of object, all content-addressed (BLAKE3, as `cancho-id` and `OpId` already are), all immutable,
all stored as plain files beneath `.cancho/vcs/` (so the store is inspectable with the tools an agent has,
and there is no database to corrupt).

* **Blob**: the bytes of a file, any bytes. (`blobs.rs` stores one file's UTF-8 source today; it generalises
  to bytes.) A file over a bound (default 16 MiB) is refused with a tag, not silently stored.
* **Tree**: a sorted list of `(path, mode, blob)`; a directory is not an object, the paths carry it. Modes are
  exactly three: file, executable file, symbolic link (the target is the blob). Nothing else is versioned,
  which is also why there are no submodules (section 9).
* **Change** (the commit; named *change* because it carries more than a snapshot): `tree`, `parents` (0, 1 or 2),
  `author`, `time`, and the part git has no field for, **`intent`**: `{prompt, model, session, issue?}`.
  Also `ops`, the typed operations the diff against the first parent produced for the `.cho` files in it
  (derived at record time, stored so a reader never recomputes them: an `OpId` list), and `attestations`, the
  ids of the gate results that let it exist. An unattributed change is allowed and says so, as lex-vcs does;
  an intent that is present is never rewritten.
* **Edit operation** (*designed*, not an object of its own: it is carried by a change and by `replay`): the
  replay-able form of a text edit, `{path, old, new}`. It applies to a tree only if `old` occurs **exactly once**
  in the file; zero occurrences refuses with the tag `anchor-not-found`, two or more with `anchor-ambiguous`. The
  recorder derives each operation from a display diff hunk by widening `old` with the fewest surrounding lines
  that make it unique in the base file. It is the analogue, for any text file, of a typed operation on a `.cho`
  declaration: what was *done*, which can be re-applied to a different base, not the text of a patch with line
  numbers. V1 records them for every touched non-`.cho` text file (`Change.edits`, derived and cached like
  `ops`) and offers `apply` of one list to a tree; using them to merge or to `replay` is V2. R6 (section 8)
  measured what they are worth as a merge: **not more than a line merge**, so they are proposed for `replay`
  and for the intent record, not as the merge algorithm.
* **Ref**: a branch or a tag, a file holding a change id. Moved by an atomic rename, only ever with
  `--expect <old id>`, so two agents racing on one branch cannot both win silently (compare-and-swap, section 5).

What is *derived and never authoritative*: the per-declaration index (`SigId`/`StageId`, today's `ids.txt`),
the operations, the rendered diff, blame. They can be recomputed from trees and are cached; a cache that
disagrees with the trees is deleted, not trusted.

**Always-valid head.** A branch ref moves only to a change whose tree passes the branch's *gate*. The built-in
gate is `cancho check` over every `.cho` file the change touches (the rule tag of the first failure is the
refusal). A repository may add commands to its gate (`scripts/gate.sh` for this one); a command's exit status
and the hash of its output are recorded as an attestation on the change, so "this head passed the gate at
this tree" is a fact in the store and not a claim in a CI log. `record --no-gate` exists and writes an
unattested change to a ref that is not protected; a protected ref refuses it. *(The gate cost is the open
question of section 8, R2: this repository's gate is 18 minutes.)*

## 4. The command surface *(designed)*

Agent-first means, for every command: `--output json` is the only output form (V1 has no text rendering: an agent reads JSON, and a second form is a second thing to keep in step); every
refusal carries a **rule tag** and exit code (the repository's rule), and the same refusal is the same tag on
every run; nothing is interactive, nothing opens an editor or a pager, nothing asks a question; every command
that changes state is **idempotent** (running it twice records once) and has `--dry-run`; every command that
moves a ref takes `--expect`. There is **no staging area**: a person stages to compose a commit by hand, an
agent knows the paths it changed, and `record <paths>` (or `record --all`) says them. (Git's index is its
largest source of state an agent can get wrong; omitting it is a decision, with the cost that a partial hunk
cannot be recorded. Hunk-level recording is not offered; a change is made of whole files.)

| group | commands | notes |
|---|---|---|
| start | `init`, `clone <url\|path>`, `import-git <dir>` | `import-git` reads a git repository's objects **in cancho** (loose objects and packs; zlib and delta decoding are the work), never by running `git`; it makes the whole history, one change per commit, with the commit message as the unattributed intent prompt |
| see | `status`, `diff [a [b]]`, `show <change>`, `log`, `blame <path\|decl>` | `diff` is by declaration for a `.cho` file (added, removed, signature-changed, body-changed, renamed) and by lines for anything else, in one JSON document; `log` filters by `--intent`, `--session`, `--path`, `--decl`, `--issue`; `blame` names the change **and its intent** that last touched a line or a declaration |
| record | `record [paths] --intent-prompt .. --intent-model .. --intent-session ..` | runs the gate; refuses on failure with the tag; returns the change id |
| undo | `revert <change>`, `restore <paths> [--from <change>]`, `reset-ref <ref> --expect <id> --to <change>` | `revert` records a new change that undoes one; nothing rewrites history, there is no `rebase` (a replay is `replay`, below) |
| branch | `branch {list,create,delete}`, `switch <ref>`, `tag` | `switch` refuses over uncommitted changes unless `--keep` (carry) or `--discard` |
| combine | `merge <ref>`, `conflicts`, `resolve <path\|decl> --take {ours,theirs,base} \| --with <file>`, `merge --abort` | section 6 |
| replay | `replay <change> --onto <ref>` | re-applies a change's **operations** (not its text) onto another base and gates the result; the agent's answer to cherry-pick and rebase; reports `exact` (same tree) or `moved` |
| share | `remote {add,list}`, `push`, `pull`, `fetch` | section 7 |
| bridge | `export-git <dir>`, `mirror <git-url>` | section 7; one-way, derived |
| keep | `verify` (re-hash everything, check the attestation chain), `gc`, `keys {new,sign}` | `verify` is the single command that says the store is sound |

Not offered, on purpose, and said so: staging area and `add -p`, interactive rebase, stash (a branch and
`switch --keep` do it), submodules, hooks (the gate is the hook), `reflog` (refs have an append-only
`refs/<name>.log` instead, one line per move, so a lost head is recoverable), worktrees (a second working copy
is a second `clone` of a local path, which is cheap because objects are content-addressed and hard-linked).

## 5. Concurrency and failure *(designed)*

Agents run in parallel on one machine, so this is not optional. (1) Objects are written to a temporary name,
fsynced, and renamed to their hash; a crash leaves garbage that `gc` removes and never a wrong object.
(2) A ref move is a rename under a lock file, with `--expect`; the loser gets the tag `ref-moved` and the
winner's id and can `merge` or `replay`. (3) The working copy has one lock (`.cancho/vcs/lock`), taken by
`switch`, `merge`, `restore` and `record`; a stale lock is reported with the pid and age, never broken
silently. (4) No command shells out: no `git`, no `diff`, no `patch`, no `tar` (the project rule). The text
diff and the three-way text merge are written in Rust in the crate, and the crash and race behaviour of (1) to (3) is
exercised by a test that kills the process at every write, as `tests/conformance/crash_*` do in `cancho-forge`.

## 6. Merge *(designed; the part with most risk)*

A merge works in three layers and reports in one structured document.

1. **Tree layer**: for each path, base/ours/theirs. Same on both sides, or changed on one side only, resolve
   without reading content. Added/deleted/renamed handling by path and by blob identity (a rename is a delete
   plus an add of the same blob; a rename with an edit is not detected in V2 and is a conflict with a tag
   `rename-edit`, said rather than guessed).
2. **`.cho` layer**: a file changed on both sides is merged **by declaration** (`vcs.md` section 4): a declaration
   changed on one side only takes that side; added on both with identical identity is one; changed on both is a
   conflict naming the declaration; order and layout are re-derived by the canonical printer, so a pure
   reformat conflicts with nothing. Then the merged tree goes through the gate, which is where the cancho
   difference lives: two edits that each check can together move a linear value twice, so a merge that is clean
   by declaration can fail the gate (`vcs.md` section 4 predicted this and has not measured the rate).
   A gate failure after a clean merge is reported as `merge-gate-failed` with the rule tag and **both** changes'
   intents, which is more than the agent would get from a conflict marker.
3. **Text layer** for every other file: a line-based three-way merge (diff3). *(R6, section 8, measured on the
   historical merges of both repositories: no `json` or `toml` file was edited on both sides of any merge, so
   there is nothing for a key-path layer to do on this data, and an anchored-operation merge conflicted at
   least as often as the line merge. Format-aware layers stay deferred to V2 and need a counter-example in
   a real history to be built at all.)* A conflict is **data, not markers
   written into the file**: `merge` leaves the working copy at "ours" for those paths and `conflicts` returns
   `{path, base, ours, theirs, hunks:[{base_lines, ours_lines, theirs_lines}]}`; `resolve --with <file>` or
   `--take` settles one. (An agent that edits a file full of `<<<<<<<` markers has to parse them; one that
   is given three versions does not.)

`merge` records a change with two parents only when every path is settled and the gate passes; otherwise it
records nothing and says what is open. `merge --abort` is exact because nothing was recorded.

## 7. Remotes and the GitHub bridge *(designed)*

Our PRs, CI and merges are on GitHub, which speaks git only. So the system has two relations to git:

* **Out**: `export-git` renders every change as a git commit (tree exact, message = intent prompt, trailers
  `Cancho-Change`, `Cancho-Intent`, `Cancho-Session`), and `mirror` pushes it. Derived and one-way, as lex-vcs's
  mirror; git SHAs are a function of the changes, so they are stable (author and time are stored, which the
  lex-vcs mirror's SHAs were not). This is how a pull request still exists.
* **In**: `import-git` (above) for a repository's history, and `pull --from-git <url>` to take what a person
  pushed to GitHub as new changes (unattributed intent, the commit message as prompt). The round trip
  `import-git` then `export-git` must reproduce every original commit SHA; that equality is the V1 acceptance test.

Between cancho stores: a **path** remote (a directory, objects copied by content address, refs by
compare-and-swap) in V3, and an **HTTP** remote in V4 (the shape `docs/vcs-publish.md` section 5 left open: a thin
gateway, objects by id, `push` is "here are the objects the other side lacks, then move the ref with
`--expect`"). Nothing about transport needs git. TLS and HTTP are packages cancho already has in `.cho`; the
Rust CLI uses the crate's own client or the packages, decided in V4 on what is measured then.

Where signatures matter (a branch that only accepts signed changes) the existing Ed25519 attestation chain
signs the change id; `verify` checks it.

## 8. What `cancho-forge` gets, and the open questions

Forge's uses of git today *(read)*: `repo.change` takes a checkout and a base and returns a patch;
`repo.publish` makes a commit on a branch; `repo.authority` runs `git show` and `git -C` on a published
commit; `compiler_rev` and company steps pin a 40-hex commit; the operator file names a git checkout. With
this CLI a `repo` row gains `vcs: "cancho"`, and the pins become change ids; the patch a builder returns
becomes a `Change` proposed against a base (its tree, not a text patch), the publish a ref move, and the
review step reads the change's ops and intent rather than a text diff. None of that is built, and it should not
start before V2: a worker that shells out to `git` today is not wrong, it is the next thing to replace.

Open questions, each with the measurement that closes it:

* **R1: can the history be imported exactly?** Import `cancho-forge`'s 194 commits (59 of them merges; the first
  draft said 193, which was the count before the latest commit), export them, compare every SHA. Closes V1.
  **Measured 2026-11-05, V1 as built (`scripts/vcs_r1.sh`, release build, macOS arm64): passed, no mismatch.**
  `cancho-forge`: 228 commits reachable from any branch or remote-tracking ref (194 from `HEAD`, 59 merges there),
  all 228 re-exported with their original SHA; `cancho`: 1,338 commits (545 from `HEAD`, 22 merges there; of those 545, 494
  carry a PGP and 16 an SSH signature), all 1,338 identical. The export passes `git fsck --strict`, `git rev-list --all`
  gives the same set in both, and every branch and remote-tracking ref points at the same SHA (the
  `refs/remotes/*/HEAD` symbolic refs are not imported). The two repositories are read from loose objects and
  18 (`cancho`) and 1 (`cancho-forge`) pack files with delta chains; nothing was run but `cancho`. What had to be
  true for it: the change keeps the commit's header lines and message byte for byte (signatures, encoding
  headers) and parents in order. Not met by these two but not tested either: submodules and unusual modes
  (`160000`, `100664`) are refused with `submodule-unsupported` / `mode-unsupported`; a path with a newline or
  non-UTF-8 name is refused `path-unsupported`; neither occurs in either history. Time: import 9 s and 94 s,
  export 1 s and 9 s (each object fsynced).
* **R4: is a content-addressed store larger than git's?** *(measured with the same runs; apparent bytes of the
  object files.)* **Yes: `cancho-forge` 30.9 MiB against git's 6.1 MiB (5.1x), `cancho` 826.9 MiB against 60.4
  MiB (13.7x).** The store has no compression and no deltas; `cancho` versions generated JSON fixtures many
  times. The bound in the question ("more than a few times") is exceeded, so compression and packs move from
  "V5 if R4 says" to a planned slice; V1 stays uncompressed.
* **R6: what do the non-`.cho` files look like, and which merge strategy would conflict on them?** *(measured
  2026-11-05 with `scripts/vcs_r6.py`, which runs `git` to read the two repositories; the product never does.)*
  File touches by type over every non-merge commit of `HEAD`: `cancho-forge`, 135 commits, 1,091 touches, of
  which `.cho` 499, `md` 257, `sh` 187, `cancho.toml` 49, `py` 32, `txt` 22, `json` 8, other 37. `cancho`, 523
  commits, 16,018 touches, of which `json` 4,721 (generated fixtures, 46 commits), `rs` 2,480, `ls` 2,421,
  extensionless 1,949, `md` 1,678, `.cho` 1,666, `py` 391, `txt` 255. So the part that is not `.cho` is most of
  the touches in `cancho` (90%) and half in `cancho-forge` (54%), and it is dominated by Markdown, shell and
  Rust, not by structured data. Merges: for each two-parent merge, the non-`.cho` files changed on both sides
  since the merge base. `cancho-forge`: 59 merges, 6 such files (2 `md`, 4 `sh`), a line three-way merge
  conflicts on 4, anchored operations on 3. `cancho`: 22 merges, 64 such files (43 `rs`, 21 `md`), line
  conflicts on 1, anchored operations on 3 (2 `rs`, 1 `md`). **No `json` or `toml` file was edited on both
  sides of any merge in either history**, so a key-path merge would have had no case to decide.
  Conclusions: (a) double-edited non-`.cho` files are rare (70 in 81 merges), (b) the line merge is already
  rarely wrong on them (5 of 70), (c) anchored operations are **not** a better merge (6 of 70: a short
  anchor is often ambiguous in a repetitive `sh` or `rs` file, which is the tag `anchor-ambiguous` doing its
  job), (d) format-aware layers have no evidence behind them. Anchored operations stay as the form `replay`
  re-applies, which a line merge cannot be. The merge-versus-real-result comparison (does a clean strategy
  reproduce the file the human merge produced?) was not done.
* **R2: what does the gate cost per record?** This repository's gate is 18 minutes on CI; a gate that runs on every `record`
  makes the agent loop unusable. Proposed: the built-in gate is the check of touched `.cho` files (seconds),
  heavier commands are attested **asynchronously** and a protected ref requires them at `merge`/`push`, not at `record`.
  Measure the check time of a typical change in `cancho-forge`. *Not measured.* *Partly measured in V1:* the built-in gate
  as first written (each touched `.cho` file checked as a program of its own) refused 30 of the first 60 files
  of `cancho-forge` with `unknown-name`, because they import sibling modules: a false refusal rate of 50%. The
  gate as built follows imports to the modules the tree declares and adds `std` when imported; of all 182 `.cho`
  files of `cancho-forge`, touched by a comment, 168 pass and 14 are still refused `unknown-name` because they
  import packages that `cancho.toml` installs (`http_client`, `tls`, `http.server`) and are not in the tree.
  Those need project-dependency resolution (V2) or `--no-gate`. The cost: `record --dry-run` of one forge file
  is 0.06 s wall (release). The heavier-gate and asynchronous-attestation parts of R2 are not built or measured.
* **R3: how often does a clean declaration merge fail the gate?** Take the 59 merge commits of `cancho-forge`
  (the PR merges) and replay each as a declaration merge; count conflicts and
  gate failures, against what a text merge did. *Not measured; this is the claim of `vcs.md` section 4 and it decides
  whether layer 2 of section 6 earns its complexity.*
* **R4: is a content-addressed store larger than git's?** No delta compression is planned in V1 (loose, zlib-free
  objects). Measure the repository sizes. If it is more than a few times git's, packs become V5, not never.
* **R5: how much of `.cho` must the structural diff cover?** `vcs.md` section 4 sizes `compute_diff`+`diff_to_ops`+
  `body_merge` at about 1,500 lines for lex-lang and says cancho's differ in shape. V1 needs only a
  declaration-level diff (what `vcs sync` already does: it knows added, removed, signature-changed, body-changed
  per declaration); the body-level diff is for merge and `replay` and is V2.

## 9. Slices, and what measures each

| slice | content | measured by |
|---|---|---|
| **V1: a local repository** | objects, trees, changes (with recorded anchored edit operations, section 3), refs; `init`, `status`, `diff`, `record` with intent and the built-in gate, `log`, `show`, `blame`, `branch`, `switch`, `revert`, `restore`, `verify`; `import-git` and `export-git` | R1 (SHA-exact round trip on two real repositories); a crash test at every write; two agents racing one ref (`ref-moved`, nobody loses an object); every command's JSON pinned by a test; `cancho` and `cancho-forge` histories open with `log` and `blame` |
| **V2: merge and replay** | the three layers, `conflicts`, `resolve`, `merge --abort`, `replay`; the body-level `.cho` diff | R3 (historical merges), R5; a merge of two branches that git merges clean produces the same tree, except where the declaration merge is finer, and the exceptions are listed |
| **V3: path remotes** | `clone`, `fetch`, `pull`, `push`, `--expect` | two stores, interleaved pushes, an interrupted push leaves both sound (`verify` on each) |
| **V4: HTTP remote and the GitHub bridge** | the gateway, `mirror`, `pull --from-git` | a pull request opened from a mirrored branch; a change pushed to GitHub by a person arrives as a change |
| **V5: forge** | `vcs: "cancho"` rows, workers and pins, packs if R4 says | `cancho-forge` conformance scripts run unchanged against a cancho-VCS repo; one real pilot step end to end |

Each slice ends only when the gate in `CONTRIBUTING.md` passes, no file is over 2,000 lines (the crate is split
by concern: `object`, `tree`, `change`, `refs`, `diff_text`, `diff_cho`, `merge`, `remote`, `bridge`), and every
refusal in the table of tags below has a test that reaches it.

Refusal tags built in V1, each reached by `crates/cancho/tests/vcs_repo.rs`: `not-a-repository`,
`path-escapes`, `file-too-large`, `gate-failed`, `ref-moved`, `ref-protected`, `dirty-working-copy`, `lock-held`,
`object-corrupt`, `object-missing`, `anchor-not-found`, `anchor-ambiguous`, `nothing-selected`, `bad-ref-name`,
`bad-revision`, `revert-conflict`, `revert-unsupported`, `git-unreadable`; also built, with a test of the
library only or none yet: `bad-argument`, `path-unsupported`, `tree-invalid`, `binary-file`, `path-not-found`,
`decl-not-found`, `submodule-unsupported`, `mode-unsupported`. An I/O failure is `io-error` and exits 3, not 1.
Not yet built (V2 onward): `conflicts-open`, `merge-gate-failed`, `rename-edit`, `chain-broken`, `unsigned-change`,
`intent-missing`, `remote-unreachable`, `remote-diverged`. The first list was *(designed)*: `not-a-repository`, `path-escapes`,
`file-too-large`, `gate-failed`, `ref-moved`, `ref-protected`, `dirty-working-copy`, `lock-held`, `conflicts-open`,
`merge-gate-failed`, `rename-edit`, `object-corrupt`, `chain-broken`, `unsigned-change`, `intent-missing`
(only on a ref that requires it), `remote-unreachable`, `remote-diverged`.

## 10. What this does not do

* It does not replace GitHub: pull requests, CI and reviews stay there, fed by `mirror`.
* It does not version a file's meaning across languages: only `.cho` is read structurally; a `.py` or `.sh`
  file is bytes and lines.
* It does not offer history rewriting. A mistake is a `revert` or a `reset-ref` (logged), not a lost change.
* It does not run on its own authority: like every cancho program it is checked for the effects it uses, and
  the CLI performs file and (V3 onward) network effects only, never starts another program.
* V1 is built; R1 did not show its scope to be wrong. Not built, said plainly: `merge`, `conflicts`, `resolve`,
  `replay`, `tag`, `reset-ref`, `gc`, `keys`, remotes, `clone`, signatures and attestation chains on changes,
  `.cho` body-level diff, hunk-level recording, format-aware merge layers, compression. `switch` and `restore`
  work on whole files; `blame` follows first parents only; the histogram diff falls back to one replacement
  when no line occurs 64 times or fewer. `.canchoignore` supports names, `dir/`, `*suffix` and path prefixes
  only. Edit operations are derived at `record` and not for imported changes. A kill leaves a lock that
  `verify` reports and `unlock` removes (a pid-liveness check needs `unsafe` or another program).
