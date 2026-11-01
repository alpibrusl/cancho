# The refusal message in the compiler in cancho

> **Status: design, written before the code.** Nothing here is built. Every number in §2 was measured on 2026-11-01 and the method is given, so it can be run again; what is a choice or open is marked.

## 1. What asked for it

`check --output json` answers, for each refusal, a `rule` (a tag a program matches on), a `message` (the sentence for this occurrence) and an `explanation` (the sentence for the rule), `docs/agent-errors.md` §5. The Rust command fills all three. The command line in cancho, the `cancho` of a release, fills the first and the third, and puts the third in the second:

```
$ cancho check t.cho --output json        # t.cho returns `true` from a function that answers `int`
Rust:        "message": "expected `int`, found `bool`",                     "explanation": "An expression's type does not match what the context requires — …"
standalone:  "message": "An expression's type does not match what the context requires — …",  "explanation": "An expression's type does not match what the context requires — …"
```

`cancho-code` found it. Its same-signature stop rule (`docs/design.md` §4 there; `src/signature.cho`) compares the *message* of a refusal, because "expected `int`, found `bool`" and "expected `bool`, found `int`" are different mistakes, and the rule says so. Run with the compiler of release 5, three of its gate scripts (`test-verdicts.sh`, `test-driver.sh`, `test-profile.sh`) fail on it: two different failures sign the same, and the loop stops on the wrong rule. A refusal that gives an agent the rule and the place but not the occurrence is less than the Rust command gives.

## 2. Measured

Corpus: the 393 programs of `tests/reject/`, each with its expected message in a `//~ ERROR` comment and its rule in `//~ RULE`. Each was run as `cancho check F --std --output json` with the Rust command (built from `main`) and with release 5; the first refusal of each was compared. (The scripts are not in the repository: they are about 40 lines of Python over `subprocess` and `json`, and the numbers can be had again from the corpus alone.)

| | |
|---|---|
| fixtures | 393 |
| fixtures with refusal JSON from the Rust command | **384** (the other 9 are the `*_on_wasi` ones, which refuse only under a wasm `--target`, not passed) |
| rules among them | 57, every one with a message that is not its explanation |
| distinct message shapes (a quoted part, a number or a string made `_`/`N`) | **180** |
| the standalone gives the same rule | 382 of 384 |
| the standalone's `message` equals the Rust one | **1 of 384** (`no_main`, which the standalone words as the Rust command does) |
| the standalone's `message` is the rule's sentence | 383 of 384 |
| the `//~ ERROR` text is contained in the Rust `message` | 381 of 384 (identical in 119) |
| Rust refusals per program (1 / 2) | 382 / 2; the standalone gives one in every case |

Where the text of a message comes from, as bounds, because the exact split needs a reading of every check: the quoted parts of a message (`` `x` ``) were looked for in the source. **At least 122 of 384 (32%)** have every quoted part on the line of the error (the strict reading, which misses a name written on an earlier line). **At most 300 of 384 (78%)** have every quoted part somewhere in the file (too loose: `int` is in most files); a further 14 have no quoted part (11 with no variable part, 3 with only a number). **At least 70 of 384 (18%)** need what the checker knows (a type, a count, a mode): a quoted part of theirs is found nowhere in the file, or they carry a number as well. Rust spells its messages at **305** `Diagnostic::new` sites (the largest files: `lower/stmt.rs` 38, `lower/expr.rs` 32, `ingest.rs` 31), and the corpus covers only part of them: the 180 shapes are what the fixtures reach, not the size of the catalogue.

The shapes are not spread evenly: `type-mismatch` has 3 (`expected `_`, found `_``, and two with an identifier), `unknown-name` 8, `arity-mismatch` 11, `linear-value-unconsumed` 10, `capability-not-narrowable` 13.

Two things found on the way:

* **The order of the checks differs.** For `reference_escapes_via_inference.cho` and `undefined_name.cho` the standalone refuses with `program-shape` (no `main`) where the Rust command refuses with the error in the file (`reference-escapes-region`, `no-function-values`). The Rust command reports a program-wide fact after the file's own errors; the standalone before.
* **There is no function in the compiler in cancho that spells a type** (`types.cho`, `scope.cho`: no `show`, `spell`, `describe`, `name_of` or `render`). A message that prints a type needs one.

## 3. What this must fit

* **A refusal is data with a stable shape** (`docs/agent-errors.md`). The rule and the position stay as they are; the message is added to them, never instead of them, and `explanation` stays the rule's sentence.
* **The Rust message is the reference, and the corpus is the oracle.** The text a fixture expects is already written down 393 times; a test of "the message contains the `//~ ERROR` text" is possible today.
* **Every refusal has a rule tag, and no input reaches a panic.** A message is built from state the checker already has; a part it cannot spell is left out, not guessed.
* **Measured claims only; a gate is fixed before the code it judges, and must be able to fail.**

## 4. The design

**D1. A refusal carries its parts, and the message is made where the refusal is reported.** **[choice]** `ast.fail(st, rule, from, to)` keeps the first refusal in `st[2..5]` and nothing else. Add to it a *shape* (a small integer naming one of the message shapes of the rule) and up to three *arguments* (integers: a span in the source for a name, a type's index in the table, a count). The checker that refuses knows these at the moment it refuses; it passes them. The text is built once, by the driver, when the program is reported (the state is still alive there), into a buffer that `Built` carries beside `rule`, `from` and `to`. The program is never given text to carry through the checker. **Open:** whether three arguments are enough; the corpus says how many each of the 180 shapes has, and that is counted when the catalogue is written (§5, step 1).

**D2. One table of shapes, written once.** **[choice]** For each rule, its shapes as templates (`expected {type}, found {type}`) in a table in `rules.cho` beside the explanations, in the same order as the Rust text, so that a template is checked against the Rust message by eye and by the test of D4. The table is the catalogue; this document does not repeat it.

**D3. A function that spells a type, in `types.cho`.** **[choice]** Needed by at least 70 of the 384 messages, and by `type-mismatch`, the rule `cancho-code` leans on. It prints what the Rust `Display` prints for the types the corpus shows (the scalars, `[T]`, references with their mode and region, tuples, named types). A type it cannot spell is printed as its kind, which is a message less exact than Rust's and not a wrong one. **Open:** how much of the Rust type syntax the corpus actually exercises; the shapes list of §5 step 1 answers it.

**D4. The gate, fixed first: a ratchet over the corpus.** A conformance test (`selfhost::messages`) runs every fixture of `tests/reject/` through the command line in cancho and counts those whose `message` contains the fixture's `//~ ERROR` text. The count is written in the test (today it would be 1), the test fails if the count is lower, and fails if it is higher and the number was not raised: a slice raises it by exactly what it fixed. It can fail: break one renderer and the count drops. It also lists the fixtures that do not match, so that the next slice is a list and not a search.

**D5. The order, by what it buys.** **[choice]** Slice 1: `type-mismatch`, `operator-type-mismatch`, `unknown-name`, `arity-mismatch` (what `cancho-code` needs, with D3). Then by the number of fixtures a rule covers, taking the rules whose messages are all source text before those that need the checker's state. A slice names its target count before it starts and reports the count after.

**D6. The order of the checks.** The two fixtures above: the standalone reports the error in the file before a program-wide `program-shape`, as the Rust command does. Done with slice 1 (small, and the corpus covers it).

**What this does not do.** It does not make the standalone answer *every* independent refusal. The Rust command answers more than one in 2 of 384 fixtures (the corpus is of single faults on purpose), `ast.fail` keeps the first, and a plural answer is a change to how the checker carries on after a refusal, not to how it words one. It is a separate item, and `docs/agent-errors.md` §5 is its place. It does not touch the text printed for a person (the line, the caret, the sentence), which already matches the Rust command's; and the 9 wasm fixtures are not in scope.

## 5. To do before the first slice

1. **Count the parts.** For each of the 180 shapes, how many arguments, and of what kind (a name from the source, a type, a number). A script over the corpus, committed with the table of D2.
2. **Read the checks of slice 1** and write down where each of the four rules refuses, what it knows there, and what `ast.fail` would need to be given.
3. **Measure the cost** of D1 on the compiler's own build: the refusal path is not hot, but `ast.fail` is called from many places, and the change to its signature is one every call site sees.

A slice that finds the design wrong corrects this document in place, with the commit.
