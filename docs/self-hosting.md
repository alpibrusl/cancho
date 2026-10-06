# Self-hosting: the spike, and the decision it was for

`bootstrap.md` named the reason the v0 compiler is written in Rust
(*"Cranelift is a Rust library"*) and, in the same breath, named the
cheap experiment that would tell this project whether that reason is
load-bearing forever: *"port the `lex-ast`/`lex-vcs` canonical forms and
check byte-identical `OpId`/`SigId`/`StageId` over the existing op-log
corpus."* `ROADMAP.md`'s "Beyond" section repeats the same line. This
document runs that experiment, on the corpus this sandbox actually has
rather than the one the original note assumed, and answers the
question `bootstrap.md` deferred: is self-hosting worth planning next?

**No — not yet, and not because anything found here is a hard
blocker.** Every concrete thing checked came back feasible. What is
missing is an asker (`AGENTS.md` §7): no lex-sys program needs to
compute a lex-lang `SigId`, and no consumer is waiting on a
lex-sys-language compiler. This is the same discipline `docs/vcs.md`
§8 already applied to whole-function merge and typed issues —
*"has no asker in this repository yet... not started"* — applied here
to a much larger piece of unrequested work.

---

## 1. Two different questions this note does not conflate

*"Self-host the toolchain"* and *"make `lex-sys-vcs`'s `OpId` match
`lex-vcs`'s"* sound like the same idea and are not.

`docs/vcs.md` §5 already answered the second one, on contact, while
building `lex-sys-vcs`: **`OpId` is BLAKE3, not SHA-256**, deliberately
— *"matching `lex-vcs`'s algorithm would have meant a second hash
dependency for no reason but appearance."* `lex-sys-vcs` hashes
lex-sys programs; a lex-sys program and a Lex program are never the
same bytes, so there was never a reason for their content hashes to
agree, and none was sought.

What `bootstrap.md` actually asks is the first, larger question: could
*lex-sys itself* — the language — eventually be the implementation
language of its own compiler (today's `lex-sys-syntax`/`lex-sys-ir`/
`lex-sys-codegen*`, ~29,400 lines of Rust, counted below), the way a
self-hosted compiler is normally understood. That is what the rest of
this document measures.

---

## 2. The corpus this sandbox actually has

`ROADMAP.md`'s line says *"the existing ~136k-op corpus."* No such
corpus is checked into any of the four repositories this session can
reach — `lex-lang`, `lex-sys`, `lex-os`, `lex-gpu` — confirmed by
searching each for an op-log store, not assumed from the absence of a
memory of one. Whatever corpus that line originally meant lives
outside this sandbox (a production op-log from real use, most likely),
and this spike does not have access to it.

What does exist, real and checked into `lex-lang` itself: 31 real
`.lex` files (`examples/`, plus the fuzz seed corpus under
`fuzz/corpus/`), two of which are deliberately malformed fuzz seeds
that do not parse (`expected type expression, got Some(Fn)` — a fuzzer
seed testing the parser's own error path, not a program). The other 29
parse into 168 real `Stage`s (`fn`/`type` declarations) through the
real, unmodified `lex-syntax`/`lex-ast` pipeline.

This changes what is actually testable. Diffing a lex-sys hash against
a lex-lang hash for the *same* corpus was never going to say anything
— different languages, different ASTs, no reason to expect agreement.
What is testable, and what the "byte-identical" phrasing in
`bootstrap.md` is actually insurance against, is narrower and more
useful: **is the canonicalization *algorithm* — RFC-8785-flavored JSON,
SHA-256, the specific `SigId`/`StageId` composition — specified clearly
enough, in `lex-ast`'s own source and doc comments, that an independent
implementation reproduces it exactly?** A self-hosted port would be
exactly such an independent implementation, in a different language;
if the algorithm cannot be reproduced byte-for-byte from its own
documented spec in the *same* language, porting it to a different one
has no chance.

---

## 3. The experiment

A from-scratch Rust reimplementation of `canon_json`'s writer (UTF-8,
no whitespace, object keys sorted byte-wise, `serde_json`'s default
number rendering, JSON string escaping including `\u00XX` for control
bytes) and of `sig_id`/`stage_id`'s composition (read from
`lex-ast/src/lib.rs`'s own doc comments: *"SigId: SHA-256 over
canonical_json({name, input_types, output_type, effects})"*,
*"StageId = SHA-256(structural_sig_hash || implementation_hash)"*) —
written without calling `lex_ast::canon_json` or `lex_ast::sig_id`
itself, only depending on the real `lex_ast::Stage` type so both sides
walk the identical parsed data. Run over every real `Stage` the 29
parseable `.lex` files produce, diffed against the real crate's own
`sig_id(stage)` / `stage_id(stage)`.

```
corpus: 31 real .lex files
parse failure: fuzz/corpus/parser/seed_02.lex (deliberately malformed)
parse failure: fuzz/corpus/type_checker/seed_02.lex (deliberately malformed)
parse failures (excluded): 2
stages examined: 168
SigId byte-identical: 168/168
StageId byte-identical: 168/168
RESULT: every SigId and StageId reproduced byte-for-byte.
```

**168 for 168.** The canonicalization algorithm is specified precisely
enough, in the source it already has, to reproduce without the
original code — the property a port needs and the property
`bootstrap.md`'s "byte-identical" phrasing was actually checking for.
This is the one part of the spike with a clean, positive, falsifiable
result.

---

## 4. What else a self-hosted toolchain would need, checked rather than assumed

Three more questions, each answered against this repository's own real
code rather than guessed:

**Does lex-sys support what a hand-written compiler needs structurally?**
Recursion — `fn fib(n: int) -> [] int { if n < 2 { return n; } return
fib(n - 1) + fib(n - 2); }`, built and run in this sandbox, returns 55
for `fib(10)`. Heap-allocated recursive data (an AST node needs a
`Box`ed child) — already proven, not hypothetical: `examples/tree.ls`
is a binary search tree, exactly this shape, already in the corpus
`docs/README.md` calls *"why a language needs a heap at all."* Neither
is a new risk.

**What is the actual size of what would move?** The Rust crates that
are the compiler today, real line counts:

| Crate | Lines |
|---|---|
| `lex-sys-syntax` | 5,318 |
| `lex-sys-ir` | 12,282 |
| `lex-sys-codegen` | 3,397 |
| `lex-sys-codegen-llvm` | 4,615 |
| `lex-sys-types` | 704 |
| `lex-sys-id` | 1,909 |
| `lex-sys` (CLI) | 1,143 |
| **Total** | **29,368** |

That is the scale of a full port, not a slice — an order of magnitude
past anything this repository has ported in one piece before (§2 of
`docs/vcs.md` measured `lex-vcs`'s *diff*-facing code at 1,477 lines as
the largest prior comparison point).

**Does `bootstrap.md`'s own Cranelift blocker still hold?** Yes, and it
is not a soft one: Cranelift and LLVM are libraries with Rust (and C++)
APIs — builder patterns, trait objects, complex owned types crossing
by value — none of which a foreign call can cross today
(`docs/reach.md` §3: *"a foreign result is `int`, `bool` or `()`"*).
Reaching either from `.ls` directly is not a missing convenience, it is
outside what `extern fn` can express at all. But `docs/reach.md` §3.4's
own note that `fork` is already reachable through `Ffi("libc")` points
at the actual escape hatch a self-hosted backend would take: emit
textual assembly (a `[byte]` slice, no different from any other output
this language already writes) and shell out to the system `as`/`ld`
the way an early self-hosting C compiler did, rather than reimplement
a code generator. Untried here — a real slice of work in its own
right — but not blocked by anything this language currently lacks.

---

## 5. The decision

Nothing this spike checked is a wall. The hashing algorithm ports
byte-for-byte from its own spec (§3). Recursion and heap-allocated
trees, the structural minimum, already work (§4). The Cranelift/LLVM
dependency that keeps v0 in Rust has a named way around it — shelling
out to `as`/`ld` — that does not require reimplementing a code
generator (§4). What is missing is scale (29,368 real lines, §4) and,
more to the point, an asker: nothing in this repository today needs a
lex-sys compiler written in lex-sys, and `AGENTS.md` §7's rule against
building what nothing asks for applies at this size exactly as it does
at `docs/vcs.md`'s smaller ones.

**Decision: stay on Rust for the compiler. Revisit this document,
rather than re-running the spike, the day a concrete asker exists** —
most plausibly `lex-os`'s own port maturing to the point where running
the *lex-sys compiler itself* inside a sealed box (rather than just
lex-sys *programs*) becomes something that box's own trust model
needs. Nothing in this document's findings would need to change before
that day; only the "no asker yet" line would.

---

## 6. The staged port (epic #295)

§5 decided to stay on Rust and nothing below reopens it. What §3 and §4 could not
give was a *measured* size, so the question "how big is a compiler in this
language" has been run one stage at a time, the Rust compiler the oracle at every
stage and any stage free to stop the effort by failing. This section records what
the stages found, in place, the way this document corrects its own claims.

| Stage | Written in lex-sys | Oracle | Result |
|---|---|---|---|
| 1. Lexer | `examples/selfhost/lexcore.ls` (a module) and `lexer.ls` | `examples/dump_tokens.rs` in `lex-sys-syntax` | Same token stream and the same refusals on every program in the repository (614 files) |
| 2. Parser | `examples/selfhost/parser.ls` | `examples/dump_ast.rs` in `lex-sys-syntax` | Same syntax tree, node for node and span for span, or the same refusal (rule and span), on the same 614 files, **including its own source**, and on a fuzz corpus (below) of 52,272 cases, 51,911 of them comparable and all identical, 29,097 of those refusals |
| 3a. The tree | `examples/selfhost/ast.ls` (a module, the parser) and `parser.ls` (a walk that prints the tree) | the same `dump_ast.rs` | The parser **builds the tree** in flat tables and the listing is produced by walking it; the same 621 programs and 52,314 fuzz cases (6 seeds), all comparable ones identical, 29,055 of them refusals |
| 3b. Resolution and the rest of the checker | not started | `lex-sys check --output json` | |
| 4. Backend | not started | | |

**The method.** A port that builds no tree has nothing to compare, and one that does
needs a printer, which is another port. So stage 2's parser wrote the tree the Rust parser
would have built as a *postfix listing*, one node per line, children before their
parent, at the moment the Rust parser pushes the node into its arena, and says how
many children it takes. `dump_ast.rs` writes the same listing from the Rust AST. Each
node carries its span, so the comparison covers every position the parser records;
every refusal is one line, `ERR rule start end`. `examples/selfhost/diff.sh` compares
the two over files, `fuzz.py` over the files plus hostile edge cases (the limits of
`int` and of `float` and `f32`, escapes, the order of `module`, `import` and items) and
mutants (a token dropped, duplicated, replaced or swapped, the file cut short, a byte
range deleted), and `tests/conformance/selfhost.rs` runs both ports over the
repository's programs on every CI run.

**Stage 3a: the tree.** Stage 2 printed as it parsed; the checker needs something to walk,
so `ast.ls` is the same parser building the Rust AST's own design: flat tables and
indices, not owned children. A node is a record of 16 integers in one table (kind,
span, the next node of its list, and what its kind keeps), a reference to a node is its
index, a list is a chain (the parent holds the first child and the count). The whole
front end, state, tokens and nodes, is **one allocation whose size follows from the
length of the text** (every node and token takes a byte, so tokens bound nodes), 160
bytes of state per byte of source. `parser.ls` is now only the walk that prints the tree
in the listing's order, so a match with the oracle is a statement about the *tree*:
the walk reads nothing the tree does not hold, except where the parser had already checked
the tokens and the node keeps only where they start (an effect row, a declaration's
`[T, &r where ...]`, a destructuring pattern, a match pattern). Everything stage 2
was checked with passes unchanged, and the translation compiled and matched the Rust
parser on all 621 programs the first time.
The cost of the walk: `ast.ls` (1,921 lines, 1,678 not comment or blank) and `parser.ls`
(847, 761) are 2,768 lines against stage 2's single 1,970; parsing and walking its own 80 KB
takes 0.24 s (stage 2 took 6 ms for its own 59 KB), with a 13 MB table to fill first; where
the time goes was not measured.

**Size.** `parser.ls` is 1,970 lines (1,760 that are not comment or blank) for the
Rust parser's 1,507 (1,225): 1.3 to 1.4 times. `lexcore.ls` is 963 lines (897) for the
Rust lexer's 633, with its generated tables. Counted with `grep -v '^\s*//'` and
without blank lines; both ports are written one statement to a line, as `lex-sys fmt`
leaves them. The port parses its own 59 KB in 6 ms.

**What it found.**

* **The lexer had drifted, silently.** Between the spike and this stage the Rust lexer
  gained the `f32` suffix on float literals (`docs/f32.md`), and the port, which had no
  test, would have refused every such literal. It has one now (`selfhost.rs`), which
  is the first thing the oracle approach is *for*: a port that is not run against its
  oracle in CI is a port of whatever the oracle was.
* **No integer constants.** A token has to be stored as an integer, so the lexer
  grew a generated `code` function (one arm per token kind) and the parser compares
  with `look(st, 0, lc.Tok::Comma)`, which runs that match on every test. A table of
  named integers is the one thing the port wanted from the language here.
* **No error propagation, so a refusal is sticky.** The first refusal is recorded in
  the state and every later call does nothing; `kind` answers end of file once the parser
  has failed, so every list loop ends. That works, and it has one failure mode `?`
  does not: *every* function has to be total on the state a refusal leaves behind.
  The fuzzer found the one place it was not (`edition )`: the number was parsed after
  the refusal, as if it were a number, and the arithmetic trapped). Fixed, with the
  fuzz seeds that found it kept.
* **Checked arithmetic shaped one function.** An integer literal is read as a magnitude
  that must fit in 64 signed bits; summing it positively overflows at exactly the
  boundary the check is for, so it is accumulated as a negative number (which holds one
  more value) and each step is checked before it is taken. The boundary cases
  (`9223372036854775807`, `...808`, `-...808`, the hexadecimal ones) are in the corpus.
* **Row exactness was a help.** `declaration_params` first declared `io_write`
  without writing anything, and the checker refused it: the parse is pure and the
  printing is a separate walk over its tokens, which is the structure the listing
  needed anyway (the `[...]` of a declaration interleaves two lists the Rust AST keeps
  apart).
* **Smaller:** `region` is a keyword, so no local can carry the name most parsers
  give it; the state is one slice of integers because that was the cautious choice,
  and a `&!p` reference to a struct with field assignment does work (checked while
  writing this), which would read better for the scalar slots.

**What it does not show.**

* The tree is flat tables, not recursive `Box` values, because that is the design of the
  Rust AST being ported and what an arena-sized front end wants. Whether recursive boxed
  trees at this size are comfortable is still unmeasured (§4 showed they work, at a small
  one).
* Float *values* are not computed: the listing has a float literal's span and whether
  it is an `f32`, not its bits. Whether a literal rounds to infinity is checked, exactly,
  by comparing its digits with those of 2^1024 − 2^970 (2^128 − 2^103 for `f32`); the
  conversion itself is `std.json`'s `to_float`, which is there to be reused.
* One file at a time. The Rust parser's `parse_into` (many files, a base offset on every
  span) is not ported.
* Inputs that are not UTF-8. The oracle takes a `&str`, so it decodes lossily and reports
  spans in the decoded text; the port reads bytes. The two cannot be compared and the
  harnesses skip those inputs (361 of the 52,272 fuzz cases).
* Names are printed from their spans, not interned, so symbol ids are not compared.

**Where it stands.** Stages 1, 2 and 3a are done and nothing in them blocks the rest of
stage 3, the checker (`lex-sys-ir`, 12 thousand lines), which is the real test: it is the first
stage with enough shape (resolution, linearity, regions, effect rows) to tell whether
the language is comfortable writing its own compiler. §5's decision does not change.

