# Removing Rust: the plan, and the compiler in cancho as the repository's main code

Status: **design, written before any move.** Epic [#491](https://github.com/alpibrusl/cancho/issues/491); the seven gaps it lacked are in
[the comment of 2026-11-05](https://github.com/alpibrusl/cancho/issues/491#issuecomment-6666808122). Numbers are from `main` at `c7435d6e`
(2026-11-05) by `wc -l` and by reading `docs/bootstrap.md`, `docs/cli-in-cancho.md` and `docs/standalone-cancho.md`; nothing was re-run. A
claim below that turns out false is corrected here, in place.

## 1. What "Rust" is, in this repository

| part | lines | what it is | what replaces it |
|---|---|---|---|
| `crates/cancho` `src/` | ~17,000 (60k with tests) | the command line, `selfhost_build.rs`, `build.rs` (generates the tables the compiler embeds) | `examples/selfhost/cli.cho` (77 files, built, §39 of `cli-in-cancho.md`) |
| `crates/cancho/tests/` | ~42,600 in the 109 files of `conformance/`, plus `files.rs`, `fuzz.rs`, `vcs_repo.rs` | **the oracle**: conformance, parity, `files.rs` (the 2,000-line ceiling), fuzz, the `selfhost` parity tests | `cancho test` over the same programs, or a Rust shell kept on purpose (decision D3 below) |
| `cancho-syntax`, `-types`, `-id`, `-ir` | ~35,000 | lexer, parser, checker, identities, IR | `lexer.cho`, `parser.cho`, `checker.cho`, `ids.cho`, `emit.cho` and the rest of `examples/selfhost` |
| `cancho-codegen` | ~9,700 | Cranelift backend | nothing: retired (#666, D6) |
| `cancho-codegen-llvm` | ~15,000 | LLVM text backend | `emit.cho`, `builder.cho` |
| `cancho-vcs` | ~5,300 | V1 repository store | #680 (the native port; blocked on #684 to #688) |

There is **one binary**, `cancho` (`crates/cancho/Cargo.toml`). "All the binaries" therefore means: the release assets of every target
(`.github/workflows/release.yml`), `scripts/install.sh`, and the `cancho-rust` backup asset, which is retired.

## 2. Where it stands

The compiler in cancho (`examples/selfhost`: 97 files, ~49,700 lines, plus ~17,400 of `runtime/`) is already the default of `build`, `run`
and `test`; a standalone `cancho` written in it reproduces itself (stage 2 equals stage 3, `scripts/build-standalone.sh`); a release
built that way is published. What Rust still does: stage 0 of that build, the oracle, Cranelift and WebAssembly, `authority`, the V1
`vcs`, and the CI gate (`cargo fmt`, `clippy`, `test`).

## 3. Decisions to take first (each with a date and an owner in #491)

- **D1. Remove, not keep.** `standalone-cancho.md` §6 calls the crates "a backup". Keeping them means two implementations to hold equal for
  ever. This plan assumes removal; if D1 is "keep", only §5 phases 0 to 2 apply and the repository layout of §6 does not change.
- **D2. Cranelift is retired with the Rust compiler** (#666 says so; it is open). WebAssembly: port to `emit.cho` (#486) or retire with it.
  Both change what a release promises, so each needs a release note.
- **D3. The oracle's shape.** The 726 conformance tests run `cancho` and compare output. Two ways: (a) `cancho test` runs them as cancho
  programs with a `.expected` file each, no Rust anywhere; (b) a small Rust shell stays as the test runner only. (a) is the goal; (b) is
  allowed as a stage, with a date for its end.
- **D4. Windows.** Neither compiler targets it (#490). Decide before the release pipeline is rewritten, since it is a column in the matrix.

## 4. Gates: what must be true before each crate is deleted

A crate is deleted in a PR by itself, only when:
1. nothing in the workspace names it (`cargo tree -i CRATE` is empty; checked by CI);
2. the standalone `cancho` built **without** it passes the whole oracle, on all three targets (macOS aarch64, Linux x86-64, Linux aarch64);
3. the goldens (§5, phase 1) stand in for what it was the oracle of;
4. the fixed point holds: stage 2 equals stage 3, built from the seed.

## 5. Phases

**Phase 0: housekeeping and the decisions (days).** Take D1 to D4. Correct the stale numbers (#491's body, `bootstrap.md` "22,000 lines",
the conflict marker at `docs/cli-in-cancho.md:619-620`). Close the primitives that block ports: #684 to #688.

**Phase 1: the seed and the goldens (#488).** (a) A committed seed: the LLVM IR or object of the standalone compiler for each target with
a recorded BLAKE3, reproducible from `cancho.files` plus `clang`; a script `scripts/reseed.sh` that rebuilds it and a CI check that the
seed rebuilds itself to the same bytes. (b) Golden outputs: for every test whose expected value is "what the Rust compiler writes" (parity,
identities, IR, refusals), record the output once and compare to the file. After this the Rust compiler is no longer the only way to know
the answer.

**Phase 2: close the compiler's own gaps (#482, #485, #487, #665).** The remaining CLI commands, the runtime text generated from the Rust
crates (`build.rs`) moved to checked-in files with a test that regenerates them, the device verbs, `authority`, `--emit obj`, `-l`/`-L`.
`vcs` V1 (#680) in cancho. Done when `CANCHO_COMPILER=auto` skips nothing in `.github/selfhost-known-differences.txt` that is not a retired
backend.

**Phase 3: the gate moves (D3).** `cancho test` runs the conformance programs; the Rust test shell shrinks file by file as each moves
(a test file is moved or deleted, never both left). `tests/files.rs`'s 2,000-line check becomes a script or a cancho program.
`CLAUDE.md`, `CONTRIBUTING.md` and `AGENTS.md` change from `cargo fmt/clippy/test` to the new gate **in the same PR** that makes the new gate
required, not before.

**Phase 4: the release without Rust.** `release.yml` builds from the seed (stage 0 = the seed, not a Rust build), runs stages 1 to 3,
publishes stage 2, and the smoke test (`install.sh` then build hello) stays. The `cancho-rust` backup asset is last published in the release
before deletion and named in its notes.

**Phase 5: delete the crates**, in this order, each a PR with the §4 gates: `cancho-codegen` (Cranelift), `cancho-codegen-llvm`, `cancho-ir`,
`cancho-syntax`, `cancho-types`, `cancho-id`, `cancho-vcs`, then `crates/cancho` and the workspace `Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`, `rustfmt.toml`, `benches/` if Rust.

**Phase 6: the move (§6).**

## 6. The compiler in cancho becomes the repository's main code

After phase 5 the repository's top level is the cancho compiler, not `examples/`. Proposed layout (a rename, no content change, one PR so
history follows with `git mv`):

```
compiler/        was examples/selfhost/  (the 97 files, cancho.files, runtime/)
std/             unchanged
tests/           unchanged (accept, reject, programs, ...); the oracle's programs and goldens
packages/        unchanged
scripts/         build-standalone.sh, install.sh, stamp-version.sh; the Rust-only ones removed
seed/            the committed seed of phase 1
docs/
```

Things that name `examples/selfhost` and move with it: `cancho.files`, `build-standalone.sh`, `stamp-version.sh`, the cache key (a hash of its
sources), `CANCHO_SELFHOST_SOURCES`, `docs/*.md` links (a link checker belongs in the gate), and the generated tables. Moving it is safe only
**after** the Rust crates are gone, since `selfhost_build.rs` embeds the sources by path; doing it earlier means two layouts to keep working.

## 7. Risks, and what each is measured by

- **No oracle but the compiler itself.** A bug present in the seed and in the compiler passes the fixed point. Measured by: the goldens
  (phase 1) and the reject corpus (339 of 339 today), kept and not regenerated from the new compiler.
- **Seed rot.** A seed nobody rebuilds stops matching the source. CI rebuilds it every PR that touches `compiler/`, and fails on a diff.
- **Code quality.** Measured once and again with `-O2` (`bootstrap.md`: within 1.3 on 36 rows; the P-256 rows 1.03 to 1.05). The
  `tls-perf` job is pinned to the Rust compiler; its baseline must be re-recorded with the compiler in cancho **before** phase 5.
- **Compile time and `clang`.** 33 s a stage for the 77-file command line. The compiler still needs `clang` (#489); say so in the install docs.
- **Windows.** Out of scope until D4.
- **Flaky tests** (`dup2`, #592) are noise in the gates; fix or quarantine them in phase 0.

## 8. Order of work, and what runs in parallel

Serial: D1 to D4 → phase 1 → phase 3 gate → phase 4 → phase 5 → phase 6.
Parallel with phase 1: phase 2's independent items (#684 to #688 primitives, #680, #665, #485, #487), each its own PR.

## 9. What this document does not do

It moves no file and deletes nothing. Every phase is a PR of its own, and none merges without the owner's word.

## 10. What the open work adds to the removal (2026-11-05, from #491's comments and the open PRs)

`main` has not moved since #683 (the audit). Two open pieces change what the removal carries:

- **#681 (draft: `ci`, `profile`, `trace`, `-g`)** adds no compiler Rust but +955 lines in `crates/` (`acli.rs` +90 and 865 of tests) and 4,891 lines in
  `examples/selfhost`. Its comment on #491 names what the removal must do for it: (1) the command tree (`commandtree.cho`, `--help`, `introspect`,
  `skill`) is **generated from the Rust binary's own output**, with the three commands advertised only under `CANCHO_ACLI_EXTRAS`, so the generator
  moves to the cancho side; (2) its four tests (`ci.rs`, `debug.rs`, `profile.rs`, `trace.rs`, ~850 lines) are the only tests of those commands, and
  need process spawning and a JSON reader in a cancho test (both exist: `std.process`, `std.json`); (3) `debug.rs` needs `gdb`, `llvm-dwarfdump` and
  `objdump` and very likely **skips on CI**, so green does not mean it ran; (4) three generated modules (`commandtree.cho`, `guidelines.cho`,
  `instrtext.cho`) have Rust generators in `statics.rs`; (5) **every new `.cho` file is listed twice**, in `cancho.files` and in `with_front_end()` in
  `selfhost.rs`: the plan needs `cancho.files` as the only list; (6) `-g`, `--instrument`, `-O0` exist only in the compiler in cancho, so the Rust
  compiler cannot be their oracle.
- **#691 (`std.flate`) and #690/#692 (SHA-1, `vcsobj.cho`)** add one line of Rust (the std table in `main.rs`, registration only) and no compiler code; each
  new std module costs that line until `main.rs`'s `STD` table is generated from `std/` (or read from `stdorder.cho`): add it to phase 2.

**Moving `examples/selfhost` to the root now** (the goal stated 2026-11-05) conflicts with these: #681 touches 22 files there, #690 and #692 add files
there, and `selfhost_build.rs` and about a dozen tests name the path (`include_str!`, `selfhost.rs`, `bootstrap.rs`, `statics.rs`). A `git mv` is one
PR, but any open PR touching the directory has to be rebased onto the new paths. The order that costs least: merge or rebase #681, #690, #691 and
#692 first; then the move as one PR that changes the path constants and nothing else (measured by `cancho.files` regenerating to the same list and
the `selfhost::` group passing); then the deletions of phase 5. The move itself does not remove Rust, so it can wait for phase 3 without blocking
anything; the new code written meanwhile (`examples/selfhost/test.sh`, the `vcs*.cho` files) uses paths relative to its own directory so it moves with it.
