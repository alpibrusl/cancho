# `cancho fmt`: one layout, with the comments put back

> **Status: built.** `cancho fmt <file|dir>... [--check]`
> (`crates/cancho-syntax/src/format.rs`, `crates/cancho/src/fmt_cli.rs`).
> Applied to every tracked program, and held there by a test (§5).

## 1. What was missing

`cancho print` renders one parsed file in canonical form, and says in
its own header that it is not a formatter, because it cannot be: comments
are discarded by the lexer, so that formatting can never change a content
hash (`docs/canonical-ast.md` §3), and a printer built on the AST can
only delete them. So an agent writing cancho had a canonical layout it
could not use on a real file, and nothing to hold a codebase to one
layout either — `cargo fmt --check` exists for the compiler's own Rust,
and there was no counterpart for the programs written *in* this language.

## 2. How it works

The AST has no comments, no blank lines and no literal spellings, so a
formatter cannot work from the AST alone. This one does not try to
extend it (that would let layout reach a hash); it uses the text as well:

1. `print` the parsed file. That is the layout.
2. Tokenise the source and the printed text and **align** the two token
   streams. They differ in a short, known list of ways, and the aligner
   knows each one and nothing else:
   * the printer drops redundant parentheses (`(a) + 1` → `a + 1`);
   * it may add or drop a trailing comma;
   * it spells a literal one way. `'a'`, `0x10`, `1_000` and `1.50` print
     as `97`, `16`, `1000` and `1.5`, so the **source's spelling is
     put back** — a formatter that rewrote `'a'` to `97` would be
     deleting the one readable thing in the line;
   * `else { if .. }` prints as `else if ..` (§3);
   * `pub` before `extern` is read and discarded by the parser
     (`ExternDecl` has no visibility to keep), so it does not print;
   * every `import` is written before the first declaration.

   Any other mismatch is not a layout difference, and the formatter
   stops: the file is reported and left alone.
3. Put each comment where it was, by the token next to it: at the end of
   the output line that token ended up on, if it was written at the end
   of a line there; otherwise on its own line above the line of the next
   token. A comment in the middle of an expression has no stand-alone
   place, and moves above its statement. Keep **one** blank line where
   the source had one or more — never right after a `{` or right before
   a `}`.
4. Check the result (§4), and return it only if every check passes.

## 3. What it decides

* **`else if`.** `print` used to write an `else` block holding one `if`
  as `else { if .. }`, one level deeper per link: the parser reads
  `else if` as exactly that block, so the two are one AST and one hash,
  and the ladder was the shape nobody writes. `print` now writes the
  chain, so `fmt` and `print` agree. This changes `print`'s output for any
  program with an `else if`; no identity moves.
* **One statement per line.** `if c { x = 1; }` on one line becomes
  three. The printer has one layout and this is it.
* **Blank lines.** One is kept where the source had one or more; one
  separates items regardless, which `print` already did.
* **Trailing whitespace and CRLF** go; `\n` endings are written.
* **A comment's text is never edited**, apart from trailing whitespace.

## 4. Why it can be trusted

A formatter that is sometimes wrong is worse than none, so `format`
returns text only if the text passes three checks, run on every call:

1. It **parses**, and `print` of it equals `print` of the original: no
   token changed meaning.
2. It holds **the same comments, in the same order**.
3. It is a **fixed point**: formatting it again changes nothing.

A file that fails one is not half-formatted; `fmt` reports it
(`path: not formatted: <why>`, exit 1) and leaves it as it was. What
each check is for was found, not assumed: building the aligner against
the repository's own sources turned up four things the first version
could not reproduce — `else if` (§3, which was a *printer* problem, not
an aligner one), `pub extern fn`, a dropped `}` that looked exactly like
a printed one (the drops are decided before the matches), and
`std/ed25519.cho`'s `import` in the middle of the file, which is now at
the top.

Measured on the repository: of the 138 programs under `std/`,
`examples/`, `packages/` and `tests/accept/`, all 138 format, 74 would
change, and formatting all 74 leaves `cancho ids` unchanged for every
one of the 80 `tests/accept` fixtures (a `SigId` and a `BodyId` per
function — `cancho ids` before and after, byte for byte), and a second
`fmt --check` over the result finds nothing to do.
`formatting::every_tracked_program_formats` is that sweep as a test, and
`formatting_never_panics_on_a_damaged_program` deletes each line of three
real files in turn and formats what is left.

## 5. What this does not do

* **Applied, and kept applied.** 93 files under `std/`, `examples/`,
  `packages/`, `tests/accept/` and `benches/` changed (1,281 lines added,
  817 removed, nearly all of it one-line `if c { x; }` made three), and
  `formatting::the_repository_is_formatted` fails on any of them that
  stops being canonical. `tests/reject/` is left alone: most of it does
  not parse, on purpose, and its line numbers are what the fixtures
  assert. The full suite passes unchanged, which is also the check that
  no identity moved: the conformance tests hash these files.
* **No line wrapping.** The printer writes one statement per line however
  long. Wrapping a long call is a layout decision with its own
  documentation to write, not a side effect of this.
* **A comment inside an expression moves** (§2.3). It is kept and
  ordered, not left where it was.
* **Imports are not sorted**, as `print` does not (`print.rs`'s
  `module_header`: nothing hashes an import).
* **`--check` and a refusal share exit 1.** The first means "would
  change", the second "cannot be formatted"; the message says which.
  A caller that needs them apart reads the prose.

## 6. Open

| Question | Why it waits |
|---|---|
| Wrapping long lines | No measured case yet where one matters |
| A distinct exit code for `--check` | Nothing has asked to tell "would change" from "cannot format" |
