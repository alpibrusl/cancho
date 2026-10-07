# The next phase: verification, not another hunt

> **Status: the argument is made, §3's migration has landed, and so
> has §4's standing check (§4.1) — which promptly found more of what
> §3 found, by a stronger method than the hunt that found §3.** M0–M3
> are done, the LLVM backend is the default, the package system closes
> real duplication, and the CLI is self-describing
> (`docs/agent-cli.md`). `docs/ROADMAP.md`'s own "What is next" table
> has nothing left unstruck. This document is what comes after that
> table is empty: not a list of features, but a change in *how* the
> next features get found — argued from `MANIFESTO.md` ("Trust Without
> Comprehension") and from three things this repository just did to
> itself in the same session. §3.1 has the migration's own two
> findings, one of them (a root-namespace import collision) not
> previously documented anywhere in this repository.

---

## 1. What this session's own mistakes were made of

Three things happened, in order, that are the actual evidence for
everything below:

1. **A stale claim, found by chance.** `docs/benchmarks-game.md` §3 and
   §5 described `spectral.cho` as still hand-rolling `sqrt`, four months
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
| A function's effects match its body | `cancho check`, every build |
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

| Function | Duplicated in | Already in `std/io.cho`? |
|---|---|---|
| `print_nat` | 7 examples (`tally.cho`, `pipeline.cho`, `tree.cho`, `tour.cho`, `modular/text.cho`, `slab/main.cho`, `wordfreq/text.cho`) | Yes, `pub fn print_nat`, byte-identical body |
| `write_all` | 8 examples (`lines.cho`, `hello.cho`, `tree.cho`, `tour.cho`, `modular/text.cho`, `slab/main.cho`, `wordfreq/text.cho`, `buffer/main.cho`) | Yes, `pub fn write_all` — but `std.io`'s calls the `write_bytes` builtin (`docs/bulk-io.md`); every example's own copy is the pre-bulk-I/O `putchar`-per-byte loop. `hello.cho`'s copy stays: §3.1 |

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

`examples/buffer/main.cho`'s own `write_all` had **zero** call sites in
that file: dead code, found by the same grep that found the
duplication, deleted outright rather than migrated.

### 3.1 Landed, with two exceptions found doing it

The migration above is done (this PR). `lines.cho`, `tally.cho`,
`pipeline.cho`, `tree.cho`, `tour.cho`, `wordfreq/{text,main,counts}.cho`
and `slab/main.cho` now `import std.io;` and call
`io.write_all`/`io.print_nat`; `buffer/main.cho`'s dead copy is gone.
Every migrated example's output was diffed against its own `//~
STDOUT` directive before and after — byte-identical in every case,
which is the whole point of the fix being mechanical. Seven files
moved, not the ten §3 counted, for two reasons found only by doing it:

**`examples/modular/text.cho` is deliberately excluded.** Its own header
comment says why it exists: *"The functions here are byte-for-byte the
ones 25 other files in this repository each define for themselves --
and moving them here changed **no hash**... because a call encodes the
callee's hash rather than its spelling."* That sentence is this
document's §3 finding, stated as a `docs/modules.md` teaching point
four PRs before this one noticed it as duplication. Migrating this
file onto `std.io` would delete the functions its own comment uses as
the worked example.

**`examples/hello.cho` is excluded for a different reason, found only
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
`wordfreq/`'s three files (`text.cho`, `main.cho`, `counts.cho`) have no
`module` line, so they share one root namespace the way `write_all`
and `print_nat` themselves used to be reached unqualified from `main.cho`
and `counts.cho`. `import std.io;` in more than one of them is refused —
*"`io` is already bound to another import here"* — the same rule that
refuses two definitions of one name in that namespace, applied to an
import for the first time in this repository. One import, in `text.cho`,
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
`serve.cho`/`collect.cho`/`agent_supervisor.cho`/`results_stub.cho` — meets
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
`packages/` (post-canonicalization, via `cancho print` or
`cancho-id`, not regex, so it survives reformatting the regex-based
hunt script would not) and fails when two bodies in different files
match, above a length floor to exclude one-line coincidences like `fn
main`. It would not have caught §1.1/§1.2 (those are prose, not code),
but it makes §3's category — the one the manifesto's own argument is
actually about — a red build instead of an occasional discovery. Worth
building once the §3 migration lands, so the new test starts clean
rather than red on day one.

### 4.1 Landed, and it found more than the hunt did

`duplication::no_function_body_is_duplicated_across_files`
(`crates/cancho/tests/conformance/duplication.rs`) is that test. It
settles §6's open question: a conformance test, not a `cancho audit`
subcommand — `identity.rs`'s own precedent, and nothing here needs an
agent to run it mid-task rather than `cargo test` catching it on every
build.

It is built on `cancho-id`, not on `cancho print`'s text, for a
reason found while building it rather than argued in advance: each
file is parsed and identified **alone**, exactly as
`identity.rs::printing_preserves_every_identity_and_is_idempotent`
already does — no `--std`, no cross-file resolution — and
`cancho-id`'s own `qualified_name` already encodes a call to a name
declared in the *same* file as that declaration's hash, and a call to
anything else (a builtin, an import) as the literal name. Two files
calling the same builtins the same way still collide, which is what
catches a real copy; two files whose functions merely *read* alike but
resolve an unqualified name against two different local declarations
do not. `examples/rational.cho` and `std/result.cho` both define an
`is_ok` whose `match` arms print identically — but `rational.cho`'s own
`Result[T]` (§3.1's own migration left it alone: it is a different,
locally-declared enum, not `std.result`'s `Result[T, E]`) makes its
`Result::Ok`/`Result::Err` hash differently from `std/result.cho`'s own.
A text diff cannot tell those two cases apart — the regex hunt behind
§3 would have flagged it as a third cluster — a content hash always
can, and the test finds no cluster there at all. `benches/`'s own
`*_checked.cho`/`*_wrapping.cho` pairs are excluded from the scan for the
opposite reason: `benchmarks.rs::every_benchmark_pair_agrees` already
asserts each pair agrees on purpose, so including them here would mean
allowlisting every pair for no added safety.

Being alpha-equivalence-aware (`docs/canonical-ast.md`: "bodies hash up
to alpha-equivalence") also means it caught what the raw-text hunt
behind §3 structurally could not: three clusters with **different**
parameter or function names on the two sides, not previously
documented anywhere —

| Function(s) | Files | Already under a shared name? |
|---|---|---|
| ~~`abs`~~ | `examples/rational.cho`, `std/math.cho` | **Migrated** — `rational.cho` now `import`s `std.math` and calls `math.abs`; its own copy is gone |
| ~~`larger` / `max`~~ | `examples/tree.cho`, `std/math.cho` | **Migrated** — `tree.cho` now `import`s `std.math` and calls `math.max`; its own copy is gone |
| `append` / `put` | `examples/lines.cho`, `packages/net-sockets/sockets.cho`, `examples/ocpp_ws/ocpp.cho` (the WebSocket spike's third copy, kept for the same reason) | Yes — `net.sockets.put`, same byte-blit loop, different name |

— plus one the §3 hunt's own scope already should have caught and
didn't, because it only hashed the four files each duplicate cluster
already lived in and never rechecked a package's own extraction
point: `packages/net-connect/connect.cho`'s `address` says in its own
comment it was "extracted from" three files' copies, but
`examples/tls_client/socket.cho` was never migrated onto the package
and still carried the pre-extraction copy. **Migrated too**:
`socket.cho` now `import`s `net.connect`/`net.sockets` and forwards into
them, verified against a real OpenSSL server over an actual TLS 1.3
handshake, not just a build. The migration surfaced a real gap the
type checker does not cover: the backend's own symbol for a function
is its name alone (`crates/cancho-codegen/src/abi.rs`'s `lexs_`
prefix, no module qualifier), so naming the forwarding wrapper
`connect_to` — the obvious choice, and what the type checker itself
resolves without complaint, module-scoped — collides with
`net.connect`'s own `connect_to` at the object file the moment both
are linked into one program; `clang -c` refuses the emitted LLVM IR
with "invalid redefinition of function." Renamed to `socket.open`
instead. `crates/cancho/tests/conformance/net.rs`'s
`the_network_programs_are_counted` — the standing count `docs/net.md`
§5 rests on — moved with it: outbound is one declaring file now,
matching inbound.

Both migrated rows landed the same session `§4.1` was written, each
output-diffed against its own `//~ STDOUT` before and after (identical
in both cases, and `address`'s migration diffed a real TLS handshake's
bytes too) and the full gate rerun. `rational.cho`'s migration also
moved `identity.rs::ids_are_stable_across_runs_and_survive_a_body_rewrite`,
which patched `abs`'s own body text to exercise "a body rewrite moves
no signature" — `gcd` carries that job now, and the `ids` calls there
gained `--std` for the same reason every other example needs it once
it imports something.

Everything still open — `append`/`put`, §3.1's own two
already-documented exceptions, and the §3 `nat_of`/`port_of`
cluster — is recorded in the test's own `ALLOWED` list with the same
reasoning as here, so the check stays green rather than red. The test
itself asserts every `ALLOWED` entry still names a real cross-file
match, so an entry that stops being true (because someone does the
migration, as just happened three times now) fails loudly rather than
rotting.

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
| ~~Does `audit::no_function_body_is_duplicated` belong in this repo's own test suite, or as a `cancho audit` subcommand an agent can run on demand?~~ | Decided, §4.1: a conformance test, `identity.rs`'s own precedent |
| Is there a mechanical check for §1.1/§1.2's category at all, even a partial one (e.g. flag a doc paragraph whose cited PR number is more than N merges behind `HEAD`)? | Speculative; no design exists yet, and §5 says why it's harder than §4 |
| The `nat_of`/`port_of` cluster (§3) | Real, smaller, a shape question rather than a mechanical fix — pick up opportunistically |
| ~~`address` (`examples/tls_client/socket.cho`, never migrated onto `packages/net-connect/connect.cho`)~~ | **Migrated**, §4.1 — and found a real backend gap doing it (a same-named `pub fn` in two modules collides at the object file; the type checker does not catch it) |
| ~~`abs` / `larger` (`examples/rational.cho`, `examples/tree.cho`, not yet calling `std.math`)~~ | **Migrated**, §4.1 |
| `append` (`examples/lines.cho`, not yet calling `packages/net-sockets/sockets.cho`'s `put`, §4.1) | Real, mechanical, but pulls a network package into a file that otherwise has no package dependency — worth a second look before migrating, not a pure copy-paste |
| `read_stdin`/`read_file` (`examples/sort/sort.cho`, `examples/seek/seek.cho`, §4.1) | Real, identical present-day logic kept apart on purpose as two worked examples of the same fix (`docs/file-handles.md` §1) — extracting a shared helper would need a place to put it that isn't either example |
| The whole of `examples/buffer/buffer.cho` (predates `std/buffer.cho`, never migrated, §4.1) | Real, larger than a one-function fix — the example's own `res struct Buffer` would need to become `std.buffer.Buffer` throughout, which is a rewrite of the file, not a swap |
| `n-body`, why `fasta` moved the "wrong" direction under `--backend llvm`, a quieter `revcomp` host, a stated precision in `std.fmt` | `benchmarks-game.md` §5 and `float-printing.md` §7's own Open rows, unrelated to this document's argument, still on file |
