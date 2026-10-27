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
several files with both commands, and with the standard library for a tenth of them (chosen by a hash of the program's name, so that adding a file to the repository does not change which): 107 runs (19 with the library) write the same table byte for byte, 887
are refused by both, 9 cannot be judged (status 3), and 1 is a refusal the checker in cancho does not make (`examples/tls_nb/gaps/g17_conn_payload.cho`, §12; the test names it).

**Found by this test, and fixed.** The first run of it had 6 such refusals, four of them a program with no `main` *with the library*: the library has a function
named `run`, which `driver.cho` took for the entry point the builder uses (`entry`), so the missing `main` was not seen; `cli.cho build --std-dir` and
`check --std-dir` accepted a two-line file with no `main` for the same reason (measured). Only a `run` of the root module is an entry now (`driver.cho`,
`build_module`); the 6 became 1 (one of the others, a program of several files, was the same).

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

## 19. The command line is five files

`cli.cho` had grown to 1,910 of the 2,000 lines a source file may have (`CONTRIBUTING.md`), so it is split by concern and nothing else changed (every
function and every line of it is where it was, with `pub` and the name of its module in front of the calls that cross a boundary): `cli.cho` (117 lines) keeps
`main` and the choice of the command; `clibase.cho` (263) what every command shares (reading a file, the usage, the report of a refusal, names and paths);
`clifmt.cho` (523) `fmt`, `print`, `ingest`, `introspect` and `skill`; `cliprog.cho` (540) the loading of a program, `check`, `ids`, `layout`, `build` and `run`;
`clitest.cho` (579) `test` and `satisfy`. The 18 tests of `selfhost::cli`, `selfhost::parity` and `selfhost::statics` pass unchanged.

## 20. Slice 13: the project, without dependencies

`cli.cho install`, `build` and `test` with no file (`docs/package-system.md` section 8.3) work in a project whose `cancho.toml` has no dependency. `projfile.cho` (1,185 lines) is the
`cancho.toml` parser, a subset of TOML written in cancho: a lexer, the tables `[package]`, `[dependencies.NAME]`, `[[bin]]` and `[[test]]`, and the validation of `load` (names,
revisions, unique names, sources) with the messages of the Rust command. `cliproject.cho` (860 lines) finds the project (the current directory by `/bin/pwd`, then each ancestor
in turn, up to 64: a `..` in a path is a trap under `Fs`, so no relative path is used), checks `[package] cancho`, and builds (`build_files`, a program's `--bin NAME`) or runs
(`clitest.test_set`, a set's `--test NAME`, under a heading `== test NAME`) what it names. A project that has a dependency is a status 3, said, and nothing is made; so is `add`.
The commands fall back to the file forms when the arguments are not the ones of the project form.

**Differences.** The complaint of the TOML library is said in its first line only (and not with its picture of the line); the commit the compiler was built from is not known, so a
`[package] cancho` pin cannot be checked (a refusal that says so, or with `--ignore-compiler-rev` a note); `build/deps` is cleared and made, as Rust does, only since section 25 (before it was neither); `std = true` needs
`--std-dir DIR`; `--compiler` is accepted and ignored.

**Measured (Linux x86-64).** `selfhost::cli::the_command_line_in_cancho_works_in_a_project_like_cancho` runs 21 cases with both commands and compares the status, the
standard output, the standard error and the files left: build all, one, none such, from a subdirectory; test all, one, none such, from a subdirectory; install (here, below, with an
argument); no program, no tests; an unknown field, not TOML, a bad name, missing sources, an empty directory, a name twice; a compiler pin (refused, and ignored): none differs
(the three texts above are not compared on the standard error). A project with a dependency exits 3 under `build`, `install` and `test`, and `add` does too.

## 21. Two corrections: a path with `..`, and an unknown option

**A path with `..` in it trapped.** An `Fs` refuses a path by a trap when it has two dots in a row anywhere (`..` and `a..b` alike) or is 4,096 bytes or more (`docs/filesystem.md`
section 4.1, `cancho_path_covers`), and `cli.cho` passed the arguments it was given to `open_read`, `open_dir` and `open_write` as they were: `cli.cho check ../a.cho` died of
`SIGILL` (measured: status 132; it was found by the project slice, §20, whose first lookup went up with `../cancho.toml`). `walk.dotdot` now asks first, at the five places that open
a path (`read_file`, `is_file`, `is_directory` with the two walks, `store`, `spawn_to`), and the command says `PATH: cannot read the file` and answers 2. **Difference:** the Rust
command reads such a path (the operating system resolves it) and `cli.cho` does not, because it has no way to turn a relative path into an absolute one without asking
`/bin/pwd` at each of those places; the project lookup does ask it once, and goes up by absolute paths.

**An unknown option was read as a file name** by `check`, `ids`, `layout`, `build`, `run` and `test` (it was refused only by `satisfy` and, in a project, `build`). They now answer
`unknown option `X`` and the usage, status 2, as the Rust command does (compared in `the_command_line_in_cancho_works_in_a_project_like_cancho`, with the `..` cases).

## 22. Dependencies: the design (written before the code)

> **Status: design.** Nothing in this section is built. The numbers are measured on the repository at `53bee285` (Linux x86-64, the debug build of the Rust command, the
> command line in cancho built from `cli.cho`); a claim that the code shows to be false is corrected here, where it was made.

**What is missing.** A project with a `[dependencies.NAME]` table is a status 3 (§20), and so is `add`. What the Rust command does for one dependency (`project.rs::install_all`):
(1) `vcs_origin::ensure`: a checkout of the commit `rev` of `git` in a cache keyed by the commit, filled by `git`; (2) `lock_of_all`: a pin, by name, for every declaration in the store's
`manifest.json`; (3) `fetch_verified`: for the pins, the sources from the store's `sources/HASH.cho`, **re-parsed, re-type-checked and re-identified** together with the closure of the store's
`requires/N.json` (each an origin or a path to another store, walked with a cycle check and a diamond check), the sources written to `build/deps/HASH.cho` only if every pin held, and `std`
added to what is checked if any of them imports it. The Rust that this replaces: `project.rs` 521 lines (§20 has ported 330 of them), `vcs_origin.rs` 158, and in `vcs_cli.rs` the
functions `select_locked`, `verify_selected`, `resolve_closure`, `resolve_own_requirements` and `fetch_verified` (about 400 lines), `lock_of_all` (16), and the readers of `Manifest`, `Blobs`,
`Lock` and `Requirement` in `cancho-vcs` (640 lines of 1,340; the op log, the attestation chain and `publish` are not on this path and stay with the `vcs` commands, below).

**What the command line in cancho already has for it** (measured). The check and the identities: for the closure of `packages/x509/.cancho-vcs/x509_verify` (3 files: 55 declarations,
35 KB of source in the one file of the store, two more from its `requires/`) plus the library, `cli.cho ids FILES --std-dir std` answers in **0.69 s** and 2,551 lines of identities, and
the Rust `cancho vcs fetch` of the same pins takes 2.2 s (`vcs resolve`: 3.0 s; both the debug build of Rust, so this says only that the work is not out of reach, not that cancho is
faster). The `sig` and `body` hashes that `manifest.json` holds as `sig_id` (the key) and `stage_id` are the two lines per function that `ids` writes (measured on `examples/hello.cho`). The JSON
reader (`jsonread.cho`), the BLAKE3 of a blob (`blake3.cho`, the name of a file in `sources/` is the hash of its text), the walk of directories (`walk.cho`), the run of a program with its
output (`cliprog.run_capture`), and the check of a program that needs no `main` (driver mode 10, what `satisfy` uses) are all in the files. **What is not:** the body hash in a listing
(mode 13 writes `module`, `name` and the signature only), a reader of the imports of a source (is there an `import std.`?), `git` run safely, and the cache.

**Decisions.**

1. **`git` through `Exec`, with the settings of `vcs_origin.rs` unchanged** (`-c core.hooksPath=/dev/null`, only the five protocols the Rust code allows, `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0`),
   as `init`, `fetch --depth 1` of the commit (then all heads and tags if the server refuses), `checkout --detach`, `rev-parse HEAD` compared with `rev`, then the `.git` taken away (`/bin/rm -rf`)
   and the directory renamed into the cache (`fs_rename`; a lost race is fine: the other directory is as good). `ls-remote` for `add --ref`. The program is `/usr/bin/git` (`run_capture` takes a path, and a name with no directory is looked for in the directory the command ran in). **Corrected after measuring:** a program started from cancho has only the environment it
   is given, so `HOME` and `$CANCHO_CACHE` cannot be read by running `env` (it would show only what was passed to it), and `grep` for `getenv`, `env_var` and `environ` in `std/*.cho` finds nothing; this paragraph first said
   `HOME` could be had "through `/usr/bin/env` run once", which is false. What was measured instead, on Linux with `env -i`: `/bin/sh -c 'echo "$HOME"'` is empty and `cd ~` fails (dash), and `/bin/bash -c 'echo ~'` prints the home
   directory from the password file (`/root`). So the cache root is `--cache DIR` if given, else `.cache/cancho` in the home directory that `bash` finds; the darwin job of CI is what measures it there. Differences, stated now: `$CANCHO_CACHE`
   and `$XDG_CACHE_HOME` are not read (`--cache` is the one way to move the cache), `git` has no `HOME`, no ssh agent and no credential helper (a private repository over ssh or https cannot be fetched from cancho; public https and `file://` can), and the half-made
   neighbour is named with the clock in milliseconds where the Rust code has the process number.
2. **The verification is the compiler in cancho, not a second implementation of it.** A source is checked with the closure and (if it imports `std`) the library as one program (mode 10); its identities
   are a new **mode 14** (`identity.pairs`: the signature and the body of each function, foreign function and static, `SIG BODY` a line each; **corrected when it was built:** this paragraph first said mode 13 would gain a column, but `satisfy` reads mode 13 and the statics are in the manifest too, so a mode of its own was cleaner). The `imports_std` test reads the
   import lines of the source: a token scan in `testscan.cho`'s style, which skips comments and strings, as the Rust one reads the parsed imports and not the text. **Risk named:** the Rust
   check is on the parsed `Ast`, and a scan of tokens is a copy of that rule, which `testscan.cho` and `satisfytext.cho` are too; the differential test below is what says whether it differs.
3. **Slices, each with a differential test against the Rust command, each one a pull request:**
   - **14a** `cliorigin.cho`: `ensure` and `resolve_ref`, tested on local repositories (`file://`, as `project.rs` tests do): the cache directory, a lost race, a commit that is not there, a path that is not in
     the commit, a ref that is not one, and that no `.git` is left. Measured goal: the same directory content as `vcs_origin::ensure`.
   - **14b** `clistore.cho`: `Manifest`/`Blobs`/`Requirement` readers, `select_locked`, the closure walk with its two checks, `verify_selected` on mode 10 and 13-with-body, the write to `build/deps`.
     Tested on every store in `packages/` (26 `manifest.json`, 1,415 declarations, 15 stores flagged `uses_std`; counted with `find` and `grep`), on stores whose blob was changed, whose pin was moved, with a cycle and with a diamond (the cases of `tests/conformance/vcs_*.rs`).
   - **14c** `install`, project `build` and `test` with dependencies (the `nothing is published` and `dependency NAME:` messages of `install_all`; `build` and `test` install first).
   - **14d** `add` (the checks of the store, the append to `cancho.toml`, the rollback if the install fails), and the compiler pin (`--version`), which is what lets the `[package] cancho` check say more than §20's refusal.
4. **Not in this work, and said so:** `vcs publish`, `vcs lock`, `vcs sync`, `vcs log`, the op log and the attestation chain (`cancho-vcs` 1,341 lines, `vcs_cli.rs` 1,114, `vcs_sync.rs` 355, `vcs_dir.rs` 189, `vcs_origin.rs` 158; *corrected in §28: the first two were written as 700 and 700 here, which is not what `wc -l` says*). They write stores;
   a project only reads one. They are the last of the Rust to go (#486–#490) and need the canonical JSON writer, which nothing else asks for. *Corrected in §28:* this said they also need `ed25519`; no `vcs` command calls `attestation.rs` (the only user of `ed25519-dalek`, 297 lines), so they do not.

**What would show this design wrong:** a store in `packages/` for which `verify_selected` in cancho disagrees with Rust on whether the pins hold (the identities differ from `ids`, or a text the Rust parser accepts and the one in cancho
refuses, the known gaps of the checker are the ones asserted in `the_command_line_in_cancho_checks_like_cancho_check`); a `git` that cannot be run with `PATH=/usr/bin:/bin` on a machine where it lives elsewhere; or a cache that two
processes fill at once and break (the Rust code relies on `rename` onto an existing directory failing; `fs_rename` must be measured to do the same).

## 23. Slice 14a: the origin cache

`cliorigin.cho` (`ensure`, `resolve_ref`, `cache_root`, `invalid`) is a port of `vcs_origin.rs` as §22 designed it, with one change to `cliprog.cho`: `run_capture_in` is `run_capture` with the environment a caller gives (`run_capture` passes
`PATH=/usr/bin:/bin` as before). `git` is run with the settings of the Rust code (hooks off, five protocols, no prompt, no optional lock); the checkout is made in `REV.tmp.MILLISECONDS` beside its place in the cache (`CACHE/git/REV`),
the `.git` is removed with `/bin/rm -rf`, and the directory is renamed into place with `fs_rename` (if that fails and the destination is there, another process finished first and its directory is as good). `origincheck.cho` is the small
program that runs it for the test; the commands that use it are the next slices.

**Measured.** `selfhost::origin::the_origin_cache_in_cancho_makes_the_checkout_the_rust_code_makes` makes two commits of a local repository (`file://`) into a cache with `cancho add` (which fills the cache before it looks at the store) and with
`origincheck ensure`, and compares the two caches: the same files, no `.git`, the same directory names; a second ask for a cached commit does not touch the repository (it is given a URL that is not there); five refusals say what the Rust
code says (a path that is not in the commit, a revision that is not a hash, a commit that is not there, a path that leaves the repository, an absolute path); `ref` finds the hash of `HEAD` and of a tag as the Rust `--ref` does and refuses a ref that is
not there with the same words, and a URL or a ref name that git would read as an option; and the root with no `--cache` is `$HOME/.cache/cancho` (2.7 s for the whole test). **Eight mutants of `cliorigin.cho`, seven killed:** the `.git` kept; the
revision not checked; `..` allowed in the path; the half-made neighbour not removed (found only after the test compared the caches after a refusal); the ref name not checked; `ext::` allowed in a URL (the last two only after the test asserted the message and
not just the status); the store directory not checked. **Not verified:** the branch for a lost race (`rename` fails and the destination is a directory): the mutant that removes it did not compile, and a test would need two processes filling one cache at once.

## 24. Slice 14b: the store

`clistore.cho` (1,090 lines) reads a store and checks what is pinned in it as `cancho install` does for a dependency: `manifest.json` and `requires/N.json` into records (`jsonread.cho`), a blob hashed again (`blake3.cho`) and compared with its name, the pins of a lock matched against the manifest, the closure of the stores it requires walked with the Rust code's two checks (a store met again on the path is a cycle; a name pinned to two signatures by different paths is a diamond), and each source behind a pin compiled again with its context (mode 10, then mode 14) and required to produce the signature it is filed under and the body the manifest says. Nothing is written unless every pin held; then each source is copied to `OUT/HASH.cho`. A requirement whose lock says where its store is is made a directory by `cliorigin.ensure` (§23); any other is a path relative to the requiring store, and since an `Fs` traps on `..` it is normalized lexically (`normalized`; a link is not followed, where Rust canonicalizes). `imports_std` reads the import lines of a source with the comments and the string and character literals set aside, where Rust reads the parsed imports; it agrees with `grep '^\s*import\s+std\b'` on every blob of `packages/` (all 1,415 declarations' sources). `storecheck.cho` is the small program that runs it for the test; nothing user-facing calls it yet.

**Differences.** A problem is worded as cancho words it (the refusal of the compiler in cancho is said as `check` says it) and not as the Rust code does; a source that imports `std` needs `--std-dir DIR` (Rust bundles the library); `uses_std` in the manifest is ignored by both (the check must not be satisfiable by editing a flag).

**Measured (Linux x86-64, debug build).** `the_stores_of_packages_are_fetched_in_cancho_as_cancho_install_fetches_them` puts `packages/` in a git repository and, for a store at a time, runs `cancho install` on a project that depends on it and `origincheck ensure` + `storecheck fetch` on the same commit, comparing the status and every byte written. **All 26 stores of `packages/` agreed** (one run, 797.9 s: too long for CI, which compares four: no dependency, one dependency, a chain that imports `std`, and a store of four sources; 56 s). `a_store_that_does_not_hold_is_refused…` changes a copy of `net-sockets` nine ways and compares the status: the store as it is (fetched), a blob that is not what its name says, a blob that is not there, a body that is not the manifest's, a signature the source does not produce, a name published twice with the later signature invalid, a manifest that is not JSON, one with nothing in it, a store that requires itself: all agree (40 s). **Mutants of `clistore.cho`: eight tried, six killed:** the blob hash not checked, the body not compared, no context passed (the chain that imports `std` fails), `std` never detected, and the smallest signature picked for a name published twice (killed only after that last case was added). The two that survive change the *words* and not the status (a missing signature falls to the body check, which refuses it too; a missing blob is said as "cannot read" instead of "no blob recorded"). **Not tested by a mutant:** the cycle check (removing it recurses without end) and the diamond check (no store of `packages/` has one; the Rust code's tests build it from three stores, which is the next test to port).

## 25. Slice 14c: `install`, `build` and `test` with dependencies

A project that has `[dependencies.NAME]` is no longer a status 3. `cliproject.install_all` does what `project.rs::install_all` does: `build/deps` is cleared (`/bin/rm -rf`) and made, even for a project with no dependency; then for each dependency in the order of the file, its commit is made a directory by the origin cache (§23, `cliorigin.ensure`) and its store fetched into `build/deps` and every pin of it, and of the stores it requires, checked (§24, `clistore.fetch_store`), with `installed NAME (REV12): N file(s)` on the standard output. A store with no declaration is `dependency NAME: nothing is published at PATH in GIT (REV12)` (status 1), the others `dependency NAME: WHY` (the status the fetch gave, 1 or 3). `install` runs it; `build` and `test` run it after their own checks and put the files of `build/deps` after the sources of each program or set (the Rust order). New options on all three: `--cache DIR` (an absolute directory: the cache of §23) and, for `install`, `--std-dir DIR`; a store whose sources import `std` needs `--std-dir` for any of them (§24). `add` is still a status 3.

**Differences.** As §23 and §24 say (`--cache` for `$CANCHO_CACHE`, no `HOME` for `git`, the words of a problem, `--std-dir` for a source that imports `std`), and `git` is `/usr/bin/git`.

**Measured (Linux x86-64, debug build).** `selfhost::store::the_command_line_in_cancho_installs_dependencies_like_cancho` runs `install`, `install` over a file an older install left, `build` and `test` of a project that depends on `net-sockets` (a store inside a local git repository) by `cancho` and by `cli.cho`, and the refusals for a directory that is not in the commit and for a store with nothing in it: the same status, the same standard output, the same first line of the standard error, the same files in `build/deps`, and the same answer to whether the program was built; all six agree (85 s). Four mutants of `cliproject.cho`, four killed: `build/deps` not cleared, the files of `build/deps` not given to the build, `--cache` ignored (found only after the test asserted where the checkout went), the outcome of a fetch ignored. **Not measured:** a dependency that requires another store by an origin (it is the code of §24, whose test walks `packages/` for that); `--std-dir` for a project (the test passes it and the sources of `net-sockets` do not import `std`).

## 26. Slice 14d: `add`

`cliproject.add_command` is `project.rs::cmd_add`: `add NAME GIT-URL [--rev HASH | --ref NAME] [--path DIR]` (and `--cache DIR`, `--std-dir DIR`, `--ignore-compiler-rev`). In the Rust order: a name that is not one (letters, digits, `-`, `_`, `.`), the project found and read and the compiler checked, a name already in the file (refused), then the revision: `--rev` as it is, or `--ref` (default `HEAD`) resolved once by `git ls-remote` and said as `NAME is HASH`, never both; a git url or a path with a quote, a backslash or a control character (usage); the origin checked to be a pin (`cliorigin.invalid`); its commit made a directory (§23); the directory checked to hold a `manifest.json` (if it does not, the message says which stores it holds one level down, `PATH/NAME, ...`, and to pick one with `--path`); then the table appended to `cancho.toml` (after a newline if the file does not end in one, so that comments and layout survive), the project loaded again and installed (§25), and, if that fails, the file written back as it was. It says `added NAME (REV12) to cancho.toml`. With this, nothing of the project half of the package system is a status 3 any more.

**Differences.** As §23 to §25 say. Not done: **the compiler pin.** `check_compiler` (§20) still cannot say which commit this command line was built from, so a `[package] cancho` is refused (or noted with `--ignore-compiler-rev`) where Rust compares it. Making it comparable needs a build that stamps its commit into the source (`--version`), which is the build of the compiler in cancho and not this slice's.

**Measured (Linux x86-64, debug build).** `selfhost::store::the_command_line_in_cancho_adds_dependencies_like_cancho` runs sixteen cases with both commands, each in a project whose `cancho.toml` has a comment and no final newline, and compares the status, the standard output, the first line of the standard error, `cancho.toml` afterwards and the files in `build/deps`: a revision, a tag, the head, a name taken, a directory that is not in the commit, a directory that is not a store (with its hint), a revision that is not a hash, a ref that is not there, a store with nothing in it (the install fails and the file is put back), a url with a quote, a path with a backslash, `--rev` with `--ref`, a name that is not one, no url, an option that is not one, no project: **all sixteen agree** (42 s). **Nine mutants of the new code, nine killed:** no rollback, no newline added, a duplicate allowed, the default ref `main`, `--rev` with `--ref` allowed, no hint, no escape check on the url, none on the path, the store check skipped (the last two only after the two cases that escape were added, which an earlier edit had silently failed to insert).

## 27. Slice 15a: the refusals the checker in cancho missed

Two of the three refusals §12 listed as missed are made now. **`ids` and the quiet check** (`driver.check_quiet`, mode 10, used by `ids`, `satisfy` and the verification of a store) used to walk the functions with the checker alone, which does not see a binding that holds a reference into a block it outlives through a type that is still a hole; it now runs the build (`build_module`) as `check` does, and lets through the two statuses that are not a refusal of the program (it was not asked for a `main`; it is not written for the code this compiler has). **`spawn`** now refuses a payload or a result that is a prelude type outside Rust's `crosses_to_a_thread` list (`File`, `Io`, `Ffi`, `Fs`, `Args`, `Heap`, `Net`, `Out`, `Ports`, `Clock`): `types.crosses_to_a_thread`, rule `ThreadPayloadType`, so `examples/tls_nb/gaps/g17_conn_payload.cho` is refused.

**Measured (Linux x86-64, debug build).** The 13 tests of `selfhost::cli` pass (315 s) with the lists of missed refusals changed: `check` and `layout` expect none (they expected `g17_conn_payload.cho`), `ids` expects only `several files 39` (it expected `g17_conn_payload.cho`, `reference_escapes_via_inference.cho` and that one). `cargo test --workspace` passes (803 tests in the largest suite, none failing), as do `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings`. **Not measured:** a mutant of either change; the allowlist indices in `crosses_to_a_thread` are the `PRELUDE_*` constants of `ir.rs` copied by hand (the test of the generated tables does not cover them).

**Still missed.** The program of several files (`several files 39`). *Corrected:* this section first described it as a generic call that lets a reference escape; it is not. Rust refuses it as an unreachable statement (`return 0;` after a `borrow … in { return …; }`). `check` answers it with status 3 ("cannot judge") at the call of `a.id`, a generic function with a region parameter, for which the compiler in cancho has no code, and `ids` lets such a body through, so it never reaches the unreachable-statement check. Measured by running `check` and `ids` on the two files. Closing it means code for a call of a function with region parameters, not a change to the checker's rules. *Corrected:* names resolved through the library are not a gap: `tests/reject/undefined_name.cho` is refused by `cancho check --std` and by `cli check --std-dir std` (status 1 both; the sentences differ, as they do for every refusal), and the `check` and `ids` differential tests list nothing else as missed. Not done in this slice: relative `..` paths, the compiler pin (§26), and a streaming `run`.

## 28. Design: the `vcs` commands

*Design before code; nothing here is built.* The remaining Rust that a project does not read is `cancho vcs {publish, sync, log, resolve, lock, fetch}` (`vcs_cli.rs`). What was read to write this (`wc -l` and the source, 2026-10-26): `vcs_cli.rs` 1,114 lines, `vcs_sync.rs` 355, `vcs_dir.rs` 189, `vcs_origin.rs` 158 (§23 ported it), and the library they use, `cancho-vcs`: `canonical.rs` 43, `operation.rs` 132, `op_log.rs` 123, `manifest.rs` 142, `blobs.rs` 116, `lock.rs` 249, `requirement.rs` 133, `gate.rs` 52, and `attestation.rs` 297, which **no `vcs` command calls** (`grep attestation\|ed25519\|Chain` over `vcs*.rs` finds nothing), so `ed25519` is not needed for these commands.

**What each command is made of.**

| command | reads | writes | needs that cancho does not have |
|---|---|---|---|
| `log` | `manifest.json` | nothing | nothing (`jsonread.cho` reads it; §24 already reads the same file) |
| `lock` | `manifest.json`, the lock being extended, an origin (§23) | the lock file, JSON | a JSON *writer* in Rust's `to_vec_pretty` format |
| `fetch` | a lock, a store | `build/deps`-like directory | nothing new: it is §24's `fetch_store` with another destination (to be confirmed by reading `cmd_fetch`, not yet done) |
| `resolve` | a lock and its closure | prints | the closure walk of §24 (160 lines of Rust, not yet read in full) |
| `publish FILE` | the file, each `--requires` lock and its store's closure | `ops/OPID.json`, `sources/HASH.cho`, `manifest.json`, `requires/N.json` | the JSON writer; the canonical bytes of an `Operation` (field order of the serde derive, `parents` left out when empty, effects a sorted set); **the effect row of each declaration as strings**; the soundness gate (`check_candidate`, which is `check` over the merged program) |
| `publish --dir`, `sync` | directories of `.cho` | as `publish`, per file | as `publish`, plus walking directories (§17) and reading each file's imports |

**The new pieces, in order of need.**

1. `jsonwrite.cho`: a writer for what these files hold, which is objects with string keys in sorted order (a `BTreeMap`), strings, booleans, arrays and one nested object; byte-for-byte what `serde_json::to_vec_pretty` writes (two-space indent, `"key": value`, `{}` and `[]` for empty). *Measured goal, not yet measured:* the bytes agree with Rust's on every `manifest.json`, `ops/*.json` and `requires/*.json` of `packages/` (the stores are in the repository: the 26 of §24) read and written back.
2. The canonical bytes of an `Operation` and its `OpId` (`blake3` of the compact JSON; `blake3.cho` exists). Measured goal: every `ops/*.json` in `packages/` has the `op_id` its file name says when recomputed by cancho.
3. The effect row of each declaration. This is the one piece that is not a writer: `publish` records `effect_strings(&ir_func.effects)` for a function, `program.externs` for a foreign function and `{}` for a `static`, which is the *lowered* row, and the compiler in cancho has no listing of it (`driver.cho` mode 13 lists module, name and signature; mode 14 the two identities). It is the same data as the first half of `authority` (§ *Remaining*), which is deferred by an earlier decision, so `publish` cannot be finished before a mode for rows exists. Whether the rows of `packages/` (recorded in each `ops/*.json`) agree with what such a mode gives is the first thing to measure.
4. The closure of the stores a publish requires (§24 reads it; `publish` also *writes* the requirement of each, with the store path made relative to the new store, or empty when the lock has an origin).

**Slices.** 15b `log` and the JSON writer (read back the stores of `packages/`, and `lock` with it: it needs only the writer, the manifest reader and §23). 15c `fetch`, `resolve` (after reading them). 15d the effect-row mode and the canonical `Operation`. 15e `publish FILE` against `cancho vcs publish` on every file the stores of `packages/` were made from, comparing every byte of the store written; then `--dir` and `sync`. Each slice's test runs the Rust command and `cli.cho` on the same input and compares the status, the output and the files.

**What would show this design wrong.** A store of `packages/` whose `manifest.json` or an operation is not what `to_vec_pretty` gives for the value read (then the writer must copy something the format has and this list lacks); an effect row in an `ops/*.json` that the lowered effects of its declaration do not give (then `publish` is not only a writer); or a `publish` whose answer depends on `HOME` or a cache in a way a pure re-run does not reproduce.

## 29. Slice 15b: `vcs log` and `vcs lock`

`clivcs.cho` (196 lines) is `vcs log [--store DIR]`: the manifest of the store (`clistore.read_manifest_with`, the fifth field of each record being `uses_std`, false when it is not there as `serde(default)` reads it), sorted by name (a name given twice keeps the order of the signatures), one row `{:<24} sig SIG  body STAGE` and `  std` if the declaration uses the library; the padding counts characters, not bytes (found by the test). `clilock.cho` (560 lines) is `vcs lock (--store DIR | --git URL (--rev HASH | --ref NAME) [--path DIR]) -o FILE (--all | NAME...)`: the usage checks in the order the Rust command makes them, the origin made a directory by the cache of §23, the manifest and the lock being extended read (`clistore.read_lock`, which shares `lock_record` with the requirement reader), the names looked up (not published, ambiguous, and `--all` with a name that two declarations share, are refused with the words of Rust), the lock written as `serde_json::to_vec_pretty` writes it (`jsonwrite.cho`, 55 lines: two spaces to a level, `"key": value`, no final newline, the escapes of `serde_json`) to `FILE.tmp` and moved into place, its directory made with `/bin/mkdir -p` if it is not there. `publish`, `sync`, `resolve` and `fetch` are still the Rust command's.

**Differences.** `--cache DIR` for `$CANCHO_CACHE` (§23); a manifest or a lock that is not JSON is said as "malformed manifest" / "malformed lock file" and not with `serde_json`'s sentence after it, and an unreadable lock file without the operating system's words; a path with `..` in it (the `Fs` of §21) is "cannot write lock file". The status is the same in each.

**Measured (Linux x86-64, debug build).** `selfhost::vcs::vcs_log_in_cancho_lists_a_store_like_cancho_vcs_log` compares status and standard output of `cancho vcs log` and `cli.cho vcs log` on every one of the 26 stores of `packages/` (about 1,000 rows, more than 100 of them with `std`) and on stores made for a name given twice, a name longer than 24 columns, two bytes to a character, `uses_std` false and left out, no entries, no manifest, no directory, a manifest that is not JSON and one whose fields are not text, and thirteen command lines: **all agree** (36 s). `vcs_lock_in_cancho_pins_like_cancho_vcs_lock` runs both commands in two fresh directories made alike and compares status, standard output, the first line of the standard error and every file left: all 26 stores `--all`, then named cases (names, escapes of `"`, `\`, a control character and `é`, ambiguous, not published, a name twice, no manifest, no store, an empty store, a bad manifest, an existing lock extended, replaced, of another origin, with a bad origin, not JSON, without entries, `-o` in a new directory and `-o` a directory), every command line the Rust command refuses, and a store in a git repository by `--rev`, by `--ref HEAD`, with the default path, with another path, a path or a ref or a commit that is not there, and a lock whose origin has no `path`: **all agree** (42 s). It found two faults of mine on the way (the padding by bytes in `log`, and the words "nothing published" where Rust says "nothing is published") and one in the test itself (`--cache` appended after a bare `--git`). **Mutants: 20 tried, 20 killed.** Of `clivcs.cho`, five (bytes for characters, no `std` flag, the order of equal names reversed, the padding one column short, `log` taking a file); of `clilock.cho`, eleven (an ambiguous name allowed, the origin of another store allowed, the entries of the lock dropped, a replaced entry kept, the stage taken from the signature, the entries not sorted (the program then never ends: killed by the test not finishing, not by an assertion), no origin written, no `mkdir`, a bad origin not checked, the ref not said, the default path `.cancho`); of `jsonwrite.cho`, three (no escape of `"`, none of control characters, one space to a level); of the reader, one (the default path of an origin without `path`). **Two mutants survived the first set of cases** and were killed by adding cases: the default path of `--git` (no case left out `--path`) and the default path read from a lock (the lock the case used had no entries, and an empty lock is not compared with the origin asked for).

**Not measured.** `vcs lock` on a machine where `HOME` is the only source of the cache (`--cache` is always given in the test); a lock file of more than 4,096 bytes of path.

## 30. Slice 15c: `vcs fetch` and `vcs resolve`

`clifetch.cho` is `vcs fetch --lock FILE [--store DIR] -o DIR` and `vcs resolve [--lock FILE] [STORE]`. Both read the lock with `clistore.read_lock` (§29), and both name the store as the Rust command does: the one given, else the one the lock's origin points at (made a directory by the cache of §23), else a usage error (`this lock has no origin, so the store must be named`). `clistore.fetch_store`, which `install` uses (§25), was split into stages so that the three commands share them: `verify_store` (the stores a store requires walked, then each selected declaration compiled again, §24), `fetch_chosen` (the write, only if everything held), and the selections: the latest of each name (`install`), the declarations a lock pins (`fetch_pinned`, which refuses a pin that is no longer published), and every record, or those a lock pins, counted (`resolve_store`). `fetch` says `fetched PATH` for each file in the order the Rust code writes them, and the note about `--std`; `resolve` says how many declarations it checked in how many files, and how many files the requirements added. `cliorigin.joined` now adds no second slash after a base that ends in one (the Rust `Path::join`), found by `-o out/`.

**Differences.** `--cache DIR` for `$CANCHO_CACHE` (§23) and `--std-dir DIR` for the library a source may import (§24); a lock that is not JSON is said without serde's sentence after the path (the status is the same); a pin that does not hold is said with the words of the compiler in cancho.

**Measured (Linux x86-64, debug build).** All 26 stores of `packages/`: `resolve STORE`, then `vcs lock --all` by the Rust command and `fetch --lock --store -o` by both, each in a directory of its own, and everything compared (status, standard output, standard error, and every file written): **26 agree, none differs.** The test that runs in CI, `selfhost::vcs::vcs_fetch_and_resolve_in_cancho_check_a_store_like_cancho_vcs` (68 s), does this for the four stores `store.rs` samples (no dependency, one dependency, a chain that imports `std`, four sources) whole and by lock, with `-o` in a directory that is there, and adds: two pins of a lock, an empty lock, no lock file, a lock that is not JSON, a pin that is not published, a store with nothing in it or not there, a lock with no origin and no store, a source changed after it was published, a source gone, a lock that names a git commit (`resolve` and `fetch` with and without `--store`), and every command line the Rust command refuses: **all agree.** **Mutants: 13 tried, 13 killed** (the empty-lock message, the `std` note, the `plus N file(s)` suffix, a named store ignored, a word taken by `fetch`, a second store taken by `resolve`, the wording `fetched`, declarations and files swapped, `resolve` ignoring the lock, a missing pin not refused, an empty lock not said by `resolve`, the requirements' files not counted, and the doubled slash of `joined`).

**Gate.** `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass. The workspace tests found one failure (the duplication test: `say` was in two files); after the fix the duplication test and the `selfhost::vcs` tests pass, and the workspace has not been run again whole.

**Not measured.** `fetch` into a directory that cannot be made; `resolve`/`fetch` on a machine where `HOME` is the only source of the cache.

## 31. Slice 15d: `vcs publish FILE`

`clipublish.cho` (929 lines) is `vcs publish [--store DIR] [--std] [--requires LOCK[:STORE]]... FILE`. In the order of `publish_one`: the command line (a file, exactly one; `--dir` is refused with a file or a requirement); each lock read and the store it pins found (§30); the file parsed alone, for the names it declares; each required store walked and verified as `fetch` does (§24), the files each brings kept apart (the walk drops what it holds when it meets the next store: found by the first run on a store that requires two); the library asked for (`--std`) if the file or anything it requires imports it; the file, what it requires and the library checked together; then `ops`, `sources` made, the manifest read, the file kept as `sources/HASH.cho`, each declaration of the file that is new logged as `ops/OPID.json` (one already published at the same body is left, one at another body is refused), the manifest written, `requires/` made what this publish says. The new piece of the compiler is **mode 15 of the driver**, `identity.published`: for each function, foreign function and static, in the order `print` writes them, its module, name, the identity of its signature and of its body, and the labels of its declared effect row as written (`name` or `name("value")`, which `clipublish.cho` sorts bytewise and makes a set of, as the `BTreeSet<String>` of the Rust code does). The bytes of an operation that give it its `OpId` are `{"kind":"AddFunction","sig_id":..,"stage_id":..,"effects":[..],"parents":[],"edition":1}` (`parents` always there; the file of the operation leaves it out when empty); measured before it was built: the BLAKE3 of those bytes is the name of the stored operation. `jsonwrite.cho` and `clilock.lock_text` (now with a base indentation, so that a lock can sit inside a requirement) write the rest as `serde_json::to_vec_pretty` does. **The declared row is the one the Rust code records**: for a function it takes the lowered effects, and for a foreign function the declared ones; a row is exact in this language (a function that declares an effect it does not perform is refused), so the two agree, and the 1,415 operations of `packages/` are the measurement.

**Differences.** `--cache DIR` and `--std-dir DIR`; `--dir` is §33 and `vcs sync` §32; a file must be UTF-8 for the Rust command (`read_to_string`) and is read as bytes here; a label whose argument holds a newline or a NUL would end its record early in the listing (no program in the repository has one); the words of a refused program are the compiler's, as for every command; a store that cannot be made says `cannot open store at ..` without the operating system's words.

**Measured (Linux x86-64, debug build).** **Every source of `packages/` published again by both commands into a store of its own, and everything compared: the 26 stores agree, and the 26 stores are byte for byte the ones stored in the repository** (the manifest, the 1,415 operations, the sources, the requirements: the Rust command reproduces what is stored, and so does the one in cancho). Each was published against the stores it requires (`tls` against 12, `x509_verify` against two and the library). `selfhost::vcs::vcs_publish_in_cancho_logs_a_file_like_cancho_vcs_publish` (151 s in the debug build) does this for four of them (`net-sockets`, `http-response`, `x509_verify`, `tls_hello`, each without `--std` and then with it) and then changes the source of `net-sockets` in the ways a publish tells apart: published again and a comment added (nothing new), a body changed (refused), a new function, a new function with a row of two labels written out of order, a second module into the same store, a file that does not parse, one that is refused, one that imports `std` (refused without `--std`, then published, then nothing new), the default store, a store that is a file, a manifest that is not JSON, a store required and then published again without it (the old requirement goes), and the command lines the Rust command refuses. **Mutants: 20 tried, 20 killed**, after four survived the first set of cases and the cases were mended: a body changed (the case had no first publish, so no body ever changed), a row of two labels (the test had none, and then one that both commands refused), `requires/` not emptied, the missing `--std` message (the comparison cleared it). The others: every declaration taken for the file's own; the module not compared; `parents` left out of the bytes of an operation; `uses_std` not recorded and not written; a requirement's store always empty; the common part of two paths not taken off; the source not kept; the wording of `published`, of `nothing new`, of the label quote, of `edition`; two files taken; `--dir` with a file allowed; the array not closed on its own line; the indentation of a lock inside a requirement.

**Gate.** `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` pass. The full workspace run is in the pull request.

## 32. Slice 15e: `vcs sync`

`clisync.cho` (about 560 lines) is `vcs sync [--dir DIR] [--std] [--check]`, a port of `cmd_sync` (`vcs_sync.rs`, 355 lines). What it does, in the order the Rust command does it: walk the tree (`walk.tree`: every name, a name that starts with a dot too, but no link and no directory called `.cancho`, `.git` or `target`; a file called `.cho` has no extension, so it is none; the order is that of a sorted list of paths, which is a depth-first walk with each directory in bytewise order of name, because a path is ordered by its components); compile each file **alone** (and with the library if `--std`, which puts the library's functions in the index of every file), without checking it, and list its declarations with mode 15 of the driver (§31: module, name, signature, body); keep one entry for each `(module, name)`, the first to come (a later one with another identity is a collision, said in a warning, and the index keeps the first); read the index the tree had (`.cancho/ids.txt`: `fn MODULE NAME SIG BODY` to a line, a line that is not one passed over, the last of a key kept, `\r` taken off the end); say what changed (`semantic diff against the committed index:` and a line for each of: added, body changed, signature changed, removed; or `no semantic change: the index matches the source`) and the warnings; then, unless `--check`, write the index (sorted by module and name) and one operation for each addition (`AddFunction` with no effects), body change (`ModifyBody`) and removal (`RemoveFunction`), as `ops/OPID.json` as `vcs publish` writes them (`clipublish.logged` for the first; `clisync.other_op` for the other two, whose bytes for the `OpId` are `{"kind":"ModifyBody","sig_id":..,"from_stage_id":..,"to_stage_id":..,"parents":[],"edition":1}` and `{"kind":"RemoveFunction","sig_id":..,"last_stage_id":..,"parents":[],"edition":1}`; a signature change has no operation, and the entry is overwritten, as the Rust command's report says). `--check` writes nothing and answers status 1 for any change or collision (a file that does not parse is a warning, not a change).

**How the entries are kept.** The Rust command keeps two `BTreeMap`s. Here the entries of every file are laid in one buffer in the order they come (with a `W` entry for a file that was not accepted, so that the warnings come out in the order of the files), and a bottom-up merge sort over their places (stable, so that the first to come is first among equals) puts them in the order of their keys; the entry that stands for a key is the first (the new index) or the last (the old: a later line replaces an earlier). The difference is a merge of the two lists. The sort is the one `std.dirs` has, over places in a buffer instead of names.

**Differences.** `--std-dir DIR` stands for the bundled library, as for `publish` (§31); a file that does not parse is a warning with one line (`FILE could not be parsed`) where the Rust command prints the parser's rendering of the error, which is several lines; a file that cannot be read is treated as one that does not parse where the Rust command refuses the whole sync (status 3); the words of the operating system after a path that cannot be walked are not reproduced; relative paths with `..` are refused (a platform limit, as in §27).

**Measured (Linux x86-64, debug build).** `selfhost::vcs::vcs_sync_in_cancho_indexes_a_tree_like_cancho_vcs_sync` (about 40 s) runs the Rust command and `cli.cho` on the same trees and compares the status, the standard output (a warning about a file that does not parse is compared as a warning) and every file left: empty trees, new, again and checked; a body changed, a signature changed, a declaration gone and one added, everything at once; the entries of a tree with `.git`, `target` and `.cancho` (not walked), a hidden directory and a hidden file (walked), a file called `.cho`, `x.CHO` and a text file (not read), and links to a file, to a directory and to nothing (not followed); no module (the root), a declaration made twice alike and not alike (in the root too), a file that does not parse beside others, a collision and a file that does not parse together; the library; an index with lines that are not entries (`garbage`, `\r\n`, a key twice, three fields, an empty name, a fifth field, a name that is gone, `fn `); `--dir` with and without a final `/`; a directory that is not there, one that is a file, an unknown option, a word, `--dir` last; and the 26 sources of `packages/` together, synced, changed and synced again (**1,415 declarations, 1,415 operations of which 1,401 are different**, the same files in both). **Mutants: 27 tried, 26 killed, 1 equivalent**, after two survived the first set of cases and the cases were mended: an old index that repeats a key (the first kept instead of the last: no case had a repeated key whose last line was the right one) and a line ending in `\r\n` (the one in the case was for a key that a later line replaced). The one that stays is the default directory written `./` for `.`, which gives the same paths, since `Path::join` and `walk.child` both add no second slash. The others: the sort not stable, the name or the module left out of the key, a collision that ignores the body, the new index keeping the last and the old the first, a count of removals twice, signature changes not counted, `--check` ignoring collisions or writing, the body before and after swapped, the kind of a removal misnamed, `parents` left out of an operation, an empty name allowed, `--std` ignored, the root named with `::`, the first file of a collision, the message of the count, the tag of a line of the index, the effects of an addition, and `.git`, `target`, `.cancho`, links and `.cho` walked.

## 33. Slice 15f: `vcs publish --dir`

`clidir.cho` (about 500 lines) is `vcs publish --dir DIR [--store ROOT] [--std]`, a port of `publish_dir` (`vcs_dir.rs`, 189 lines). In the order the Rust command does it: the `.cho` files directly in the directory (`walk.direct`, a name of more than `.cho`, in bytewise order), none being refused; each file's modules and imports, **read off the parse** by a new **mode 16 of the driver** (`identity.modules`: for each module the file declares, not the root, its dotted path, a NUL, the dotted path of each import and a NUL, and a newline; the `as` of an import is left out) -- none is a program, said `skipped FILE (no \`module\` declaration: a program, not a library module)` and left (a file with two is refused by the parser, in both commands, so the Rust command's own check for it is never reached and is not ported); a module already declared by a file before is refused with both files named; imports of `std` and `std.*` are not units' imports; no unit at all is refused; every import must be a module of the directory, the first that is not is refused, with the words of Rust; then the depth-first order of the imports (with the units in the order of their module names, as the `BTreeMap` of Rust has them, and a cycle refused as `modules import each other: a -> b -> a`; the recursion of `visit` is a stack of frames here, the module and how many of its imports are done); then, for each module in that order, `ROOT/MODULE` is cleared (a directory is removed; a path that is a file is `cannot clear the store`, status 3: `remove_dir_all` does not remove a file), the manifest of the store of each import is read (`lock_of_all`), `== MODULE (STORE)` is said, and the file is published against those stores as `publish FILE` does (§31).

**What is new, and what is reused.** The lock a file requires is not read from a file: it is every declaration the store of the import holds (`lock_of_all`), by name, a name given twice standing for the last (a `BTreeMap` insert). It is made here as the record `read_lock` would make from a lock file with no origin, and `clipublish.walk_required` (the part of `require_walked` that comes after the lock is read and its store found) takes it as it takes one from a file. `clipublish.publish_walked` is the rest of `publish_requires`. So `publish FILE` and `publish --dir` share every step that writes a store.

**Differences.** As for `publish` (§31): `--cache DIR` and `--std-dir DIR`; the words of the operating system after a path are not reproduced; a file must be UTF-8 for the Rust command and is read as bytes; the order of imports is read from the parse, so a file that does not parse is refused with the compiler's words (status 1), as is a file that parses and is refused by the check when it is published.

**Measured (Linux x86-64, debug build).** `selfhost::vcs::vcs_publish_dir_in_cancho_publishes_a_library_like_cancho_vcs_publish` (about 40 s) compares status, standard output and every file left for: a chain and a diamond of modules (beside a program, a text file and a directory that are not published), published again over the stores they made, after a body changed, over a store with a stranger in it, over a store that is a file; the library (without `--std`, refused, then with it, twice); modules that import each other, themselves, and a module that is not there; one module in two files, two modules in a file, a file that does not parse, only programs, no source, a directory that is not there, one that is a file; and the default store. `...publishes_the_libraries_of_packages` (about 7 minutes) does `packages/x509` and `packages/http-client` with the library. **Mutants: 19 tried, 17 killed, 1 removed, 1 not reachable**, after four survived the first set of cases: the separator of a cycle and the wording of the other refusals (the test cleared the first line of the standard error of every refused command: it is now compared for the refusals that are not the compiler's), imports skipped in the walk (no case had a module that sorts before what it imports), and the branch for a file that declares two modules (dead: the parser refuses it first, so the branch was removed). The one left is a name given twice in the lock of a store: a store made by `publish` holds one module's declarations, whose names are not repeated, so no input to this command reaches it. The others killed: the text of `skipped` and of `==`, a module declared twice, the library's imports not left out, an import that is not a module, a cycle not seen, the stage taken for the signature in a lock, a store not removed, a store that is a file allowed, no module not refused, the default store, an empty directory not refused, and the dots of a module's path and of an import's in mode 16, and the imports of other modules.

## 34. Design: `authority`, with `pure` and `functions` (written before the code)

*Written before the code; slice (a) below is built and measured, the rest is not.* The decision (the user's, 2026-10-27) is **full parity** of `cancho authority` with the Rust command, except `folded_operators` and `folded_calls`, which an earlier decision leaves out (§17). The programs the compiler in cancho has no code for stay status 3 (§14), as for every command.

**What the Rust command reports** (`print_authority`, `main.rs`, about 230 lines; read 2026-10-27), all from `compile_to_ir`'s `Program`: `performed`, the distinct `(name, argument)` of the `performs` row of every function of `program.funcs`, sorted; `kinds`; `bounded`, from `bounds_its_domain` of each label and from `foreign_report::attribute`; `unbounded_by`; `never_touches` (`untouched_domains`, a table); `foreign_symbols`, the externs `reachable_externs` finds, sorted without repeats; **`pure`**, the names of the functions that `Func::is_pure` says are, sorted; **`functions`**, `program.funcs.len()`; and with `--target wasm32-wasip1` the imports the row licenses (`wasi_imports`, 310 lines of `cancho-ir`, a function of the kinds alone). The prose form says the same in sentences.

**What `program.funcs` is** (`cancho-ir/src/lib.rs`, pass 2): the **instances** the worklist `Mono` reaches from `main` (with no `main`, every function without type parameters), one for each `(function, type arguments)`, plus whatever a `static` body calls; nothing is inlined at this stage. An instance is named `base` with `$` and the display of each type argument after it (`defs.rs:instance_name`): `first[int, bool]` is `first$int$bool`, a type is displayed as `Unifier::display` writes it (`int`, `Job`, `[int]`, `(int, bool)`, `Vec[int]`, `Ffi("libc")`) and `[`, `]`, ` ` and `,` become `_`. **The name has no module in it**, so two functions of one name in two modules are two entries with one name. `Func::is_pure` (`ir.rs`) is: the lowered row is empty, and no parameter's type has a unique reference (`&!`) in it, looking through references, slices, tuples and the arguments of a named type (not into the members of a declared type, not into a function type).

**What the compiler in cancho has that this can be made of.** The pruned module (`prune.cho`) defines the functions of the program as `@lexs_<item>` and, for an instance, `@lexs_<item>i<k>`, `<item>` being the node of the declaration and `<k>` the instance (`instances.cho`: the declaration, the codes of the type arguments, at most four, none a reference). So the reached set is *read from the text the compiler has written*; each name gives back the declaration (its name token, its row `[`, its parameters' types as syntax) and the type arguments (as codes: a scalar, a declared type with arguments, a run, a box, a tuple of two or three, a prelude type with its literal). Measured so far, one program: `examples/hello.cho` is 2 functions for the Rust command and defines `@lexs_30` and `@lexs_68` (and `@main`, which calls `@lexs_68`, the `main` of the program) in the pruned text. **Not measured yet, and the thing this slice stands on: that the two sets are the same over the corpus.** Where they are not, the difference is a finding to be explained (a function reached only from dead code after folding, a runtime helper that is written as a `lexs_` function, a function the Rust lowering makes up), not to be hidden.

**What is derived and how.** `performed` is the union of the declared rows of the reached functions: a row is exact in this language (the 1,415 stored operations of §31 are the measurement for `publish`), but **discharged**: `main` of `hello.cho` has an empty row and the labels come from `write_all`, so the union is taken over all reached functions, not read off `main`. `pure` is `row is empty and no parameter has a unique reference`, read off the syntax of the parameters (`TRef` with the unique flag, looking through `TSlice`, `TTuple` and the arguments of a `TName`). The externs are those a reached function names; the `ffi` scope is the argument of the extern's row. The tables (`bounds_its_domain`, `untouched_domains`, `foreign_report`, `wasi_imports`) are written out in cancho as the Rust code has them, and each is tested against the Rust one by running both on every combination of labels (not only those programs have).

**Plan, in slices; each one a differential test against `cancho authority` on the programs of the repository, with mutants.** (a) driver mode 17 (`authority.cho`): the list of reached functions with name, row and purity, and the measurement of the previous paragraph: of `functions` and `pure` over the corpus, how many programs agree and why the others do not. This decides whether the rest is a port or a research problem, and is reported before (b). (b) the prose and `--output json` reports for the host. (c) `--target wasm32-wasip1`, if the command line in cancho gets `--target` (§14 says it has not). Programs of several files and `--std-dir` are in (a) from the start: the library is where the generic instances are.

**Slice (a), built and measured (2026-10-27).** The internal command `cli.cho authority-reached` (not `authority`: it writes a NUL-separated list, for the tests) is **driver mode 17**: the program is built as `build` builds it, the module is pruned, and `authority.cho` lists the `@lexs_` definitions that are left. The name is the declaration's and, for an instance, `instances.name_suffix` adds `$` and each type argument as `Unifier::display` writes it (scalars, declared types with arguments, `Box`, tuples; a prelude type is `?` for now). **Purity needed one more fact than the design had**: Rust's `Func::performs` is the set the body performed *before* what the parameters own is taken off it (`function.rs`: `let performs = performed.clone();` before `performed.discharge(&authority)`), so `main` of `hello.cho` has an empty row and still is not pure. The checker in cancho keeps that set (`st[54]`) for each function and threw it away; `effects.record` now keeps it in slot 15 of the function's item (slot 15 is used by the walk over types only for a struct and an enum). So `pure` is *the performed set is empty and no parameter has a unique reference*, and `performed` for the report is the union of those sets, **not** of the declared rows (`main` owns what it performs: a union of rows would miss `io_write` in `hello.cho`).

The 16 programs of `examples/` that have a `main` (6 of them with the library), the Rust command's `--output json` against the list: **`functions`: 16 of 16 the same count**, the generic instances named alike (`length$Job`, `size$int` in `queue.cho`). The 11 programs in which the checker followed every label: **`pure`: 11 of 11, labels: 11 of 11**. The other 5 are the 5 programs for which the Rust command reports a label *with an argument* (`fs_read("")` and `fs_write("")` of an `Fs` that was not narrowed, `fs_read("/tmp")`, `tty_open("/dev")`, `ffi("libc")`), which the checker in cancho marks as not followed (`effects.mark_arg`) without saying which: the 5 and the 5 are the same programs. So the open problem of this slice is exactly one: **the labels with an argument**. Their argument is the literal a capability was narrowed to (and the empty string for one that was not), or, for `ffi`, the library of the foreign declaration's own row. Slice (a2) is to keep them where the capability is used (`atoms.push_prefix`, `push_call_bound`, `extern_call` are the places the checker already marks them), measured the same way; a program for which a label is still not followed answers status 3, as the programs this compiler has no code for do.
