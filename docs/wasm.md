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

Status: **W0, W0.1 and the errno decision are built** (§W0 results). W1 onward is the plan below.

---

## W0 results

`lex-sys run examples/hello.ls --target wasm32-wasip1` prints `Hello, world!`
under wasmtime, and `scripts/wasm_coverage.py` ran every `tests/accept`
fixture for the target. This is the honest map (104 fixtures), run with
`WASMTIME_FLAGS=--dir=/` because several fixtures open `/` as their
capability; that grant is the harness's, not the compiler's:

| | count | meaning |
|---|---|---|
| **pass** | 79 | built, ran, stdout and exit code match the fixture's `//~` annotations |
| **refused** | 15 | the toolchain declined; see below. Today these are *link* errors, not located refusals |
| **wrong** | 10 | built and ran and disagreed with the annotations. Each is a bug or a missing target arm |
| trap | 0 | no accept fixture expects a trap |

The first run was 35 / 9 / 60. One cause, `size_t`, was behind 38 of the 60;
the OS-constant tables (W0.1) took 73 to 78 and cleared `slicing`, and the errno
translation took it to 79.

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
  `write`, `memchr`, ... (11 functions, ~25 call sites). `wasm-ld` treats a call
  whose type disagrees with the definition as a *warning* that swaps in a
  trap, so such a program **links and then dies at its first `malloc`**.
  `wasm32_size_t_shims` (a post-pass over the emitted text) declares each with
  its real signature and routes calls through a wrapper that clamps a size
  above `u32::MAX` (so the allocation fails and traps, rather than silently
  asking for a few bytes) and widens results back. **This is scaffolding**:
  the permanent fix is threading the target's `size_t` through the call
  sites.
- **`wasm-ld --fatal-warnings`.** Because of the above, the linker is run
  with it, so every remaining mismatch is a build failure naming the symbol
  (`function signature mismatch: write`) instead of a trap at run time.
  Two of the 15 refusals are this working.
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
- **An operating system with no tables is refused**, in `emit_module`,
  instead of taking the Linux numbers. (`x86_64-unknown-freebsd` used to
  build.) The per-site `_ => linux` arms that remain are behind that guard.
- `run` uses `WASMTIME` (default `wasmtime`) and passes `WASMTIME_FLAGS`
  (e.g. `--dir=.`). A WASI module gets **no** directory unless one is
  granted, so the grant is spelled where it is made.

Environment: `CLANG` (a clang with the wasm32 target, e.g. Homebrew's `llvm`;
Apple's has none), `WASM_LD`, `WASI_SYSROOT` (a wasi-libc sysroot holding
`lib/wasm32-wasip1/crt1-command.o`), and `wasmtime`.

### The 15 refused

| cause | fixtures | what it is |
|---|---|---|
| `__multi3` undefined | `borrowed_fields`, `collections`, `f32_text`, `math_floats` | checked 64-bit multiply lowers to a libcall on wasm32; needs the target's compiler-rt builtins (`libclang_rt.builtins-wasm32.a`), which this sysroot does not carry. **Not installed here**; installing `wasi-runtimes` (or wasi-sdk) is the likely fix, or an inline checked multiply |
| sockets | `connect_a_refused_address`, `listen_accept_bad_fd`, `spawn_owned_net`, `process_spawn` (`recv`) | no `wasip1` sockets (§Builtin coverage); a located refusal is the intended end state |
| `extern fn` signature | `foreign_narrow_return` (`access`), `bytes_to_c` (`write`), `opaque_pointer` (`fdopen`), `spawn_parallel_sleep` (`usleep`), `foreign_two_libraries` and `spawn_thread_ids` (`pthread_self`) | a program's own `extern fn` declares C's `int`/`long` as `i64`; wasi-libc's is `i32` (or the function does not exist). The declaration is a claim about a native ABI |
| `pthread_sigmask` | `signals_claim` | signals: refuse |

### The 10 wrong

| fixtures | likely cause |
|---|---|
| `fork_clock_workers`, `fork_heap_workers`, `spawn_heap_in_struct`, `spawn_join`, `spawn_join_operands`, `spawn_owned_clock`, `spawn_owned_file`, `spawn_owned_io`, `spawn_struct_ref` | threads (`pthread_*`): trap at run time. Must become located refusals |
| `index_of_byte_beside_own_memchr` | a program that defines its own function named `memchr` replaces wasi-libc's, whose internals call it with a 32-bit `size_t`. Real on any target, but only visible once the widths differ |

Cleared since the first map: the file and directory fixtures (all but
`spawn_owned_file`, which is a thread test), `slicing` (its exit 1 was the same
read-only flag), and `directory_handles` (its `e != 2` was the errno numbering,
now translated).

### Findings that change the plan

1. **`_ =>` arms fall through silently.** Every OS `match` in the backend is
   `Darwin(_) => ..., _ => linux`. A WASI target quietly took the Linux arm.
   `hello` worked only because wasi-libc happens to export `stdout`, `stderr`
   and `__errno_location`. **Partly fixed:** the file and directory tables are
   an exhaustive `match` on `Os`, and `emit_module` refuses any OS that is not
   Linux, Darwin or WASI. Still open: the `is_darwin()` sites for sockets,
   signals, the poller and processes, which on WASI still read Linux numbers
   until W0.2 turns them into refusals.
2. **A link error is not a refusal.** 9 of the 15 above are messages from
   `wasm-ld`, not located errors. `check --target` should reject a builtin
   with no WASI meaning at lowering time, with its source location, before a
   linker is involved. That is the table in §Builtin coverage, and it should
   be data a test checks, not prose.
3. **`--fatal-warnings` is the cheapest safety net on the table**, and
   worth keeping even after the `size_t` fix: it is what stops a *future*
   libc call with a hardcoded width from shipping as a trap.
4. **The language had no portable errno**, and now has one on WASI: see "`errno` is translated" above. Darwin remains raw.
5. The roadmap's slice-length offset (`i64 8`, `expr.rs`) was **not** what
   broke first; `size_t` was. It is still untested because no fixture has
   exercised it separately yet. W1's corpus should.

---

## Milestones

| Milestone | What | Acceptance |
|---|---|---|
| **W0** -- spike | `--target wasm32-wasip1`, `examples/hello.ls` runs in wasmtime | **Done**: one example prints the right bytes; conformance map above |
| **W1** -- a real command | A stdin→stdout JSON filter on `std.json` | Imports are a **documented, measured baseline** plus `fd_read`/`fd_write`/`proc_exit` (see below); byte-identical output vs. the native build over a fixture corpus |
| **W2** -- the authority check | `lex-sys authority --target wasm32-wasip1` cross-checked against the module | A test that fails if the module imports anything the row does not explain, **and** if a label has no import (the rows are exact both ways); JSON report carries the import list |
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
`--backend` gaps already follow -- never a link error or a crash. **W0 does not
meet this yet** (finding 2).

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
