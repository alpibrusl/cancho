# Toolchain pins: one place, moved by a command, accepted by a gate

> **Status: design, written before the code.** Nothing here is built. Every number is either measured (with the command) or marked a choice or open.

## 1. What asked for it

A repository pins the compiler by commit (`package-system.md` §8.4). Moving that pin to the release that is
built by cancho (`standalone-cancho.md`) meant, for each of the sibling repositories, editing the same hash in
several places by hand (`scratch/repin.py` did it, and is not in this repository), re-running the generators, and
finding out from CI what the new compiler could not yet build. Five compiler gaps (#650, #651, #653, #654,
#655) were found that way: by a sibling's CI, *after* a release.

**Measured, 2026-10-30**, in the working copies of the 24 repositories that name a pin:

```
git grep -lI "<the 40-hex commit>" | wc -l      # per repository, on the branch that carries the pin
```

The pin is written in **52 files**: 1 file in 13 repositories, 2 in 7, 3 in `cancho-fetch`, 5 in `cancho-stats`, 6 in
`cancho-pg`, 11 in `cancho-ml`. What the files are: `cancho.toml` (all of them), `.cancho-rev` and the CI workflow
(the file that makes CI install the compiler), documents (`docs/numerics.md`, `docs/serving-http.md`,
`docs/board.html`), generated files that embed the pin text (`generated/*/built.cho`, `generated/mcp/tools.cho`,
`tests/conformance/golden_describe*.json`), gate scripts that check for the pin (`scripts/mutants_*.py`,
`scripts/check_workflow.py`), and vcs `requires/*.json` lock files. The count is for the full hash only; a
shortened form (8 hex digits in a sentence) is not counted, so it is a lower bound.

Only `cancho.toml` should hold it. Everything else either reads it or does not need it.

## 2. What this has to fit

The package system's own commitments (`package-system.md` §3) apply to the compiler as to a library:

* **A name is chosen once and resolved by hash forever after (§4.5).** The pin stays an exact commit. There are no
  ranges: a range makes a build depend on what is published today.
* **No code runs to resolve (§4.3), and a dependency is data.** Moving a pin is a command someone runs, never
  something a build does by itself.
* **Refuse, don't downgrade (§4.4).** A new compiler must not silently widen what a program may do: the authority
  report before and after is compared, and a widening is refused until someone accepts it.
* **A pin is not proof it still compiles (§4.2).** A new pin is accepted by the repository's own gate passing
  under the new compiler, not by the release existing.

## 3. Slice 1: one place, and a command that moves it

**The pin lives in `cancho.toml`** (`[package] cancho = "<commit>"`, as now). Nothing else holds the hash:

* `scripts/install.sh --project [dir]` reads the pin from `cancho.toml` (found upwards, like the other project
  commands) and installs that release. CI runs that line and no workflow names a hash. `.cancho-rev` is deleted
  where a repository has one; `cancho-forge` keeps it equal today by convention and a check, and moves to this.
* Documents say "the commit in `cancho.toml`", as `cancho-gpu`'s `CLAUDE.md` already does.
* Generated files and goldens that embed the pin text: **open (§6)**. Whether they must is measured first (§6, M2);
  the answer decides whether the compiler or the generator changes.

**`scripts/toolchain.sh update [--to COMMIT]`** (a script first, a subcommand later, a *choice*: moving a pin is data
movement and running a gate, and `install.sh` already holds the download, checksum and commit checks it reuses):

1. `--to` defaults to the commit `releases/latest` redirects to (the same lookup `install.sh` has).
2. Installs that release into a scratch prefix, with the checks `install.sh` makes (checksum, the binary reports the
   commit). Nothing is touched on a refusal.
3. Runs the repository's gate with that compiler first on `PATH`. The gate is a command the project names:
   `[toolchain] gate = "scripts/gate.sh"` in `cancho.toml` (a key this slice adds; unknown keys stay refused).
4. Compares `cancho authority` for each program the project pins an authority file for (the files its gate already
   compares) before and after. **A widening refuses** unless `--accept-widening` is given; a narrowing or no change
   applies. This is the §4.4 rule, applied to the compiler.
5. Only if 3 and 4 passed, rewrites the pin in `cancho.toml` and nothing else, and prints the one-line diff.
   `--check` does 1 to 4 and writes nothing (a CI job runs it on a schedule and opens the pull request).

Exit codes follow `install.sh` (2 usage, 3 no platform, 4 checksum or commit mismatch, 5 no release) and add 6 for a
gate that failed and 7 for a widening.

**Fixed before the code** (the gate this slice is judged by, and it must be able to fail):

* G1: with a gate that exits non-zero, `cancho.toml` is byte-identical afterwards, and the exit code is 6.
* G2: with a release whose checksum is wrong, nothing is installed, `cancho.toml` is unchanged, and the exit code is 4.
* G3: with a compiler whose authority report for a pinned program is wider, the exit code is 7 and `cancho.toml` is
  unchanged; with `--accept-widening` it moves.
* G4: after a successful update, `git diff --stat` shows `cancho.toml` only.
* G5 (the migration's measure): `git grep -lI <pin>` in a migrated repository lists `cancho.toml` and nothing in
  the "should not hold it" list of §1, except what §6 M2 shows must.

## 4. Slice 2: the release is gated on the ecosystem

Release 2 was published and then three siblings found gaps (#653 by `cancho-client-gen`, #654 by `cancho-pg`, #655
by `cancho-ml`). The release workflow gains a job before `publish`: it builds the candidate, then runs each
listed repository's gate against it.

* `ecosystem.toml` in this repository lists `repo`, `ref` (`main`) and the gate command of each. A repository that
  has no gate yet is not listed, so adding one is a visible act.
* The job needs no new authority: it checks out public repositories and runs their scripts with the candidate on
  `PATH`, which each repository's own CI already does.
* **Open:** the cost. The two slowest legs of this repository's CI are the macOS ones (35 to 40 minutes each, measured
  on #655); about 20 repositories each running a build and a test set may be too slow to gate every release. The
  first version lists the five that found the most (`ml`, `client-gen`, `pg`, `code`, `web`); the cost is measured
  with those before the rest are added.

## 5. Not in these slices

* **The toolchain as an entry of the vcs store**, with a release's conformance result recorded beside it, so that
  `cancho install` resolves the compiler like a library and `vcs dependents` says which repositories a release
  touches. This is the larger change and the one that makes the store the first source of truth for the whole
  dependency set; it waits for slice 1 to show what the lock needs to hold, and for a second asker.
* **A compatibility ledger** (a release says: repository X at commit Y passed its gate with me). Slice 2's job
  produces exactly the data; whether it is recorded and signed is decided after it has run a few releases.
* Version ranges, a registry, a channel that moves by itself. Refused, for the reason in §2.

## 6. To measure before the code

* **M1.** How many of the 52 files are *read* as the pin (CI, `.cancho-rev`) and how many are *copies* of it
  (documents, goldens). Counted by file in §1; the split is by reading each, and recorded here.
* **M2.** Why the generated files embed the pin text. `generated/*/built.cho` and the goldens are written by the
  repository's generator from `cancho.toml` or from `--version`; if from `cancho authority`, then it is the compiler
  (the report names the `cancho` row) and an update regenerates them by running the repository's generator, which
  the gate already does. Read each generator before writing G5's exception list.
* **M3.** The time `toolchain.sh update --check` takes on `cancho-hooks` (two files, three scripts), `cancho-stats`
  (the slowest gate) and `cancho-ml`.

## 7. Migration

The order is by risk, one pull request per repository, each **draft until the maintainer says merge**, none
force-pushed, and none merged before the release the pin moves to exists.

| Wave | Repositories | What changes |
|---|---|---|
| 0 | `cancho` | release 3 (it carries #653 to #655); `install.sh --project`, `toolchain.sh`, G1 to G4 as conformance tests |
| 1 | the 13 repositories with the pin in one file (`client-gen`, `code`, `gateway`, `gpu`, `hdl`, `iso8583`, `log`, `oci`, `opt`, `schema`, `table`, `tools`, `web`) | `cancho.toml` only; CI runs `install.sh --project` |
| 2 | the 7 with two files (`cache`, `dns`, `forge`, `hooks`, `mqtt`, `mqtt-client`, `robot`) and `fetch` (three) | the same, and delete `.cancho-rev` and any copy in a document |
| 3 | `stats` (5), `pg` (6), `ml` (11) | documents say "the commit in `cancho.toml`"; the gate scripts that check the pin read it from `cancho.toml` instead of holding a copy; generated files per M2 |
| 4 | repositories with no pin yet (`contract`, `industrial`, `recog`, `ui`, `verifactu`, `xades`, `xml`, `www`, `spikes`, `ev-evidence`, `quantum`) | each adopts `[package] cancho` and the install line when it next needs the compiler; none is touched now |

A repository is done when G5 holds for it and its own CI is green on the installed release. Waves 1 and 2 run as
soon as release 3 exists; the pull requests that move to release 2 today (`cancho-code#101`, `cancho-forge#53`,
`cancho-fetch#13`, and the sibling branches) are re-pinned once, by hand, to release 3 and then are the first to use
the command.
