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
| 4d. Output: `split(world)` and its parts, a `borrow` of the `Io`, `putchar`, references to whole numbers and booleans, and what an owned capability discharges | `emit.cho`, `body.cho`, `exprs.cho`, `effects.cho` | the Rust compiler, by what the built programs do and write | **Fourth slice of the backend.** 268 builds run and compared, 14 of them print (characters, decimal numbers by recursion, loops, `if` chains, nested helpers, sequential borrows, a trap after output): the same exit status or trap and the same bytes on the standard output (56 trap); none different |
| 4e. Strings and slices: a literal as a slice of bytes, `len`, `s[i]` with its bound, `write_bytes`, `int_of`/`byte_of`, a slice passed, kept and reassigned, a store through `*r` and `s[i]` | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho` | the Rust compiler, by what the built programs do and write | **Fifth slice of the backend.** 292 builds run and compared, 24 of them new, 60 trap; none different |
| 4f. Structs of whole numbers, bytes and booleans: a literal, a field read, a copy, passed and returned, a field written through a unique reference, `*r` of one | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **Sixth slice of the backend.** 312 builds run and compared, 20 of them new, 63 trap; none different |
| 4g. Enums and `match` on them: a variant with payloads, `match` by value and behind a reference (payloads are references, written through), `_` arms, nested | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **Seventh slice of the backend.** 332 builds run and compared, 20 of them new, 64 trap; none different |
| 4h. Arenas: `region` and `alloc_slice[a](n, fill)` of whole numbers, bytes and booleans, in the checker (the arena in scope, the count, the fill a value that copies, the slice cannot leave) and in the code | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho`, `types.cho`, `driver.cho` | the Rust compiler: the checker by answers (1,945 cases), the code by what the built programs do | **Eighth slice of the backend, and a checker rule.** 352 builds run and compared, 20 of them new, 71 trap; none different. The checker's 1,945 cases are identical, 27 of them new |
| 4i. The heap: `box_slice`, `contents`, `unbox_slice` of whole numbers, bytes and booleans, with a `Heap` borrowed and passed (compile mode only: the checker still answers `SKIP` for a function that has one) | `emit.cho`, `exprs.cho`, `types.cho`, `scope.cho`, `effects.cho` | the Rust compiler, by what the built programs do | **Ninth slice of the backend.** 367 builds run and compared, 15 of them new, 76 trap; none different. The checker's 1,945 cases are unchanged |
| 4j. `static` tables: `static name: [T] { .. alloc_slice[static](n, fill) .. return t; }` of whole numbers, bytes and booleans, and the name used as a shared reference to the slice, in the checker (20 new cases) and in the code | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho`, `driver.cho` | the Rust compiler: the checker by answers (1,965 cases), the code by what the built programs do | **Tenth slice of the backend, and a checker rule.** 382 builds run and compared, 15 of them new, 78 trap; none different. The library's skipped functions go from 641 to 612 |
| 4k. Functions that return a slice, and sub-slices `s[a..b]` of whole numbers, bytes and booleans | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **Eleventh slice of the backend.** 404 builds run and compared, 22 of them new, 84 trap; none different. The census: 529 of the compiler's own 548 functions get code (488 before) |
| 4l. Structs and enums whose fields and payloads are structs, enums or a `Box[[T]]` (and `res` ones, `std.buffer`'s `Buffer`), destructuring a struct of the file, a field of a `res` type read through a reference as a borrow, byte parameters and results, and the builtins `wrapping_*`, `copy_into`, `index_of_byte`, `getchar`, `write_err` | `emit.cho`, `body.cho`, `exprs.cho`, `scope.cho`, `types.cho` | the Rust compiler, by what the built programs do | **Twelfth slice of the backend.** 424 builds run and compared, 20 of them new, 85 trap; none different. The census: **558 of the compiler's own 558 functions get code, and all 39 of `std.bytes`/`buffer`/`io`** |
| 4m. Several files, and the bootstrap: a function is named by its node (`@lexs_<n>`), so two modules may have a function of one name; what the code is not written for is left out and a comment says how many; `compiler.cho` is the compiler for a program of several files | `driver.cho`, `emit.cho`, `body.cho`, `compiler.cho`, `bootstrap.rs` | **the compiler against itself**: built by Rust, it compiles its own source to IR, `clang` builds that, and the result compiles the same source to the same IR | **The fixed point.** 100,409 lines of IR, 637 functions, byte for byte the same from the compiler Rust built and from the one it built; 55 functions left out, all of `std.math`, which nothing calls. The checker built by the second compiler agrees with the Rust oracle on 1,980 of 1,980 cases |
| 4n. Capabilities and handles in the code: a target (`TARGET darwin`/`linux` in front of the input), `runtime.ll` (paths checked against the prefix a capability was narrowed to, `open_*`, `file_write`/`file_read`/`file_close`, `exec_spawn`, `child_wait`, `arg_count`/`arg`), prelude enums and handles as values, `narrow`; and `builder.cho`, the compiler that runs `clang` itself through its `Exec` | `runtime.ll`, `runtime.cho`, `emit.cho`, `exprs.cho`, `types.cho`, `scope.cho`, `driver.cho`, `builder.cho` | the Rust compiler: the programs by what they do (433 builds, 89 trap), the builder by building itself | **The compiler builds a program with no help.** The builder built by Rust builds itself, the result builds itself again, and the three write the same 3.2 MB of IR (equal sha256). The census: 614 of 614 functions (the compiler, its runtime and the builder) and 39 of 39 in `std.bytes`/`buffer`/`io` |
| 4o. `static` tables computed while the program is compiled: `consteval.cho`, an interpreter for the bodies that build a table (loops, conditionals, the operators, table reads and writes, string literals, `len`/`int_of`/`byte_of`, earlier statics), whose answer is data of the module; a body it cannot run (a call to a function of the program, a trap, too much room) still runs when the program starts | `consteval.cho`, `body.cho`, `ast.cho`, `driver.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 442 builds run and compared (9 new), 89 trap; none different. The census: 644 of 644 |
| 4p. One value on the heap and in an arena: `Box[T]` of any type the code holds, `box`, `unbox`, `contents` of a box, `alloc[a](v)`, and so a type that holds itself through a box (`enum List { Empty, Cons(int, Box[List]) }`) | `types.cho`, `emit.cho`, `exprs.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **Thirteenth slice of the backend.** 454 builds run and compared (12 new), 89 trap; none different. Of the 115 programs of `tests/accept`, 55 build and give what their headers say (49 before) |
| 4q. Tuples of two or three parts as values (`(int, bool)`, `(Box[int], int)`, a tuple in a struct or a payload), `.0` and `let (a, b) = e;`; `narrow(fs, "a", "b")` as a tuple of capabilities; the one-shot `fs_read` and `fs_write` | `types.cho`, `emit.cho`, `exprs.cho`, `body.cho`, `scope.cho`, `runtime.ll` | the Rust compiler, by what the built programs do | **Fourteenth slice of the backend.** 467 builds run and compared (13 new), 90 trap; none different. Of the 115 programs of `tests/accept`, 66 build and give what their headers say (55 before) |
| 4r. Generics, by monomorphization: a call to a generic function asks (`instances.cho`) for the function at the types its arguments solved, each type travelling as a code from one function's table to another's; the driver compiles the instances after the other functions, each with its parameters standing for those types, and a call from one can ask for more; literals of generic structs and enums (`Pair { .. }`, `Option::Some(5)`, `Option::None` learning its type from a `let` annotation, a `return` or a call's other arguments) | `instances.cho`, `types.cho`, `scope.cho`, `exprs.cho`, `body.cho`, `driver.cho`, `emit.cho` | the Rust compiler, by what the built programs do | **A difference closed**; up to two type parameters and four fields or payloads, none that is a reference; a `var x = Option::None` that a later statement types has no code yet |
| 4s. `float` and `f32`: literals read to the last bit with whole numbers only (`floats.cho`: the digits as a big number, a power of five and a shift, one rounding, ties to even, as `f64::from_str` and `f32::from_str` do), `+ - * /` and the ordered comparisons, negation, the thirteen builtins (`float_of`, `truncate` with its three traps, `bits_of`, `is_nan`, `sqrt`, the `f32` conversions and bit casts), floats in locals, parameters, results, structs, slices, arenas and `static` tables (run when the program starts) | `floats.cho`, `floatcode.cho`, `exprs.cho`, `emit.cho`, `types.cho`, `body.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 577 builds run and compared (135 new: 23 programs, 32 `float` and 17 `f32` literals, and the negative of each `float` one), 94 trap; none different. `tests/accept`: 76 of 115 |
| 4t. Threads: `spawn(payload, f)` and `join(t)` on real OS threads (`pthread_create` with the function as the start routine and the payload as its one pointer; `pthread_join` and the cell the result comes back in), a function used as a value (`let w = worker;`) and called through the name (`w(5)`), payloads and results that are a whole number, a byte, a boolean, a handle, a reference or a capability | `threadcode.cho`, `exprs.cho`, `types.cho`, `scope.cho`, `emit.cho`, `fold.cho` (split out of `exprs.cho`) | the Rust compiler, by what the built programs do | **A difference closed.** 9 thread programs added to the compile differential, none different. `tests/accept`: 82 of 115 (the spawn programs that stop do so at the clock, the network, `usleep` and `pthread_self`, not at threads) |
| 4u. Foreign functions: a call of an `extern fn` crosses as the Rust backend sends it (a `c_ptr` as a pointer, an `int` as 64 bits, a `byte` or `bool` as 8, a `float` as a double, a borrowed `[byte]` as its pointer and length, a borrowed capability as nothing) and comes back as a pointer, 64 bits, a `c_int` made 64 by sign, or a boolean; `c_ptr` values, `null_ptr()`, `==` and `!=` on them; the call spells its own function type, so a program's `extern fn write` or `memchr` need not agree with the runtime's declaration | `externcode.cho`, `codes.cho` (the builtin numbers, out of `exprs.cho`), `exprs.cho`, `driver.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 4 foreign programs added to the compile differential, none different. `tests/accept`: 90 of 115 |
| 4v. Sockets and the clock: `bind`, `connect`, `listen`, `accept` (edition 2), `tcp_listen`, `tcp_connect`, `tcp_accept`, `conn_read`, `conn_write`, `conn_nonblocking`, `listener_nonblocking`, `conn_nodelay`, `conn_close`, `listener_close` (edition 5) with their answers (`Listening`, `Dialed`, `Accepted`, `Received`, `Sent`), and `clock_ms`, `clock_unix_ms`; the host and the port checked against the bound a `Net` was narrowed to, trapping when they differ. The support is C (`runtime/net.c`) turned into LLVM IR for each target by `runtime/make.sh` and printed only into a program that uses it | `netcode.cho`, `runtime/net.c`, `runtime/net_linux.ll`, `runtime/net_darwin.ll`, `runtime.cho`, `exprs.cho`, `codes.cho`, `driver.cho` | the Rust compiler, by what the built programs do | **A difference closed, on both systems.** 4 programs added (the edition 2 integers, a connection made, accepted, written and read inside the process, one refused, the clocks), none different. `tests/accept`: 94 of 115 on macOS arm64 and on Linux x86-64 (`gram`), the same 94 |
| 4w. Pipes: `pipe_open` (a socket pair, close-on-exec, `SO_NOSIGPIPE` on Darwin) with its four-leaf answer `Piped`, `pipe_read`, `pipe_write`, `pipe_close`, `child_end_close`, `pipe_nonblocking`, which are the connection verbs on the same support; a child's streams as `Stdio::Pipe` | `netcode.cho`, `runtime/net.c`, `exprs.cho`, `codes.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 2 programs added (a child's output read, and a child written to and read from: `cat`), none different. `tests/accept`: 95 of 115 |
| 4x. Directory handles and listings: `open_dir`, `dir_enter`, `dir_open_read`/`new`/`append`, `dir_rename`, `dir_rename_new`, `dir_remove`, `dir_sync`, `dir_close`, `dir_list`, `dir_next`, `dir_list_close`, `dir_stat`, `dir_mode`, `dir_own_mode`, `dir_mkdir` (#595), `dir_open_new_mode` (#596), each name one path component checked first; the support is C (`runtime/dirs.c`) whose only per-target text is the array of constants the compiler writes (`platform.cho`: open flags, `AT_FDCWD`, `dirent` and `stat` offsets, errno values) for macOS, Linux x86-64 and Linux aarch64 (`TARGET linux aarch64`) | `platform.cho`, `runtime/dirs.c`, `netcode.cho`, `driver.cho` | the Rust compiler, by what the built programs do | **A difference closed.** A program that makes, measures, renames, removes through a handle compared both ways; the two accept programs pass. `tests/accept`: 97 of 115 |
| 4y. `defer`: the expressions wait on a stack and are written out where their block ends and at every `return` (after its value is read), innermost and last-written first, as the Rust lowering expands them | `body.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 4 programs in the compile differential (the accept program, a loop, two frames, a `res` closed on both paths). `tests/accept`: 98 of 115 |
| 5a. The builtins on words: `load_le64`, `byte_mask64`, `trailing_zeros`, `leading_zeros`, `popcount`, `mul_wide`, `add_carry`, `sub_borrow`, `value_barrier`, `copy_within`, `trap`, with the bounds checks that trap | `wordcode.cho`, `codes.cho`, `exprs.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 11 programs added, among them the traps; none different. `tests/accept`: 104 of 115 |
| 5b. Capabilities held in structs (a place in the aggregate, `{}`, and no value), `fork_heap` and `fork_clock` (a new capability, nothing in the code) | `emit.cho`, `scope.cho`, `exprs.cho` | the Rust compiler, by what the built programs do | **A difference closed.** The four accept programs that hold a heap or capabilities in a struct, and the two that fork, are compared both ways. `tests/accept`: 109 of 115 |
| 5c. References held in structs, enums and tuples: a reference to a value is a pointer, a run of bytes a pointer and a length (`{ ptr, i64 }`), put in and taken out like any field; the region such a field has is `any_region`, which outlives and is outlived by all | `types.cho`, `emit.cho`, `exprs.cho`, `body.cho`, `scope.cho` | the Rust compiler, by what the built programs do | **A difference closed.** 5 programs added (a struct, an enum with a match, a tuple, through a borrow, destructured). `tests/accept`: 110 of 115 |
| 5e. The hardware AES and GHASH builtins (`hw_aes_gcm`, `aes_encrypt_block`, `aes_ctr32`, `ghash_update`, `ghash_powers`, `gcm_tag`, `gcm_tag_diff`) and the SHA-256 ones (`hw_sha256`, `sha256_blocks`, `sha256_block_words`): the functions are the text `cancho-codegen-llvm` writes for each target (`runtime/crypto_*.ll`, dumped and checked by a test of that crate), carried by a module that calls them; the call sites check lengths and trap as the Rust backend does (`cryptocode.cho`) | `cryptocode.cho`, `runtime/crypto_*.ll`, `ex.cho` (split out of `exprs.cho`) | the Rust compiler, by what the built programs do | **A difference closed on macOS.** 4 programs added; of the 105 programs of `examples/` and `tests/programs/` the Rust compiler builds, 69 now build and give the same output (52 before). Not run on Linux yet |
| 5f. The poller (`epoll`, `kqueue`: `poller_new`, `poller_add_*`, `poller_modify`, `poller_remove`, `poller_wait`, `poller_close`), the hand-over of a connection (`conn_detach`, `conn_attach`, with the table of epochs), `conn_peer`, `conn_connect_status`, `tcp_connect_start`, `flush_out`, `read_bytes`; runs of structs, enums and tuples (the offset is the size of the element), a reference returned, generic literals of up to 12 fields, a variant given as an argument typed by what the callee wants or by the other arguments; `exec_spawn_in`, a child's `pidfd` on Linux, and the signal set and `closefrom` a spawn has | `runtime/net.c`, `runtime/os_*.ll`, `netcode.cho`, `emit.cho`, `exprs.cho`, `lookup.cho` | the Rust compiler, by what the built programs do | **A difference closed.** A program with a poller, a hand-over, a peer and a connect status, and one with `flush_out` and `read_bytes`, compared both ways; runs of structs. Of the 106 programs of `examples/` and `tests/programs/` that can be compared, 86 build and give the same output (69 before) |
| 5g. A reached function the code is not written for is a refusal (`SKIP`, at its span); `cancho build --compiler <exe>`; functions held by their type (`f: fn(int) -> [] int`, up to three parameters) in a parameter or a name, called through the pointer; `atoms.cho` split from `exprs.cho` | `driver.cho`, `types.cho`, `scope.cho`, `exprs.cho`, `atoms.cho`, `emit.cho`, `crates/cancho/src/selfhost_build.rs` | the Rust compiler, by what the built programs do | **A difference closed.** `tests/accept` 111 of 115 build and agree (110 before); the programs of `examples/` and `tests/programs/` that can be compared stay at 86 of 106 |
| 5h. Operations on an open file (`file_pwrite`, `file_pread`, `file_sync`, `file_truncate`, `file_size`, `file_lock`) and on a path (`fs_remove`, `fs_rename`); the signal claim (`signals_watch`, `signals_pending`, `signals_close`, with the thread count `spawn`/`join` keep); tuples of up to eight parts; a reference or a capability a function lends and gives back; `cancho build --compiler` in a project; `filecode.cho` | `runtime.ll`, `runtime/signals.c`, `filecode.cho`, `netcode.cho`, `types.cho`, `exprs.cho`, `crates/cancho/src/project.rs` | the Rust compiler, by what the built programs do, and the signals conformance with the compiler in cancho as a third backend | **A difference closed.** `tests/accept` 112 of 115 build and agree (111 before); 20 of 20 signal tests pass with it as a backend; cancho-cache, cancho-oci (golden digests), cancho-robot, cancho-gpu and cancho-hooks build with it |
| 5i. A variable whose type is what later statements make it (`var x = list.List::Empty;`): a function with one is gone over twice, once to learn such types (written down after the function in the instance table as rows with a negative declaration) and once for the code | `instances.cho`, `driver.cho`, `body.cho`, `scope.cho`, `exprs.cho` | the Rust compiler, by what the built programs do | **A difference closed.** `tests/accept` 113 of 115 build and agree (112 before); of the 106 comparable programs of `examples/` and `tests/programs/`, 89 give the same output (86 before) |
| 5j. The serial-port verbs (`tty_open`, `tty_configure`, `tty_read`, `tty_write`, `tty_flush_input`, `tty_close`), from `runtime/tty.c` with the layout of `struct termios`, the open flags, the Linux rates and the macOS `IOSSIOSPEED` step of `cancho_ir::tty` | `runtime/tty.c`, `netcode.cho`, `codes.cho`, `driver.cho` | the Rust compiler, by what the built programs do; a program on a pseudo-terminal opened, configured, written to and read agrees with the Rust build on what it does and on what fails (macOS) | **A difference closed.** `tests/accept` 114 of 115 build and agree (113 before); of the 106 comparable programs of `examples/` and `tests/programs/`, 91 give the same output (89 before), 1 is refused (`process_capture`) |
| 5k. `child_kill` (the signal by its bit, to the child whose word is in the slot; `cancho_native_signal` per system); a literal of a declaration with parameters assigned to a variable (`x = Option::None;`) is the type the variable has | `runtime.ll`, `runtime/os_*.ll`, `netcode.cho`, `body.cho` | the Rust compiler, by what the built programs do | **A difference closed.** Of the 106 comparable programs of `examples/` and `tests/programs/`, `process_capture` builds and agrees; `tests/accept` stays at 115 of 116 (`aliasing_in_a_slice_of_references`, a slice whose elements are slices, is the one refused) |
| 5l. The conformance suite run through the compiler in cancho: arenas of one 64 KiB chunk that trap when a request goes past it and are given back at a `return`; the datagram sockets (`udp_connect`, `udp_bind`, `udp_recv`, `udp_recv_from`, `udp_send_to`, `udp_peer`, `udp_local_port`, with the ring of senders and its tickets); `tcp_listen_on` (the address of its literal); the signal a child was killed by as its bit; `udp_attach` answering `UdpOpened`; `--compiler auto` and `CANCHO_COMPILER`; a refusal explained by the Rust front end's sentence | `driver.cho`, `emit.cho`, `body.cho`, `netcode.cho`, `runtime/net.c`, `runtime.ll`, `crates/cancho/src/selfhost_build.rs` | the Rust compiler, by the whole conformance suite with `CANCHO_COMPILER` set, so that every plain `cancho build` of a test goes through the compiler in cancho | **A difference closed.** 695 of 702 tests pass that way (667 of 703 before this stage); the 7 left are differences, not gaps: three cranelift-only tests, the wasm target, the object bytes of a tuple against a struct, the stride of a struct of bytes (3 bytes packed here, 24 there, so an arena of them holds more before it traps), and the one fixture whose slice has slices for elements |
| 5m. The corpus of refused programs, measured against the Rust front end (`tests/reject`, 333): the compiler in cancho refused **180** with the Rust rule and **built 127** that the language refuses (27 linear values not consumed, 27 effects not declared, 18 capabilities not narrowable, 14 uses after a move, 9 linear values taken apart, and a handful of others; #512). The effect row is now checked while code is written, in the direction that cannot be wrong (the body performs what the row does not declare; the other direction waits until every builtin records its effect: 24 valid programs were refused by it), and a builtin records the labels it performs whichever way its code is written (the sockets, the clock, the directories, the files had a path of their own that did not), which moves 23 programs to the right rule: **203 of 333** agree. `refusals_agree_with_the_rust_front_end` holds those 203 to the Rust rule and the 130 in `reject_gaps.txt` to still differing, so the list only shrinks. Also: the first builds with `--compiler auto` no longer collide on the compiler's cache (eight started together failed 5 to 7 times in 8) | `body.cho`, `effects.cho`, `selfhost_build.rs` | `cancho check --output json` | **A difference measured, and the first part closed.** `tests/accept`: 115 of 116 build as before (the one `SKIP` is as it was) |
| 5n. Linearity while code is written (`docs/linearity-and-effects.md` §4, `linear.rs`): each binding is 0, holds a `res` value, or has had it consumed (a fifth int in the log of bindings); a read of a `res` name moves it, a block that closes or a `return` with a value still held is `linear-value-unconsumed`, a second read or a `borrow` of a consumed one is `linear-use-after-move`, an assignment over a held one, a `let` that shadows a held one, an expression statement and a `_` (arm or payload) that drop one are `linear-value-unconsumed`; branches save the states and the arms that go on must agree, a loop must leave them as it found them. A `res` value is a capability, a handle, a box, a thread, a prelude enum, a tuple or a struct or enum with such a member. Also `linear-value-taken-apart` for a field or component read out of a `res` value, `*` of a reference to one, and a pattern on a handle or a box. Compile mode only: the Rust oracle of the checker alone does not run the pass | `scope.cho`, `body.cho`, `exprs.cho`, `rules.cho` | `cancho check --output json` | **A difference closed, in part.** `tests/reject`: **256 of 333** get the Rust rule (203 before); the accept corpus is unchanged and the whole suite through the compiler is 697 passed with the 7 known differences. Left in this family: the generic ones that need the mode bounds of a type parameter (`val_generic_*`, `mode-bound-violated`) |
| 5g. What is not done: UDP, ttys, signals, function values as parameters, a struct as a thread payload in the code (5 of the 115 programs of `tests/accept` still stop: a function-typed parameter, a slice of references, a generic literal typed only by a later statement, `tty_open`, `signals_watch`); a static that calls a function of the program is still run at start; a compiler that checks as fully as the Rust one (about a fifth of the functions the fuzz cases and the library hold are still `SKIP` in the checker) | not started | | |
| 5p. `region-chunk-exceeded` (#514, `docs/region-limit.md` §2): a constant or folded count of `alloc_slice` in a region whose bytes do not fit the 65,536-byte chunk is refused while code is written, with the stride the Rust lowering uses (`byte` 1, `f32` 4, other scalars 8, a tuple of scalars 8 each; any other type is left to the trap at run time). Found by `selfhost::parity` going red on `main` when the rule landed in Rust (#518) without the compiler in cancho knowing it | `exprs.cho`, `rules.cho` | `cancho check --output json` | **A difference closed.** The three `tests/reject` fixtures get the Rust rule and leave `reject_gaps.txt`; `tests/accept/region_slice_at_chunk.cho` (the exact limit) builds and runs as the Rust build does |
| 5q. `narrow` (`lower/mod.rs`, `lower/narrow.rs`, `docs/narrowing-into-several.md`): a literal narrowing a path capability (`Fs`, `Exec`, `Tty`) must be inside what it holds, at a separator, and not equal; `Net` by plain prefix; a set of libraries (`Ffi`) must be among those held and not the same set; several literals only for `Fs` and `Exec`, unrelated paths, none equal or nested; a borrowed capability, a non-literal, a capability with nothing to narrow are refused (`narrowing.cho`, from the literal each capability type already carries). A literal with an escape is left alone. Also: every trap is the one instruction the Rust backend writes (`ud2`, `udf #0xc11f`), once in the module as `@cancho_trap`, so a program stops with SIGILL whichever compiler built it (`llvm.trap` is SIGTRAP on arm64); the arena's trap says why on standard error, as the Rust one (#514); an explicit `--backend cranelift` is not sent to `CANCHO_COMPILER`; the arena keeps how much is used beside its chunk and not in it, so all 65,536 bytes are the region's (an allocation of exactly the chunk wrote 16 bytes past it and made glibc abort in the next `malloc` on Linux, found by the `selfhost` CI job) | `narrowing.cho`, `exprs.cho`, `driver.cho`, `emit.cho`, `runtime.ll`, `runtime/net.c` | `cancho check --output json` | **A difference closed.** `tests/reject`: **278 of 336** get the Rust rule (259 before); every `capability-not-narrowable` program of the corpus; three of the known differences close (`identity`, `crypto_builtins`, `sha256_builtins` build with Cranelift natively); `tests/accept` unchanged |
| 5r. The rest of the refusals that had a rule apiece (#512): `program-shape` (a `main` that does not take the one `World` and return `int`, or no `main`, said after every function is checked); `capability-misused` (a struct literal or a pattern of a capability or a handle, `release` of what is not one, a path or program reached through a capability that is not the borrowed `Fs` or `Exec`, a heap operation or `fork_heap` without the uniquely borrowed `Heap`); `listen-host` (the address of `tcp_listen_on` a literal dotted quad that the `Net` bound covers, `tcp_listen` and `udp_bind` only under a bare port); `signal-not-claimable`, `foreign-scope`, `unsized-type` (`unbox` of a boxed slice), `thread-payload-type`, `no-function-values`, `not-a-tuple`, `not-a-place`, `pattern-shape`, tuple and pattern arities; `mode-bound-violated` for a `res` value in an arena, a boxed slice or a `T: val` argument; `type-mismatch` for `unbox` of a reference to a box; an argument label (a path, a program, a library) performed by a function whose row is empty and which owns no capability is `effect-not-declared`, and a row that says more than the body does is refused again, with `conc`, `heap` and `clock` recorded and a function that performs what the masks do not keep (a label with an argument, a function value, a foreign call) left alone. New modules `allocs.cho` (the ends of `alloc_slice`, `box`, `unbox`, `unbox_slice`, `box_slice`) and, from `narrowing.cho`, the checks for listeners; `exprs.cho` is kept under the 2,000-line budget by moving the part of each that follows the evaluation of its operands | `body.cho`, `exprs.cho`, `atoms.cho`, `allocs.cho`, `narrowing.cho`, `effects.cho`, `driver.cho`, `rules.cho` | `cancho check --output json`, and the rule each fixture declares | **A difference closed, almost.** `tests/reject`: **322 of 336** get the rule (278 before; the three fixtures that are a property of a target are not compared). The 14 left need a port of the thread leases of `linear.rs`, the check of a generic definition with its parameters unknown, a row in the type of a function value, and what an owned capability discharges by path |
| 5s. The leases of `linear.rs` (`docs/aliasing.md` §6.1, `docs/threads.md` §3): a `&!` given to a thread is lent to it until the handle is joined; a `spawn` of a reference that is lent already, a read of a copy of a lent reference, branches that disagree about whether a handle was joined, and a loop that leaves a lease behind are `borrow-conflict`. A reference is a copy of the one it was bound from (a sixth int in the log of bindings); five leases are kept in the header after the log, and branches and loops save and compare them as they do the states; only the two shapes `linear.rs` follows end a lease (a handle named by a binding, a `spawn` joined where it is made). `Thread[T, R]` is a type that can be written in a signature, carries its payload so that a handle cannot outlive the `borrow` block the payload points into (`reference-escapes-region`), and unifies by its result; `join` no longer records `conc` (only `spawn` does). A `static` the interpreter is sure traps (an index out of range, a division by zero) is `constant-traps` | `scope.cho`, `types.cho`, `exprs.cho`, `body.cho`, `consteval.cho`, `tables.cho` (the `Thread` index) | the rule each fixture declares | **A difference closed.** `tests/reject`: **329 of 336** get the rule (322 before). Left, each a feature of its own: the check of a generic definition with its parameters unknown (`arithmetic_on_a_type_parameter`, `generic_body_checked_when_unused`, `generic_array_over_a_resource`), a row in the type of a function value (`function_value_row_undeclared`), what an owned capability discharges by path (`fs_owned_prefix_covers_sibling`, `exec_owned_prefix_covers_sibling`), and the region of a variable settled by inference (`reference_escapes_via_inference`) |
| 5t. Refusals as sentences (#479): the compiler writes the sentence of the rule it refuses by (`Rule::explanation`, generated into `tables.cho`) on a line of its own after `ERR <rule> <from> <to>`, and `cancho build` renders where (`file:line:column`), why, the rule, the line and what of it, without asking the Rust front end; the CLI no longer calls the Rust front end to explain or to say whether the two agree. The sentence is the rule's, the same for every program it refuses; the names and types of the Rust compiler's messages (125 shapes) are not ported | `tables.cho` (generated), `rules.cho`, `driver.cho`, `selfhost_build.rs` | the sentence of `Rule::explanation` | **A stage.** What `docs/agent-errors.md` calls the shape in JSON waits for `check` in cancho (#485) |
| 5u. The default (#478): the compiler in cancho writes the code of `build`, `run` and `test` on macOS and Linux unless the Rust compiler is asked for (`CANCHO_COMPILER=rust`), the build is Cranelift or WebAssembly, or the target is not a host it writes code for. Its sources are compiled into the `cancho` binary (`build.rs` lists each file) and built once into the cache, so nothing has to be next to the binary; `CANCHO_SELFHOST_SOURCES` builds from a directory. While `reject_gaps.txt` lists a program, the Rust front end checks first (`RUST_CHECKS_FIRST`, and a test that says it goes when the list is empty); a build the compiler has no code for, or that breaks, is done by the Rust compiler (an arena of aggregates with a `byte` or `f32` leaf is such a build: the two compilers do not lay them out alike), and named alone (`--compiler`, `CANCHO_COMPILER=auto`) the compiler in cancho says what it cannot do. `write_bytes` is one `fwrite`, as the Rust backend's, so a write that fails is reported by `flush_out` as it is there. A bug found on the way: the binding log grew to six ints and the header of the linearity state fell, in a program of 330,000 tokens, into the module text | `selfhost_build.rs`, `main.rs`, `project.rs`, `build.rs`, `driver.cho`, `allocs.cho`, `ast.cho` | the conformance suite under the default, and with `CANCHO_COMPILER=auto` in CI | **The first version.** 710 of 710 conformance tests pass under the default on macOS |
| 5v. The process builtins of edition 9 (`docs/processes.md` §4.11, #524/#525): `exec_spawn_detached` is `exec_spawn` whose child leads a session of its own (a 19th argument of `cancho_spawn`, and `POSIX_SPAWN_SETSID` in the flags `cancho_spawn_setup` sets: `0x80` on Linux, `0x400` on Darwin) and performs `exec_detached` as well as `exec(p)`; `child_pid(&child)` reads the pid out of the word; `child_release(child)` closes the `pidfd` and starts a detached thread of 64 KiB of stack whose one call is `waitpid` on that pid (`runtime/process.ll`, included only by a program that releases a child). All three are edition 9, as in Rust. A row that carries an argument label is no longer a reason not to compare the plain labels: `Label::covers` never lets a label with an argument cover a plain one, so a plain label performed and not written in the row is `effect-not-declared` whatever else the row has | `filecode.cho`, `exprs.cho`, `codes.cho`, `effects.cho`, `driver.cho`, `runtime.ll`, `runtime/os_linux.ll`, `runtime/os_darwin.ll`, `runtime/process.ll` | the rule each fixture declares; the seven `process_release` conformance tests and `tests/accept/process_release.cho` through `CANCHO_COMPILER=auto` | **A difference closed.** `tests/reject`: **330 of 337** compared programs get the rule (327 before): `child_release_undeclared`, `child_released_then_used` and `exec_spawn_detached_undeclared` leave `reject_gaps.txt`; the three builtins leave `unported_builtins.txt` and six `process_release::` tests leave `.github/selfhost-known-differences.txt`. Measured on Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5w. A generic function is checked once with its type parameters unknown (rigid), as the Rust front end does (#512): the body runs through the checker with code writing off (`body.cho` `check_function`), so a name that only a type parameter can't support (`arithmetic_on_a_type_parameter`), a body that is wrong whether or not anything calls it (`generic_body_checked_when_unused`) and an array of a resource type (`generic_array_over_a_resource`) are refused with the Rust rule. Linearity is tracked only when code is written, so a generic function with an unbounded parameter that passes this check is `SKIP`, left to its instances | `types.cho`, `scope.cho`, `body.cho`, `exprs.cho`, `allocs.cho` | the rule each fixture declares; `the_body_checker_in_cancho_agrees_with_the_rust_one_per_function` | **A difference closed.** `tests/reject`: **333 of 337** compared programs get the rule (330 before); three fixtures leave `reject_gaps.txt`, four remain. Measured on Linux x86-64 only (conformance `selfhost` filter: checker, body checker and refusals tests); not run on macOS or Linux aarch64 before CI |
| 5x. `child_kill_group` (`docs/processes.md` §4.12, #533): `child_kill`'s signals to the process group the child leads. Builtin 138 in `codes.cho` (spelled exactly, so `child_kill` is untouched), arity 2 and `cancho_child_kill_group` in `netcode.cho`, and no support flag in `exprs.cho` since the function is in the runtime every module has; `runtime.ll` has it beside `cancho_child_kill`, the pid negated and a pid of 1 or less `EINVAL` with no call. Edition 9, row `child_signal`, in the regenerated tables | `codes.cho`, `netcode.cho`, `exprs.cho`, `runtime.ll`, `runtime.cho`, `tables.cho`, `builtins.cho`, `signatures.cho` | the five `process_group` conformance tests, which build the driver with the compiler in cancho on the `llvm` backend and with the Rust one on Cranelift, and `tests/accept/process_group_kill.cho` through `CANCHO_COMPILER=auto` | **A port with no gap.** Nothing is added to `unported_builtins.txt`, `reject_gaps.txt` or `selfhost-known-differences.txt`; the two fixtures that reach the Rust front end's rules (`child_kill_group_is_edition_nine`, `child_kill_group_undeclared`) get the same rule from both. Mutating `runtime.ll` (the pid not negated) fails three of the five tests. `CANCHO_COMPILER=auto cargo test -p cancho --test conformance`: 724 passed, 7 failed, all seven already in `selfhost-known-differences.txt`. Measured on Linux x86-64 only |
| 5y. What an owned capability discharges by path (`docs/filesystem.md` §1.1, `docs/processes.md` §4.1, #512): a call of a function whose row names `fs_read("/tmpevil")`, `fs_write(...)` or `exec(...)`, made by a function that owns `Fs("/tmp")` or `Exec("/bin")` and declares no label with an argument, is `effect-not-declared` unless an owned capability of that family holds the path at a separator (`Label::covers`, `extends_path`); owning a `World` covers every path. Only the rows of functions are read: what a builtin performs through a borrowed capability is not judged here. The state keeps the function being checked (`st[64]`, so the table of tokens starts at 65) | `effects.cho`, `narrowing.cho`, `ast.cho`, `body.cho`, `exprs.cho` | the rule each fixture declares; `the_body_checker_in_cancho_agrees_with_the_rust_one_per_function` for the programs that must still be accepted | **A difference closed.** `tests/reject`: **337 of 339** compared programs get the rule (335 of 337 before the two `child_kill_group` fixtures of 5x joined and this change); `fs_owned_prefix_covers_sibling` and `exec_owned_prefix_covers_sibling` leave `reject_gaps.txt`, two remain. Measured on Linux x86-64 only (all 17 `selfhost::` conformance tests, in three runs: the body checker alone in 298 s); not run on macOS or Linux aarch64 before CI |
| 5z. The row of a function type (`docs/function-values.md` §4.2, #512): calling a parameter written with a function type, `f: fn(&!i Io, &r [byte]) -> [io_write] int`, performs the row written in that type, and is refused as `effect-not-declared` when the caller's row does not declare it, as a named call is. Only a parameter of the function being checked is read (its binding's name token is the parameter's); a function value held any other way (a local, a field) stays untracked | `effects.cho` (`row_mask_at`, `perform_pointer`), `exprs.cho` | the rule each fixture declares; `the_body_checker_in_cancho_agrees_with_the_rust_one_per_function` | **A difference closed.** `tests/reject`: **338 of 339** compared programs get the rule (337 before); `function_value_row_undeclared` leaves `reject_gaps.txt`, one remains (`reference_escapes_via_inference`). Measured on Linux x86-64 only (refusals, parity and the body checker, 322 s); not run on macOS or Linux aarch64 before CI |
| 5aa. A reference into a block's region filled in by an annotation (`docs/linearity-and-effects.md` §5 rule 4, #512): `let used: Holder[&r Bytes] = hole;` inside `borrow b as &r in { .. }`, where `hole` was declared outside the block with a type whose argument is still unknown (`let hole = Holder::Empty;`), is `reference-escapes-region`: the annotation fills the argument with a reference into `r`, which a binding outside the block would hold. Only this shape is judged (an annotation that names a block's region in a generic argument, a plain name as the value, an argument of the type still a variable); any other annotation naming a block's region stays `SKIP`. With it `reject_gaps.txt` is empty and `RUST_CHECKS_FIRST` is false: the default refuses by the compiler in cancho's own checks, with the sentence of the rule (#479). Four conformance tests that hold the Rust front end to the words of its refusals (`memory::a_static_outgrows_what_an_arena_could_hold`, `modules::a_match_names_an_enum_through_its_qualifier`, `modules::the_module_rules_are_enforced_across_files`, `project::a_compiler_other_than_the_one_named_is_refused`) now build with `CANCHO_COMPILER=rust` for those assertions and leave `selfhost-known-differences.txt` | `scope.cho`, `body.cho`, `selfhost_build.rs`, the four tests | the rule the fixture declares; the default build test; the conformance suite under the default | **The last difference closed.** `tests/reject`: **339 of 339** compared programs get the rule (338 before). Measured on Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5ab. `exec_covers` and `fs_covers` (`docs/processes.md` §4.13, #535): whether a path is under the bound of a borrowed `Exec` or `Fs`, by the rule the operations trap on. Calls 14 and 15 of `capability_call` in `exprs.cho`; `runtime.ll` gets `cancho_path_covers` (the rule, `i1`) and `cancho_checked_path` calls it and traps on `false`, so the trap and the answer are one function. `atoms.push_bound` pushes the bound without marking an argument-carrying label performed (the row stays empty); `filecode.capability_which` (the dozen `is_word` ifs, moved to give `exprs.cho` room: it was 1,998 lines of 2,000) numbers the calls. Edition 9, no row, in the regenerated tables. **Found:** `cancho_checked_path` trapped on a NUL after the bound (`"/bin\0etc"` under `Exec("/bin")`), where both Rust code generators return `Spawned::Failed`; it now agrees with them | `exprs.cho`, `filecode.cho`, `atoms.cho`, `runtime.ll`, `runtime.cho`, `tables.cho`, `builtins.cho`, `signatures.cho` | the eight `path_covers` conformance tests, which build the driver by the compiler in cancho (`--compiler auto`), the Rust LLVM backend and Cranelift, and hold the answer against the real `exec_spawn`/`open_read`; `tests/accept/path_covers_asks_first.cho` and the six reject fixtures through `CANCHO_COMPILER=auto`. Mutating `runtime.ll` (no separator boundary; `..` not refused) fails three and five of the eight | **A port with no gap.** Nothing is added to `unported_builtins.txt`, `reject_gaps.txt` or `selfhost-known-differences.txt`. Measured on Linux x86-64 only |
| 5ac. `exec_spawn_under` (`docs/processes.md` §4.14, #534): `exec_spawn_in` whose program is a name in the `Dir`, under `Exec("")`, performing the path-free `exec_under`. Call 16 of `capability_call` in `exprs.cho`; `atoms.push_under` (refuses a non-empty literal with `capability-misused`, performs the label, pushes the bound); `runtime.ll`'s `cancho_spawn` reads bit 2 of its existing `%detached` argument as "under" and traps on an empty or rooted name before the shared `cancho_checked_path`, bit 1 staying the session flag, so `filecode.spawn_process` is unchanged. The label reaches the self-hosted tables through `Builtin::effects` (the generator reads label names off the builtins' rows). Edition 9, in the regenerated tables | `exprs.cho`, `atoms.cho`, `filecode.cho`, `runtime.ll`, `runtime.cho`, `tables.cho`, `builtins.cho`, `signatures.cho` | the seven `spawn_under` conformance tests (each built with the compiler in cancho on the `llvm` backend, the Rust LLVM backend and Cranelift) and `tests/accept/exec_spawn_under.cho` through `CANCHO_COMPILER=auto` | **A port with no gap.** Nothing is added to `unported_builtins.txt`, `unported_rules.txt`, `reject_gaps.txt` or the known differences. Measured on Linux x86-64 only |
| 5ad. The canonical printer (`cancho print`, #485, `docs/formatting.md`): `printing.cho` writes the tree `ast.cho` builds back as the one text the language has for it, a port of `print.rs` (parentheses by binding power, `else if` chains, a struct literal in a condition, the `[T, &r where ..]` lists and the effect rows and patterns that the tree keeps only as tokens, grouped by module), and `floatprint.cho` writes a floating-point literal as `Display` does: the shortest digits that read back to the same bits, by Burger and Dybvig's free-format algorithm on 40-limb bignums in the scratch region of the state, no exponent, `.0` after a whole number, the `f32` suffix. Driver mode 4, root `printer.cho`; a refusal is the line `ERR rule from to` (no sentence) | `printing.cho`, `floatprint.cho`, `printer.cho`, `driver.cho` (mode 4, `refusal_line`), `floats.cho` (`room` is public), `dump_ast.rs` (`printed`, the oracle) | `selfhost::the_printer_in_cancho_agrees_with_the_rust_printer`: every program of the repository (452 files that parse and the refusals), 40 programs for precedence, `else if`, patterns, brackets, escapes and one-part tuples, and 600 random `float` and `f32` literals; two mutants (a unary operator that binds like `*`; no `else if`) are killed | **446 of 446** parsing files of `std`, `examples`, `packages`, `tests/accept` and `tests/programs` print the same text as `cancho print`; 3,439 further literals (random bit patterns, powers of ten, the edges of `float` and `f32`) printed the same in a one-off run. Not done: nothing of the three (`fmt` is row 5ae, `print --output json` row 5af, `ingest` row 5ag); the Rust CLI does not call it yet. A literal of more than 120 significant digits (`floats.cho` holds 120) is written as it was spelled, which is the printer's text only when the spelling is already the shortest one, as it is for the long literals of the repository (`1` and 300 zeros); another long spelling would differ. Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5ae. `cancho fmt` (#485, `docs/formatting.md` section 6): `formatting.cho` is the port of the part of `format.rs` that needs no tree: given a file and the text the printer wrote for it, it aligns the two token streams (redundant parentheses and trailing commas, `else { if .. }`, `pub extern`, the literals respelled as the source had them, every `import` before the first declaration, the `edition N;` line), puts each comment at the end of the line of the token it followed or above the line of the token it preceded, keeps one blank line where the source had one, and writes the result; a file it cannot format is the line `CANNOT <sentence>` with `format.rs`'s sentence. Driver mode 6 (the input is two files, source then printed text), root `formatter.cho` | `formatting.cho`, `formatter.cho`, `driver.cho` (`format_run`), `dump_ast.rs` (`formatted`, the oracle) | `selfhost::the_formatter_in_cancho_agrees_with_the_rust_formatter`: every program of the repository, 25 edge programs and the programs with their layout spoilt (2,474 in all: 1,550 changed, 698 already formatted, 15 refused); four mutants (no blank line kept, no trailing comment, no respelling of literals, no indent before a `}`) are killed | **2,474 of 2,474** programs format as `cancho fmt` does. Not done: the three checks of `docs/formatting.md` section 4 (it parses and prints as the original; the same comments in the same order; a fixed point), which need the printer run on the result and belong to the command line in cancho; a Unicode white space at the end of a comment is not trimmed as the Rust `trim_end` does. Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5af. `cancho print --output json` (#485, `docs/structured-ingest.md` section 6): `jsonprint.cho` is the port of `ast_to_json` and of the writer of `json.rs`: the tree as data, every name as text, two spaces to a level, one element to a line, `[]` for an empty array, the bits of a `float` as an unsigned decimal and those of an `f32` from the same value's `float` bits. Driver mode 7, root `jsonprinter.cho`; a refusal of the parser is the line `ERR rule from to` | `jsonprint.cho`, `jsonprinter.cho`, `driver.cho` (mode 7), `dump_ast.rs` (`printed_json`, the oracle) | `selfhost::the_json_printer_in_cancho_agrees_with_the_rust_one`: every program of the repository, 62 edge programs and 1,200 random and subnormal float literals (904 programs); five mutants (the carry of the unsigned decimal, the subnormal shift of an `f32`, a bound, an outlives clause, a control-character escape) are killed | **904 of 904** programs print the same JSON as `cancho print --output json`. A floating-point literal of more than 120 significant digits is the line `CANNOT ...` (the repository has none). Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5ag. `cancho ingest` (#485, `docs/structured-ingest.md` section 7): `jsonread.cho` is `read_json` (every position and span of `json.rs`, the depth limit and the second one `json_to_ast` makes), and `ingest.cho` is the builder of `json_to_ast` in the order `ingest.rs` checks things: it makes the nodes the parser would have made and a synthetic text with the tokens they point at (names, string literals, effect rows, `[T, &r where ..]`, patterns), so `printing.print` writes the result and is the only printer there is; a float arrives as bits and `floatprint.from_bits` writes its digits (with `NaN` and the infinities). The rules `ingest-json`, `ingest-node` and `ingest-arity` are in `rules.cho`. Driver mode 8, root `ingester.cho`; a refusal is the line `ERR rule from to` with a span in the JSON | `jsonread.cho`, `ingest.cho`, `ingester.cho`, `driver.cho` (mode 8, `err_line`), `floatprint.cho` (`from_bits`), `printing.cho` (a float node that has its bits), `lexcore.cho` (`keyword` is public), `rules.cho`, `dump_ast.rs` (`ingested`, the oracle) | `selfhost::the_ingester_in_cancho_agrees_with_the_rust_one`: 8,000 inputs (the JSON of the repository's programs, the same damaged in twenty ways, 110 written to be wrong in each way the three rules list, 10 of nesting at the limit); eight mutants are killed | **8,000 of 8,000** inputs give the text or the refusal (rule and span) that `cancho ingest` gives. Not done: the sentence of a refusal (a line `ERR rule from to` only). Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5ah. `cancho ids` (#485, `docs/canonical-ast.md`): `blake3.cho` is BLAKE3 in the streaming form (the tree of chunks as a stack of chaining values, bytes given one at a time, so no encoding is kept), and `identity.cho` is `cancho-id`'s `identify`: the same encoder rules in the same order (tags, little-endian integers, text as length and bytes, a nested hash as its 32 bytes; locals, type parameters and regions by position; a name by the hash of the declaration it reaches, resolved through the imports of its module; effect rows sorted and without repeats; destructuring fields sorted; the types hashed twice, the first time with no name known), printing `type`, `sig` and `body` lines. Driver mode 9 (a stream of files), root `ids.cho`; the program is not checked first | `blake3.cho`, `blake3sum.cho`, `identity.cho`, `ids.cho`, `driver.cho` (mode 9), `floats.cho` (`single_bits`, `all_literals_ok`), `dump_ids.rs` (the oracle) | `selfhost::blake3_in_cancho_agrees_with_the_blake3_crate`: messages of every length from 0 to 130, around every size at which the tree changes shape (a block, a chunk, each power of two chunks) and of 200,000 and 1,048,576 bytes; `selfhost::the_identities_in_cancho_agree_with_the_rust_ones`: every program of the repository alone, the edge programs, the programs of several files, and a sample with the standard library after them (1,035 in all); twelve mutants (a binary tag, the repeats of an effect row, the sort of a destructuring, the shadowing of a binder, the second pass over the types, an escape, the order of a `where` clause, a bound, an extern symbol, a unary tag, the binder a `borrow` makes, a name declared twice) are killed | **1,035 of 1,035** programs give the lines `cancho ids` gives, hash for hash. BLAKE3 was first wrong for every message of 5 bytes or more (one entry of the message permutation, written from memory): the test against the crate found it. Not done: `cancho ids` checks the program first and refuses it if it is wrong; this one only parses (the command line will do both); a floating-point literal of more than 120 significant digits is the line `CANNOT ...`. Linux x86-64 only; not run on macOS or Linux aarch64 before CI |
| 5ai. `exec_spawn_detached_under` (`docs/processes.md` §4.15, #562): `exec_spawn_under` whose child leads a session and a process group of its own. Call 17 of `capability_call` in `exprs.cho`; `filecode.in_dir`, `under` and `spawn_bits` (new; the three `which == 10 \|\| which == 16` tests became one call, `exprs.cho` is 1,994 of 2,000 lines) number it, and `runtime.ll` is unchanged: `cancho_spawn`'s `%detached` word already carries bit 1 (a session) and bit 2 (a name beneath the directory) independently, so 17 is 3. `exec_detached` is performed at the call as for 13. Edition 9, in the regenerated tables (170 builtins) | `exprs.cho`, `filecode.cho`, `tables.cho`, `builtins.cho`, `signatures.cho` | the four `spawn_detached_under` conformance tests (each built with the compiler in cancho on the `llvm` backend, the Rust LLVM backend and Cranelift) and `tests/accept/exec_spawn_detached_under.cho` through `CANCHO_COMPILER=auto`. Mutating `spawn_bits(17)` to 2 fails the deadline test on the compiler in cancho only | **A port with no gap.** Nothing is added to `unported_builtins.txt`, `reject_gaps.txt` or the known differences. Measured on Linux x86-64 only |

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
which parameters are `val`-bounded) and the 137 builtins' names (118 when this was written) and editions are data the port
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

**Stage 4d: output.** `split(world)` answers a prelude struct (which one depends on the edition) and the
destructuring `let Split { io, ffi, fs, heap, args } = split(world);` binds each part with its own
type, taken from the generated fields; none of them has a code, as in the Rust backend. A reference is
a `ptr`: `borrow mut io as &!r in { .. }` stores in the reference's slot a pointer to a byte of its own
(a `Io` has no representation to point at), and a borrow of a whole number or a boolean stores the
address of its stack slot, so a function can be given `&!i Io` and a call passes the pointer.
`putchar` is a `trunc` to `i32`, a call to the C function and a `sext` back, as the Rust backend
writes it. What it made visible was the row of `main`: a function that owns a capability needs no label
for what the capability allows (section 8.2 of `docs/linearity-and-effects.md`), so the effect check
subtracts what the parameters discharge, taken from the same generated table that `discharged_by`
fills, before comparing. The test now compares what a program writes as well as how it ends, and
the first thing a cancho-written compiler printed was `Hi`.

**Stage 4e: strings and slices.** A reference to a slice is two values, a pointer and a length: a slot
for each (`%s7_0`, `%s7_1`), two parameters in the header, two arguments in a call, and `len` is a load
of the second. A string literal is a private constant `@str.N` written to a fourth buffer (the six
escapes the lexer knows are decoded as the constant is written) and the driver prints the constants
before the functions. `s[i]` is a comparison of the index against the length, unsigned so a negative
index fails the same test, a trap, then a `getelementptr`; a store through it or through `*r` is the
same address and a `store`. `write_bytes` is a loop over `putchar` in a helper the driver writes once,
so the two stay in order in the C library's buffer. `int_of` is a `zext`; `byte_of` traps outside 0 to
255, which the first run showed: the first version truncated, and the Rust build trapped.
Reassigning a slice variable (`s = "yyyy"`) stored one pointer into a slot that does not exist, which
clang found.

**Stage 4f: structs.** A plain struct is an LLVM aggregate of its fields, `{ i64, i1 }`, which is one value:
the slot, the parameter, the argument and the return are each a single operand, as they are for a whole
number, and the checker's existing walk needed only the pieces that look inside. A literal is a chain of
`insertvalue` from `undef`, in declaration order (the order Rust already enforces); a field read of a
value is an `extractvalue`; behind a reference it is a `getelementptr` and a load, and `r.x = v` is the
same address and a store. A reference to a struct binding is the address of its slot, as for a whole
number. Only structs of whole numbers, bytes and booleans have code; a struct with a float or another
struct in it is still skipped, so the answer is `SKIP` and not wrong code.

**Stage 4g: enums and `match`.** An enum is an aggregate of its tag and, one after the other, the payloads of
every variant (`{ i64, i64, i1 }` for `A(int)` and `B(bool)`); only the tag and the payloads of the
variant it names mean anything, and the tag is the variant's position. A value is built with
`insertvalue` as a struct is; a `match` extracts the tag once (an `extractvalue`, or an address and a
load behind a reference) and is a flat chain of tests: arm `n` is `ma<n>` when the tag is its variant and
`mb<n>`, the next test, when it is not; the last arm has no test, since the arms cover everything
(Rust has already said so), and each ends in a branch to the end of the match. A payload is bound by
`extractvalue`, or, behind a reference, as the address of its element, so `*w = ..` writes the
enum in place. Enums with a float or a non-scalar payload are still skipped.

**Stage 4h: arenas.** `alloc_slice` was not in the checker at all (it answered `SKIP`), so this stage is
a rule first: the arena named must be open here, and an `alloc_slice[static]` in a function is an
escape (only a `static` item may add to the static region); then the count, an `int`; then the fill,
which must copy (a `res` one is `mode-bound-violated`); the result is a unique reference to a slice in
the arena's block. A `return` of anything pointing into a block is now an escape checked before the
types are compared, as Rust does it: a function returning `s` from its arena as an `int` said
`type-mismatch`, and Rust says `reference-escapes-region`; the new cases found that. In the code,
a `region` is a cell holding the head of a list of its allocations, each a `malloc` with a 16-byte
header linking to the one before, and the end of the block frees them all; both are two helper
functions the driver writes (`@cancho_arena_alloc`, `@cancho_arena_free`). A negative count traps, and
so does a size that overflows, as in the Rust backend, and the fill is a loop. What differs from Rust
is the allocator (it bumps within a 64 KiB chunk) and one thing that is a gap: a `return` from inside
a `region` leaves without freeing, so the memory lasts until the program ends.

**Stage 4i: the heap.** `Box[[T]]` is a pointer and a length, which in the code is the aggregate
`{ ptr, i64 }`: one value, like a struct, so it is stored, moved and passed as one operand, and a
reference to a box is the address of its slot, so `borrow mut b as &!q in { contents(q) .. }` is the
borrow of a whole number again. `box_slice` is the arena's size and fill with a `malloc` in place of
the arena (a null answer traps, as in the Rust backend); `contents(q)` loads the pointer and the length
back into a slice in the block of the borrow; `unbox_slice` is a `free` and the length. They are
written for compile mode only: the checker has no `Box` type and no ownership of one, so in the other
modes a function that holds a box is still `SKIP`, as it was, and the 1,945 cases are unchanged. That
is the honest limit of this stage: the code for the heap is compared against Rust by what programs do,
and the rules for it (`unbox_slice` consumes, `contents` on an owned box) are Rust's alone until the
checker learns to follow ownership.

**Stage 4j: `static` tables.** The compiler's own generated tables are `static` items, so the compiler could
not compile itself without them. In the checker the name of a static of this module declared `[T]` for a
scalar `T` is a shared reference to a slice in the `static` region, as `docs/compile-time-data.md`
says; the oracle checks only functions, so what had to agree was the uses (20 new cases, none
different, and 29 more of the library's functions are verified because they read a table). In the code a
static is a global pair of a pointer and a length, `@sd<n>`, and a function `@sinit<n>` that runs its
body and stores the pair there; `alloc_slice[static]`, legal only inside such a body, is the heap's
`malloc` and fill; and the C `main` the driver writes calls every `@sinit` before `main`, in the order the
items come. Rust evaluates a static while compiling and puts it in read-only data; this builds it when the
program starts, so a static that reads another is not supported, and the table is writable memory. Neither
is visible to a program, but the first is a difference: a static whose body traps is a compile error in
Rust and a trap at startup here.

**The census (mode 4, `census.cho`).** To measure how far the compiler in cancho is from compiling a program,
the bodies mode runs with the code on and says, for each function, `OK` (the checker accepts it and code
is written) or `SKIP <from> <to>` with the first construct it has no code for. Fed the compiler's own source and
the `std` modules it imports (`bytes`, `buffer`, `io`, `math`; 644 functions), on 2026-10-12: **488 of
the compiler's own 544 functions (90%)** get code, 20 of the 39 in `std.bytes`/`buffer`/`io`, and 6 of the 61
in `std.math`, which is floating point and which the compiler does not use. `OK` here means code is written,
not that it has been run: the programs run so far are the 382 of the differential test. What the other 56
of the compiler's functions stop at: a function that returns a slice or calls one (`tables.*`,
`builtins.effect_name`, `rules.rule_tag`: about 20), a sub-slice `s[a..b]` (about 12), a struct with a field
that is not a whole number, byte or boolean (`block`'s result, `Buffer`: about 14), a few builtins
(`wrapping_mul`) and floats (1). Not measured: calls across modules, which the test programs do not make, and
a binary that has been built from all of it.

**Stage 4k: slices that come back.** A function that returns a reference to a slice answers the pair, the
LLVM aggregate `{ ptr, i64 }`, and a call to one takes the pair apart again into a pointer and a length;
`return s;` builds it. `s[a..b]` is Rust's two tests, as in the backend: an end past the length traps, then a
start past the end, both unsigned so that a negative number is a large one; then the address of the first
element and the difference. The census, rerun after it: 529 of the compiler's 548 functions (97%) and 24 of
the 39 in `std.bytes`/`buffer`/`io`. What is left of the compiler's own: `block` and `lc.step`, which return
structs with a field that is a list or a boolean pair (8), `wrapping_mul` (2), `Buffer` and what returns it
(`std.buffer`, 19 of its own): the struct that owns a `Box`.

**Stage 4l: structs that hold structs.** The census said what was left, and it was a few things. A field or a
payload now has a type that is a scalar, a struct or an enum of the file, or a `Box[[T]]` of scalars
(`types.node_type`), and the LLVM aggregate nests: `{ { i64, i1 }, i64 }`. A struct declared `res`
is one too, since what ends it is Rust's to say; `let S { a, b } = value;` takes one apart. Reading
a field through a reference follows `docs/reading-references.md` §2.0: a `val` one is copied out, a `res`
one (the `Box` of `Buffer`) comes back as a reference to it, which is what `contents(b.held)` needs. Two
smaller things were in the way of `std.buffer`: a byte could not be a parameter or a result, and five
builtins had no code (`wrapping_add`/`sub`/`mul` are the plain instructions, `copy_into` a bounds test and
`memmove`, `index_of_byte` a `memchr`, `getchar` the C function sign-extended, `write_err` a `write` to
descriptor 2). The census, rerun: **558 of 558** of the compiler's functions and 39 of 39 in
`std.bytes`/`buffer`/`io`. That is the end of what the census can see and the start of what it cannot: a
function that calls another module's, two modules that each have a function of one name, and a whole
program built into a binary.

**Stage 4m: several files, and the fixed point.** A function was named in the code by its identifier, which two
modules may share (`ast.get` and `lexcore.get` would both be `@lexs_get`); it is now named by the number of its
node in the tree, `@lexs_<n>`, which nothing else shares, and a call names the node it resolved to. A
function the code is not written for is no longer a refusal of the program: it is left out, with its body
rolled back from the output and a comment at the end of the module (`; left out: 55`) saying how many, and a
call to it, or the program's `main`, being left out, is a refusal (`SKIP`, at the construct the compiler stopped at); a function nothing names is left out in silence. A static that is left out gets a stub.
`compiler.cho` is the compiler for a program of several files (`drive(world, 3, true)`), and the first try
was to give it its own source and the `std` modules it imports, in the stream `driver.cho` reads:

* the compiler built by Rust (the *first*) wrote 100,409 lines of IR, 637 functions, for it;
* `clang -O1` built that without a word, 343 KB: the *second*, which no Rust compiler took part in;
* the second, given the same stream, wrote the same IR, byte for byte (`shasum` equal); `clang` builds that
  too, and the third writes it again. `bootstrap.rs` is that test (5.5 seconds);
* the checker (`bodies.cho`) built by the second compiler agrees with the Rust oracle on 1,980 of 1,980
  cases, as the one Rust built does.

What this is and is not. It is a compiler, written in the language, that is a fixed point of itself on its
own source. It is not yet the end of `epic #295`: `clang` is still run by hand (or by the test), the first
compiler is still built by Rust, the 55 functions left out are `std.math`'s (floating point), a program that
needs generics, tuples, floats, sockets or threads in the code is not one this compiler can build, a `static`
is evaluated when the program starts and not while compiling, and the checker part is a port of what the
Rust checker does for about four functions in five (the rest is `SKIP`, which in compile mode means the
program is trusted to have been checked). The compiler trusts a program that Rust has checked; it
does not yet replace Rust as the thing that says a program is wrong.

**Stage 4n: the compiler runs `clang`.** The compiler wrote a module and a person ran `clang` on it. The builder
(`builder.cho`, edition 7) does both: it reads a program on standard input, compiles it, writes the module to
`<output>.ll` through an `Fs`, and starts `clang` on it through an `Exec`, waits for it, and answers its status. It
is made of what the language has for the purpose, and the stage is mostly the front end that makes that
possible in the code:

* a prelude type that is a **handle** (`File`, `Child`...) is one value of the code, a descriptor or a pid; a prelude
  **enum** (`Opened`, `Read`, `Spawned`, `Exited`, `Stdio`) is the same aggregate as an enum of the file, built and
  matched alike; a capability **narrowed to a literal** (`Fs("/tmp")`, `narrow(exec, "/usr/bin")`) is its type with the
  token of the literal kept, which the code gives the runtime to check a path with. The generated tables gained the
  enums' arms and how many leaves a handle is; the rows of functions are Rust's to check in this mode;
* `runtime.ll` is the support the code calls, written by hand and printed by the compiler into every module
  (`runtime.cho` is its text, checked against the file by a test): a path copied and checked (no `..`, the prefix,
  and `/tmpevil` is not under `/tmp`), `fopen` and `dup` for a file (the close-on-exec descriptor the Rust backend
  makes), `read` and `write`, `posix_spawn` with the three streams, `waitpid`. An error is its negative
  `errno`, and the code builds the enum from the number;
* the module differs by **target** only in where `errno` lives (`__error`, `__errno_location`), so a line
  `TARGET darwin` or `TARGET linux` in front of the input says which. The Rust backend knows its triple; the
  compiler is told. The harness gives every program the line of the machine it runs on.

What this does not copy from the Rust backend: `posix_spawn` is given the three streams and nothing else (no
`closefrom`, no signal mask: the child inherits what the parent has open above descriptor 2 that is not
close-on-exec, and its signal dispositions), a child is its pid and not a pid with a pidfd, and `clang`'s
own output is discarded (`Stdio` has no way to inherit), so a build that fails says its status and nothing more.

Along the way: an emit buffer was never bounds-checked, and the runtime alone is bigger than the buffers of a
program of a few tokens, which silently wrote the end of a function into the middle of the runtime; a buffer is
now given a floor, and a module that outgrew its buffers is a refusal (`emit.overflowed`). The signature table moved
to its own generated module (`signatures.cho`) because `builtins.cho` passed the repository's 2,000-line budget.

**The bootstrap, again, with no hand.** `bootstrap.rs` has a second test: the builder built by Rust builds the builder's
own source (and runs `clang` itself) into the second builder; the second does it again into the third; the three
write the same IR. The `tests/accept` fixtures are a scoreboard for what is left: 49 of 115 build with the compiler
built by Rust and give the output and the exit status their headers say, none gives a different one, and the
other 66 stop at a function the code is not written for: generics (`Option[T]`), tuples (`narrow(fs, "a", "b")`,
`(int, bool)`), threads, floats, pipes and the one-shot `fs_read`/`fs_write`.

**Stage 4o: a static is computed by the compiler.** Stage 4j built each `static` when the program started, which a
program could not tell from `docs/compile-time-data.md`'s data in the binary except by a trap, a write or the
start-up cost. `consteval.cho` is a small interpreter: it runs a static's body over the tree, with one int to an
element of its store whatever the type, and the answer is written into the module as `@sd<n>.data`, a constant
array, and `@sd<n>`, a constant pair of its address and its length: read-only, in the binary, and nothing runs at
the start (the initialiser stays, but empty). It gives up, and the static is run at start as before, on anything it
does not do exactly: a call to a function of the program (`doubled` in `static_data.cho` calls `twice`), a `match`, an
overflow or a division by zero (the Rust compiler refuses these at compile time; here the program is
trusted to have been checked and the trap is at start), an index outside a table, a `byte` outside 0 to 255, more
store than the 1,048,576 elements it has or fuel than 20 million steps, or a table that would not fit what the module has
left. A static that reads another reads the answer of the one before it, if that was computed. The tables of the compiler
itself, 1,100 lines of `t[i] = v;` each, are computed this way, and the fixed point holds.

**Stage 4p: one value on the heap.** `Box[T]` for a `T` that is not a slice is a pointer in the code, whatever `T` is, so
a type that holds itself through a box has the size of a tag and a pointer and the declaration that did not
compile in `docs/heap.md` §4 does. `box(heap, v)` is a `malloc` of the size of `T` (the address of the second
element of an array of them, a constant of the module) with the value stored, `unbox` loads it and frees the
block, `contents` of a borrowed box loads the pointer from the slot the reference points at and is a reference
to what it holds, with the mode and the region of the borrow; `alloc[a](v)` is the same store into the arena's
list. A field of a struct or a payload of an enum that is a box is read through a reference as a borrow, as the
other `res` ones are. The corpus of the stage: a list built, walked by a reference and freed, boxes of structs
written through a unique borrow, a box in a box, structs allocated in a loop in an arena, a `res` struct holding
a `Box[int]`. The first run of the checker modes found that the new expression also ran there and wrote code
into buffers that were not: it is code only, and `SKIP` elsewhere, as it was.

**Stage 4q: tuples.** A tuple of two or three parts is an anonymous struct, so it is an aggregate (`{ i64, i1 }`), built with
`insertvalue`, taken apart with `extractvalue`, passed and returned as one operand; `.0` through a reference copies a
`val` part and borrows a `res` one, as a field does, and `let (a, b) = e;` binds each part. A tuple of nothing in the
code (the capabilities `narrow(fs, "/tmp", "/var")` returns) is nothing in the code. Tuples of more than three parts have
no code in this stage; stage 4q-bis below has it. Along with them, the two builtins that open a file, read or write it once and close it (`fs_read` and
`fs_write`) are one more call of the runtime.

**Stage 4q-bis: the three gaps of #637.** Replaying the CI of the sibling repositories without the Rust fallback found three constructs
the compiler in cancho had no code for (status 3), each of which was why a repository built only through the fallback. (1) A tuple of
more than three parts with a part that is a reference to a slice: a tuple of that size is built from the parts pushed on the stack of
arguments, which held a pointer and no length, so the part is now packed into one value (`ex.push_part`, `ex.put_pushed`) as
`ex.put` packs it in a tuple of two or three. (2) A call that returns a reference to a `res` struct field, `fn entries[&m](m: &m
Members) -> &m tar.Entries { return m.entries; }`: a reference to anything the code holds as an aggregate is a pointer, so `lent` in
`exprs.cho` now accepts it where it accepted a reference to a capability or a handle only. (3) A tuple of more than eight parts: the
table already held any number (`make_many` allocates `n / 3` extension entries), and the limit was a number in two places
(`tuple_expr`, `node_type`), now 64, which the Rust compiler does not have; a larger tuple is a status 3 and builds through the
fallback. **Measured (Linux x86-64).** `selfhost::compile::the_compiler_in_cancho_builds_programs_that_do_what_the_rust_ones_do` has
10 more programs (`WIDE`: tuples of 9, 12, 16 and 40 parts and a ten-part one passed and returned; `SLICE_TUPLES`: three programs;
`LENT`: two with a `res` part lent shared and unique), each built both ways and run to the same exit status; the test fails
without the three changes (the compiler in cancho answers status 3). With the compiler in cancho alone (`CANCHO_COMPILER=auto`),
`cancho-hdl`'s `scripts/build.sh`, `cancho-table`'s `table` and all eleven programs of `cancho-tools` build, which they did not.

**Stage 4r: generics.** A function with type parameters is not compiled where it is declared: a call to it knows the types
its parameters are (the checker solved them from the arguments), so the call asks for an *instance* and names it
`@lexs_<node>i<n>`. The types cross from the caller's table to the instance's as codes (`instances.cho`: scalars, a
declaration with its arguments, boxes, tuples, prelude types), and the driver, after the functions, compiles each
instance with its `T` standing for the type asked; a call from an instance asks again, so the worklist grows until nothing
new is asked. A literal of a generic declaration has variables for its parameters until the values it holds say them;
`Option::None` learns its parameter from the type a `let` or a `return` expects, or from the other arguments of the
call it is in (it is written last), and the code of a literal is written only when all of them are solved.

**Stage 4s: floats.** A `float` is an LLVM `double` and an `f32` a `float`; a constant is written as the hexadecimal
of the bits of the `double` (an `f32` is widened, which is exact). The one hard part is the literal: the Rust compiler
reads it with `from_str`, correctly rounded, so the compiler in cancho has to as well, and has no floats to do it with.
`floats.cho` keeps the digits as a big number in 30-bit limbs, multiplies by 5^e or shifts left and divides by 5^e (in
chunks of 5^13, remembering whether anything was left over), takes the top 53 (24) bits, or fewer in the subnormals,
and rounds to nearest even by the next bit and the rest. Arithmetic never traps; `==` is `oeq` and `!=` is `une`, so NaN
is unequal to itself; `truncate` and `int_of_f32` trap on NaN and at 2^63 either way, as the Rust backend's three
checks do. The scalar types a slice, a field or a payload may hold are now `int`, `byte`, `float`, `f32` and `bool`
(`types.is_plain`).

**Stage 4t: threads.** `spawn(payload, f)` runs `f` on a new thread: `f` is `pthread_create`'s start routine as it is, so
nothing is built for it, and the payload is the pointer it receives (a whole number, byte or boolean widened and made a
pointer, a reference as it is, `null` for a capability, which is nothing in the code). `join(t)` waits and reads what it
returned out of the cell `pthread_join` writes. A thread is a pointer in the code and a value that must be joined; a
function used as a value is the address of its code, and a call through the name that holds it is a call to that
function. A payload or a result of more than one leaf (a struct) has no code yet.

**Stage 4u: foreign functions.** A call of an `extern fn` is a `call` that spells its own function type (`call i64 (i64,
ptr, i64, i64) @write(...)`), so the symbol is declared once whatever a program or the runtime says of the same C
function, and a `declare` is written only where the runtime does not already have one. A `bool` is widened to the byte
C is given, a `[byte]` is its pointer and then its length (so `write(fd, buf, n)` passes the length where C reads the
count, as the Rust backend does), a `c_int` result is 32 bits and is sign-extended. A `c_ptr` is a pointer the code
holds, compares with `null_ptr()` and gives back to C, and nothing else.

**Stage 4v: sockets and the clock.** The Rust backend writes each socket builtin out as a hundred lines of LLVM in the
body of the function that calls it. Here they are C (`runtime/net.c`), compiled once for each system with
`-DDARWIN=0` and `-DDARWIN=1` into IR that needs no header and no target (`runtime/make.sh`; every constant that
differs, `SOL_SOCKET`, `SO_NOSIGPIPE`, `EAGAIN`, the offsets in `addrinfo`, is a macro, the same values `SocketOs` has), and
the module of a program that uses a socket or the clock carries the IR of its own target. A call pushes its arguments
and the bound the `Net` was narrowed to (pointer and length, as a path prefix is), and a function that answers a tag, a
handle and a reason writes the three whole numbers to a cell, which become the leaves of the prelude enum. The Rust
compiler's behaviour is kept to the detail: an empty buffer to `conn_read` never reaches the kernel, `accept` clears
`O_NONBLOCK` on Darwin, a host that is not the bound's traps. The 115 programs of `tests/accept` were also built on
Linux x86-64 (`ssh gram`, clang 21, `LINUX_OUT` in the scoreboard script) and the same 94 pass there.

**Stage 4w: pipes.** A pipe is a socket pair, so its verbs are the connection's: `pipe_read` is `conn_read` and
`pipe_write` is `conn_write`, and `pipe_open` answers `Piped`, whose four leaves are the tag, the parent's end, the
child's end and the reason.

**Stage 4x: directory handles.** `runtime/dirs.c` is the same text for every architecture: what differs, the open flags
(`O_DIRECTORY` and `O_NOFOLLOW` are not the same on Linux x86-64 and aarch64), `AT_FDCWD`, the offsets in `struct dirent`
and `struct stat`, are an array of 21 whole numbers (`cancho_plat`) that the compiler writes into the module for its
target, so the input line says `TARGET linux aarch64` where Linux has two. A name is one component, checked before any
call, as in the Rust backend: not empty, not longer than `NAME_MAX`, not `.` or `..`, no `/` or NUL.

**Stage 4y: `defer`.** `defer E;` is sugar, and stays sugar here: the statement puts the expression on a stack, nothing
is written, and the block that holds it writes the expressions out in reverse where it ends (unless its last statement
returns), and a `return` writes out all of them, after the value it returns has been read. Only in compile mode, like
the other things the checker's own modes do not handle yet.

**Stages 5a to 5c.** The builtins on words are plain LLVM written the way the Rust backend writes it (`wordcode.cho`).
A capability held by a struct has no value, so its place in the aggregate is `{}` and nothing is put in or taken out.
A reference held by a declaration is typed with a region that matches any: the code is written for programs the Rust
compiler has accepted, so the regions a declaration names are not followed, which loses the escape check of a
reference taken out of a struct and nothing else. A run of bytes is held as `{ ptr, i64 }`.

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


## Building with it from the command line

`cancho build --compiler <exe> <file.cho>... [--std]` builds the program with the compiler in cancho: `<exe>` is
`examples/selfhost/compiler.cho` and its modules built once (`cancho build ... --std`). The command reads the files (and the
standard-library modules they import), sends the target and the files to it, takes the LLVM IR, and has `clang` and the
usual link finish the build. A refusal is exit status 1 with the Rust front end's sentence for the same files and the rule the compiler in cancho
refused it by (the compiler in cancho answers a rule and a place; if the Rust front end accepts what it refused, the message says the two disagree), and a program
the compiler has no code for yet says so, at the construct. `--compiler auto` (or `CANCHO_COMPILER=auto`, or `CANCHO_COMPILER=<exe>`) has the compiler built from this repository's `examples/selfhost` by the Rust compiler the first time, and kept in `$CANCHO_CACHE` or `~/.cache/cancho` under a hash of its sources (a binary installed without the repository next to it says so). Targets: aarch64 macOS, x86-64 and aarch64 Linux. It is a way to try the
compiler on real programs; `cancho build` without the flag is unchanged.

### Two disagreements found by a real program, and fixed (alpibrusl/cancho#614)

cancho-web's `examples/users_threads` built with the Rust compiler and was refused by the compiler in cancho (which `cancho build` uses by default), while `cancho check` accepted it. Two rules of the Rust checker's effect rows had not been ported:

* **`fork_clock` performs nothing.** The Rust checker gives it no effect (`Builtin::effects`; `parallelism.md` §9.1: the parent already holds the authority). The port made it perform `clock`, so a function whose only act was forking a clock was refused with its true row `[]` and refused as decoration with `[clock]`: no row passed. `tests/accept/fork_clock_row.cho`.
* **`spawn` costs the caller the thread function's row** (`threads.md` §2), less what the caller's borrowed capabilities authorise. The port added `conc` and stopped there, so `serve` in `users_threads`, whose row `[heap, conc, clock, conn_read, conn_write, poll]` the Rust checker requires exactly, was refused for declaring labels it "never performs". `tests/accept/spawn_row.cho`, which fails on the compiler in cancho before the change and passes after it.

Both programs are in the lists that build with both compilers and compare (`conformance/backends.rs`, `selfhost/compile.rs`). The corpus the earlier stages compare over did not contain either shape, which is what the numbers in §6 could not say: they are agreement on the programs that were compared, not a proof.
