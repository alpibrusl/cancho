# The command line in cancho

Status: design (epic #491, issue #485). Nothing here is built yet; the numbers
quoted are from the tree at the time of writing and are the only ones claimed.

## 1. What is left of the Rust binary

`crates/cancho/src/` is about 5,800 lines of Rust around the libraries. The
libraries that did the work now exist in cancho (`examples/selfhost`): the lexer,
parser, checker, emitter, printer, formatter, JSON printer, JSON ingester,
BLAKE3 and the identities. What is still Rust is the **dispatch**: argument
parsing, reading files and directories, calling the right mode, and turning the
answer into text and an exit code. Today the cancho pieces are reached through
`driver.cho` modes 0–9, each of which reads one stream on standard input and
writes one answer on standard output; the Rust binary is the thing that feeds
them.

Removing Rust therefore needs one more program: a `cancho` executable written in
cancho that does the feeding in-process.

## 2. The step before: a sink (done)

`fmt` needs the printer's text in memory (to compare with the file, to reparse
it, to check it is a fixed point). The printer used to write to the console.
`sink.cho` (merged, #569) makes `printing.cho` and `floatprint.cho` write either
to the console or into a region of the state array. The printer parity test is
unchanged (460 of 460 inputs the same).

## 3. The program

`examples/selfhost/cli.cho`, a module `selfhost.cli` with a `main` that takes the
world, reads the arguments, and calls one function per command:

| command | what it does | needs |
|---|---|---|
| `print [--output json]` | parse, print canonical text or JSON | modes 4, 7 |
| `ingest` | JSON to canonical text | mode 8 |
| `fmt [--check] PATH...` | walk, format, write if changed | modes 4, 6; directory listing, file write |
| `ids` | identities of each declaration | mode 9, the embedded standard library |
| `agent-guidelines` | print `AGENTS.md` | the file embedded in a generated module |
| `check`, `build`, `run`, `test` | later | `clang` orchestration, text and JSON refusals |
| `introspect`, `skill` | later | static data |
| `install`, `add`, `vcs`, `satisfy` | later | network, git objects |
| `authority`, `layout` | later | IR-level data the cancho compiler does not yet produce |

Exit codes are the Rust ones: 0 success, 1 a refusal, 2 a usage or environment
error, 3 an internal failure.

## 4. Order of work

1. **`fmt` first.** It exercises everything the others need (argument parsing,
   walking directories, reading files into the state, running a mode in-process
   and capturing its output, writing a file) and it has a complete oracle
   (`cancho fmt` in Rust, plus the three safety checks: the reparse prints the
   same, the same comments survive, formatting twice changes nothing).
2. `print`, `ingest`, `agent-guidelines`: thin.
3. `ids`: needs the standard library text embedded, since the Rust binary embeds
   it.
4. `check` with text and JSON output and the rule sentence.
5. Everything that needs `clang`, then the rest.

## 5. How it is tested

As every port so far: the Rust command is the oracle, the cancho command is run
on the same inputs, and the outputs and exit codes are compared. A mutation of
the new program that the comparison does not catch is a missing input. The
differential test is `crates/cancho/tests/conformance/selfhost.rs`.

## 6. What this does not claim

- Refusal *messages* differ: Rust has a message per call site; cancho reports the
  rule tag, the span and the rule sentence. The JSON `message` field would differ
  until that is decided (open).
- `authority` and `layout` are not in the first slice; `layout` is already known
  to differ in the cancho compiler.
- No timing is claimed for the new command until it is measured.
