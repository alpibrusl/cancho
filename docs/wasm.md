# WebAssembly target

A `wasm32-wasip1` (later `wasip2`) target for `lex-sys`, through the existing
LLVM backend. **Not a new backend**: `lex-sys-codegen-llvm` already emits
textual IR and shells out to `clang -target <triple>`; wasm is one more triple
plus per-target builtin coverage.

Why it is worth doing, in one line: [`related-work.md`](related-work.md) frames
WASI as the incumbent and lex-sys as *"authority known before execution."* A
wasm build gives **both** -- the static row from `lex-sys authority`, and a
module whose import section the runtime enforces -- and makes `row ⊆ imports`
a mechanical check. Defence in depth without a Firecracker VM per unit.

Status: **W0 through W0.4, the errno decision, W1 and W2a (the console without libc stdio) are built** (§W0 results). W1 onward is the plan below.

---

## W0 results

`lex-sys run examples/hello.ls --target wasm32-wasip1` prints `Hello, world!`
under wasmtime, and `scripts/wasm_coverage.py` ran every `tests/accept`
fixture for the target. This is the honest map (104 fixtures), run with
`WASMTIME_FLAGS=--dir=/` because several fixtures open `/` as their
capability; that grant is the harness's, not the compiler's:

| | count | meaning |
|---|---|---|
| **pass** | 83 | built, ran, stdout and exit code match the fixture's `//~` annotations |
| **refused** | 20 | 16 are located `unsupported-on-target` refusals from the compiler (W0.2); 4 are still the toolchain's own message |
| **wrong** | 1 | built and ran and disagreed with the annotations: a program's own `memchr`, below |
| trap | 0 | no accept fixture expects a trap |

The first run was 35 / 9 / 60. One cause, `size_t`, was behind 38 of the 60;
the OS-constant tables (W0.1) took 73 to 78 and cleared `slicing`, the errno
translation took it to 79, W0.2 turned nine thread traps into located refusals, and
W0.3 took the four `__multi3` link errors to passes.

### What W0 changed

- `--target <triple>` on `build`, `run` and `check` (LLVM backend only;
  `--backend cranelift` with a target is a usage error, `test` refuses it for
  now). `check --target` runs the backend, so it is also the cheap way to ask
  "does this program build for wasm?".
- **`__main_argc_argv`.** A C compiler targeting wasm renames
  `main(argc, argv)` because the wasm ABI cannot give one name two
  signatures; wasi-libc's `__main_void` calls that name. Our hand-written IR
  defined `main`, and the program trapped on a weak undefined symbol before
  running a line.
- **Trap instruction.** `unreachable` on wasm32 (`trap_asm`). A trap is a
  wasmtime trap with **exit code 134**, not `SIGILL`/132: the behaviour is the
  same, the number is not (§Risks).
- **`size_t` is 4 bytes.** `lex-sys`'s `int` is `i64` everywhere, and the
  backend declared `malloc(i64)`, `fwrite(ptr, i64, i64, ptr)`, `read`,
  `write`, `memchr`, ... (11 functions, 23 call sites). `wasm-ld` treats a call
  whose type disagrees with the definition as a *warning* that swaps in a
  trap, so such a program **links and then dies at its first `malloc`**.
  **Fixed at every call site (W0.4).** The first version was a post-pass over the
  emitted text that declared each with its real signature behind a clamping
  wrapper; it worked and was labelled scaffolding. It is gone. Now
  `emit::size_ty(triple)` says `i32` on wasm32 and `i64` elsewhere, the module
  header declares all eleven functions with it, and each of the 23 call sites
  says so itself through `FuncEmitter::size_ty`, `size_arg` and `size_result`.
  `size_arg` clamps a size above `u32::MAX` rather than truncating it (so an
  allocation too large for the target fails and traps, instead of quietly asking
  for a few bytes); `size_result` widens a `size_t` or `ssize_t` back to the
  `i64` the rest of the backend works in, signed for the latter so `-1` stays
  `-1`. Native output is unchanged. A test scans the emitted wasm32 module for
  *every* sized libc call and fails on any `i64` (save `pread`/`pwrite`'s 64-bit
  `off_t`), without a toolchain. It found a site the rewrite had missed on its
  first run: one call in `fs.rs` builds its name dynamically (`@{name}(`), so a
  search for `@write(` never saw it. The coverage map did not move: 83 pass, 20
  refused, 1 wrong, the same fixtures.
- **`wasm-ld --fatal-warnings`.** Because of the above, and kept now that every site is fixed, the linker is run
  with it, so every remaining mismatch is a build failure naming the symbol
  (`function signature mismatch: write`) instead of a trap at run time.
  Two of the refusals in the first map were this working.
- **W0.1: a `Wasi` arm for the file and directory constants.**
  `lex_sys_ir::Os { Linux, Darwin, Wasi }` and `open_flags_for`,
  `dirent_layout_for`, `dirent_types`, `enametoolong_for`, `stat_layout_for`
  (the `(darwin, aarch64)` helpers Cranelift calls are unchanged wrappers).
  The values are wasi-libc's headers', pinned in `tests/os_tables.rs`, and two
  of them contradicted what `ir.rs` said was true of every target, now
  corrected in place:
  - `O_RDONLY` is **not zero**: it is `0x04000000`, and an open with access
    mode 0 asks for no rights and answers `EINVAL` (28). That is what the
    six file fixtures were dying of. A read-only open now ORs in
    `OpenFlags::read_only` (0 elsewhere).
  - `d_type`'s values are **not shared**: WASI's `DT_DIR`, `DT_REG`, `DT_LNK`
    are 3, 4, 7 (Linux and Darwin: 4, 8, 10).
  Also: `O_CLOEXEC` is 0 (a module cannot `exec`), `AT_FDCWD` is -2,
  `AT_SYMLINK_NOFOLLOW` is `0x1`, `struct dirent` is `{ino_t; u8 d_type;
  char d_name[]}` (`d_type` at 8, `d_name` at 9), `ENAMETOOLONG` is 37, and
  `struct stat` happens to have Linux x86-64's offsets.
- **`errno` is translated on WASI** to the language's numbering, which is
  Linux's. A program that compares a failure's `errno` (`e == 2` for a missing
  file) now means the same thing on every target it is built for. The table is
  `lex_sys_ir::WASI_ERRNO_TO_LINUX`, all 76 of wasi-libc's `E*`, applied by a
  generated `@lexsys_wasi_errno` at the one place the backend reads `errno`
  (zero stays zero; a number WASI does not define passes through). Why this
  over per-target accessors: the language already fixes some of its own error
  numbers at Linux's (`std.dirs.einval()` is 22), so a program on WASI
  otherwise saw two numberings at once, and translating needs no change to any
  program. Two judgment calls, written in `errno.rs`: `ENOTSUP` is Linux's
  `EOPNOTSUPP` (95), and `ENOTCAPABLE`, which Linux has no errno for, is `EPERM`.
  `enametoolong_for(Wasi)` therefore answers 36, not WASI's 37. **Darwin is not
  translated**: its raw `errno` still reaches a program, which is why
  `enametoolong` has a per-OS answer. That inconsistency is older than WASI and
  is not changed here; if it should be, the same mechanism applies.
- **W0.2: what WASI cannot do is a located refusal.** A new rule,
  `unsupported-on-target` (the 58th), and `lex_sys_ir::unsupported_on_target`,
  a pass over `Program::funcs` -- which *is* the reachable set -- run by `check`
  and `build` before any code is generated, so it needs no wasm toolchain. It
  refuses at the function that reaches the builtin, once per (function,
  family), naming the first builtin found:
  `` `main` uses `spawn`, and threads do not exist on `wasm32-wasip1` ``.
  Five families, from `wasi_gap`, an exhaustive `match` over all 118 builtins
  (43 refused, 75 supported; a new builtin is a compile error until someone
  says which side it is on): **threads** (`spawn`, `join`, `fork_*`),
  **sockets** (`connect`, `bind`, `listen`, `tcp_*`, `conn_*`), **the poller**
  (`poller_*`; `poll_oneoff` is the eventual mapping), **signals**, and
  **processes and pipes**. Reject fixtures can now name a target
  (`//~ TARGET wasm32-wasip1`); `tests/reject/spawn_on_wasi.ls` is the first.
  What it does not cover: an `extern fn` names a C symbol, and whether that
  symbol exists on the target is the linker's to say, which is the one place a
  target gap is still a link error (the four `extern fn` rows below).
- **W0.3: `__multi3` is gone.** On wasm32 LLVM lowers a 64-bit
  `smul.with.overflow` through a 128-bit multiply, a call to `__multi3` in
  compiler-rt's builtins, which the wasi-libc sysroot does not ship, so four
  fixtures (`borrowed_fields`, `collections`, `f32_text`, `math_floats`) failed
  to link. The alternative to an inline multiply was installing compiler-rt
  builtins (`wasi-runtimes` or wasi-sdk); that would have made every user of the
  target need one more package. Instead the module defines
  `@lexsys_smul_overflow`, the same answer from 32-bit halves, used only on
  wasm32 (`checked_arith`, the one place a checked multiply is emitted;
  native output is unchanged). It is checked against LLVM's own intrinsic on the
  host over the 400 pairs of twenty edge values (both extremes, the square root
  of `i64::MAX` either side, 2^32 either side) and two million random pairs of
  random magnitude: the overflow flag always, the value wherever it is defined.
  A second test breaks the algorithm on purpose and requires the harness to
  notice, because a differential test that cannot fail proves nothing. Cost is
  unmeasured; W3's overhead table is where it gets measured.
- **An operating system with no tables is refused**, in `emit_module`,
  instead of taking the Linux numbers. (`x86_64-unknown-freebsd` used to
  build.) The per-site `_ => linux` arms that remain are behind that guard.
- `run` uses `WASMTIME` (default `wasmtime`) and passes `WASMTIME_FLAGS`
  (e.g. `--dir=.`). A WASI module gets **no** directory unless one is
  granted, so the grant is spelled where it is made.

Environment: `CLANG` (a clang with the wasm32 target, e.g. Homebrew's `llvm`;
Apple's has none), `WASM_LD`, `WASI_SYSROOT` (a wasi-libc sysroot holding
`lib/wasm32-wasip1/crt1-command.o`), and `wasmtime`.

### The 20 refused

| cause | fixtures | kind |
|---|---|---|
| threads | `fork_clock_workers`, `fork_heap_workers`, `spawn_heap_in_struct`, `spawn_join`, `spawn_join_operands`, `spawn_owned_clock`, `spawn_owned_file`, `spawn_owned_io`, `spawn_owned_net`, `spawn_parallel_sleep`, `spawn_struct_ref`, `spawn_thread_ids` | **located** (W0.2) |
| sockets | `connect_a_refused_address`, `listen_accept_bad_fd` | **located** (W0.2) |
| processes and pipes | `process_spawn` | **located** (W0.2) |
| signals | `signals_claim` | **located** (W0.2) |
| `extern fn` signature | `foreign_narrow_return` (`access`), `bytes_to_c` (`write`), `opaque_pointer` (`fdopen`), `foreign_two_libraries` (`pthread_self`) | toolchain. A program's own `extern fn` declares C's `int`/`long` as `i64`; wasi-libc's is `i32` (or the function does not exist). The declaration is a claim about a native ABI, and only the linker can say |

### The 1 wrong

| fixture | likely cause |
|---|---|
| `index_of_byte_beside_own_memchr` | a program that defines its own function named `memchr` replaces wasi-libc's, whose internals call it with a 32-bit `size_t`. Real on any target, but only visible once the widths differ |

Cleared since the first map: the file and directory fixtures, `slicing` (its exit 1 was the
read-only flag), `directory_handles` (the errno numbering), and the nine thread fixtures,
which are now located refusals rather than run-time traps.

### Findings that change the plan

1. **`_ =>` arms fall through silently.** Every OS `match` in the backend is
   `Darwin(_) => ..., _ => linux`. A WASI target quietly took the Linux arm.
   `hello` worked only because wasi-libc happens to export `stdout`, `stderr`
   and `__errno_location`. **Partly fixed:** the file and directory tables are
   an exhaustive `match` on `Os`, and `emit_module` refuses any OS that is not
   Linux, Darwin or WASI. The `is_darwin()` sites for sockets, signals,
   the poller and processes still read Linux numbers on WASI, but W0.2 refuses
   their builtins first, so nothing reaches them.
2. **A link error is not a refusal.** *Fixed for builtins (W0.2):*
   `check --target` rejects a builtin with no WASI meaning with its function's
   source location and a rule tag, before a linker is involved, and the table
   is code a test checks (`wasi_gap`, exhaustive). Still link errors: `extern
   fn` signatures, 4 fixtures (`__multi3` was the other 4: W0.3).
3. **`--fatal-warnings` is the cheapest safety net on the table**, and
   worth keeping even after the `size_t` fix: it is what stops a *future*
   libc call with a hardcoded width from shipping as a trap.
4. **The language had no portable errno**, and now has one on WASI: see "`errno` is translated" above. Darwin remains raw.
5. The roadmap's slice-length offset (`i64 8`, `expr.rs`) was **not** what
   broke first; `size_t` was. It is still untested because no fixture has
   exercised it separately yet. W1's corpus should.

---

## W1 results

`tests/programs/json_roundtrip.ls` -- parse standard input with `std.json`, write it
back, or print `E <code> <position>` if the document is refused -- built for the
host and for `wasm32-wasip1` and fed the same documents by
`scripts/wasm_json_differential.py`. It is a real program (128 lines) that exercises
the heap, arenas, boxed slices and bytes, which is where a pointer-width bug would
show, and `std.json`'s float parser and printer, where a numeric one would.

| | |
|---|---|
| documents | **1,585**: ten valid seeds, 600 mutations of them (a byte replaced, inserted, removed, swapped, or the document truncated), and 975 numbers (every exponent, 17-digit and 20-digit decimals, and the halfway cases between adjacent doubles that a floating-point shortcut gets wrong) |
| accepted, byte-identical | 1,102 |
| refused, byte-identical | 483 (the error code and position included) |
| **different** | **0** |

Not done: the determinism check *across two hosts* (this is one machine), and the
traps (a trap on wasm is exit 134, measured at W0).

### The import list is not the one the roadmap expected

The roadmap said a console filter would import exactly `fd_read`, `fd_write` and
`proc_exit`. It imports nine. Measured per effect, with one tiny program for each
(`wasm-ld --why-extract` says which libc object pulls in which):

| a program that | imports |
|---|---|
| does nothing, or uses the heap, or reads `args` | `args_get`, `args_sizes_get`, `proc_exit` |
| writes the console (`[io_write]`) | those, plus `fd_write`, `fd_fdstat_get`, `fd_seek`, `fd_close`, `clock_time_get` |
| reads the console (`[io_read]`) | those, plus `fd_read`, `fd_seek`, `fd_close`, `clock_time_get` |

- **The heap costs nothing**: `malloc` grows linear memory with `memory.grow`, which
  is an instruction, not an import.
- **`args_get` and `args_sizes_get` are imported by every program**, even one that
  released its `args` capability, because `main(argc, argv)` is the entry and
  wasi-libc's `__main_void` fetches them before calling it.
- **Four imports come from stdio, not from anything the program does.**
  `fd_fdstat_get` is `isatty` on stdout, `fd_seek` is `__stdio_seek`, `fd_close` is
  `__stdio_close`, and `clock_time_get` is `clock_gettime` called from a futex
  timeout in stdio's locking. The backend writes the console with `putchar`,
  `getchar` and `fwrite`, so a program that merely prints brings all four.

### What that means for W2

`clock_time_get` is the one that matters. A program whose row is `[io_write]` has
**no `clock` label**, yet its import section hands the runtime a clock to grant it.
The module is not more capable in practice (only libc's locking calls it), but the
point of the exercise is that **the import section and the row agree**, and here
they cannot while the console goes through libc stdio: `row == imports` is false
for every program that prints.

Two ways out, and W2 has to pick one:

1. **Map each `io_*` label to its measured import set** and accept that `io_write`
   also licenses `clock_time_get`. Cheap, and the table is data a test checks. The
   cost is the over-grant: the check would pass a module that really did read the
   clock, because the label that "explains" it is `io_write`.
2. **Do not use libc stdio on wasm**: declare `fd_write` and `fd_read` directly
   (`wasm-import-module` / `wasm-import-name` attributes on the IR declarations) and
   give the module its own console buffer, flushed when full, at `flush_out` and at
   exit, which is what libc does to a pipe anyway. Then the imports are exactly
   `fd_write` for `io_write`, `fd_read` for `io_read`, and nothing else, and the
   equality check is honest. The cost is owning a buffer and its flush-at-exit.

**Recommendation: 2, built as W2a below.** The whole value of the target is that the import section is
the enforced half of the same fact the row states; an over-grant the check blesses
by construction defeats that. The `args` pair remains (it is startup, not stdio),
documented as the unlabelled baseline with `proc_exit`, until a custom entry point
that reads `args` only when asked is worth writing.

---

## W2a results: the console without libc stdio

`lex-sys-codegen-llvm/src/wasi_console.rs` is the console written against the two WASI
calls it needs. On wasm32 `putchar`, `getchar`, `write_bytes`, `write_err` and `flush_out`
no longer touch libc: the module declares `fd_write` and `fd_read` itself
(`wasm-import-module` / `wasm-import-name` attributes on the IR declarations), keeps its
own buffers, and `main` flushes stdout when it returns. Native output is unchanged.

- **stdout** is buffered (4 KiB), flushed when it fills, on `flush_out`, and when `main`
  returns, which is what libc does to a pipe, so the bytes and their order are the native
  build's. A write that fails sets a sticky error, as the `FILE`'s indicator does, and
  `flush_out` reports it: the failed flush's errno translated to the language's numbering,
  or `EIO` (5) for an earlier failure a later flush did not see.
- **stderr** is unbuffered. **stdin** is read 4 KiB at a time; end of input or a failed
  read is `-1`.
- Everything is `internal`, so a program that uses none of it carries none of it. The one
  thing that is not free is the flush at exit, which is emitted **only for a module that
  writes**: an unconditional one would make every pure program import `fd_write`.

### Imports, before and after

Measured by `scripts/wasm_console_check.py`, one tiny program per effect, with the import
set required to be **exact**:

| a program that | W1 (libc stdio) | now |
|---|---|---|
| does nothing, uses the heap, or reads `args` | `args_get`, `args_sizes_get`, `proc_exit` | the same |
| writes stdout | + `fd_write`, `fd_fdstat_get`, `fd_seek`, `fd_close`, `clock_time_get` | + `fd_write` |
| writes stderr | (as stdout) | + `fd_write` |
| reads stdin | + `fd_read`, `fd_seek`, `fd_close`, `clock_time_get` | + `fd_read` |
| reads and writes | | + `fd_read`, `fd_write` |

`clock_time_get` is gone from every console program: a row of `[io_write]` no longer comes
with a clock. The JSON filter went from nine imports to five.

### Still correct

The differential from W1, with large documents added so the buffer's spill and direct-write
paths run (strings whose output lands either side of 4,096, 8,192 and 12,288 bytes, a
6,000-element array, a 2,500-key object, a 400-deep nest): **1,617 documents, 1,131
accepted and 484 refused byte-identical, 2 trapped on both builds, 0 different.** The two
that trap are strings over 64 KiB, which exhaust the 64 KiB arena `json_roundtrip.ls` uses
and trap natively too; the script counts a native SIGILL and a wasmtime trap (exit 134) as
the same behaviour and requires both. The accept-fixture map did not move (83 / 20 / 1).

`flush_out` is held to `checked_output.rs`'s own probe: a small and a 100,000-byte write to
a live reader answer `Ok` and the bytes arrive, and a reader that has left answers `EIO`
and keeps answering it.

### What this does not establish

- **`ENOSPC` and `EBADF`.** A Mac has no `/dev/full`, and a closed stdout (`>&-`) did **not**
  make the write fail under wasmtime (the probe answered `Ok`); I did not establish why, so
  the `EBADF` case is untested for the module. A Linux run, with `/dev/full`, is the way to
  cover both.
- **A reader that left is `EIO`, not `EPIPE`.** wasmtime answers a write to a pipe whose
  reader left as "success, zero bytes written", and the console treats no progress as `EIO`.
  A native run says `EPIPE` where `SIGPIPE` is ignored (`checked-output.md` §2). That is the
  host's behaviour, and the console reports the honest thing it can.
- **The `args` pair is still unlabelled startup.** `args_get` and `args_sizes_get` are
  imported by every program, because wasi-libc's `__main_void` fetches them before calling
  `main(argc, argv)`, so a program that released its `args` capability still imports them.
  Removing them needs an entry point of our own that reads `args` only when asked: W2b.

---

## Milestones

| Milestone | What | Acceptance |
|---|---|---|
| **W0** -- spike | `--target wasm32-wasip1`, `examples/hello.ls` runs in wasmtime | **Done**: one example prints the right bytes; conformance map above |
| **W1** -- a real command | A stdin→stdout JSON filter on `std.json` | **Done**: byte-identical to native over 1,585 documents; the import list is measured, and is wider than the row (§W1 results) |
| **W2** -- the authority check (**W2a, the console without libc stdio, is done**; the table and the check are next) | `lex-sys authority --target wasm32-wasip1` cross-checked against the module | A test that fails if the module imports anything the row does not explain, **and** if a label has no import (the rows are exact both ways); JSON report carries the import list |
| **W3** -- a pure library | `packages/x509` verify as a **reactor** module (exports, no `main`) | Import section is empty beyond memory; results identical to native on the x509 conformance fixtures; overhead measured |
| **W4** -- components | `wasip2`, one WIT `resource` mapped to a `res` handle | `wasi:filesystem` descriptor as a linear handle, `own`/`borrow` ↔ `res`/`borrow`; then `examples/serve` as `wasi:http` |

W0–W2 are the thesis. W3 is the showcase. W4 is where the design question
lives and should not start until W2 is green. **Name who asks for W3 and W4
before starting them** (`AGENTS.md` §7).

On W1's imports: the backend emits `fwrite`, so wasi-libc's stdio comes with
it, and `crt1-command` typically adds argument and environment imports
(`args_get`, `environ_get`, probably `fd_seek` and `fd_fdstat_get`). Measure
the real list with W1's module and call it the baseline; or skip libc and
declare the WASI imports directly in the emitted IR, which makes the
label-to-import table exact at the cost of owning an allocator. W0's map says
which is cheaper.

---

## W0 -- spike (as planned)

1. **Triple plumbing.** `--target` on `build`/`run`/`check`. Built.
2. **OS match arms.** `emit.rs` and `body/*.rs` switch on
   `triple.operating_system` for `stdout`/`stderr` symbols, `errno`, and
   several libc names (`emit.rs`, `body/expr.rs`, `files.rs`, `fs.rs`,
   `net.rs`, `sockets.rs`). Add a `Wasi` arm to each, **make the match
   exhaustive**, and let any arm with no WASI meaning become a refusal.
   *Done for files and directories (W0.1); sockets, signals, the poller and
   processes are W0.2.*
3. **Pointer width.** `int` is `i64` and does not change; pointers and
   `size_t` become 4 bytes on `wasm32`. `size_t` is handled by the shim
   above. Still to audit: the slice length at `getelementptr i8, ptr …, i64 8`
   (`body/expr.rs`, ~line 191), the four `ptrtoint … to i64` sites (a
   widening on wasm32, so probably harmless), and every hand-laid libc struct
   (`alloca i8, i64 16` for `timespec`, `sockaddr`, `sigaction`, poller
   events). Fallback: `wasm64` first. The review of this roadmap recommends
   **against** it: it undercuts "runs anywhere".
4. **Linking.** `wasm-ld` against wasi-libc with `crt1-command.o`, driven from
   the CLI's `link_wasm` (the object comes from `clang`; the final link is
   the CLI's, not the backend's).
5. **Coverage map.** Done once, above. Re-run with
   `WASMTIME_FLAGS=--dir=/ python3 scripts/wasm_coverage.py <lex-sys>`.

## W1 -- a real command

- Target: a jq-lite over `std.json` (`std/json.ls`), reading stdin, writing
  stdout. Row `[io_read, io_write]`.
- Exercises heap, arenas, boxed slices and bytes -- exactly where a pointer-width
  bug would surface.
- **Determinism check:** same module hash + same input ⇒ same output bytes,
  across wasmtime on two hosts. Canonical NaN (`CANONICAL_NAN`,
  `CANONICAL_NAN_32`) already covers the one place wasm is nondeterministic,
  if the filter touches floats.
- **Traps:** checked arithmetic, bounds and division traps must surface as
  wasm traps with a non-zero exit, matching native exit semantics where
  wasmtime allows (document where it doesn't). Measured so far: wasmtime
  exits 134 on `unreachable`.

## W2 -- the authority check

- Map each effect label to the WASI imports its builtins lower to
  (`io_write` → `fd_write`, `fs_read` → `path_open`/`fd_read`, …). This table
  is the spec; generate it from the builtin definitions rather than
  hand-writing it, and keep it as data inside the codegen crate with a
  completeness test, in the spirit of `every_rule_has_a_fixture`.
- `lex-sys authority --output json` gains `imports: [...]` for wasm targets.
- A conformance test: build every accept fixture for wasm, assert
  `imports(module) ≈ map(row(main)) ∪ runtime_baseline`, **equality** rather
  than subset, since effect rows are exact in both directions. Needs
  `--gc-sections` so dead code does not inflate the import list. Anything
  outside is a failure, not a warning.
- Document the residual: the allocator and `proc_exit` are baseline imports
  with no label. State them once, in `authority.md`.

## W3 -- a pure library (reactor)

- Needs a **library mode**: no `main`, `pub` functions with scalar/slice
  signatures exported. Today there is no export surface, and this is a
  language-design project hiding inside a milestone: design it in `docs/`
  first (what an exported signature may mention; capabilities may not cross
  the boundary, so exported functions must have row `[]` or take only
  host-provided handles). Start that document while W2 is in flight.
- Target: `packages/x509` verify. Row `[]`, so the module imports nothing but
  memory. Claim to demonstrate: *a certificate verifier that provably touches
  nothing, statically and at runtime.*
- **Measure** against native. Crypto is arithmetic-bound, which
  [`overflow-cost.md`](overflow-cost.md) puts at +40.5% for checked arithmetic
  natively; wasm may add its own cost on top. Publish the number either way.
- Host demo: a tiny JS or Rust embedder calling `verify`.

## W4 -- components (wasip2)

- Switch target to `wasm32-wasip2`; emit a component (`wasm-tools component new`
  or wasi-sdk's native p2 support).
- **The design question:** WIT `resource` + `own<T>`/`borrow<T>` is `res` +
  `borrow`. One resource first -- a `wasi:filesystem` descriptor -- and check
  that the linearity rules, `release`, and the authority report all carry over
  without a special case. If they need one, write down why before going
  further.
- **What it may fix in [`reach.md`](reach.md):**
  - *"A library is not an authority domain"* -- a WIT interface is one. Rows
    could say `wasi:sockets/tcp` rather than `ffi("libc")`.
  - Opaque foreign results -- WIT handles are opaque by construction; check
    against what [`opaque-pointers.md`](opaque-pointers.md) already settled
    natively.
- Then: `examples/serve` as a `wasi:http/incoming-handler` component.

---

## Builtin coverage

Expected, to be confirmed by W0's map (the map above already confirms the
sockets, threads and signals rows).

| Area | wasip1 | wasip2 | Note |
|---|---|---|---|
| Console (`io_read`/`io_write`) | yes | yes | |
| Files, directories, `openat`, `pread`/`pwrite` | yes | yes | Preopens replace absolute paths; `Fs` path prefix ↔ preopen. Constants differ (`open` flags, `stat`, `dirent`) and are tabled in `lex_sys_ir::Os`; `errno` is translated to the language's numbering |
| Heap, arenas | yes | yes | wasi-libc `malloc`; `size_t` is 32-bit |
| Clock | yes | yes | |
| `flock`, `fsync`, `rename` | partial | partial | Check per call |
| Sockets, `getaddrinfo` | no | yes | `wasi:sockets` |
| epoll / kqueue | no | `poll` | Map the poller onto `poll_oneoff` / `wasi:io/poll` |
| Processes (`posix_spawn`, `kill`) | **no** | **no** | Refuse; no WASI equivalent |
| Signals | **no** | **no** | Refuse |
| Threads (`pthread_*`) | experimental | experimental | Refuse until wasi-threads settles |
| `extern fn` to arbitrary C | link-time only | link-time only | Only libraries compiled to wasm; `Ffi("libc")` means wasi-libc, whose `int`/`size_t` are 32-bit |

Every **no** is a located refusal on that target -- the same discipline
`--backend` gaps already follow -- never a link error or a crash. **Met for
builtins since W0.2**; an `extern fn` the target lacks is still the linker's to
report.

---

## Risks

- **Pointer-width assumptions** beyond `size_t` and the known slice-length
  site. Mitigation: the coverage map, and `--fatal-warnings`.
- **Hand-laid libc structs** differ on wasi-libc. Each `alloca i8, i64 16` is
  a candidate; the conformance suite is the net.
- **Exit codes and traps.** wasmtime reports a trap as exit 134, not
  `SIGILL`; the semantic exit codes (`0/1/2/3`) must be re-stated for wasm,
  not assumed.
- **wasip2 churn.** Component tooling is still moving; keep W4 behind W2.
- **Thesis drift.** The pitch is *static* authority. Wasm must not become the
  reason the static check is skipped -- it is the second layer, not the first.
  Granting `--dir=.` to run a fixture is exactly the escalation the rows exist
  to make visible; it stays an explicit flag.

## Open decisions

1. ~~`--target` flag vs. a separate `lex-sys wasm` subcommand.~~ **Flag**:
   `compile_object_for` already takes a triple, and it mirrors `--backend`.
2. `wasm32` vs. `wasm64` as the first target. **wasm32**, by the argument above.
3. Library-mode export rules: may an exported function take a capability the
   host constructs, or only `[]` functions in W3?
4. Whether the effect-label → import table lives in the compiler or as data
   checked by a test. Leaning: data in the codegen crate plus a completeness
   test.
5. Whether rows should ever name WIT interfaces directly (W4), and what that
   does to per-unit content hashes.

## Non-goals

- A direct IR→wasm emitter. LLVM already does this.
- Browser-specific bindings (JS glue, DOM). Possible later; not the point.
- Replacing native as the primary target.
