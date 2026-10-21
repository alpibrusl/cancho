# The command line in cancho

Status: **slices 1 to 12 built** (`fmt [--check] PATH...`, §7, §8; `print`, `ingest`, §9; `agent-guidelines`, §10; `--help`, `introspect`, `skill`, §11; `check`, §12; `ids`, §13; `build`, §14; `run`, §15; `test`, §16; `layout`, §17; `satisfy`, §18); the rest is design (epic #491,
issue #485). The numbers quoted are from the tree at the time of writing and are the only
ones claimed.

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

## 7. Slice 1: `fmt FILE...` (built)

`examples/selfhost/cli.cho` is a root program (`main(world)`) that takes `Args`, `Fs` and the heap. For each file named after `fmt` it
reads the file (`open_read`, `file_read` into a buffer), parses and prints it **in memory** through the sink (§2), runs the formatter
(`formatting.cho`) on the source and the printed text, runs the three checks of `docs/formatting.md` section 4 on the result, and
writes the file if it changed (`open_write`, `file_write`). Output and status are `cancho fmt`'s: `formatted PATH` (or, under
`--check`, `would reformat PATH` and status 1), `PATH: not formatted: WHY` on standard error and status 1 for a file the formatter
refuses, status 2 for a file that cannot be read or written or a usage error.

The checks are in cancho: (1) the result prints as the original does (`alike`), (2) it holds the same comments in the same order
(`formatting.comments_of`, new), (3) formatting it again gives the same text (`fixed_point`). The printer's room is the one the compiler
writes code in (`200` integers a token), raised by one token in a hundred bytes of source because a long string literal is one token;
a file whose printed text still does not fit is refused with a sentence rather than cut short.

Not done in this slice: directories (§8), the other commands, and the words of a
parse refusal (cancho says `ERR rule from to` where Rust renders the diagnostic). The checks of §4 only fire on a formatter bug, so no
program in the corpus reaches them; they are run, not exercised.

**Measured (Linux x86-64, debug `cancho`).** `selfhost::cli::the_command_line_in_cancho_formats_like_cancho_fmt` builds `cli.cho` with
the Rust compiler and runs it, as a process on a file, with and without `--check`, against `cancho_syntax::format` and the words
`fmt_cli.rs` says, on every program of the repository, the edge programs and the programs spoilt two ways each (2,525 programs, 5,050
runs; the formatter changes 1,580, refuses 14 and leaves 714 as they are): the standard output, standard error, exit status and the file afterwards are the same in all of them. Two mutants (`--check`
leaves the status 0; `--check` writes the file) each make 1,580 of the 5,050 runs differ.

## 8. Slice 2: directories

`examples/selfhost/walk.cho` finds the files a command is given: a path that is a file is that file; a directory is every `.cho` file
beneath it, in bytewise order of name within each directory, skipping names that start with a dot, descending into directories (a link to
one is followed, as `Path::is_dir` does, by trying `open_dir` on it). It uses `std.dirs.list` over a directory handle. `cli.cho fmt`
collects every path first and then formats them in order, and stops at the first environment error (a file it cannot read or write), as
`cancho fmt` does.

**Measured.** `selfhost::cli::the_command_line_in_cancho_walks_directories_like_cancho_fmt` makes a tree (nested directories, a `.hidden`
directory, a `.dot.cho`, a `notes.txt`, a `Q.CHO`, a directory called `x.cho`, names that differ in case and punctuation, a file that does
not parse, a link to a directory, an empty directory) and runs both commands on it four ways (`--check` on several roots, a root with a
trailing `/`, an empty directory, and a rewriting run through the link): standard output, exit status and every file afterwards are
the same. It then runs both with `--check` over `std`, `examples/selfhost`, `packages` and `tests/accept`. Four mutants (no skip of dotted
names; every file taken whatever its extension; links not followed; a `/` always added) are each killed. Not the same: a message on the
standard error for an unreadable directory, which `cancho fmt` words and this does not (it is a file it then fails to read).

## 9. Slice 3: `print`, `print --output json`, `ingest`

`cli.cho print FILE` writes the canonical text of the program to the standard output, `print --output json FILE` the tree as JSON
(`jsonprint.cho`), and `ingest FILE` the canonical text of the program a JSON file stands for (`ingest.cho`). A program the parser
refuses is reported as `FILE: ERR RULE FROM TO` on the standard error with status 1, where `cancho print` renders the diagnostic with a
line and a column; an ingest that is refused writes `ERR RULE FROM TO` on the standard output (the form of the stand-alone ingester),
status 1, where `cancho ingest` renders a diagnostic on the standard error and, with `--output json`, answers a `refused` object: that
form is not available (the command says so and exits 2). `jsonprint.print` used to answer the number of bytes its last write took
as a status; it answers 0 now.

**Measured (Linux x86-64, debug `cancho`).** `selfhost::cli::the_command_line_in_cancho_prints_and_ingests_like_the_rust_front_end` runs
each of 920 programs (the repository's, the edge programs) through `print` and `print --output json` (1,708 prints that succeed, 132
refusals) and 964 inputs through `ingest` (871 succeed; the JSON of each program, and the ingester's edge inputs), against the
oracles of the stand-alone ports: the standard output, the standard error of a refusal, and the status are the same in every run. Three
mutants (the JSON form never chosen; a refusal not reported; the status not set) each fail it.

## 10. Slice 4: `agent-guidelines`

`cli.cho agent-guidelines` writes `AGENTS.md`, as the Rust binary does (which embeds it with `include_str!`). A cancho program has no
such embedding, so `examples/selfhost/guidelines.cho` is **generated**: one `buffer.append` a line of the file, in parts of forty
lines (the text is built in a buffer, so that the same module serves a file and the standard output). A string literal is one line (`docs/strings.md` §4), so the text is written as it is, line by line, with `\\`, `\"`, `\n`, `\r` and
`\t` the only escapes. The test `selfhost::statics::guidelines_module_is_the_text_of_agents_md` regenerates the module from `AGENTS.md` and fails
if the committed file differs; run it with `CANCHO_REGENERATE=1` to write the file again. The module is 1,165 lines for a file of 1,024:
the 2,000-line ceiling of a source file is reached when `AGENTS.md` has about 1,860 lines, and the module is then split in two.

`the_command_line_in_cancho_writes_the_agent_guidelines` compares the bytes the command writes with those of `cancho agent-guidelines` and
with the file (37,734 bytes), and checks that an argument is a usage error (status 2, nothing on the standard output).

## 11. Slice 5: `--help`, `introspect`, `skill`

These three write text that the Rust binary gets from its usage string and from the ACLI library's tree of commands (`src/acli.rs`).
`examples/selfhost/commandtree.cho` holds that text, **generated** the way `guidelines.cho` is (§10) by the test
`selfhost::statics::command_tree_module_is_what_cancho_says`, which runs the Rust binary and fails if the committed module differs
(`CANCHO_REGENERATE=1` writes it): the usage (10,150 bytes), `introspect` in its three forms (text 12,937 bytes, JSON 20,125, table 12,954),
and `skill` (text and table are the same 14,686 bytes; JSON 15,301, cut where the path goes, since `skill --output json PATH` answers the
path and `null` otherwise). The module is 1,453 lines. `cli.cho` parses `--output FORM` and `--output=FORM` (text, json or table), answers
a bad form or a missing one with the message, a blank line and the usage on the standard error, status 2, and, for `skill PATH`, writes the file
and says `Skill file written to PATH`.

Until the Rust binary is gone the module is derived from it; after that it is the source, and the tree of commands is edited in it (or
the module is made from a table in cancho: open).

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_answers_help_and_the_command_tree_like_cancho` runs 20 command lines on
both (`--help`, `-h`, `help`; `introspect` in the three forms, `=` and separate, with a stray argument; `skill` in the three forms, with and
without a path, the path last or first, a path with a quote and a backslash in it; a missing and a bad form for each): standard output, standard
error, status and the file written are the same in all of them. Four mutants (a quote not escaped, the table form chosen as text, `-h` not
known, `--output=` not known) each fail it.

**Not the same.** The Rust command (the ACLI library) also writes a `.cli/` directory beside where it runs (a README, a changelog, the
commands as JSON, examples and schemas), makes the directories above a skill file, and reports `Skill file written to PATH` even when it
could not write it (a path under a file). This command writes only what it is asked, has no `mkdir` to make directories with (the standard
library has none), and says status 3 on the standard error when the file cannot be written. The `--version` line (the revision and host
of the build) is not here: it is a property of the build, not of a text.

## 12. Slice 6: `check`

`cli.cho check [--output json] [--host H] FILE...` checks a program of several files and writes its code and throws it away, as `cancho check`
does (the backend runs there too): it is the build of the compiler in cancho (`driver.run_many`, mode 5), whose answer is a status, a rule
and an offset into the files laid end to end. `examples/selfhost/diagnose.cho` turns the offset into a file, a line and a column (counted in
characters, as `SourceFile::line_col` does) and renders it: the text of `Diagnostic::render` (the path, line, column, a sentence, the line
of the source and a caret under it, as wide as the span and no wider than the line) or the entry of the `refused` array. `--output json` of an
accepted program is `{"refused": []}`, as Rust writes it. Exit status 0, 1 for a refusal, 2 for a bad command line or a file that cannot be read.

**Decided (the owner, in this session).** The `message` of a refusal is the **sentence of the rule**, the one the `explanation` already is: the
Rust command has a hand-written message at each of about 350 sites and the checker in cancho has none, and the checker stops at the **first**
refusal where Rust reports every independent one. Porting the messages is open work, not a decision against it.

**Not available yet.** `--std` and `--target` (the standard library's text is not in the command line, and a WebAssembly target is the Rust
backend's) answer a usage error; `--backend` takes `llvm` only. A cancho program cannot ask what it runs on, so `--host linux|darwin|linux-aarch64`
(default `linux`) names the target the code is written for. A program the compiler in cancho has no code for (its `skip`: it hands those builds
to the Rust compiler) cannot be judged here: the command says where, on the standard error, and exits 3.

**Measured (Linux x86-64, at the time of writing).** `selfhost::cli::the_command_line_in_cancho_checks_like_cancho_check` runs `check --output json` of both on
the 867 programs of the repository, one at a time, and on the 41 programs of several files of `SEVERAL_FILES` (908 in all): 88 are accepted by both
with the same answer; 783 are refused by both with the same rule and file and, for all but the 28 below, the same line and column; in 28 the
rule and file are the same and the place is not (the checker in cancho and the Rust one name another span for some capability rules, or find
another refusal first); 8 cannot be judged (exit 3); and 1 is a refusal the checker in cancho does not make
(`examples/tls_nb/gaps/g17_conn_payload.cho`, `thread-payload-type`). For every one of the 783 whose place is the same, the text of the refusal has the
same place, source line and caret. The counts move with the repository's programs; the test asserts the shape (the same rule and file always, at most
one program in twenty with another place), not these numbers.

**A trap found by this.** `examples/selfhost/runtime.cho` (a 650,000-byte string literal) made the build in cancho trap (an illegal instruction):
`emit.put` wrote past the end of its buffer, and past the end of the state when the literal was long enough. It counts a byte past the end and
does not write it (`overflowed` is how the answer is known to be no good), and the room for the code is raised by the length of the text.

## 13. Slice 7: `ids`

`cli.cho ids [--host H] [--std-dir DIR] FILE...` writes the identity of every declaration (`type`, `sig` and `body` lines), as `cancho ids` does: the
program is checked first and a refused program is a status 1 and no lines. Mode 10 of the driver is that check: the statics, the bodies, and the
generic functions as they are called, as `build_module` checks them, with nothing written and no `main` asked for (`ids` of a library works); a body the
compiler in cancho has no code for (its `skip`) is let through, since an identity does not depend on the code. Mode 9 then writes the lines.

`--std` is `--std-dir DIR`: this command line does not carry the standard library, so the files are read from DIR, in the order `--std` of the Rust
command adds them (`STD` in `main.rs`, kept as `stdorder.cho`, **generated** by `selfhost::statics::std_order_module_is_the_std_table_of_cancho`, which also
fails if `std/` and the table differ). The identities, and so the lines, do not depend on where the files were.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_writes_the_identities_like_cancho_ids` runs `ids` of both on the 908 programs of
`check` above, and, for every tenth, with the library: 182 runs write lines (26 of them with the library, 767,106 bytes of it), 813 refuse, and the lines
are the same byte for byte, the status the same. Four refusals the checker in cancho does not make are named in the test: `g17_conn_payload.cho`
(`thread-payload-type`), `reference_escapes_via_inference.cho`, a tuple index that is no name, and one program of several files; each is a
body the checker lets through or has no judgement of.

## 14. Slice 8: `build`

`cli.cho build [-o OUT] [--emit exe|obj] [-l NAME]... [-L PATH]... [--clang PATH] [--host H] [--std-dir DIR] FILE...` checks the program as `check`
does (the compiler in cancho, mode 5) and, if it is accepted, writes the module it made to `OUT.ll`, has `clang` (`/usr/bin/clang` unless
`--clang` says another) make the executable, or with `--emit obj` the object file, from it, and takes the temporary files away. `OUT` is `-o`, or the
stem of the first file (and `.o` for an object file), as in the Rust command. Everything is done through a capability: the files through
`Fs`, `clang` and `uname` through `Exec`. What `clang` says on a failure is passed on (standard error), with status 3.

**Decided (the owner, in this session).** A program the compiler in cancho has no code for (its `skip`) is **not handed to the Rust compiler**: it is a
status 3 with the place on the standard error, and nothing is made. And a program cannot ask what machine it is on, so the host is found by
running `uname -sm` (its answer goes to a file beside the output and is read back and deleted); `--host` names it instead.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_builds_like_cancho_build` takes the programs of the repository that
`cancho build` makes without the standard library and that finish by themselves in five seconds with no input (82 of the 374 outside `selfhost/` and
`reject/`; 292 do not build without `--std`), builds each with the command line in cancho and runs both: 80 write the same and end with the
same status, 2 cannot be judged (status 3), none differs. The only file left beside `-o` is the output; `--emit obj` makes an ELF (or Mach-O)
object file; `-l`, `-L` and `-o` work; a refused program makes nothing (status 1); a bad command line is a status 2.

**Not here yet.** `--std` (use `--std-dir`, which reads the files, so a program that imports the library builds only with it), `--target`
(WebAssembly), `--backend cranelift`, `--compiler`, a project (`cancho.toml`) with no file named, and the Rust command's own words for what the linker
says. (§15 says how `run` got round the child's streams; §16 is `test`.)

## 15. Slice 9: `run`

`cli.cho run [--stdin] [-l NAME]... [-L PATH]... [--clang PATH] [--host H] [--std-dir DIR] FILE...` builds the program as `build` does, to
`STEM.cancho-run` beside where it is run, starts it, waits, takes it away, and ends with the program's own status. A child of this program
cannot be given the caller's standard streams (`Stdio::Inherit` is refused in `docs/processes.md` §4.4: a child would print for a parent
that released its `Io`), so the program is started with pipes and `std.process.capture_both` (§7.2 of that document) is the relay.

**What that costs, and the Rust command does not.** Nothing the program writes appears while it runs: standard output and standard error are
written, in that order, when it is over (a program that prints a prompt and waits for the user, or runs for a long time, shows nothing until it
ends); at most 64 MiB of each is kept, and a program that writes more, or runs for more than a day, is killed (status 3, with a sentence);
the program has an **empty standard input** unless `--stdin` reads ours to its end and gives it all at the start (so an interactive program
cannot be run); and its environment is a `PATH`, not the caller's. The program killed by a signal is a status 3, silently, as in the Rust
command; one that could not be watched or started is a status 3 with a sentence.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_runs_like_cancho_run` runs the 82 programs of §14 with both commands: 80
write the same on both streams and end with the same status, 2 cannot be judged (status 3, nothing written), none differs, and nothing is left
beside the program. `--stdin` hands a program its three bytes and an empty input is read as the end; a program that divides by zero is a status 3.
A streaming relay (a `std.process` loop that writes as it reads) and an input that is read as the program asks for it are what would close the
differences; neither is built.

## 16. Slice 10: `test`

`cli.cho test [--host H] [--std-dir DIR] [--backend llvm] [--clang PATH] [-l NAME]... [-L PATH]... FILE...` finds the `test_*` functions of the
files, builds one program of the files and a runner written for them, and runs each test as a process of its own, as `cancho test` does
(`docs/testing.md` §3, `test_cli.rs`).

**Finding the tests** is `testscan.cho`, driver mode 11: the files are parsed (nothing is checked) and the declarations walked in order. A function
named `test_*` is a test if it has no type parameter, answers `int`, and asks for nothing but a unique `Heap` and a unique `Io`, once each; in a
module that is not the root it must be `pub`. The refusals are those of the Rust command, with its words: a `main` in the root module of a file
(`cancho test` supplies its own), a function that looks like a test and is not one (why, and the shape a test has), no test at all, and two test
modules whose paths end in the same word (the runner imports each by its last segment): status 2 for each.

**The runner** is a file, `runner_main.cho`, that is read like the others: an `import` for each module that has a test, a `main` that reads its
first argument as a number, and an `if which == K { status = CALL; }` for each test; `testscan.runner` writes it, with the text `test_cli.rs` has.
The program is built to `STEM.cancho-test` beside the first file (the Rust command uses a directory of the temporary directory), each test is run
with `std.process.capture_both` (§15) with its number as the argument and an empty input, and the file is taken away when the tests are over.

**Differences.** The tests run with a `PATH` for an environment, not the caller's; a test that runs for more than a day is killed (status 3 of
that test, "ended without a status"); a refusal says the sentence of its rule (§12), where the Rust command says what is wrong at the place; the
Rust command reads each file separately for the tests, and this one reads them together, so a parse error is placed in the stream (the same file
and place); a file that cannot be read is a status 2 (the Rust command's 3). `std.signals` has no bit for the fault signals and `child_wait` says
0 for them, so a test that ends by a trap, a SIGSEGV (a stack overflow) or a SIGABRT is said to have ended by SIGILL, the trap every checked
operation ends with; the other signals are named by their number. `--std` and `--target` are what they are in §14.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_tests_like_cancho_test` runs 14 programs with both commands and
compares the status, the standard output and the standard error (of a refusal, up to the place): all pass, some fail (with the output of a test
that wrote, and a status of 256, which the runner turns into 1), a trap, tests in modules, two modules that end the same, a test that is not
`pub`, no tests, a `main`, a type parameter, a result that is not `int`, a parameter that is not a capability, a capability twice, a file that
does not parse, and a program that is refused: none differs, and nothing is left beside the files.

## 17. Slice 11: `layout`; and what `authority` would take

**What was read first.** The two read-only reports of the Rust command are not alike. `print_layout` (60 lines of `main.rs`) is a function of the declared
types: it asks `cancho_codegen::layout_of` of each struct and enum that has no type parameter, which counts *leaves* (`abi.rs`, `leaves_into`) and
reports `leaves`, `size` (8 a leaf), `packed` (each leaf at its natural width, rounded up to the widest) and `stride`. `print_authority` (about 230 lines)
is a report on the *IR* the Rust pipeline builds: the labels each reachable function performs, the externs it reaches, the functions that are pure
(`Func::is_pure`), how many operators and calls compile-time evaluation folded (`folded_operators`, `folded_calls`) and how many functions are left
after pass 2 (`functions`), and, for a WASI target, the imports the row licenses. The compiler in cancho has the effect rows and the reachable
set (`effects.cho`, `prune.cho`), but **no purity analysis and no count of what its evaluator folded**, so `authority` is not a port of one function: it is the
effects, a purity pass (`docs/purity.md` §2: no label and no parameter that is written through), and a counter in `fold.cho` and `consteval.cho` that agrees with
the Rust one to the unit. That is the next slice, and it starts with the counter, because it is the part that can disagree.

**`layout`** is driver mode 12 and `layoutrep.cho`. The program is built as `build` builds it (so that a refusal, and a program with no `main`, are the same
ones), then each struct and enum that has no type parameter, in the order they are declared, is walked: a `int`, `float` or pointer is a leaf of 8, an `f32` 4,
a `byte` or `bool` 1; a reference is one pointer and, to a slice, a length; a `Box[[T]]` is two; a tuple its parts; a struct its fields; an enum a tag and every
variant's payload. The rows the prelude declares come first, as a table of 48 names and leaf counts (`File` to `Ports`; every leaf of those is 8 wide, so
`packed` is `size`); the line about what packing would save is the sum over all rows of `size - packed`, and is written only if it is not 0. A type with a field
the compiler in cancho has no type for (`node_type` answers 0: a function-typed field, a tuple of more than eight) is a status 3 as in §14, saying where.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_reports_layout_like_cancho_layout` runs the repository's programs and the programs of
several files with both commands, and with the standard library for a tenth of them: 109 runs (21 with the library) write the same table byte for byte, 878
are refused by both, 8 cannot be judged (status 3), and 6 are refusals the checker in cancho does not make (the test allows at most 10). The 6 are
`examples/tls_nb/gaps/g17_conn_payload.cho` (§12), one of the programs of several files (not looked into), and four runs **with the library** of a program that has no `main` (three files of `packages/` and `tests/programs`, and a two-line edge case; measured one by one):
the library has a function named `run`, which `driver.cho` takes for the entry point the builder uses (`entry`), so the missing `main` is not seen; the
same gap makes `cli.cho build --std-dir` and `check --std-dir` accept a program with no `main` (measured on a two-line file), and is not fixed here.

**Not here.** `--std` and `--target` (§14); a generic type is skipped, as in the Rust command, but so is one that has a type parameter no member mentions
(the Rust command measures that one); `authority` (above).

## 18. Slice 12: `satisfy`

`cli.cho satisfy [--host H] [--std-dir DIR] [--clang PATH] CONTRACT CANDIDATE` (`docs/satisfy.md`, `satisfy_cli.rs`) is a composition of what is already here, and no new
analysis: the two files (and the library) are checked as one program (driver mode 10, as `ids` does: nothing is asked of the pair, which has no `main`); the tests are
those of the contract alone (`scan_tests`, mode 11 on the first file of the stream, which `satisfytext.record` cuts out, so that a `test_*` of the candidate is not the
contract's); there must be some (status 2, as the Rust command says); the contract must call, as `name(` in its text with the comments and the string literals set aside
(`satisfytext.calls_any`, a copy of the Rust function, by name and not by resolution), something the candidate declares that is not a test (status 2, naming them in the order
of their modules and names); then the tests are built and run as `test` does (§16), and the verdict is `satisfied` and, for each function of the candidate, `  MODULE:: sig NAME`
and `    sig_id HEX`, or `not satisfied: see the test failures above` and status 4. The identities are driver mode 13 (`identity.listing`, a sibling of the `ids` output
that writes into memory: the module, the name and the 64 digits of the signature of each function, in the order they are declared), run on the whole program and on the
candidate alone, which says which are the candidate's.

**Differences.** `--std` is `--std-dir DIR` (§14); `--host` and `--clang` are the ones of `build`; a refusal says the sentence of its rule, the first only (§12); the tests
run with a `PATH` for an environment (§16). The Rust command builds the tests with its Cranelift backend and this one with clang, which changes nothing that is said.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_satisfies_like_cancho_satisfy` runs 12 pairs with both commands and compares the status, the standard
output and the standard error (of a refusal, up to the place): satisfied (with a module, with a root candidate that declares a struct, and a candidate with a test of its own),
a test that fails (4), a contract with no tests, a contract that calls nothing, a contract that names a function only in a comment and in a string, a candidate that is refused, a
contract that calls what is not there, a test of the wrong shape, an option that is not one, one file, and three: none differs, and nothing is left beside the files.
