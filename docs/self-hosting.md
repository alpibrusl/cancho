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

**Decision (as first written): stay on Rust for the compiler.** Revisit this document,
rather than re-running the spike, the day a concrete asker exists. Nothing in this document's
findings would need to change before that day; only the "no asker yet" line would.

**Update: there is an asker.** The project's owner asks for a cancho compiler written in cancho,
for the reason that a language that can build itself is the strongest proof of the language
(2026-10-07). That is the "concrete asker" this section waited for, and `AGENTS.md` §7's rule
against building what nothing asks for no longer applies to it. What stands: the Rust compiler is
the oracle at every stage (the staged port of section 6 compares against it, and a stage is not
done until it agrees), and nothing is switched over until the last stage, a fixed point where
the compiler written in cancho compiles itself to the same output. The aim is the whole
bootstrap: the checker complete, IR generation, a textual backend (LLVM IR or assembly, with the
system tools shelled out to as section 4 describes), and the fixed point.

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
| 3c. Bodies, function by function (scalar functions so far) | `examples/selfhost/body.cho` (and `bodies.cho`) | `check_bodies` (`cancho-ir`), the Rust checker's body check, answered per function | **First slice.** The port answers `OK`, a refusal, or `SKIP` for each function; every function it answers is the Rust answer: 644 repository programs alone (278 `OK` bodies and 11 refusals among those it answers; 898 function answers skipped), the library's 789 functions with one program (271 `OK`, 518 skipped), 304 targeted cases, and 57,579 fuzz cases (51,588 alone, 5,991 with the library): 57,166 identical, none different; 413 are not UTF-8 |
| 3d. References and slices of scalars | `examples/selfhost/types.cho` and `body.cho` | `check_bodies` | **Second slice.** Every function the port answers is the Rust answer: 644 repository programs, the library's 789 functions with one program (**437 verified `OK`, 55%**, from 34%), 408 targeted cases (104 for references, regions and slices), and 39,826 fuzz cases (35,352 alone, 4,474 with the library): 39,547 identical, none different; 279 are not UTF-8 |
| 3e-1. Structs of scalars | `body.cho` (types in `types.cho`) | `check_bodies` | **Third slice.** Every function the port answers is the Rust answer: 81 targeted cases for struct literals, field reads and writes (539 in all), 8,000 fuzz cases alone and 4,200 with the library, 1,165 library cases: none different |
| 3e-2. Enums of scalars and `match` | `body.cho` | `check_bodies` | **Fourth slice.** Every function the port answers is the Rust answer: 85 targeted cases for enum values, `match` and its errors (624 in all, 5 cross-module in the several-files test), ~11,000 fuzz cases and 1,269 library cases: none different; the library's verified bodies are 461 of 814 (57%) |
| 3e-3. Generic functions | `body.cho` (type variables and parameters in `types.cho`) | `check_bodies` | **Fifth slice.** Every function the port answers is the Rust answer: 88 targeted cases for type parameters, inference at a call and `ambiguous-type` (712 in all, 4 cross-module in the several-files test), 15,774 fuzz cases (11,220 alone, 4,554 with the library) and 1,359 library cases: none different; the library's verified bodies stay at 461 of 814 |
| 3e-4. `borrow` and `region` blocks | `body.cho` (block regions in `types.cho`) | `check_bodies` | **Sixth slice.** Every function the port answers is the Rust answer: 1,271 targeted body cases (456 of them for borrows, regions and their variants), 2 cross-module in the several-files test, 16,000 fuzz cases (11,463 alone, 4,726 with the library) and 1,475 library cases: none different; the library's verified bodies stay at 461 of 814 |
| 3e-5. Builtins that perform nothing, and prelude types | `prims.cho`, over a generated `builtins.cho` | `check_bodies` | **Seventh slice.** Every function the port answers is the Rust answer: 1,376 targeted body cases (105 new, among them the ones that must be answered and not skipped), ~19,000 fuzz cases and 2,033 library cases: none different; the library's verified bodies go from 461 to **590 of 814 (72%)** |
| 3e-6. Effect rows | `effects.cho` | `check_bodies` | **Eighth slice.** Every function the port answers is the Rust answer: 1,443 targeted body cases (67 new, with the strict ones that must be answered), ~19,500 fuzz cases (13,434 alone, 6,044 with the library) and 2,156 library cases: none different; the library's verified bodies are **625 of 855 (73%)** |
| 3e-7. The rest of the bodies: arena allocation (`alloc`, `alloc_slice`), the heap builtins (`box_slice`, `contents`), generic types, tuples, threads, then linearity, effects with arguments, regions | not started | `check_bodies` | |
| 4a. Writing LLVM IR for functions of `int` and `bool`: literals, the arithmetic and bitwise operators with their traps, comparisons, calls, `let`, assignment, `return` | `examples/selfhost/emit.cho`, written by the checker as it walks (`compile.cho`) | the Rust compiler, by what the built programs do | **First slice of the backend.** 74 programs built both ways and run: the same exit status, or a trap in both (18 trap); none different |
| 4b. Control flow: `if` with `else`, `while`, `&&` and `||` (the right side only when the left has not decided it) | `emit.cho` and `body.cho` | the Rust compiler, by what the built programs do | **Second slice of the backend.** 123 programs built both ways and run, 49 of them new (loops, recursion, early returns, short-circuit that must not run its right side, traps inside loops): the same exit status or a trap in both (27 trap); none different |
| 4c. `World`, `release` and a real `main`: `main(world: World)` calls the program, a `World` passed between functions, `release(world)` | `emit.cho`, `body.cho`, `types.cho`, `driver.cho` | the Rust compiler, by what the built programs do | **Third slice of the backend.** 254 builds run and compared: every one of the 123 programs through the old `run` convention and through a real `main`, and 8 more with a `main` of their own (a `World` passed on, `release` used as a value): the same exit status or a trap in both (55 trap); none different |
| 4d. The rest of the backend: bytes and strings, output (`split`, `Io`), structs, enums, references, generics, `clang` run by the compiler itself | not started | | |

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
tables.** The prelude's 49 types (46 before `udp.md` added `Udp`, `UdpOpened` and `Datagram`) (name, arity, edition, whether another module may name it,
which parameters are `val`-bounded) and the 133 builtins' names (118 when this was written) and editions are data the port
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

**Stage 3c: bodies, one function at a time.** The second half of the Rust checker reads function
bodies, and a body is checked against other functions' *signatures* and never their bodies, so each
function is a case of its own. `cancho_ir::check_bodies` (the loop `lower` already ran, extracted
as `body_results`) answers per function, and the port answers per function: `OK`, the first
refusal of the body, or `SKIP` where the body uses something it does not check yet. That makes a
partial port useful at once: it can be exactly right about the functions it handles while it
does not handle the next. `SKIP` is raised where the Rust checker would have met the construct, so a
refusal found earlier in the function still counts.

`body.cho` handles functions with no type or region parameters and only scalar parameters and
result: `let` and `var`, assignment to a local, expression statements, `if`, `while`, `return`;
integer, float, `f32` (edition 6) and `bool` literals, locals, the unary and binary operators,
and calls to such functions (arity, argument types, locals that are not functions, module
qualifiers, visibility). What the Rust checker does for them is more than types: a literal
operation that can only trap is refused at compile time (`constant-traps`) and **folds through
nested literals**, so `(1 + 2) * 9223372036854775807` is refused where the outer operation is
met; `unreachable-statement` and `missing-return` are decided on the lowered statements; and an
empty row is exact, so a declared label that nothing performs is refused. The order is
the Rust checker's: the body, then the row, then the return. Integer arithmetic in cancho traps
on overflow, so every folded operation is decided before it is taken (the boundary cases for
`+ - *` are in `fuzz.py`).

Results: nearly all of what the port can say is checked against the Rust answer and none
differs. How much it can say is the number to watch: with the library, 271 of its 789 functions
(34%) are verified `OK` and the rest are skipped, because the port has no references, structs,
`match`, generics or builtins yet. Every program's `main` is skipped (a `World` is not a scalar),
so no *program* is wholly verified yet.

How strong the comparison is was measured, not assumed: a **mutation test** changes one comparison
or arithmetic operator at a time in `body.cho` (144 mutants) and asks whether the comparison with
the Rust checker notices. The first run caught 82; the survivors were boundaries the edge cases
never reached (an overflow decided at exactly `int::MAX`, a product at exactly `-2^63`), and
adding them took it to 101; an operator-by-operand-type matrix (every operator on `int`, `bool`,
`float` and `byte`) took it to **105 of 144**. The 39 that survive were not each analysed. The ones
looked at are of two kinds: *equivalent* (a loop over the node list that starts at the first
item, `b > 0` against `b >= 0` where the zero case is decided earlier) and *hidden by `SKIP`*,
which is a limit of the method: a mutant that makes the port skip a function it used to answer
cannot be told from a function it does not handle yet. So the comparison cannot see a port that
becomes *more* cautious, only one that becomes wrong. `fuzz.py --bodies` and `bodies_diff.py` are the
harnesses, and `selfhost.rs` runs it in CI over the corpus, the edge cases, and a sample with the
library.

**Stage 3d: references and slices of scalars.** `types.cho` is the type machinery a reference needs:
a table of types in the state (a scalar is just a small integer, so it costs nothing), regions as
integers (a parameter of the function, `static`, or a variable a call makes), `Unifier::unify` and
`unify_regions`, `outlives` (reflexive and transitive over the declaration's `where` clauses, with
`static` outliving everything), and `expect_type` with its two coercions: a reference whose region
outlives the expected one, and a unique reference where a shared one is wanted, never the
reverse, the referent invariant. A call instantiates the callee's region parameters as fresh
variables that its arguments solve, then checks the callee's `where` clauses against what they
solved to (an unsolved variable outlives nothing but itself). Order matters and is the Rust one: a
reference is unified by uniqueness first, then region, then referent, which decides whether a
mistake is a `type-mismatch` or a `region-mismatch`.

On top of it `body.cho` gains `*r`, `*r = v`, `s[i]`, `s[i] = v`, `s[a..b]`, `len(s)`, string
literals (a shared slice of bytes in `static`), `let` with a reference type, and calls whose
parameters and results are references. A reference is `val` whatever it points at, so nothing here
is a resource and the linearity half of the checker cannot refuse any of it; that is what made
this a slice the port could take without the `borrow` blocks, which are where `borrow-conflict`
lives. What it still skips: references to anything but a scalar or a slice of one, `borrow` and
`region` blocks, `*s` of a slice, and writing through a slice reference.

Result: with references the port verifies **437 of the library's 789 function bodies (55%)**, up
from 271 (34%), every one the Rust answer, and 375 `OK` bodies among the repository's own programs,
up from 278.

**Stage 3e-1: structs of scalars.** A struct the file declares, with no parameters, not `res`, and
only scalar fields, is a type the table knows (`NAMED`, by the node of its declaration); it is `val`,
so a value of it copies and linearity still has nothing to say. On top of it `body.cho` checks the
struct literal (every field once, in declaration order, each of the declared type: `unknown-name`,
`duplicate-declaration`, `field-order`, `missing-field`, `not-a-struct`, `not-public` in the order
Rust meets them), `p.x` through a value or one reference, and `p.x = v` through a unique reference.
Anything else that mentions a struct (a prelude type, an enum, a generic or `res` struct, a field that
is not a scalar) is `SKIP`. The library gains little, since its structs are mostly generic or `res`
(439 of 788 verified); the repository's own programs gain the most.

The mutation test of `body.cho` kills 139 of 199 (105 of 144 before); the survivors in the new code
are node numbers that are never 0 and the equal-index case a duplicate has already caught.

**Stage 3e-2: enums of scalars and `match`.** An enum the file declares, with no parameters, not
`res`, and only scalar payloads is a `NAMED` type like a plain struct, and `val`. `body.cho` checks
the variant expression (`not-an-enum`, `unknown-name`, `not-public`, `arity-mismatch`, each payload
of its declared type) and `match` on a value or on a reference to one: the arms, in order, each a
`_` or a variant named once (`match-arm-unreachable` for a repeat, for an arm after `_`, and for a `_`
when every variant is already named; `unknown-name`, `arity-mismatch`, `duplicate-declaration` for
a pattern; `match-not-exhaustive` at the end), with the bindings in a scope of their own. Through
a reference each binding is a reference to the payload with the scrutinee's uniqueness and region,
so nothing moves. A pattern is not kept in the tree, so the checker reads the tokens after the
arm's first. `.` on an enum is `match-on-a-non-enum` (a read) or `linear-value-taken-apart` (a
write), and a `match` terminates when every arm does. Anything else about an enum (a generic or
`res` one, a payload that is not a scalar, a prelude type) is `SKIP`. The mutation test of
`body.cho` kills 180 of 255; the survivors in the new code are node numbers that are never 0 and
loop bounds the guards above them already settle.

**Stage 3e-3: generic functions.** The type table gains a rigid type parameter (`Param`, by its
position) and a type variable (`Var`, with its solution in an array beside the table, like the
region variables). A call to a generic function makes a fresh variable for each type parameter of the
callee, lowers the callee's declared types with the parameters as those variables, and unifies each
argument with its parameter, binding variables as `Unifier::unify` does (an occurs check, and the
result a `type-mismatch`, a `region-mismatch` or an `infinite-type`). Once the arguments have been
unified every variable has to be solved: the first that is not is `ambiguous-type` at the call. The
return type is then resolved all the way down, so no variable leaves the call, and nothing else in
the checker needs to look through one. A function whose own type parameters are all `val`-bounded
is checked with them as rigid types; one with an unbounded `T` is `SKIP`, because a value of `T` could
be a resource and linearity is not checked yet. Generic structs and enums (a type applied to types)
are the next slice. The library gains nothing (its generic functions mostly take a `Heap` or have an
unbounded `T`), so the number of bodies verified stays at 461 of 814. The mutation test of
`body.cho` kills 190 of 271, its survivors in the new code changing only whether a function is
`SKIP` (which the comparison cannot see) or a loop bound the guards settle; that of
`types.cho` kills 60 of 78 (40 of 54 before), the survivors in the new code being an occurs check no
found type can reach, a variable compared with itself that a found type cannot be, and counters that
stay distinct under the swap.

**Stage 3e-4: `borrow` and `region` blocks.** A block is a region of its own, numbered in the
order it opens and remembering the block that was open around it; a reference into it is `Ref`
with that region, and `outlives` answers as `Region::Block` does (a block outlives the ones it
encloses, a region parameter outlives every block, nothing outlives `static`). `borrow x as &r in {}`
looks `x` up (`unknown-name`), opens the block, freezes `x` (or locks it, for `borrow mut`), binds
`r` to a reference of the block's region, checks the body and thaws; `region a {}` is the same
with nothing frozen. Whether a binding is borrowed is a flag in its record: owned, frozen by
n shared borrows, or locked by a unique one. A name read while locked, an assignment while frozen
or locked, a second unique borrow, and a unique borrow of a frozen name are `borrow-conflict`.
Rust finds those in its linear pass, which runs after the body and only if the body had no
error, so the port notes the first as it walks and reports it at the end, after the check that
no binding holds a reference into a block it outlives (every binding is logged with the block open
where it was declared) and before the row and the missing `return`. A `let` annotation that names a
region inside a block is `SKIP`, because the name may be the block's own. A `borrow` or `region`
whose body returns terminates the block around it. The header of the state grows to 48 slots and
a binding record to four. Of the first mutation run's survivors, some showed that the cases
used the first binding and the first block everywhere (so an offset swap changed nothing); the
cases now come padded with bindings, with sibling blocks opened before, and without the final
`return` that made the statement after a returning block `unreachable-statement` before a
conflict could be reported. The mutation test of the new code in `body.cho` kills 33 of 42, the
survivors being equivalent comparisons, a stored name nothing reads, and loops whose only effect is
whether a function is `SKIP`.

**Stage 3e-5: builtins and prelude types.** What the checker needs to know of a builtin is data, so it
is generated: `builtins.cho`, beside `tables.cho`, holds for each builtin its signature as a prefix-coded
run of integers (a scalar, a reference with its region parameter, a slice, a prelude type, a tuple, a type
parameter), its region parameters and its effect labels, and for each prelude struct its fields (the
`Split` of each edition). `prims.cho` decodes a signature into the checker's types, with each region
parameter a fresh variable, and a call to a builtin goes the way a call to a function does: the
arity (`arity-mismatch`), each argument unified with its parameter, the result. What it cannot hold (a
tuple, a type parameter, a literal type, a prelude type that is a resource and not behind a reference) decodes to
nothing and the call is `SKIP`; so is a builtin the lowering checks by hand (`split`, `release`, `len`,
`box`), and a builtin that performs an effect, because rows are not checked yet and answering `OK`
for a function whose row Rust would refuse would be wrong. The prelude types without arguments are now
types of the checker too: `Io`, `Heap`, `Args`, `File` behind a reference (a borrowed capability is
`val`), and `Done`, `Read` (not resources) by value; a field read or a `match` on one is `SKIP`. The case
list now has a strictness: a skipped function cannot be seen by a comparison, so a change that only
makes the port give up would pass; the cases written to be answered are marked, and one of them
skipped is a failure. The mutation test of `prims.cho` kills 27 of 38 (9 of 38 before the cases
were written for it), the survivors being branches no builtin the checker can call reaches. The state
and `body.cho` were split for length: `scope.cho` (bindings, borrows, blocks and the lowering of written
types), `exprs.cho` (expressions, folding, calls) and `body.cho` (statements and `check_function`).

**Stage 3e-6: effect rows.** A function's row is exact (`docs/linearity-and-effects.md` section 7.3): what
its body performs, which is what the builtins it calls perform and what the rows of the functions it
calls say, must be the row, both ways. Rust finds the first label in the body that the row does not
declare (`effect-not-declared`), and failing that the first in the row the body does not perform
(`effect-declared-not-performed`); both are refused at the function. A set of labels is an integer here, bit
i for the i-th label of the generated table (twenty, so far), a label a function's row names that no
builtin performs is a flag in a high bit, and so is one that carries an argument (`fs_read("/tmp")`).
A call to a builtin adds its labels, a call to a function adds the labels of its row, in whatever branch
or loop it is, and `check` compares the two sets once the body is clean and the borrow conflicts are
reported, where Rust does, and before the missing `return`. What it leaves to `SKIP`: a callee whose
row has an argument or an unknown label, and a body that performs anything in a function whose own row
has an argument label, because a plain label and one with an argument are different labels and
the port does not yet follow how an owned capability discharges them. A function that owns a capability
by value is not checked either, so nothing is discharged here. The cases include the row in either
order, with a trailing comma, with a duplicate, with a name nothing performs, rows that propagate
through two and three callees and through recursion, and an effect in a dead branch, a short-circuit
and an argument. The mutation test of `effects.cho` kills 24 of 31, the survivors being loop bounds
that read one label past the table and a sentinel, and one case it found missing (a callee whose row is
only an argument label), which is now one. With the pure builtins this takes the library from 590 of
814 to 625 of 855 verified, and the checker can now answer for a function that prints.

**Stage 4a: the first code.** The backend is written the way the Rust one is: the module is LLVM IR as
text, and `clang` turns it into the executable (`cancho-codegen-llvm` does the same, so nothing here
needs a library cancho cannot reach). The port writes it while the checker walks, because the checker
already visits every expression in the order it runs and knows its type: the state gains three
buffers (the finished output, the function being written, and its `alloca`s, which belong in the entry
block however late a `let` comes), and `emit.cho` the operations that write to them, an operand being
a constant or a temporary `%t<n>`. Each function is `define i64 @lexs_name(...)` with a stack slot for
each binding, a `load` for a name, `llvm.sadd.with.overflow` and its siblings and a branch to a trap
for `+`, `-` and `*`, the explicit checks `sdiv` and `srem` need for a zero divisor and `int::MIN` by
-1, the range check on a shift, comparisons as `icmp`, a `call` for a call and a `ret` for a
`return`. The checker does what it did and writes only when `compile` mode asks; anything it cannot
write yet (floats, bytes, references, control flow, a generic function) is a `SKIP`, which in this
mode is a refusal with the function named. The IR is not compared with the Rust backend's, which is
another text; the programs are: each of 74 is built by both compilers and run, and must end the same
way, the same exit status or a trap. A program the Rust build needs a `main(world)` for: the cancho
compiler calls a function named `run` from a C `main` for now, until `World` and `release` are checked.

**Stage 4b: control flow.** An `if` is a branch on the condition to a `then` block, an `else` block (empty
when there is none) and an `end` block; a `while` is a `head` block where the condition is computed, a
branch to the `body` or the `end`, and a jump back. Each construct takes one number from the same counter
the trap blocks use, so no two share a label. `a && b` and `a || b` keep their result in a stack slot of
their own: after `a` the slot holds it and `b` is computed only if `a` did not decide the answer. The
condition now hands its value back so the branch can name it. After a `return` the next block is a
`dead` one, so every `br` that ends an arm has a block to end, and the end of the function is an
`unreachable` the checker has shown nothing reaches. Programs that matter here: a short-circuit whose
right side would trap if it ran (it must not), loops that trap on the iteration that overflows, an early
`return` from inside a loop, and the recursive ones.

**Stage 4c: `World` and `main`.** `World` is the first prelude type with a type of its own in the table
(by its index in `tables.cho`), and only in compile mode, where it is a type a parameter may have and a
function may pass on. Like the Rust backend the port writes no code for it: a function's `World`
parameter has no argument and no stack slot, and a call's `World` argument is not passed. `release(w)`
is a call that writes nothing and answers `0`. The driver calls the program's `main` from the C one
when it has it, and a function called `run` when it has not, as before. The checker's answers do not
change: outside compile mode a `World` still makes a function `SKIP`, because checking `main` means
checking that its `World` is consumed exactly once, which is linearity and not written; in compile mode
the program is taken to be well formed, which is the Rust compiler's to say. The 123 programs are built
through a real `main` as well as through `run`.

The mutation test of `types.cho` (54 operator swaps) killed 40 on the first corpus of 458 targeted
body cases (after region-variable, `where`-closure and coercion cases were added; 33 before); the 14
that survive are guards the other conditions already make redundant (`&&` on kinds that are
checked again below, the bounds of a table index, the fuel of a closure that cannot cycle).

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

