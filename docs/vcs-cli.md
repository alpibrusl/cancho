# `cancho vcs` as a complete version-control CLI, agents first

> **Status: slices V1 (a local repository) and V2 (merge, `resolve`, `replay`, a gate that resolves modules outside the tree; section 9) are built; V3 onward is design.** Written before the code (`CONTRIBUTING.md`). A claim marked
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
  `ops`) and offers `apply` of one list to a tree. V2 uses them for `replay` (section 6) and not for merge: R6 (section 8)
  measured what they are worth as a merge, **not more than a line merge**. An imported change has no recorded edits;
  `replay` derives them from its diff when it needs them.
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

## 6. Merge, `resolve`, `replay` *(built in V2; measured in section 8, R3)*

A merge works in three layers and reports in one structured document. Built as follows; what the first draft
of this section claimed and V2 corrected is said where it differs.

1. **Tree layer** (`crates/cancho-vcs/src/merge.rs`): the merge base is the common ancestor that is not an ancestor
   of another common ancestor (of several, the latest by time, then by id; the count is reported as
   `base_candidates`; no common ancestor is `unrelated-histories`). For each path, base/ours/theirs: equal, or
   changed on one side only, resolves without reading content, mode included. A path deleted on one side and
   changed on the other is `delete-modify`, or `rename-edit` when the deleted blob is one the deleting side added
   under another path (a rename with an edit is not followed: **said, not guessed**; git follows it, see R3);
   added on both with different content is `add-add`; two modes is `mode-conflict`; a file where the other side
   has a directory is the refusal `file-directory-conflict`. A merge whose `theirs` is an ancestor is
   `up_to_date`; one whose `ours` is an ancestor is a fast-forward (a ref move, no new change) unless `--no-ff`.
2. **`.cho` layer** (`merge_cho.rs`): a file changed on both sides, that parses on all three, is cut into
   top-level declarations (each with the `//` lines directly above it); every declaration becomes one line of a
   skeleton of the file, the skeleton is merged by line, and any region of the skeleton made only of
   declarations is settled **declaration by declaration**: changed on one side takes that side, changed on both
   alike is one, two declarations added at the same place keep both, changed on both differently (or changed on one
   and deleted on the other) is a conflict tagged `decl-conflict` that names it (`fn m.a`). Two versions of one
   function with the same `cancho-id` identity and the same comments are the same declaration, so a reformat of
   a function conflicts with nothing. *Corrected from the first draft*: "order and layout are re-derived by the
   canonical printer" is **not what is built**: layout and order come from the sides (a whole-file reformat
   conflicts on every declaration both sides edited, unless its identity is unchanged), and no printer runs. Text between declarations
   (imports, the `module` line, loose comments) is merged by line like any other file, and a conflict there
   makes the file a text conflict. The merged tree then goes through the gate: a merge that is clean by
   declaration and fails it is the refusal `merge-gate-failed`, carrying the rule tag, the diagnostics and **both
   tips' intents**; nothing is recorded. `--no-gate` records it, unattested.
3. **Text layer** (`merge3.rs`, `diff_text.rs`): a line-based three-way merge over the histogram diff, whose
   hunks are slid to git's canonical place (found by R3: without it two sides that each append a block to a
   file of repeated lines merged "clean" with the block twice). Hunks of the two sides that overlap **or touch**
   conflict, as in git; an identical change on both sides is one. A conflict is **data, not markers**: the working
   copy is not touched until the merge is recorded, and `conflicts` returns, per path, `{path, tag, layer,
   base, ours, theirs, base_text, ours_text, theirs_text, hunks:[{base_start, base_lines, ours_lines,
   theirs_lines}], decls:[{decl, base, ours, theirs}]}`. A file that is not text is `binary-conflict`.
   *(R6, section 8: no `json` or `toml` file was edited on both sides of any merge, so there is no key-path layer.)*

`merge` records a change with two parents only when every path is settled and the gate passes; otherwise it
records nothing, leaves `.cancho/vcs/merge-state.json` (the three changes and the resolutions so far; the merge
is recomputed from them, deterministically) and **exits 2** with `open_conflicts` in the JSON (not a refusal:
exit 1 is a refusal, 3 an I/O failure). `merge --abort` deletes the state and is exact because nothing was
written; a state whose branch has moved since is dropped as stale. `resolve <path> --take ours|theirs|base`
or `--with <file>` settles a path (`--decl <name>` settles one declaration of a `.cho` conflict; `--with` then
holds the declaration's text); when the last path is settled the same command records the merge (so there is no
`conflicts-open` tag: a merge is either recorded or reported open). `--dry-run` computes and writes nothing,
and reports the tree and change id the real merge would record (the same ids, by test); `--onto <branch>` merges
into a branch that is not checked out (the working copy does not move), `--onto <change>` with `--dry-run`
merges any two changes (what R3 uses); `--expect <id>` is the compare-and-swap on the ref (`ref-moved`); the
working copy must be clean (`dirty-working-copy`) when the checked-out branch is the target. Running a
merge again is a no-op (`up_to_date`) or the same open conflicts; an intent can be given (`--intent-*`),
else the merge's is `Merge <short id>` with the other tip's subject.

**`replay <change> [--onto <branch>]`** re-applies one change onto another base: a merge whose base is the
change's parent and whose "theirs" is the change, except that a text file the change edited is first tried by
re-applying the change's **anchored edits** (section 3) to the onto side, so what was *done* lands where the text now
is, and only then by the line merge. A `.cho` file goes through layer 2. The result is `exact` (the tree equals the
change's), `moved`, or `unchanged` (already applied: nothing is recorded; replaying twice records once), the new change
has one parent, keeps the original's intent and author, and carries the attestation `replay:<change id>`. An edit
whose text is gone is a conflict tagged `anchor-not-found` (`anchor-ambiguous` if it now occurs twice), settled
with `resolve` like any other. Only a change with exactly one parent can be replayed (`replay-unsupported`).

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
  reproduce the file the human merge produced?) was not done in V1 and is R3's second table. R6 counted 22
  merges in `cancho`; `git rev-list --merges HEAD` gives 23 (the 82nd merge of the V2 corpus; the difference in
  R6's script was not traced).
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
  **V2, corrected and measured (2026-11-05).** The V1 explanation was wrong: `cancho-forge`'s `cancho.toml` has no
  `[dependencies]`, and `cancho install` installs nothing for it. The 14 files import `http_client`, `tls`
  and `http.server` from the compiler's `packages/` directory, which forge's scripts pass on the command line
  (`scripts/packages.sh`: `CANCHO_PACKAGES`, else `packages` next to the compiler). So no resolution *inside the
  repository* can find them; the gate now looks in *module roots outside the tree*, in this order: `--gate-path <dir>`
  (repeatable), the lines of `.cancho/vcs/gate-path`, `build/deps` (what `cancho install` fetched, when it
  was run), `$CANCHO_PACKAGES` or the `packages` beside the compiler; dot directories and `build/` inside a root are
  skipped (a package's `.cancho-vcs` holds stored copies of its own sources, which made `buffer` bound twice). Nothing is
  fetched. A module found nowhere is the checker's `unknown-name`, and the refusal lists `unresolved_modules` and the
  roots searched. A std module the tree declares itself (cancho's own `std/`) is used from the tree, not added twice,
  and `.cancho/vcs/gate-skip` (one path prefix a line) excludes fixtures meant to be refused (`tests/reject/`).
  Measured by touching each of the 182 `.cho` files of `cancho-forge` with a comment and `record --dry-run`:
  **168 pass without a root (as in V1), 182 pass with `CANCHO_PACKAGES` set** (`/Users/alfonso/Workspace/alpibrusl/cancho/packages`).
  The gate costs 0.1 s per merge on the historical merges (59 merges: 8 s with it, 6 s without).
* **R3: how often does a clean declaration merge fail the gate, and how does the merge compare with git's?**
  *(measured 2026-11-05 with `scripts/vcs_r3.py`: every two-parent merge reachable from `HEAD` of both repositories is
  replayed with `cancho vcs merge --onto <first parent> <second parent> --dry-run` on an `import-git` of the repository and compared with
  `git merge-tree --write-tree` and with the tree the human merge recorded; git is run by the script, never by the product.)*
  **82 merges: 59 in `cancho-forge`, 23 in `cancho`.** Of them 46 are *trivial* (one parent contains the other: a fast-forward or
  `up_to_date`), 28 are merges of divergent histories that cancho settles clean (16 and 12), and 8 conflict (4 and 4).
  **git's verdict is the same on all 82: 74 clean (55 and 19), 8 conflicts, 0 disagreements**, and on every one of the 74
  that git merges clean, **cancho's merged tree is the tree the human merge recorded** (and so is git's). The 8 conflicts
  are the same merges in both tools: 5 text hunks in `cancho-forge`; 1 text and 6 `.cho` declarations in `cancho`. What
  the corpus does not contain: the `.cho` layer settled **2 paths** of these merges (the corpus is mostly text), so the
  historical merges say little about layer 2. To exercise it, 400 pairs of divergent commits of `cancho-forge` and 300
  of `cancho` were drawn at random (seeded) from every ref and merged the same way (no human result exists; only the
  verdict and the conflicted paths are compared): **`cancho-forge` 400 of 400 pairs agree with git on clean versus conflict
  (163 clean, 237 conflict); `cancho` 299 of 300** (26 clean against git's 25). The one disagreement is the `.cho`
  layer being finer than git, as designed: ours edited `run_clang` in `examples/selfhost/builder.cho` and theirs deleted the two functions
  just above it; the hunks touch, git conflicts, the declarations do not. Where both conflict, the conflicted *paths* are the same in 171 of 237
  (`cancho-forge`) and 160 of 274 (`cancho`) pairs; in the rest cancho conflicts on more paths in 49 and 20 (`rename-edit` and `delete-modify`
  where git follows a rename and merges the edit: the price of the design's "said, not guessed") and on fewer in 3 and 12.
  **The gate.** Of the 28 divergent clean merges, **26 pass the gate and 2 are refused for reasons that are not the merge**:
  `spike/r1/ask.cho` imports `anthropic`, a module in no root searched, and `tests/programs/server_stream.cho` of `cancho` is refused
  `duplicate-declaration` by the gate (a program that needs a build mode the gate does not model; not investigated further).
  On the random pairs of `cancho-forge`, 152 of 163 clean merges pass and **the 11 refused are 8 `spike/r1/ask.cho` (the same
  `anthropic`) and 3 `pilot/contract.cho` (a draft that does not parse at its own parent either)**. So **none of the 191 clean merges (28 + 163) failed the gate because two edits that each check do not check together**, and the claim of `vcs.md` section 4 is not
  contradicted by the corpus, only unsupported by it. The gate *can* do it: the test `a_merge_that_is_clean_by_declaration_can_still_fail_the_gate`
  builds one (one side adds a parameter to a function and updates its caller, the other adds a new caller of the old
  shape) and gets `merge-gate-failed` with both intents. On this evidence layer 2 earns its place only as "finer than git on
  adjacent declarations and on two additions at one place": it changes the verdict in 1 of 700 pairs and 0 of 82 merges, and conflicts on
  fewer paths in 15 of the 511 pairs that conflict. That is small, and it is what is measured.
* **R4: is a content-addressed store larger than git's?** No delta compression is planned in V1 (loose, zlib-free
  objects). Measure the repository sizes. If it is more than a few times git's, packs become V5, not never.
* **R5: how much of `.cho` must the structural diff cover?** `vcs.md` section 4 sizes `compute_diff`+`diff_to_ops`+
  `body_merge` at about 1,500 lines for lex-lang and says cancho's differ in shape. V1 needs only a
  declaration-level diff (what `vcs sync` already does: it knows added, removed, signature-changed, body-changed
  per declaration). **V2 built no body-level diff and found no need for one**: the declaration layer of the merge
  compares a declaration's text and `cancho-id` identity, which settled every `.cho` merge of R3 that did not conflict; a
  *finer* merge (two edits inside one function body) is where a body diff would act, and on the 6 `decl-conflict`s of the
  historical merges git conflicts too. `replay` re-applies anchored text edits and the declaration merge, not typed
  body operations. R5 stays open for typed `OpId`s.

## 9. Slices, and what measures each

| slice | content | measured by |
|---|---|---|
| **V1: a local repository** | objects, trees, changes (with recorded anchored edit operations, section 3), refs; `init`, `status`, `diff`, `record` with intent and the built-in gate, `log`, `show`, `blame`, `branch`, `switch`, `revert`, `restore`, `verify`; `import-git` and `export-git` | R1 (SHA-exact round trip on two real repositories); a crash test at every write; two agents racing one ref (`ref-moved`, nobody loses an object); every command's JSON pinned by a test; `cancho` and `cancho-forge` histories open with `log` and `blame` |
| **V2: merge and replay** *(built)* | the three layers, `conflicts`, `resolve`, `merge --abort`, `replay`; a gate that resolves modules outside the tree; compressed objects and packs (R4); the body-level `.cho` diff is **not** built | R3 (82 historical merges and 700 random pairs, against `git merge-tree`), R5, R2 again (182 of 182 files gate), a kill at every write of `merge`; a merge of two branches that git merges clean produces the same tree, except where the declaration merge is finer, and the exceptions are listed |
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
Built in V2, each reached by `crates/cancho/tests/vcs_merge.rs` (but `unrelated-histories`, reached by a test of the library): the
refusals `merge-gate-failed`, `merge-in-progress`, `no-merge-in-progress`, `not-in-conflict`, `decl-not-found` (also for
`resolve --decl`), `unrelated-histories`, `replay-unsupported`, `file-directory-conflict`, `ref-moved` (of `merge --expect`),
`gate-failed` (with `unresolved_modules`); and the conflict tags a merge reports as data: `content-conflict`, `decl-conflict`,
`delete-modify`, `rename-edit`, `add-add`, `binary-conflict`, `mode-conflict`, `anchor-not-found` and `anchor-ambiguous`
(a replayed edit; every one is in the tests but `mode-conflict` and `anchor-ambiguous`). `conflicts-open` is not a tag:
open conflicts are not a refusal (exit 2). Not yet built: `chain-broken`, `unsigned-change`, `intent-missing`,
`remote-unreachable`, `remote-diverged`. The first list was *(designed)*: `not-a-repository`, `path-escapes`,
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
* V1 and V2 are built; R1 did not show V1's scope to be wrong. Not built, said plainly: `tag`, `reset-ref`, `keys`,
  remotes, `clone`, signatures and attestation chains on changes, the `.cho` body-level diff, typed `OpId`s, hunk-level
  recording, format-aware merge layers, rename detection in merge (`rename-edit` is a conflict, git merges it), the
  canonical-printer re-layout of a merged `.cho` file, `revert` of a merge change, blame through the second parent
  of a merge, and the `vcs` section of `cancho --help` and of the compiler in `examples/selfhost` (the commands exist,
  their usage text does not). `merge` into a detached HEAD works; a kill between the ref move and the working-copy
  write of a merge leaves a working copy that `status` shows as differing from HEAD (`restore --from HEAD` fixes it),
  which the kill test accepts. `switch` and `restore`
  work on whole files; `blame` follows first parents only; the histogram diff falls back to one replacement
  when no line occurs 64 times or fewer. `.canchoignore` supports names, `dir/`, `*suffix` and path prefixes
  only. Edit operations are derived at `record` and not for imported changes. A kill leaves a lock that
  `verify` reports and `unlock` removes (a pid-liveness check needs `unsafe` or another program).
