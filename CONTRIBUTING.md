# Contributing to cancho

This is how the compiler itself is worked on. For how to *write*
cancho, read [`AGENTS.md`](AGENTS.md). Everything here is a practice the
repository already follows, and each one that can be checked is checked
by the build.

---

## The gate

CI runs these on linux-x86_64 and darwin-aarch64, and a change is not
done until all of them pass locally:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

`cargo test` includes the conformance suite, which builds and runs real
programs, the documentation link check, and the file budget below.

---

## How a slice is done

1. **The design is written before the code**, in `docs/`, and what it
   claims is measured, not asserted. `docs/README.md` indexes every
   document with its status.
2. **A claim that turns out false is corrected in place**, in the
   document that made it, with the PR that corrected it. It is not
   silently rewritten.
3. **A feature earns its way in when a program asks for it**, counted by
   reading the programs. The bar is two askers (`docs/standard-library.md`).
4. **A test that could not have failed proves nothing.** Anything that
   claims coverage is checked by breaking what it covers and watching it
   fail (`docs/differential.md` §3.1).
5. **Each slice is one PR with a row in `docs/ROADMAP.md`.** The row is
   written after the PR number is known.

---

## The compiler in cancho

The compiler is also being written in cancho (`examples/selfhost`, epic #491, `docs/self-hosting.md`), and the Rust
compiler is its oracle. **A change to what the compiler accepts, refuses or emits is not done until the compiler in cancho
follows it** or the gap is written down. That means a new or changed:

- **rule** (`crates/cancho-syntax/src/rules.rs`): a number and a name in `examples/selfhost/rules.cho`, numbers
  sequential, and the check that raises it;
- **builtin, prelude type or effect label**: the tables regenerate with
  `UPDATE_SELFHOST_TABLES=1 cargo test -p cancho-ir selfhost_tables`, and the code for it goes in `examples/selfhost`;
- **runtime support or code generation** (`crates/cancho-codegen-llvm`): the same text in `examples/selfhost/runtime*`
  and `runtime/` (`runtime/make.sh`), and the backend's behaviour in `emit.cho`;
- **fixture** in `tests/reject/` or `tests/accept/`: it is built through the compiler in cancho by the conformance suite.

What checks this, in order of how early it fails:

1. `selfhost::parity` fails when Rust has a rule or a builtin that no source of `examples/selfhost` names. List it in
   `tests/conformance/selfhost/unported_rules.txt` (or `unported_builtins.txt`) with the issue that will port it, or port it.
2. `refusals_agree_with_the_rust_front_end` fails when a program of `tests/reject/` is refused by another rule than Rust's
   by the compiler in cancho, unless it is in `reject_gaps.txt`; a listed one that now agrees fails until the line is
   taken out, so the list only shrinks.
3. The `selfhost` job of CI runs the whole conformance suite with `CANCHO_COMPILER=auto`. A failure that is not in
   `.github/selfhost-known-differences.txt` is a difference you introduced.

Run 3 locally with `CANCHO_COMPILER=auto cargo test -p cancho --test conformance`. A PR that adds to any of the three lists
says why, in its description, and links the issue.

---

## Files

**No source file over 2,000 lines.** `crates/cancho/tests/files.rs`
enforces it on every `.rs`, `.cho`, `.py` and `.c` file in the
repository. rustc's own `tidy` refuses files over 3,000 lines, and these
crates are smaller. A file over the budget can be given a **ceiling**,
its current size, which may only come down: the number drops as the
file shrinks and the row is deleted once it is under the budget.
Raising a ceiling is not a fix. The table is empty today: the three
files that were over the line (`cancho-ir`, `cancho-codegen`, and the
conformance suite) were each split when the rule arrived.

When a file gets close, split it **by concern**, not by size:

- `lib.rs` is the table of contents. It holds the crate's documentation,
  its `mod` declarations, its public re-exports and its entry points,
  and little else. `cancho-ir` is the example: `ir.rs` is the IR,
  `defs.rs` the declarations, `function.rs` one function at a time, and
  `lower/` the walk over a body.
- **A large `impl` can be split across child modules.** Children see
  their parent's private fields, so `lower/stmt.rs`, `lower/memory.rs`
  and `lower/expr.rs` each hold part of `impl FnLowering` without making
  anything more visible.
- **Unit tests live in their own files**, beside the code. They are
  declared with `#[cfg(test)] #[path = "tests/…"] mod …;` so a test's
  name does not change when it moves.
- **Integration tests are one binary with topic modules**:
  `tests/conformance/main.rs` holds the shared helpers and each topic
  (`traps`, `net`, `ports`, …) is a module beside it. One binary links
  once; a helper used by one topic lives in that topic's file.
- **A move is a pure move.** Splitting a file changes no behaviour, and
  shows that: the same test names and counts before and after, and
  every line of the old file present in the new ones apart from
  formatting.

---

## Code

- **Visibility is as narrow as it can be.** Crate-internal items are
  `pub(crate)`, and the public API is what `lib.rs` re-exports.
- **Imports are explicit in new code.** The modules split out of
  `cancho-ir` start with `use crate::*;` because the split was a pure
  move out of one namespace. New modules name what they use.
- **Every refusal has a rule tag** (`cancho_syntax::Rule`), a located
  span where one exists, and a fixture under `tests/reject/` that
  reaches it (`every_rule_has_a_fixture`).
- **Input never reaches a panic.** An `unreachable!` or `expect` states
  an invariant between the checker and the backend, with its reason in
  the message. If one ever fires, it is reported as an `internal`
  refusal at the function it fired in, not as a crash
  (`docs/internal-errors.md`). `tests/fuzz.rs` goes looking for one in
  every `cargo test` run (`docs/fuzzing.md`).
- **Comments say why, not what.** Each one cites the document or section
  its decision comes from.

---

## Commits and pull requests

A commit message says what changed and why, in prose. A PR describes
what was measured and what was found, including what is *not* done. CI
must be green on both targets before a merge, and a red run is
root-caused, not re-run until it passes.
