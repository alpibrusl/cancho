# The next phase: verification, not another hunt

> **Status: the argument is made, §3's migration has landed, §4's
> standing check has not.** M0–M3 are done, the LLVM backend is the
> default, the package system closes real duplication, and the CLI is
> self-describing (`docs/agent-cli.md`). `docs/ROADMAP.md`'s own "What
> is next" table has nothing left unstruck. This document is what
> comes after that table is empty: not a list of features, but a
> change in *how* the next features get found — argued from
> `MANIFESTO.md` ("Trust Without Comprehension") and from three things
> this repository just did to itself in the same session. §3.1 has the
> migration's own two findings, one of them (a root-namespace import
> collision) not previously documented anywhere in this repository.

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
| `write_all` | 8 examples (`lines.ls`, `hello.ls`, `tree.ls`, `tour.ls`, `modular/text.ls`, `slab/main.ls`, `wordfreq/text.ls`, `buffer/main.ls`) | Yes, `pub fn write_all` — but `std.io`'s calls the `write_bytes` builtin (`docs/bulk-io.md`); every example's own copy is the pre-bulk-I/O `putchar`-per-byte loop. `hello.ls`'s copy stays: §3.1 |

This is `docs/line-reading.md`'s "cut's long line" pattern a second
time: example programs are not merely duplicating a helper, most of
them are stuck on the exact byte-at-a-time path `docs/bulk-io.md`
built a replacement for and never migrated onto. Files that get
`--std` through the generic example walker
(`corpus.rs::every_example_runs_and_prints_what_it_says`) already have
it; the four that live in their own subdirectory (`wordfreq/`,
`modular/`, `slab/`, `buffer/`) are built by their own dedicated test
instead, and two of those four needed `--std` added to that test's own
build command as part of this migration.

`examples/buffer/main.ls`'s own `write_all` had **zero** call sites in
that file: dead code, found by the same grep that found the
duplication, deleted outright rather than migrated.

### 3.1 Landed, with two exceptions found doing it

The migration above is done (this PR). `lines.ls`, `tally.ls`,
`pipeline.ls`, `tree.ls`, `tour.ls`, `wordfreq/{text,main,counts}.ls`
and `slab/main.ls` now `import std.io;` and call
`io.write_all`/`io.print_nat`; `buffer/main.ls`'s dead copy is gone.
Every migrated example's output was diffed against its own `//~
STDOUT` directive before and after — byte-identical in every case,
which is the whole point of the fix being mechanical. Seven files
moved, not the ten §3 counted, for two reasons found only by doing it:

**`examples/modular/text.ls` is deliberately excluded.** Its own header
comment says why it exists: *"The functions here are byte-for-byte the
ones 25 other files in this repository each define for themselves --
and moving them here changed **no hash**... because a call encodes the
callee's hash rather than its spelling."* That sentence is this
document's §3 finding, stated as a `docs/modules.md` teaching point
four PRs before this one noticed it as duplication. Migrating this
file onto `std.io` would delete the functions its own comment uses as
the worked example.

**`examples/hello.ls` is excluded for a different reason, found only
by running the full gate.** The first pass migrated it too, and `cargo
test --workspace` immediately failed three unrelated tests —
`corpus::run_builds_and_executes_in_one_step`,
`corpus::emitting_a_bare_object_file_works`, and
`refusals::a_clean_program_answers_an_empty_list` — all three of which
build this exact file **without** `--std`, on purpose: it is the one
example every one of them can rely on needing nothing else. `import
std.io;` made that assumption false. Reverted, with a comment on the
file recording why and naming the three tests, so a future hunt does
not propose the same fix and hit the same wall silently.

**A third thing found doing the mechanical part, not before.**
`wordfreq/`'s three files (`text.ls`, `main.ls`, `counts.ls`) have no
`module` line, so they share one root namespace the way `write_all`
and `print_nat` themselves used to be reached unqualified from `main.ls`
and `counts.ls`. `import std.io;` in more than one of them is refused —
*"`io` is already bound to another import here"* — the same rule that
refuses two definitions of one name in that namespace, applied to an
import for the first time in this repository. One import, in `text.ls`,
serves the whole program; a comment in the other two says where it
lives and why it is not repeated. Not previously documented anywhere,
because nothing here had shared one root namespace across an import
before.

The lesson worth keeping, for §4's own design: a check that flags a
duplicate body correctly can still be wrong to *act on* without asking
whether that particular copy is load-bearing elsewhere, for a reason
the check itself cannot see (a test fixture's own contract, a
document's own worked example). Verification finds the candidate; it
does not replace reading the one file that explains why it exists.

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
