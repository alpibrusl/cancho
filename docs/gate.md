# The gate without Rust

Phase 3 of [`removing-rust.md`](removing-rust.md) (decision D3): what "done" means for a change once `cargo` is gone, and the order in which
the checks that run under `cargo` move. Nothing here changes the gate yet; each step below is its own PR, and the documents that name the
gate (`CLAUDE.md`, `CONTRIBUTING.md`, `AGENTS.md`) change **in the PR that makes the new gate required**, not before.

## 1. What the gate is today (measured 2026-11-07)

`CLAUDE.md`: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`. Under `cargo test`:

| What | Count | Dies with |
|---|---|---|
| tests in `crates/cancho/tests/` (conformance, `files.rs`, `fuzz.rs`, `vcs_repo.rs`) | 850, in 108 files of `conformance/` plus the 3 | the Rust shell (this phase) |
| unit tests in `crates/cancho-*` (ir 304, syntax 135, id 66, codegen-llvm 64, vcs 48, types 19, codegen 15) | 651 | their crate (phase 5) |
| cancho-native tests (`*_test.cho`, run by `cancho test`) | 23 files | nothing: they are already the target |

Of the 850, the work of phase 1b is done for the ones that named a Rust crate: after #717 to #721 only the guard test
`rules_and_builtins_are_the_goldens` (in `selfhost/parity.rs`) calls `cancho_syntax` or `cancho_ir`; it goes with them. What is left is
tests that run the `cancho` binary on a fixture and compare what it says. They do not need Rust; they are in Rust because that is where
the harness was.

CI today (`ci.yml`): `build`, `conformance` (4 shards on 2 targets; a shard of linux-x86_64 took 529 s in the run of #716), `selfhost` (the
same suite through the compiler in cancho), and the seed checks (`seed-chain`, `seed-fixed-point`, `seed-ir`, `reseed`).

## 2. The new gate

One script, `scripts/gate.sh`, that a person and CI both run, with these stages in this order (cheapest first), stopping at the first
failure and printing which stage it was:

1. **fmt**: `cancho fmt --check` over the corpus of `conformance/formatting.rs` (55 s measured with the Rust binary in a debug build).
2. **files**: no source file over 2,000 lines, with the ceilings table of `files.rs`; a cancho program (`scripts/files-budget.cho`).
3. **seed**: `scripts/seed-build.sh` against the committed seed, and check 1 of `seed.md` §5.
4. **walk**: `tests/run.cho` over `tests/accept`, `tests/reject` and `examples/` (G3), and `cancho test` over every `*_test.cho` (23 files now) as the
   conformance programs move (§3, G4).
5. **reject**: `scripts/reject_golden.py check` against `tests/reject.golden` (rules; `--exact` is the Rust front end's wording and goes with it).

There is no clippy stage: with no Rust there is no Rust lint. The cancho equivalent of "warnings are errors" is that `cancho check` refuses
a program (every refusal has a rule tag), and stage 1 (`fmt --check`) holds the layout.

What stays outside `gate.sh` and stays required in CI as separate jobs: the seed's fixed point (`seed-fixed-point`: it runs the compiler
three times and a person should not wait for it), the cross-target matrix (macOS, Linux x86-64 and aarch64), and the `tls-*` assurance jobs.

## 3. Moving the 850

A test file is moved or deleted, never both left (`removing-rust.md` phase 3). Three kinds, by what they assert, with the way each moves:

- **Fixture walkers**: run every file of `tests/accept`, `tests/programs` or `tests/reject` and compare with the directives the file carries
  (`//~ STDOUT`, `//~ EXIT`, and the rule a reject fixture names). One cancho program, `tests/run.cho`, replaces all of them: 130 accept fixtures, 393 reject fixtures and the 18 programs of `examples/` (**measured 2026-11-07;
  this document first said 96 for `tests/programs`, which no walker walks: those are drivers that single tests run, and they move with those tests in G4**). This is the
  biggest block and the most mechanical, so it goes first.
- **Command tests**: write a small program to a scratch directory, run `cancho` on it, check the exit status, stdout and stderr, and sometimes
  the file the command wrote (`formatting.rs`, `project.rs`, `processes.rs`, ...). These become `*_test.cho` using `std.process` (`capture`
  exists) (the scratch-directory helper they need is the one thing to write first, in G4's first PR); each Rust file becomes one `_test.cho` file of the same name.
- **Differential and fuzz tests** (`fuzz.rs`, the Python differentials under `scripts/`): not moved by this phase. They already run
  outside `cargo` (the scripts) or compare two compilers (`fuzz.rs`), which no longer exist side by side once the crates go; each is
  retired with the thing it compares and `removing-rust.md` records which one went with what.

Order: files that name no Rust crate and spawn only `cancho` first (all of them, since phase 1b), starting with the smallest, so the pattern
is settled on `http_server_bytes.rs` (34 lines) before it is applied to `backends.rs` (74 tests, 1,531 lines).

## 4. Steps

| # | Step | Done when |
|---|---|---|
| G1 (**done**) | `scripts/files-budget.cho` | it refuses the same files `files.rs` does, measured on the repository and on a copy with one file over the budget |

G1, measured: on the repository both `files.rs` and `scripts/files-budget.cho` find nothing; on a planted tree the program refuses a 2,001-line `.rs`, `.c` and `.py` file (the last with no newline at its end), allows 2,000 lines, skips `target/`, a hidden directory and a `.txt` file, and follows a link to a directory, as `files.rs` does. `files.rs` stays until G5, not G2 as first written here: the `gate` job of G2 is not a required check, so deleting `files.rs` in G2 would leave the budget unenforced until G5 makes the job required. The two checks run side by side for that time; `files.rs` goes in the G5 PR.
| G2 (**done**) | `scripts/gate.sh` with stages 1 to 3 and 5; runs in CI as its own job, not yet required | green on `main`, red on a planted failure in each stage |
| G3 (**done**) | `tests/run.cho` (fixture walker) | its result equals the Rust walkers' on all 541 fixtures (130 + 393 + 18); the Rust walkers deleted in the same PR |
| G4 | the command tests, one file per PR | each file's tests pass as `_test.cho` under both compilers; the `.rs` deleted in the same PR |
| G5 | `gate.sh` becomes the required check; `CLAUDE.md`, `CONTRIBUTING.md`, `AGENTS.md` change | the Rust shell is empty: `cargo test --workspace` runs only the unit tests of crates still on disk |

G2, measured (macOS aarch64): `scripts/gate.sh` with no `--cancho` builds the compiler from the committed seed (stages 0 to 3, no Rust) and runs fmt, files, seed and reject in **44 s** in all (57 s with a debug Rust build given as `--cancho`, which is slower at `fmt`); that is under the minute the next section asks for, with stage 4 still to come. Planted failures, each by `--only STAGE`: a badly laid-out file under `examples/` fails `fmt`, a 2,001-line `.py` file fails `files`, a wrong hash in `seed/*.sha256` fails `seed`, and a changed rule in `tests/reject.golden` fails `reject`; each exits 1 and prints `gate: FAILED at stage NAME`. The job is `gate` in `ci.yml` on linux-x86_64 and darwin-aarch64, not required. Not measured: Linux timings (read them off the first run of the job), and "green on `main`", which needs this PR merged.

G3, measured (macOS aarch64): with the Rust `cancho` as the compiler under test `tests/run.cho` passes 393 reject, 130 accept and 18 example programs, the same that the three Rust tests it replaces passed (`accepted_programs_build_and_run`, `refused_programs_are_refused_with_the_stated_reason`, `every_example_runs_and_prints_what_it_says` keep their names, so `.github/selfhost-known-differences.txt` and the shard weights need no change; each now builds `run.cho` and runs it). With the compiler written in cancho and `--selfhost` it passes 384 reject (the 9 of `tests/reject.known` out, and the sentence of each `ERROR` not compared: that compiler says the rule's sentence by decision), 129 accept and 16 examples; what it cannot build is in `tests/run.known`, one line each with the reason (`aliasing_in_a_slice_of_references`, whose `alloc_slice[r](4, s)` it has no code for, and the two device examples, #665). Planted failures, each found: a changed `STDOUT`, a wrong `EXIT`, an undeclared `STDERR`, a wrong `RULE`, and an `ERROR` text not said. Speed: the reject walk takes 2 s, the accept walk 55 s with the Rust binary, the 18 examples 7 s; the new `walk` stage of `gate.sh` takes about 2 minutes with the compiler in cancho (it builds 145 programs with it), so the whole gate is 2 m 47 s on this machine and `--skip walk` is the quick one. What stays in Rust for G4: `the_prose_is_unchanged_by_the_json` and `every_refusal_is_valid_json_with_a_position` (`refusals.rs`; the second needs a strict JSON reader) and `every_rule_has_a_fixture` (reads `tests/rules.golden`), which still walk `tests/reject` and are command tests in kind. The environment of what `run.cho` runs is a `PATH` and, if asked, `CANCHO_COMPILER` and `CANCHO_CACHE` (`--compiler`, `--cache`): a cancho program has no environment to pass on, so the Rust tests give it theirs. `run.cho` and `scripts/` are now in the `fmt --check` corpus (`gate.sh` and `formatting.rs`).

G5 is the only step that changes what a contributor is told to run, and it is the last.

## 5. Risks, with how each is measured

- **A moved test is weaker than the Rust one.** #721 is the example: `ids` checks a whole program, so the identity comparison covers fewer files
  than the Rust test hashed. Each such PR says in its body what narrowed and by how much, and keeps a count assertion so it cannot narrow
  silently.
- **A test that no longer runs.** A `*_test.cho` that `cancho test` does not find passes by absence. `gate.sh` counts the tests it ran and
  compares with a number recorded in the repository (the way `TESTS` is in `http_server_bytes.rs`).
- **Speed.** The conformance suite is 4 shards of about 9 minutes; `gate.sh` is for the person at the keyboard and must stay under a
  minute for stages 1 to 3 on a release binary (to be measured in G2, not assumed).
