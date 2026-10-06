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
missing is an asker (`AGENTS.md` §7): no cancho program needs to
compute a lex-lang `SigId`, and no consumer is waiting on a
cancho-language compiler. This is the same discipline `docs/vcs.md`
§8 already applied to whole-function merge and typed issues —
*"has no asker in this repository yet... not started"* — applied here
to a much larger piece of unrequested work.

---

## 1. Two different questions this note does not conflate

*"Self-host the toolchain"* and *"make `cancho-vcs`'s `OpId` match
`lex-vcs`'s"* sound like the same idea and are not.

`docs/vcs.md` §5 already answered the second one, on contact, while
building `cancho-vcs`: **`OpId` is BLAKE3, not SHA-256**, deliberately
— *"matching `lex-vcs`'s algorithm would have meant a second hash
dependency for no reason but appearance."* `cancho-vcs` hashes
cancho programs; a cancho program and a Lex program are never the
same bytes, so there was never a reason for their content hashes to
agree, and none was sought.

What `bootstrap.md` actually asks is the first, larger question: could
*cancho itself* — the language — eventually be the implementation
language of its own compiler (today's `cancho-syntax`/`cancho-ir`/
`cancho-codegen*`, ~29,400 lines of Rust, counted below), the way a
self-hosted compiler is normally understood. That is what the rest of
this document measures.

---

## 2. The corpus this sandbox actually has

`ROADMAP.md`'s line says *"the existing ~136k-op corpus."* No such
corpus is checked into any of the four repositories this session can
reach — `lex-lang`, `cancho`, `lex-os`, `lex-gpu` — confirmed by
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

This changes what is actually testable. Diffing a cancho hash against
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

**Does cancho support what a hand-written compiler needs structurally?**
Recursion — `fn fib(n: int) -> [] int { if n < 2 { return n; } return
fib(n - 1) + fib(n - 2); }`, built and run in this sandbox, returns 55
for `fib(10)`. Heap-allocated recursive data (an AST node needs a
`Box`ed child) — already proven, not hypothetical: `examples/tree.cho`
is a binary search tree, exactly this shape, already in the corpus
`docs/README.md` calls *"why a language needs a heap at all."* Neither
is a new risk.

**What is the actual size of what would move?** The Rust crates that
are the compiler today, real line counts:

| Crate | Lines |
|---|---|
| `cancho-syntax` | 5,318 |
| `cancho-ir` | 12,282 |
| `cancho-codegen` | 3,397 |
| `cancho-codegen-llvm` | 4,615 |
| `cancho-types` | 704 |
| `cancho-id` | 1,909 |
| `cancho` (CLI) | 1,143 |
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
Reaching either from `.cho` directly is not a missing convenience, it is
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
cancho compiler written in cancho, and `AGENTS.md` §7's rule against
building what nothing asks for applies at this size exactly as it does
at `docs/vcs.md`'s smaller ones.

**Decision: stay on Rust for the compiler. Revisit this document,
rather than re-running the spike, the day a concrete asker exists** —
most plausibly `lex-os`'s own port maturing to the point where running
the *cancho compiler itself* inside a sealed box (rather than just
cancho *programs*) becomes something that box's own trust model
needs. Nothing in this document's findings would need to change before
that day; only the "no asker yet" line would.

---

## 6. The staged port (epic #295)

§5 decided to stay on Rust and nothing below reopens it. What §3 and §4 could not
give was a *measured* size, so the question "how big is a compiler in this
language" has been run one stage at a time, the Rust compiler the oracle at every
stage and any stage free to stop the effort by failing. This section records what
the stages found, in place, the way this document corrects its own claims.

| Stage | Written in cancho | Oracle | Result |
|---|---|---|---|
| 1. Lexer | `examples/selfhost/lexcore.cho` (a module) and `lexer.cho` | `examples/dump_tokens.rs` in `cancho-syntax` | Same token stream and the same refusals on every program in the repository (614 files) |
| 2. Parser | `examples/selfhost/parser.cho` | `examples/dump_ast.rs` in `cancho-syntax` | Same syntax tree, node for node and span for span, or the same refusal (rule and span), on the same 614 files, **including its own source**, and on a fuzz corpus (below) of 52,272 cases, 51,911 of them comparable and all identical, 29,097 of those refusals |
| 3a. The tree | `examples/selfhost/ast.cho` (a module, the parser) and `parser.cho` (a walk that prints the tree) | the same `dump_ast.rs` | The parser **builds the tree** in flat tables and the listing is produced by walking it; the same 621 programs and 52,314 fuzz cases (6 seeds), all comparable ones identical, 29,055 of them refusals |
| 3b. Declarations (the first half of the checker, complete) | `examples/selfhost/pass1.cho`, `foreign.cho` and `checker.cho`, over a generated `tables.cho`; programs of several files | `check_declarations` (`cancho-ir`), the first half of the Rust checker | Same answer, `OK` or the first refusal's rule and span, on every program of the repository alone (633) and with the whole standard library parsed with them (633, 464 of them `OK`), on 169 targeted cases, and on 62,892 fuzz cases (51,534 alone, 11,358 with the library): 62,434 identical, 37,967 of them refusals, none different; 458 are not UTF-8; nothing is skipped |
| 3c. Bodies: scopes, types, linearity, effects | not started | `cancho check --output json` | |
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
so `ast.cho` is the same parser building the Rust AST's own design: flat tables and
indices, not owned children. A node is a record of 16 integers in one table (kind,
span, the next node of its list, and what its kind keeps), a reference to a node is its
index, a list is a chain (the parent holds the first child and the count). The whole
front end, state, tokens and nodes, is **one allocation whose size follows from the
length of the text** (every node and token takes a byte, so tokens bound nodes), 160
bytes of state per byte of source. `parser.cho` is now only the walk that prints the tree
in the listing's order, so a match with the oracle is a statement about the *tree*:
the walk reads nothing the tree does not hold, except where the parser had already checked
the tokens and the node keeps only where they start (an effect row, a declaration's
`[T, &r where ...]`, a destructuring pattern, a match pattern). Everything stage 2
was checked with passes unchanged, and the translation compiled and matched the Rust
parser on all 621 programs the first time.
The cost of the walk: `ast.cho` (1,921 lines, 1,678 not comment or blank) and `parser.cho`
(847, 761) are 2,768 lines against stage 2's single 1,970; parsing and walking its own 80 KB
takes 0.24 s (stage 2 took 6 ms for its own 59 KB), with a 13 MB table to fill first; where
the time goes was not measured.

**Stage 3b: the declarations.** The Rust checker is one long function, and its first half,
`collect_declarations`, reads no body: imports, type declarations, foreign declarations,
statics and function signatures, in that order, stopping at the first refusal. That half is now a
function of its own (`cancho_ir::check_declarations`, a pure extraction; `lower` calls it), and
it is the oracle: the port's first refusal must be the Rust checker's, rule and span, which
makes the *order* of the checks part of what is compared. `pass1.cho` ports, in that order:
imports (`check_imports`), type names (built-in and duplicate), members (duplicate fields and
variants, `enum` with no variants), the finite-size check (`reaches`, with `Box` the finite way
back), statics, and signatures (built-ins, duplicates, generic and region names, `where`
clauses, duplicate parameters, `pub` signatures naming private types), and under all of it
`resolve_type`: modules, qualifiers, arity, type parameters, scalars, regions, `[T]`'s size,
tuples, `pub`. A type is resolved in place: each `TName` node of the tree is annotated with
what it names, which is what the tree is for.

Two things make a partial port honest. **`SKIP` at the point of divergence.** While the port was
partial, a construct whose checks were not ported (an `extern fn`, a `val` declaration, a
`val`-bounded parameter at an argument that was not a plain scalar, `Ffi` with a library)
ended the check with the answer `SKIP` *where the Rust checker would have reached it*, so a
refusal found before it still counted and anything after it did not, and a skipped file was
counted, by what the oracle said, not hidden. Each slice removed some; the last removed the
answer, and the harnesses now fail on anything but the oracle's. **Generated
tables.** The prelude's 46 types (name, arity, edition, whether another module may name it,
which parameters are `val`-bounded) and the 118 builtins' names and editions are data the port
cannot read from Rust, so `tables.cho` is generated, and a `cancho-ir` test fails when it is
not what `prelude_types`, `mode_of` and `Builtin::ALL` say (`UPDATE_SELFHOST_TABLES=1 cargo test -p
cancho-ir selfhost_tables` rewrites it).

What it found: the oracle caught an off-by-one in the import span in the first run, and
nothing else differed in 51,150 cases; the same two keywords as before (`module`, `region`)
cost a compile cycle each as the names of locals; and `cancho fmt` refuses `import m.ast as
ast`, an alias equal to the last segment, which the file then spells without the `as`.
`pass1.cho` is 920 lines (788 not comment or blank); the Rust it ports is spread over
`defs.rs`, `function.rs` and `lib.rs`, 942 non-comment lines in the ranges that hold it, about a
quarter of which (foreign declarations, modes) is not ported, so the ratio is nearer 1.2 than
0.8. That split is an estimate, not a count.

**Several files, and modes.** The compiler parses a program as a set of files, the user's and then
`std`'s, into one tree (`parse_into`, each file at its own base offset), so the port now does:
`driver.cho` reads a stream of files on standard input (a line `FILE <length>` and that many bytes,
each), lays them end to end with one byte between them as the Rust `SourceMap` does, and
`ast.cho` tokenizes and parses them one after another into the same tables, with a module table
(a path names one module however many files declare it) and an edition per file. Parsing
`std` and a program together, 485 KB, gives the Rust parser's listing byte for byte, in 0.27 s.
Checking with the library then exposed what the first slice had skipped: `std` names types with
a `val` bound (`Option[T]`, `Map[V: val]`) at arguments that are not plain scalars, so **modes**
are ported too. A type's mode is written as a function of its parameters, a bit for "`res`
whatever they are" and a bit for each parameter that makes it so, so nothing is substituted;
the prelude's bits are generated from `mode_of` itself (probing each parameter with a `res` type,
which is exact because a mode is a disjunction over members), and a type the file declares
has the bits of the members resolved so far, as the Rust checker, which fills members in as it
goes, sees them. `Ffi`'s scope (`parse_scope`) is ported with it. Only an `extern fn` still
ended a check in `SKIP`, which is the next paragraph.

**Foreign declarations.** The last part of the declarations, `foreign.cho`, ports the loop that
handles every `extern fn`: the name is not a builtin's nor a symbol another declaration binds,
every parameter and the result resolve (`c_ptr`, and `c_int` for a result, are names only a
foreign signature has), what crosses is what C can name (an `int`, a `bool`, an opaque pointer, a
borrowed capability, a borrowed `[byte]`), an `Ffi` is narrowed to a library and not to nothing,
exactly one `Ffi` is borrowed, and the row is the *same set* as what the borrowed capabilities
discharge. What a capability discharges is data and is generated into `tables.cho` from
`discharged_by` (each label plain, narrowed to the literal the type is written with, or to the
empty string), the way the prelude's modes are. The comparison is by equality of strings as
written, with one subtlety it found a way to be wrong about and did not: the Rust checker
canonicalises an `Ffi`'s library set (sorted, once each) in the type but compares the row's
`ffi("...")` as written, so `ffi("libssl,libc")` is not `ffi("libc,libssl")`, and the port
reproduces that. The rules moved to `rules.cho` and the order of the checks to `checker.cho`,
to keep `ast.cho` under the file budget.

What it does not show, and the next slice: the declarations half of the checker is
complete; the bodies are not started, and they are most of it. The library is parsed in
name order and not the compiler's, which cannot change an answer about the program's file
because the library has no refusal of its own and the program's items come first. A type that
contains itself met first by a mode check makes the Rust checker recurse without end (the
program is refused anyway, after the stack overflows); the port answers `res` past a depth
of 64 instead, and no case reached it.

**Size.** `parser.cho` is 1,970 lines (1,760 that are not comment or blank) for the
Rust parser's 1,507 (1,225): 1.3 to 1.4 times. `lexcore.cho` is 963 lines (897) for the
Rust lexer's 633, with its generated tables. Counted with `grep -v '^\s*//'` and
without blank lines; both ports are written one statement to a line, as `cancho fmt`
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

**Where it stands.** Stages 1, 2, 3a and 3b are done as far as stated and nothing in them blocks the rest of
stage 3, the checker (`cancho-ir`, 12 thousand lines), which is the real test: it is the first
stage with enough shape (resolution, linearity, regions, effect rows) to tell whether
the language is comfortable writing its own compiler. §5's decision does not change.

