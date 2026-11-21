# Contributing to cancho

This is how the compiler itself is worked on. For how to *write*
cancho, read [`AGENTS.md`](AGENTS.md). Everything here is a practice the
repository already follows, and each one that can be checked is checked
by the build.

---

## The gate

CI runs the gate on linux-x86_64 and darwin-aarch64, and a change is not
done until it passes locally:

```sh
scripts/gate.sh
```

It builds a compiler from the committed seed (no Rust, only clang) and
runs seven stages: `fmt` (`cancho fmt --check`), `files` (the file budget
below), `links` (every relative link and `examples/`, `compiler/` path in the documents exists), `seed` (the committed seed matches its hash and builds), `walk`
(`tests/run.cho` over `tests/accept`, `tests/reject` and `examples`),
`tests` (the `cancho test` runs of `tests/cancho-tests.txt` and the
scripts of `tests/commands`, which build and run real programs) and
`reject`. `scripts/gate.sh --only STAGE` runs one. `docs/gate.md` says what
each stage replaced and how it was measured.

That is the whole gate: there is no Rust left in the repository to check
(`docs/removing-rust.md`, Phase 5). Needing `clang` is all it takes.

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

The compiler is written in cancho (`compiler`, epic #491, `docs/self-hosting.md`); the Rust compiler that was its oracle
is deleted, and its recorded answers (`tests/vectors/`) are the oracle now. **A change to what the compiler accepts, refuses or emits
is not done until the sources, the recorded answers and the docs follow it.** That means a new or changed:

- **rule** (`crates/cancho-syntax/src/rules.rs` before it was deleted; now `compiler/rules.cho` alone): a number and a name in `compiler/rules.cho`, numbers
  sequential, and the check that raises it;
- **builtin, prelude type or effect label**: the tables are checked-in data (`scripts/builtin_tables.py check|append`; the Rust test that generated them is deleted, `docs/removing-rust.md` Phase 5), and the code for it goes in `compiler`;
- **runtime support or code generation**: the text in `compiler/runtime*` (the `.ll` and `.c` files under `runtime/` are
  sources now, with `regen.cho` embedding them in `runtime.cho`) and the backend's behaviour in `emit.cho`; the Rust LLVM backend that once
  wrote them is deleted (`docs/removing-rust.md`, Phase 5);
- **fixture** in `tests/reject/` or `tests/accept/`: it is built through the compiler in cancho by stage `walk` of the gate.

What checks this, in order of how early it fails:

1. `tests/commands/parity.sh` fails when Rust has a rule or a builtin that no source of `compiler` names. List it in
   `tests/unported_rules.txt` (or `tests/unported_builtins.txt`) with the issue that will port it, or port it.
2. Stage `walk` of `scripts/gate.sh` fails when a program of `tests/reject/` is refused by another rule than the one it declares
   (`//~ RULE`) by the compiler in cancho, unless it is in `tests/reject.known`; a listed one that now agrees fails until the line is
   taken out, so the list only shrinks.
3. Stage `tests` of the gate runs the scripts of `tests/commands` against the compiler in cancho; their expected outputs
   are the Rust compiler's recorded answers (`tests/vectors/`), so a difference is one you introduced.

A PR that adds to any of the lists (`tests/unported_rules.txt`, `tests/unported_builtins.txt`, `tests/reject.known`)
says why, in its description, and links the issue.

---

## Files

**No source file over 2,000 lines.** Stage `files` of the gate
(`scripts/files-budget.cho`) enforces it on every `.rs`, `.cho`, `.py` and `.c` file in the
repository. rustc's own `tidy` refuses files over 3,000 lines, and these
crates are smaller. A file over the budget can be given a **ceiling**,
its current size, which may only come down: the number drops as the
file shrinks and the row is deleted once it is under the budget.
Raising a ceiling is not a fix. The table is empty today: the files
that were over the line were each split when the rule arrived.

When a file gets close, split it **by concern**, not by size:

- `lib.rs` is the table of contents. It holds the crate's documentation,
  its `mod` declarations, its public re-exports and its entry points,
  and little else. `compiler` follows it: `ast.cho` is the tree,
  `parser.cho` the grammar, and `rules.cho` the refusals.
- **A large `impl` can be split across child modules.** Children see
  their parent's private fields, so `parser/*.rs` each hold part of the parser without making
  anything more visible.
- **Unit tests live in their own files**, beside the code. They are
  declared with `#[cfg(test)] #[path = "tests/…"] mod …;` so a test's
  name does not change when it moves.
- **Integration tests are scripts**: a test of a command or a program is `tests/commands/NAME.sh` and its
  `NAME.expected`, named in `tests/commands/script_tests.cho`; one that needs no shell is a `cancho test` file named in
  `tests/cancho-tests.txt`.
- **A move is a pure move.** Splitting a file changes no behaviour, and
  shows that: the same test names and counts before and after, and
  every line of the old file present in the new ones apart from
  formatting.

---

## Code

- **Visibility is as narrow as it can be.** Crate-internal items are
  `pub(crate)`, and the public API is what `lib.rs` re-exports.
- **Imports are explicit in new code.** New modules name what they use.
- **Every refusal has a rule tag** (`cancho_syntax::Rule`), a located
  span where one exists, and a fixture under `tests/reject/` that
  reaches it (`every_rule_has_a_fixture`).
- **Input never reaches a panic.** An `unreachable!` or `expect` states
  an invariant between the checker and the backend, with its reason in
  the message. If one ever fires, it is reported as an `internal`
  refusal at the function it fired in, not as a crash
  (`docs/internal-errors.md`). `tests/commands/fuzz.sh` goes looking for one in
  every gate run (`docs/fuzzing.md`).
- **Comments say why, not what.** Each one cites the document or section
  its decision comes from.

---

## Commits and pull requests

A commit message says what changed and why, in prose. A PR describes
what was measured and what was found, including what is *not* done. CI
must be green on both targets before a merge, and a red run is
root-caused, not re-run until it passes.
