# `cancho trace`: a debugger an agent can use, and the debug information it needs

> **Status: `-g` (§3.4) is built as line tables; variables, `trace` and the trap names are not.** Written before the code. What was
> measured is marked; everything else is a **gate** in §8, not a result.
>
> **Written in cancho.** `cancho trace` is a slice of the command line in
> cancho (`examples/selfhost/cli*.cho`), and the debug information is
> emitted by the emitter in cancho (`examples/selfhost/emit.cho` and its
> neighbours), **not** by `crates/cancho-codegen-llvm`. The Rust backend is
> not extended. §6 says what that costs.

## 1. The question

A program that traps gives an agent an exit status and, at best, an
address. A debugger is the usual answer, and an interactive one is the
wrong shape for an agent: *step, print, step, print* is many turns, each
costing a round trip and each depending on state the agent cannot see
between turns. What an agent wants is the thing it asks a person for: *"it
crashed; where, with what values?"*, in one reply.

So the design is **non-interactive**: one command, one run, one JSON
answer, then exit. No session.

## 2. Measured here (2026-11-05)

* The host has `clang` 18.1.3, `gdb` 15.1, `lldb` and `valgrind`, and no
  `perf`. A design that needs `perf` would not run in the environment this
  was written in, and `tls-performance.md` §3 already records a machine
  with no PMU.
* The Rust backend writes textual LLVM IR and has `clang` compile it with
  `-O2`, **always** (`cancho-codegen-llvm/src/lib.rs`, `run_clang`; the
  comment there says `-O2` is mandatory because `mem2reg` is what removes
  the stack traffic of every local). Nothing in the backend sets a
  frame-pointer attribute (searched: no `frame-pointer` in
  `cancho-codegen-llvm/src`). So debug information can be written as
  ordinary metadata in the text, and an optimised binary's variables are
  mostly SSA values with no home on the stack, which is the problem §3.2
  has to solve.
* `tls-performance.md` §3 records that the compiled code keeps no frame
  pointers and that DWARF unwinding gave a complete stack in all but 0.04
  to 0.7% of the samples.
* The command line in cancho builds by writing `OUT.ll` and running
  `clang` itself (`cli-in-cancho.md` §14), so the flags passed to `clang`
  are in cancho code, where this design needs to change them.

## 3. Debug information, `-g`

An opt-in flag of `build`, `run` and `test`: `-g`. Without it nothing
changes, and the output must stay byte-identical to the build without the
flag (a gate, §8.1).

### 3.1 What is emitted

LLVM metadata in the text module: a `DICompileUnit`, a `DIFile` per
`.cho` file, a `DISubprogram` per emitted function carrying the **cancho
name** (`@lexs_<item>` is not what a person wrote), and a `DILocation` per
statement. Line tables first; variables second (§3.2). The emitter needs
each node's source position, and `cancho-syntax` keeps spans in side tables
keyed by node id (`span.rs`), so whether the emitter in cancho still has
them at emission is **not known**. Gate §8.2 reads it before anything else
is written, and this document is corrected if it is false.

### 3.3 Gate 8.2 answered: do positions reach the emitter? (2026-11-05)

**Yes, as byte offsets; no, as lines or files.** The emitter in cancho
(`emit.cho`) writes LLVM text *while the checker walks* (`emit.cho` header),
and every node it has is a record with `nstart(st, id)` and `nend(st, id)`:
offsets into the program's text. `header_start(st, item, inst, ret_ty)` is
handed the function's node. It has no line and no file name: those are made
**outside** the checker, from the laid-out text and the length of each file
(`clibase.report_check` takes `names`, `laid`, `lens`), which is how
refusals get their `file:line:column` today.

Two measured facts (clang 18.1.3, this host) that change §3.1:

* **The verifier requires a `!dbg` on every call in a function that has
  debug info, when the callee has debug info too.** Measured with a two
  function module: `inlinable function call in a function with debug info
  must have a !dbg location`.
* **Clang then drops all the debug info and exits 0.** The message is a
  warning (`ignoring invalid debug info`), the object is written, and the
  line tables are gone. A partial annotation therefore does not fail; it
  silently yields a binary with no lines. Any build of `-g` must be checked
  by reading the object back (`llvm-dwarfdump --debug-line` or `gdb`), never
  by its exit status. This is gate 3 below made mandatory for every build.

**Design that follows (replaces the "per statement `DILocation` in the
emitter" of §3.1):**

1. With `-g`, the emitter writes **offset markers as comments**, which are
   legal IR and change nothing: `; @fn <offset>` on the line before a
   `define`, and `; @<offset>` at the start of each statement.
2. A **text pass in the command line** (`cidebug.cho`, beside `prune.cho`,
   which already works on `define` lines) reads the markers, turns each
   offset into `(file, line)` from `laid` and `lens`, appends `, !dbg !N` to
   **every** instruction of a function (so no call is missed), and appends
   the `DICompileUnit`, `DIFile`, `DISubprogram` and `DILocation` nodes.
3. Without `-g` no marker is written, so the module is byte for byte what it
   is now (gate 1 holds by construction, and is still tested).

Left to decide, and not decided here: **how `-g` reaches the emitter.** The
driver is run in numbered modes over one input stream each
(`cli-in-cancho.md` §1); a flag is either a new mode number or a byte in
the stream's header. The second is smaller and does not renumber anything.

### 3.4 `-g`, as built (2026-11-05)

```sh
cancho build -g [--emit obj] [-o OUT] FILE...
```

Built by the command line in cancho (`cli.cho`), not by the Rust binary,
which has no `-g`. The parts:

* `emit.cho`: `debugging(st)` and `marker(st, tag, offset)`. With `-g` the
  checker writes `  ; @fn N` after `entry:` of each function and `  ; @s N`
  before each statement (`body.cho`, `stmt`); `N` is `ast.nstart` of the
  function or statement. **Nothing is written without `-g`.**
* `driver.cho`: the target given to `run_many` is `host + 8 * wasi + 1024 *
  debug`; bit 16 of `st[55]` (which already holds which runtime support a
  program used) carries it to the emitter. **Decided: not a header byte**
  as §3.3 proposed. The header of the state array is 65 slots, every one
  used, and the token table starts at 65 in `ast.cho` and in `ingest.cho`;
  and `st[61]`, the target, is compared with `== 0` and `== 1` in eleven
  places. The `target` argument already carried a second value (`wasi`), so
  a third was the smaller change.
* `cidebug.cho`: the pass. It numbers the metadata (0 compile unit, 1
  module flags, 2 and 3 the subroutine type, 4 on one `DIFile` for each
  file, then the subprograms and locations as they are met), puts `!dbg !N`
  on the `define` of each function that has a marker, and `, !dbg !L` on
  every indented non-comment line after it, and takes the markers out. The
  name in the `DISubprogram` is read from the program's text (the word after
  `fn `),. The kind is
  `LineTablesOnly`, language `DW_LANG_C99` (there is no cancho code).
* `cliprog.cho`: the `-g` flag of `build` (and not of `check`); the pass runs
  on the module just before `clang`.

**Measured (this document, Linux x86-64, `a_program_built_with_g_has_line_tables`,
51 s):** `examples/hello.cho` built with `-g`: `gdb` stops at `break
write_all` in `write_all () at hello.cho:30` with `main () at hello.cho:52`
above it, the lines of `putchar(io` and of the call in the source. A
three-function program: `info line twice` says the line of `fn twice`; the
debug information has `add_one` and `twice`; **the disassembly of `.text` is
identical with and without `-g`** (so the claim of `agent-profile.md` gate 3
holds for the code, though not for the file: 24,600 bytes against 23,848).
Without `-g` the module is the bytes it was by construction (the markers are
not written), and the test does not compare it with an earlier compiler.

**What it does not do, and said so:**

* **Statements, not expressions.** A location is the statement's first
  token: a call in the middle of a long statement has the statement's line,
  and `column` is the statement's. An instruction before the first marker of
  a function (the `alloca`s) has the line of the function.
* **No variables.** `trace`'s `locals` needs §3.2.
* **Corrected (by `agent-profile.md` §9): every function has a subprogram.**
  This section first said a function folded into the C `main` had none, and
  the cause was that the entry wrapper and the support code had no debug
  information, so the program's code inlined into them lost its lines
  (valgrind saw `fl=???`). `cidebug.cho` now gives each `define` a subprogram
  (the support code in a file called `<cancho runtime>`, line 1), the kind is
  `FullDebug` and not `LineTablesOnly`, and every instruction that is not a
  comment or a label carries a location, whatever its indentation (the
  library's crypto code is written with none, and `clang` dropped all debug
  information for a `--std` program until that was found; a multi-line
  `switch` takes its location after the `]`). Even so the optimiser leaves
  line 0 on what it hoists or merges (`agent-profile.md` §9).
* **`-g` is `build` only**, not `run` or `test`; and not in
  `introspect`/`--help` (the same reason as `ci`, `agent-ci.md` §10).
* **Quadratic.** Each location asks `diagnose.locate` for a line, which
  counts from the start of the file. Not measured on a large program;
  `cli.cho` itself (80 files) is the case to time.
* The `directory` of each `DIFile` is `"."`: a program has no way to ask for
  the working directory without running `pwd`. `gdb` found the source by the
  relative name from where it was run.

### 3.2 Variables, and the `-O2` problem

At `-O2`, `mem2reg` turns a local into an SSA value and a debugger sees
"optimized out" for most of them. Two honest options:

| | how | cost |
|---|---|---|
| A. **trace builds at `-O0`** | the build `trace` makes passes `-O0`, with locals kept as `alloca` and `llvm.dbg.declare` | the behaviour under trace is not the optimised binary's. A bug that appears only after inlining is not seen. Checked arithmetic and bounds traps are emitted by the emitter, not the optimiser, so they stay |
| B. `-O2` with `llvm.dbg.value` | keep the optimised build | values are present only where LLVM could keep them |

**Decision: A**, because an agent's question is *what were the values*, and
an answer of "optimized out" is not an answer. The difference is stated in
the report (`"optimization": "O0"`), so the agent knows what it ran. B is
opened as a later flag, `--optimized`, if a defect is found that A hides.

**v1 describes scalars, slices as pointer and length, and records by
field.** Resources (`res struct`) are printed as their fields without
claiming ownership state. Generics print under their instance name
(`length$Jo…`, as `authority` names them).

## 4. The command

```sh
cancho trace PROG.cho [--std-dir DIR] [-- ARGS...]
    --at FILE:LINE [--hit N]     stop at the Nth arrival at that line (default 1)
    --on-trap                    stop at the first trap (default if no --at)
    --timeout SECONDS            default 30
    --max-frames N               default 32
    --output json                the only form; the flag exists for symmetry
```

It builds with `-g -O0`, runs under the host's debugger in batch mode
(`gdb --batch` on Linux, `lldb --batch` on macOS: the same split
`tls-performance.md` §3 made for `perf`), writes the command script itself,
reads the debugger's machine output, and answers once:

```json
{
  "stopped": "breakpoint",
  "optimization": "O0",
  "hits": 3,
  "at": { "function": "parse_header", "file": "src/http.cho", "line": 88 },
  "stack": [
    { "function": "parse_header", "file": "src/http.cho", "line": 88 },
    { "function": "serve_one",    "file": "src/http.cho", "line": 140 }
  ],
  "locals": [
    { "name": "n",    "type": "int", "value": "-1" },
    { "name": "line", "type": "slice[byte]", "value": { "len": 0 } }
  ],
  "exit_status": null
}
```

`stopped` is one of `breakpoint`, `trap`, `exit` (the program ended before
reaching the point, with `exit_status`), `timeout`.

### 4.1 Traps are named, not just located

A trap in this language is one of a few known causes (checked arithmetic,
bounds, division by zero, `assert` from `std.test`, an explicit `trap()`),
and the debugger cannot tell which from a `SIGILL`. The emitter gives each
trap site a `DILocation` whose scope is a one-line artificial function named
`cancho.trap.overflow`, `.bounds`, `.divzero`, `.assert`, `.explicit`. The
stack then contains the cause as a frame, and `trace` lifts it into a field:

```json
{ "stopped": "trap", "trap": "overflow", "at": { "file": "src/m.cho", "line": 23 } }
```

*Not tried.* Whether LLVM keeps an artificial inlined scope through the
`ud2` the trap lowers to is a gate (§8.4); if it does not, the cause comes
from a side table written at build time, mapping trap address to cause.

## 5. Findings

Tags are `trace-` and are a namespace of their own, apart from the 58
compiler rules (`agent-ci.md` §4 for the same reasoning).

| tag | when | exit |
|---|---|---|
| `trace-no-line` | `--at` names a line with no code; the finding carries the nearest line with code at or after it | 1 |
| `trace-not-reached` | the program ended without arriving; the report is the `exit` form | 1 |
| `trace-timeout` | `--timeout` passed | 1 |
| `trace-no-debugger` | neither `gdb` nor `lldb` found | 3 |
| `trace-refused` | the program is refused by `check`; the located refusals are passed on unchanged | 1 |
| `trace-unsupported` | the program is one the cancho compiler has no code for (§6) | 3 |
| `trace-bad-location` | `--at` is not `FILE:LINE`, or FILE is not one of the program's | 2 |

Exit **0** is a successful *answer*, including `stopped: "trap"`. A trap in
the program is the thing the agent asked to see, not a failure of the tool.

## 6. What it costs, and does not claim

* **Only programs the compiler in cancho accepts.** 80 of 82 in
  `cli-in-cancho.md` §14's measure, and `--std` only through `--std-dir`.
  The Rust compiler is not given a `-g`. A program outside that set gets
  `trace-unsupported`, not a silent fall back to a build without line
  information.
* **`-O0` is not the shipped binary** (§3.2).
* **Needs `gdb` or `lldb`.** No debugger of our own is written; building
  ptrace in cancho is a larger piece of work and `Ffi("libc")` is every
  authority at once (`reach.md` §5), which is the wrong grant for a tool.
* **A hostile program under trace runs.** `trace` executes the program. It
  holds the same authority the program was given and no more, and the
  design asks for `Exec` narrowed to the debugger and the one built binary.
  *Not measured*; `authority` of the tool is a gate (§8.6).

## 7. Why not a REPL

An interactive debugger is not forbidden; it is not for an agent. Each
turn is a model call; the state between turns lives in a process the agent
cannot inspect; and "continue" has no bound. The non-interactive form
costs one call, is repeatable, and is its own regression test: the same
`trace` command on the same program gives the same JSON, which a REPL
session does not.

## 8. Gates

1. `build` without `-g` is byte-identical before and after the change, over
   the corpus of `cli-in-cancho.md` §14.
2. **Spans reach the emitter.** Before anything is written: a program that
   prints the position of each node the emitter sees. If they do not, the
   change begins in the parser's output and §3.1 is corrected.
3. `gdb` reports the right `.cho` line for 100% of the sampled statements
   of the 82 programs (sampled by a script that compares the line table to
   the source, not by eye).
4. Each of the five trap causes, one fixture each, is reported under its
   own name on x86-64 Linux and on aarch64 (Docker's VM, as
   `tls-performance.md` §10 used).
5. `locals` equal what the program prints, on a set of fixtures that print
   their own locals at the line traced.
6. `cancho authority` of `trace` itself shows `Exec` narrowed and no `Net`,
   and the report is quoted here when it exists.
7. A fuzzer over the debugger's output (truncated, interleaved, a 64 MiB
   line) yields only a tagged finding.
8. **Cost to an agent.** The claim that this is cheaper than a REPL is a
   hypothesis: the evidence is a transcript, as `agent-errors.md` §5.1
   asks of `fix`. Until one exists this section is a bet.

## 9. Open

| Question | Why it waits |
|---|---|
| `--watch EXPR` (stop when a value changes) | a hardware watchpoint is host-specific; no asker |
| Reverse execution / replay | `rr` is Linux x86-64 only |
| Core files | a trap leaves none by default; `--on-trap` covers the case |
