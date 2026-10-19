# Bootstrap and self-hosting

cancho's compiler started in Rust, because Cranelift is a Rust library and the first
milestone had to prove a code generator. It is now being written in cancho, stage by
stage, with the Rust compiler as the check each stage is held to. This page says where
that stands (2026-10-15) and how to use it; [`self-hosting.md`](self-hosting.md) has the
stage-by-stage record and the measurements, and the plan to finish is epic
[#491](https://github.com/alpibrusl/cancho/issues/491).

## Where it stands

- **The compiler in cancho** is `examples/selfhost/`: about 22,000 lines of cancho. It reads
  a program and writes LLVM IR; `clang` finishes the build.
- **It is the default.** `cancho build`, `run` and `test` write the code with it on macOS and
  Linux (x86-64 and aarch64), after the Rust front end has checked the program (see below). It is built once, from sources
  compiled into the `cancho` binary, and cached; nothing has to be next to the binary. A build it
  cannot do (it has no code for a construct, or it fails) is done by the Rust compiler, and the
  Cranelift backend and WebAssembly stay the Rust compiler's. `CANCHO_COMPILER=rust` asks for the Rust
  compiler alone.
- **It compiles itself to a fixed point**, on three systems: built on macOS, on Linux x86-64
  and on Linux aarch64 (a Raspberry Pi 5), it writes byte-identical IR for its own source.
- **It builds real programs.** The whole conformance suite passes with every build going
  through the default (710 tests on macOS aarch64), and CI runs the suite through the compiler in
  cancho alone, with no Rust check and no fallback (`CANCHO_COMPILER=auto`), on macOS, Linux x86-64 and
  Linux aarch64, minus the few differences listed in `.github/selfhost-known-differences.txt`.
  The projects `cancho-cache`, `cancho-oci`, `cancho-robot`, `cancho-gpu` and `cancho-hooks`
  build with it.
- **It nearly refuses what the language refuses.** Of the 337 programs in `tests/reject/` that are not a
  property of a target (marked `//~ TARGET`), it refuses 333 with the rule the Rust compiler gives; the 4 left
  are listed in `reject_gaps.txt`: a row in the
  type of a function value, what an owned capability discharges by path, and a region settled by inference
  ([#512](https://github.com/alpibrusl/cancho/issues/512)). That is why the default still lets the Rust front end
  check first, and it stops when the list is empty (a test says so).
- **A refusal says why without the Rust front end** when the compiler in cancho is named alone:
  where (`file:line:column`), the sentence of the rule (`Rule::explanation`, generated into the
  compiler's tables), the rule, the line and what of it. The sentence is the rule's, the same for every
  program it refuses; the names and types in the Rust compiler's messages are a later stage.
- **Only code generation goes through it.** `check`, `authority`, `ids`, `fmt`, `vcs` and the
  Cranelift and WebAssembly backends are still the Rust compiler's. Windows is not a target of either compiler.

## Using it

```sh
cancho build program.cho --std -o program               # the default
CANCHO_COMPILER=rust cancho build program.cho -o program  # the Rust compiler alone
CANCHO_COMPILER=auto cancho build program.cho -o program  # the one in cancho alone: what it refuses, no fallback
cancho build --compiler ./some-compiler program.cho       # a compiler you built yourself
```

The first build takes a few seconds more: it builds the compiler in cancho (with the Rust one) into the cache,
keyed by a hash of its sources (`$CANCHO_CACHE`, or `~/.cache/cancho`). `CANCHO_SELFHOST_SOURCES=<dir>` builds
it from a directory instead, for working on it. Named alone, a construct it has no code for yet is a refusal that
says so, at that construct.

## What is left

Tracked in [#491](https://github.com/alpibrusl/cancho/issues/491): the last four refusals; the other commands
(`check`, `fmt`, ...); the Cranelift and WebAssembly backends; the runtime text that is still generated from the
Rust crates; keeping golden outputs for when the Rust compiler is gone, and a seed that needs no Rust; compile time
and whether Windows is a target.

**Code quality, measured once** (Linux x86-64, `tests/programs/tls_prims_bench.cho` built with `--std`, instructions
a call under valgrind as `scripts/tls_perf.py check` counts them, against the baseline recorded for the Rust
compiler): the Rust compiler is within 1.06 of the baseline on all 36 rows; the compiler in cancho is 1.26 to 1.32
times it on the P-256 rows (`p256_shared` 1.32, `p256_public` 1.30, ECDSA 1.26 to 1.28), about the same on the
AES-GCM, ChaCha20-Poly1305, HKDF, HMAC, SHA-2 and X25519 rows (0.97 to 1.13) and 4.97 on `calibrate` (a loop of a
single instruction a step for the Rust compiler, 5,007 instructions a call against 1,008). The binary is 360,208
bytes against 318,656. So the `tls-perf` job builds with `CANCHO_COMPILER=rust`: its baseline is the Rust compiler's
code, and it guards that. Closing this gap, or recording a baseline of the compiler in cancho, is code quality's
share of #491.

# History: the v0 compiler

Decisions M0 had to settle to exist, recorded here so they are not re-litigated
by accident. Each was an open question in [#1](https://github.com/alpibrusl/cancho/issues/1).


### Bootstrap host language: Rust

The v0 compiler is written in Rust. The alternative was hosting it in current
Lex.

Rust wins for one reason that outweighs the rest: **Cranelift is a Rust
library.** Hosting in Lex means reaching a code generator through FFI on day
one, which is the part of the pipeline M0 exists to prove. 

This was a bootstrap decision, not a permanent one: the compiler is now being written in
cancho itself (above), and the Rust one is the check it is held to.

### File extension: `.cho`

`.cho` over `.lxs`. Shorter, and the collision risk (LiveScript) is not one this
ecosystem will meet in practice. `cancho build hello.cho`.

### Layout of the Rust compiler

```
crates/cancho-syntax    lexer, canonical-shaped AST, parser
crates/cancho-ir        resolution and well-formedness checks; the M0 IR
crates/cancho-codegen   Cranelift lowering, native object emission
crates/cancho           the CLI
examples/                programs that are meant to be read
tests/accept             fixtures that must compile and run
tests/reject             fixtures that must be refused, each stating why
```

Everything the compiler can refuse, it refuses in `cancho-ir`. The backend
takes IR that has already been proven well-formed, so Cranelift lowering has no
error path for a *program* — only for the environment.

### The M0 surface

> Historical: this is what M0 shipped. M1 adds `bool`, `&&`, `||` and `!`, and
> the rules below are unchanged by it.

One type (`int`, 64-bit signed), functions and calls, arithmetic, comparison,
`if`/`else`, `while`, and `let`/`var` bindings. That is the whole language.

Rules M0 already enforces, because they were cheaper to write than to retrofit:

- Every path through a function must `return`. The check is structural and
  conservative: a `while` never counts as a terminator, even `while 1 { }`.
- Code after a `return` is refused rather than silently dropped.
- `let` is immutable and so are parameters; `var` is the mutable binding.
- A binding may shadow one from an enclosing block, but not one in its own.
- An initialiser cannot see the binding it initialises: `let x = x;` reads an
  outer `x` or is refused.
- Functions are not values. M0 has no closures and no function pointers, so a
  bare function name is an error rather than a mystery.

### Things that are scaffolding, and what replaces them

| Scaffold | Why it exists | Replaced by |
|---|---|---|
| `putchar` as a compiler builtin | A program must be able to produce output before there is FFI | M2 made output an effect that must be granted: `putchar` now takes an `&!i Io`. It stays a builtin because `Io` is the console rather than a named library; an `extern fn` reached through `Ffi("libc")` is what replaces it once there are strings to write |
| Wrapping arithmetic on `+`, `-`, `*` | Overflow semantics are still an open decision (#1) | M3: *Defined-behaviour arithmetic* |
| `cc` as the linker | The C runtime supplies `_start` and libc | Much later; "no Rust" is reachable long before "no C" |

Two entries have already left this table. M0 had comparisons yield `int` (`0`
or `1`) and had `if`/`while` test any integer for non-zero, both because it had
no `bool`. M1 introduced one, so a comparison has the type it always meant and
a condition is required to be that type. The convention became a rule, which is
the shape every scaffold above is meant to end in.

Division is *not* on that list. `a / b` and `a % b` trap on a zero divisor and
on `int::MIN / -1`. A trap is defined behaviour; C's answer here is not, and
that difference is the whole point (#1). `tests/` has a fixture that runs the
trap and asserts the process dies rather than continuing with nonsense.

### What the AST already does for canonicalisation

Nothing hashes the AST yet — per-unit identity is M3 work and
`docs/canonical-ast.md` is unwritten. But an AST retrofitted for hashing is an
AST whose hashes are already wrong, so the shape is settled now:

- Nodes live in arenas; **spans live in parallel side tables**. A hash walks
  the arenas and never sees a byte offset.
- Whitespace, comments and redundant parentheses die in the lexer and parser.
  Two files differing only in layout produce identical arenas — there is a test
  that asserts exactly this.
- Integer literals are stored as values, not spellings: `007` and `7` are one
  node.
- Names intern to dense indices, assigned in order of first appearance.
- Every list is a `Vec` in source order. No set, no map, no iteration order
  that depends on an address or a hash seed reaches any output.

### Exit codes

| code | meaning |
|---|---|
| 0 | success |
| 1 | the program was refused (a located diagnostic was printed) |
| 2 | the command line was wrong |
| 3 | the environment failed: no linker, unwritable output, unsupported host |

`cancho run` is the exception: it exits with the compiled program's status, so
`cancho run p.cho` and `cancho build p.cho && ./p` agree.
