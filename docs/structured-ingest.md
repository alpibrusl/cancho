# Structured ingest: JSON as the other text form

> **Status: built.** `cancho print --output json` renders a parsed unit
> as data, and `cancho ingest <file.json>` reads that data back and
> prints canonical `.cho` text. The two contracts of
> [`canonical-ast.md`](canonical-ast.md) §2a — identity-preserving and
> idempotent — are enforced over JSON too, by extending the walker that
> already enforces them over text, and it walks the same corpus: every
> accept fixture, every reject fixture that parses, the examples, the
> packages, `benches/`, and the whole standard library.

---

## 1. Why

`docs/agent-errors.md` §1 measured the problem this closes. An agent
writing cancho emits token-level text and inherits the syntax
hallucinations of every language it was trained on — and its first
contact with the compiler is a parse refusal about a *spelling*, which
the checker never sees. The AST is already canonical by construction
(`canonical-ast.md` §3): whitespace, comments and redundant
parentheses die in the lexer and parser. Text is therefore one *lossy
rendering* of the tree, and nothing forced an agent to write text at
all. The missing piece is the ingest path: JSON in, canonical AST out,
with the same identity contracts the printer already holds.

This is also the satisfy loop's named prerequisite
([#406](https://github.com/alpibrusl/cancho/issues/406)): a candidate
an agent writes is text today, and a rejected candidate costs a full
compile-and-turn per spelling error. Structured ingest is what lets an
agent emit a program the parser cannot refuse for a reason that is not
about the program.

## 2. What ships

Two directions, one JSON vocabulary (`canonical-ast.md` §4.1 holds for
JSON exactly as it holds for the hash encoding: **names are text,
never interner indices** — an index is a property of one parse, and a
cross-process form cannot depend on the order a lexer met names in):

```sh
cancho print <file.cho> --output json     # the AST as data, one file
cancho ingest <file.json> [--output json]   # that data back, as canonical text; refusals as data
```

Both are registered in `introspect` (`docs/agent-cli.md`), so an agent
learns them from the binary rather than from this page.

The JSON is a direct rendering of the arenas, with every `Symbol`
replaced by its text and every id replaced by the value it names:

* **Types**: `{"name": "int"}`, `{"name": "Buffer", "qualifier": "io",
  "args": [...]}`, `{"ref": {"unique": true, "region": "r",
  "inner": ...}}`, `{"slice": ...}`, `{"tuple": [...]}`,
  `{"lit": "libc"}`, `{"fn": {"params": [...], "effects": [...],
  "ret": ...}}`.
* **Expressions**: `{"int": 7}`, `{"float_bits": 4607182418800017408}`
  (the value's bits, `canonical-ast.md` §3's own reason: `0.0` and
  `-0.0` are two literals and must be two JSONs), `{"f32_bits": ...}`,
  `{"bool": true}`, `{"str": "..."}`, names, struct literals, fields,
  tuples, variants, unary/binary operators by their canonical spelling
  (`"+"`, `"&&"`, `"^"`), calls with their qualifier, indexes, slices,
  `alloc`/`alloc_slice`.
* **Statements**: `let` (with the annotation's absence as `null`),
  `assign`, `destructure`, `destructure_tuple`, `borrow`, `region`,
  `expr`, `if`, `while`, `match` (with its arms' patterns),
  `return`, `defer`.
* **Items**: `fn` (regions, generics, bounds, `where`, params, effects,
  ret, body), `extern` (with its `symbol` string), `struct` (fields,
  mode, generics, bounds), `enum` (variants with payloads), `static`.
* **A file's shape**: `{"edition": 5, "module": "std.buffer", "imports":
  [...], "items": [...]}` — an edition of `1` is the default and is
  written only when the file said it, the same convention the parser
  already keeps (`editions.md` §6.1).

## 3. The refusals

The path's own failures are data from the first slice, per
[`agent-errors.md`](agent-errors.md) §5. Three rules, added to the
catalogue because they are new ways to be wrong, not new wordings of
old ones:

| tag | what it names |
|---|---|
| `ingest-json` | the file is not JSON at all, or an object is not where one must be |
| `ingest-node` | a node's `kind` field names nothing the compiler builds — an unknown vocabulary word, not a malformed one |
| `ingest-arity` | a known node with the wrong shape: a missing field, a wrong-typed field, a negative tuple index |

`ingest --output json` answers them as `check --output json` answers — `{"rule":
"ingest-json", "message": "...", "position": {"file": ..., "line": ...,
"column": ...}}` — with the position of the offending JSON value, not
of anything in a `.cho` file that does not exist yet. Exit codes are
unchanged: **1** for a refused file, **2** for a bad command line,
**3** for an unreadable one.

A name that is not a cancho identifier (`$scope`, `my-var`, `2fast`)
is `ingest-node`, refusing the value rather than silently interning
something the lexer would never have produced.

## 4. The contracts, unchanged and enforced over JSON

The walker in `crates/cancho/tests/conformance/identity.rs` holds two
properties over text for every `.cho` file in the repository. Both now
hold over JSON, and the test that enforces them is the same test, run
over the same corpus:

1. **Identity-preserving.** `parse(source)` → `json` → `ingest` →
   `text` → `parse` gives back the same `SigId`, `BodyId` and `TypeId`
   for every declaration as `parse(source)` did.
2. **Idempotent.** Ingesting the JSON of the ingested text is the same
   text.

The direction `check` cannot cover is the one this tests: an
*arbitrary* JSON that names real node kinds must either build a tree
the printer renders or refuse with a rule — there is no third path of a
half-built AST.

## 5. Out of scope, deliberately

* **No repair.** A refused ingest is a refusal, not a guess
  (`agent-errors.md` §2 keeps `cancho repair` out; this inherits the
  decision).
* **No compiler-written generation.** The agent writes the JSON; the
  compiler checks it and renders it. Nothing here turns the compiler
  into a source of programs.
* **No schema publication.** `canonical-ast.md` §8 keeps tag values
  "not yet contracts"; the JSON vocabulary is in the same position, and
  this document is the description rather than a frozen schema. The
  forcing function §1 of the issue names is real: freezing the JSON is
  what §8's emptying looks like, and that happens when the AST stops
  growing, not before.
* **One file at a time,** matching `print`'s own shape
  (`many-files.md` §5): printing is about one text, and the JSON form
  of one text is one object.
