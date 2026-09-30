# The next phase: verification, not another hunt

> **Status: proposed.** M0–M3 are done, the LLVM backend is the
> default, the package system closes real duplication, and the CLI is
> self-describing (`docs/agent-cli.md`). `docs/ROADMAP.md`'s own "What
> is next" table has nothing left unstruck. This document is what
> comes after that table is empty: not a list of features, but a
> change in *how* the next features get found — argued from
> `MANIFESTO.md` ("Trust Without Comprehension") and from three things
> this repository just did to itself in the same session.

---

## 1. What this session's own mistakes were made of

Three things happened, in order, that are the actual evidence for
everything below:

1. **A stale claim, found by chance.** `docs/benchmarks-game.md` §3 and
   §5 described `spectral.ls` as still hand-rolling `sqrt`, four months
   after `#62` made it a builtin. Nothing had reason to reread that
   paragraph — it was found because a user asked "is `std.math` over
   floats really still open?" and the answer required rereading, not
   running anything.
2. **A stale claim, found by measurement.** `docs/benchmarks-game.md`
   and `docs/against-c-and-rust.md` both quoted a Cranelift-era
   "1.17×–2.58× slower than C" figure as current, months after `#127`
   made LLVM the default. Found the same way: a direct question,
   answered by rerunning the benchmark rather than trusting the page.
3. **A bug of the exact shape this document is about, self-inflicted.**
   Building `docs/agent-cli.md`'s CLI integration, a generated doc very
   nearly shipped telling an agent to write `--o`/`--l`/`--L` for flags
   that are actually `-o`/`-l`/`-L` — found only because the generated
   text was read once, by hand, immediately after writing it.

None of these three were caught by `cargo test --workspace`. All three
were prose or generated output *about* the code, silently disagreeing
with the code, for months in two cases. The thing that finds this
category of bug today is a person or an agent rereading a paragraph
against the code that paragraph describes — comprehension, in exactly
`MANIFESTO.md` §IV's sense. That does not scale, and the manifesto
already names the alternative: **replace comprehension with
verification wherever verification can reach.**

## 2. What verification already reaches here, and what it doesn't yet

`MANIFESTO.md` §IV: *"Verification scales cleanly for formal
properties... It scales partially for testable behaviors... It scales
poorly for aesthetic or intentional properties."* Sorting what this
repository has by that same line:

| Already verified, not comprehended | By what |
|---|---|
| A function's effects match its body | `lex-sys check`, every build |
| A refusal names a stable rule | `agent-errors.md`'s 52-rule catalogue, fixture-checked |
| `AGENTS.md`'s code examples still compile/refuse as claimed | `docs::the_agents_examples_still_work` |
| The generated doc tree can't drift from the dispatch table | `agent_cli::every_dispatched_command_is_documented` (landed this session, `#157`) |
| Two backends agree on every fixture | `backends.rs`'s pairwise checks |
| A package's declared identity matches its actual source | `vcs resolve` |

| Not verified — comprehension is still the only check | Evidence it's a real gap |
|---|---|
| A doc's prose still matches the code it describes | §1.1 and §1.2 above — two misses, months apart |
| Generated documentation doesn't quietly teach a wrong flag spelling | §1.3 above — caught once, by luck of rereading |
| A function's body doesn't already exist, verbatim, elsewhere in the tree | §3 below — never checked at all until this session's hunt found it |

Everything in the first table is a `cargo test` failure waiting to
happen the moment it's wrong. Everything in the second table is a
paragraph or a file that can rot silently, the way `benchmarks-game.md`
did for four months. The next phase's organizing idea is: **move rows
from the second table into the first**, not by hunting harder, but by
building the check once and running it every time, the way `doc-sync
--check` already does for the generated-docs tables.

## 3. A concrete instance, found while writing this document

Hashing every function body under `examples/`, `std/`, `packages/` and
`benches/` after whitespace/comment normalization — the same method
that found `http.response` (`#153`) — surfaces two clusters that
clear the project's own "two real askers" bar by a wide margin,
neither previously flagged:

| Function | Duplicated in | Already in `std/io.ls`? |
|---|---|---|
| `print_nat` | 7 examples (`tally.ls`, `pipeline.ls`, `tree.ls`, `tour.ls`, `modular/text.ls`, `slab/main.ls`, `wordfreq/text.ls`) | Yes, `pub fn print_nat`, byte-identical body |
| `write_all` | 8 examples (`lines.ls`, `hello.ls`, `tree.ls`, `tour.ls`, `modular/text.ls`, `slab/main.ls`, `wordfreq/text.ls`, `buffer/main.ls`) | Yes, `pub fn write_all` — but `std.io`'s calls the `write_bytes` builtin (`docs/bulk-io.md`); every example's own copy is the pre-bulk-I/O `putchar`-per-byte loop |

This is `docs/line-reading.md`'s "cut's long line" pattern a second
time: ten example programs are not merely duplicating a helper, eight
of them are stuck on the exact byte-at-a-time path `docs/bulk-io.md`
built a replacement for and never migrated onto. All ten already build
with `--std` (confirmed against `backends.rs`'s own build invocations),
so the fix is import, delete, and qualify the call sites — no design
question, unlike every prior package extraction. `tour.ls` (993 lines,
~35 call sites between the two functions) and `tree.ls` (283 lines, 15
call sites) are the real work; the other eight are one or two call
sites each. Scoped, not done here — the next PR, not this document.

`examples/buffer/main.ls`'s own `write_all` has **zero** call sites in
that file: dead code, found by the same grep that found the
duplication, worth deleting regardless of the migration.

A second, smaller cluster — `nat_of`/`port_of`/`port_of_listen`, four
files, ~182 characters each, already under different names in
`serve.ls`/`collect.ls`/`agent_supervisor.ls`/`results_stub.ls` — meets
the two-asker bar too but is a shape question (three different names
for the same parse), not a mechanical migration; it can wait.

## 4. The mechanical check this argues for

§3's finding was made by a 70-line script run once, by hand, exactly
once in this repository's history (`docs/package-system.md`'s own
prior extractions were found "by rereading files by eye," which this
session's hunt already improved on once). Nothing stops the next
duplication from sitting unnoticed for as long as the sqrt paragraph
did, because nothing runs the script again.

The manifesto's own worked example (`MANIFESTO.md` §VI) is a model
generating code from a spec and a type checker verifying the result —
"the model is not trusted because it was understood... it is trusted
because what it produced was verified." Applied to this repository's
own maintenance: an agent extracting a package, or copying a helper
into a new example, should not be trusted to have remembered every
prior helper — it should be *checked* against the actual body hashes
the moment it's done, not rechecked by the next person who happens to
hunt.

Proposed: a conformance test — `audit::no_function_body_is_duplicated`
or similar, mirroring `identity.rs`'s existing printing-idempotence
style — that hashes every function body under `examples/`, `std/`,
`packages/` (post-canonicalization, via `lex-sys print` or
`lex-sys-id`, not regex, so it survives reformatting the regex-based
hunt script would not) and fails when two bodies in different files
match, above a length floor to exclude one-line coincidences like `fn
main`. It would not have caught §1.1/§1.2 (those are prose, not code),
but it makes §3's category — the one the manifesto's own argument is
actually about — a red build instead of an occasional discovery. Worth
building once the §3 migration lands, so the new test starts clean
rather than red on day one.

## 5. What this does not propose

Not a doc-staleness checker for §1.1/§1.2's category: those are prose
claims about measurements, and verifying a sentence against a number
that changes with the hardware it ran on is a harder problem than this
document has an answer to. The honest fix there stays what it's been
this session — read a doc before repeating its number, and correct in
place the moment it's found wrong (`docs/ROADMAP.md`'s own stated
convention). §4 is deliberately scoped to what's mechanically checkable
today: identical code, not identical claims.

Not a rewrite of how packages get extracted (`docs/package-system.md`
§4–§8 stands) — §4's check would flag a candidate the same way this
session's own hunt did; a human or agent still designs the package,
the same two-asker bar still applies, and `vcs publish`/`lock`/`fetch`
are unchanged.

Not `lex-os` integration, self-hosting, or anything from
`docs/ROADMAP.md`'s own "Deliberately excluded" list (traits,
`comptime`, an own optimiser, incremental compilation, LSP, async) —
all still "yes, later," for the reasons already on record, and none of
them bear on the verification-over-comprehension argument this
document is making.

## 6. Open

| Question | Why it waits |
|---|---|
| Does `audit::no_function_body_is_duplicated` belong in this repo's own test suite, or as a `lex-sys audit` subcommand an agent can run on demand? | The conformance-test form is simpler and matches `identity.rs`'s precedent; the CLI-subcommand form would fit `docs/agent-cli.md`'s own "the CLI as data" argument better if an agent, not CI, is meant to run it mid-task. Decide when building §4, not here |
| Is there a mechanical check for §1.1/§1.2's category at all, even a partial one (e.g. flag a doc paragraph whose cited PR number is more than N merges behind `HEAD`)? | Speculative; no design exists yet, and §5 says why it's harder than §4 |
| The `nat_of`/`port_of` cluster (§3) | Real, smaller, a shape question rather than a mechanical fix — pick up opportunistically |
| `n-body`, why `fasta` moved the "wrong" direction under `--backend llvm`, a quieter `revcomp` host, a stated precision in `std.fmt` | `benchmarks-game.md` §5 and `float-printing.md` §7's own Open rows, unrelated to this document's argument, still on file |
