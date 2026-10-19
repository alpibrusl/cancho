# An agent toolbox: unix-like tools written in cancho

> **Status: design, written before any tool code, with the probes it
> rests on run and recorded (Appendix A and B). Nothing here is built.**
> Every claim about existing code cites the file it was read from; every
> number was measured on the prebuilt compiler in the environment
> Appendix B describes, and is a probe, not a result of the protocol in
> §7. Nothing in §7.2 has been run, and no claim that depends on it is
> made anywhere in this document.
>
> This is the sequel to [`agent-tools.md`](agent-tools.md), which built
> one tool (`examples/seek/`) and asked whether the properties this
> repository already had add up to a tool better shaped for an agent's
> own loop. That document is *prior art*, not superseded: its §3.1 (a
> `narrow` takes a literal, so no flag narrows `Fs`) is load-bearing
> here, and two of its sentences are corrected below (Appendix C).
>
> The question this one asks is the one the first left open: is a
> *set* of such tools worth building, and how would anyone know?

---

## 0. The position, tested

The position under test: *an agent toolbox in cancho is a good idea only
if it is scoped (about ten tools, not a coreutils clone, POSIX flag
fidelity a non-goal) and measured (the "agent-friendly" claim needs a
protocol with a gate, not an opinion).*

**It holds, and it needs four amendments that the evidence forced.**

1. **The claim is two claims, and only one of them needs cancho.** The
   *contract* (JSON on stdout, semantic exit codes, errors with a rule
   tag and a repair hint, determinism) is language-independent: it can
   be had by putting a thin envelope around `rg`, `jq` and `sha256sum`.
   What cancho alone can add is an **authority row the compiler
   computes**, determinism by *absence of capability* (a tool cannot
   read the environment, the clock or a tty unless it was handed the
   capability), and defined behaviour on hostile input. A protocol that
   compares the toolbox only with raw GNU tools confounds the two. §7.2
   therefore has **three arms**, and the middle one (the incumbents behind
   a conforming shim) is the one that decides whether cancho earned
   anything. The position as first stated compared two things (GNU and the
   toolbox) and could not attribute a difference.

2. **"Statically known, narrow authority" is true of *kind* and false of
   *extent*.** `narrow` takes a literal (`capability-not-narrowable`,
   measured, Appendix A.1) and a function cannot be generic over a
   prefix (`Fs(p)` is refused), so a tool that takes a path from its
   command line reports `fs_read("")` — the whole filesystem — however
   confined its behaviour. What a supervisor can read off the row is
   *which verbs* (no `fs_write`, no `net_out`, no `ffi`), never *which
   directory*. Extent comes from a per-deployment build (D14), from the
   perimeter, or from a mediator that reads the arguments (D13).

3. **Today, six of about ten tools are buildable.** The three an agent
   uses most to orient itself — `ls`, `find`, `tree` — need a directory
   listing that does not exist (`fs_list` is `not-a-function`, A.2);
   `grep` with a regular expression needs `std.regex` (does not exist,
   A.3); `http get` over TLS needs `Ffi`, which makes the report
   `bounded: false` and destroys the premise. The first gate of the
   epic is therefore a **language slice**, not a tool.

4. **lex-os cannot read a cancho authority today, and the failure is
   silent.** Fed a cancho label verbatim, `lex-os-authority` derives
   `network: none` for a program that dials a host and `fs: none, net:
   none, exec: none` for a program holding `ffi("libc")` (measured,
   §2.5). The "tool whose effect rows are exact has an authority a
   supervisor can check and narrow" argument is correct, and it holds
   only through a bridge that fails closed. That bridge is a slice (S2),
   not a given.

What survives all four is still worth building. The strongest argument
is not "a safer `grep`". It is that lex-os's bounded commands are today a
**hand-written vocabulary** (`manifests/src/commands.lex`), and a tool
whose row the compiler derives lets that classification be derived too.
In the one place I tested, the hand-written claim and the derived one
disagree (§2.5).

---

## 1. What is wanted

### 1.1 Goals

* **G1. A contract an agent can rely on without reading prose.** One JSON
  document (or NDJSON stream) per invocation, a versioned schema, semantic
  exit codes, every error a datum with a stable rule tag and, where one
  exists that a script can apply without judgement, a repair.
* **G2. An authority a supervisor can read before running.** Exported by
  the compiler, never typed by a person, checked equal in CI, narrow in
  *kind* by construction.
* **G3. Determinism.** The same bytes in produce the same bytes out, on
  any machine, locale, terminal, time and working directory.
* **G4. A claim that can fail.** "More agent-friendly" is stated as
  measurable properties with gates (§7), and the part that needs an agent
  in the loop is not claimed until it is run.
* **G5. The language is the other customer.** Each tool is a program that
  *asks* for something the language lacks; the gaps it finds are
  reported with reproducers, counted against `CONTRIBUTING.md`'s
  two-askers bar, not worked around silently.

### 1.2 Non-goals

* **Not a coreutils clone.** About ten tools, each admitted by D1's rule.
* **No POSIX flag fidelity.** `docs/flags.md` §1 measured what fidelity
  costs: two ports of GNU programs got six of twelve spellings right and
  one of them answered `--decode` by *encoding*, exiting 0. The toolbox
  does not promise `grep -r` works; it promises that `-r` is refused with
  a tag (`args.unknown-flag`) rather than ignored.
* **Not a shell, and not a pipeline language.** An agent composes tools by
  calling them in turn and reading JSON, not by `|`. Stdin and stdout
  work for the tools that need them.
* **Not a sandbox.** `docs/filesystem.md` §2.1 says `Fs` "is not a
  sandbox, and this document will not pretend otherwise"; that stands.
  Containment against a hostile caller is lex-os's job.
* **Not a speed claim.** Appendix B's first probe measured `seek` at roughly
  3 to 5 times slower than GNU `grep` on one input; that was the probe, and the
  built tools have since closed it: `cancho-tools`' README (64 MiB through a
  pipe, minimum of 7 interleaved rounds, Linux x86-64) has `seek` at 0.49 s
  against `grep -F -n -b` at 0.51 s and `rg` at 0.39 s, and several tools ahead
  of their incumbents, with `hash` the one behind (0.52 s against `sha256sum`'s
  0.17 s, which is OpenSSL's assembly). Performance is still reported (§7.1,
  M9) and gated only against pathologies, never against GNU: the claim of these
  tools is the envelope, not the speed.
* **No process spawning, no network by default, no delete.** The language
  has no `exec` builtin, so `find -exec` and `xargs` are not a thing the
  toolbox can be tempted into. No tool deletes (D15): that is what
  lex-os's `Reversibility::IrreversibleConsequential` exists to keep out
  of a no-human system (`crates/lex-os-manifest/src/lib.rs`).

### 1.3 What the incumbents do well, and the honest case against each tool

Measured here where installed (`grep` 3.11, GNU coreutils 9.4, `rg` 14.1.0,
`jq` 1.7, `curl` 8.5.0); `fd` and `nushell` are **not installed in this
environment**, so what follows about them is from their documentation, not
measured.

| Incumbent | What it already does well | Where it falls short *for an agent* (measured unless noted) | The honest case for **not** building the cancho tool |
|---|---|---|---|
| **GNU coreutils** | Ubiquitous, fast (`wc -l` of 64 MiB: 15 ms; `sha256sum`: 60 ms), decades of edge cases, an agent already knows the flags | Text output with no schema; errors are prose on stderr with no tag; some behaviour depends on locale (`sort`, `wc -w`) — *not demonstrable here: only `C`, `C.utf8` and `POSIX` are installed* | Every tool below that is a thin filter over bytes gains little more than an envelope. See the per-tool verdicts in §5 |
| **ripgrep** | `--json` already emits a begin/match/end/summary NDJSON stream, with byte offsets, `{"text":…}` or `{"bytes":"<base64>"}` for a line holding `0xFF` (measured: the precedent for D2's `b64`), and is the fastest thing here (64 MiB, 1.3 M lines: 80–110 ms through a pipe) | Its JSON mode does **not** make errors data: `rg --json hello /nonexistent` prints a prose line on stderr, still prints a `summary` object on stdout, exits `2`; the `end` and `summary` records carry `elapsed`, so output is not byte-stable (measured: three runs of one `rg --json` on a 28-byte file gave three different checksums) | A cancho search tool is slower and has no regex. The gain is the error datum, stable bytes, and an exact authority row — nothing about search |
| **jq** | A full language; fast to start (2.5 ms); strict parser | Can read a file anywhere and, with `input_filename`/`$ENV`, the environment; its authority is the process's | A path-only JSON reader is a strict subset of `jq`. Anything past a path is `jq`'s job, and the toolbox must **refuse** to grow past it (D15) |
| **fd / find** | `fd` is fast, `.gitignore`-aware, and sensible by default (docs; not installed here). `find` can do anything | `find -delete` and `-exec` are the verbs a supervisor most wants to see absent from a row, and the row of `find` is the process's | Cannot be built today at all (A.2) |
| **nushell** | Structured pipelines: tables instead of text, `ls`/`open`/`where` as data (docs; not installed here) | It is a shell: its authority is the user's, and its commands are not individually checkable | It is the strongest existing answer to "structured output". It does not give a per-command authority, an error tag or a repair — that gap, not the tables, is what the toolbox fills |
| **`curl -w '%{json}'`** | Prints one JSON object of transfer facts (measured, 8.5.0) | TLS, redirects, proxies: authority is "the network" | See `fetch` in §5: not buildable narrowly while TLS needs `Ffi` |

---

## 2. What exists today

All of it read from the source named, then probed with the prebuilt
compiler where a claim needed a number. The compiler used is
`cancho 0.0.0 (rev 052e623…)`; `git diff 052e623 HEAD -- crates std` touches
only three test files, so `std/` and the compiler are those of the commit
this document is written against.

### 2.1 Process, standard streams, exit status

| Fact | Source | Measured here |
|---|---|---|
| `main` is `fn main(world: World) -> [] int` and its return value is the exit status; there is no `exit` | `docs/arguments.md` §5, `docs/authority.md` §1.1 | A status is taken modulo 256: a program returning `r * 100` for argc 1, 2, 3 exits `100`, `200`, `44` |
| Arguments come from the `Args` capability; `arg(a, 0)` is the program name, bytes, no encoding | `docs/arguments.md` §3 | — |
| Flags: a *cursor* (`flags.step`/`flags.value`) that reports shapes and never meanings; the program decides what a value is | `std/flags.cho`, `docs/flags.md` §3 | — |
| Standard input is `getchar`, one byte per call; there is no bulk read | `docs/standard-input.md`, `docs/bulk-io.md` §3.3 | A loop that reads and counts 64 MiB (`examples/tally.cho`) takes 0.35 to 0.38 s, about 180 MB/s; `cat` takes 14 ms |
| Standard output is `write_bytes`, which is `fwrite` on the C `stdout` stream; standard error is `write_err` on the unbuffered stream | `crates/cancho-codegen-llvm/src/body/expr.rs` (`write_bytes`/`write_err`), `docs/standard-error.md` §3.3, §5 | See below |
| A **trap** (overflow, bounds, arena exhaustion, a failed allocation, an `Fs` prefix violation, `trap()`) is `SIGILL`, no message | `docs/defined-behaviour.md`, `docs/testing.md` §2 | Exit status 132 as a shell sees it; a program that wrote `{"partial":` and then trapped emitted **0 bytes** through a pipe: the stdio buffer is not flushed |
| There is no environment, no clock and no tty query unless a capability or `Ffi` is held | `docs/arguments.md` §2.1, `docs/reach.md` §3.1, `Builtin::ALL` in `crates/cancho-ir/src/builtin.rs` (no `getenv`, no `isatty`; `clock_ms`/`clock_unix_ms` take a `Clock`) | — |

#### The finding in the output row: a failed write is invisible

`docs/bulk-io.md` §3.3 said *"Output has no such question: it wrote the
bytes or the process is gone."* That is false for a stream that fails, and
it is corrected in place there. Reproducer (A.7):

```text
$ ./wr                    # io.write_all(i, "hello\n"); returns the count
hello
$ ./wr >&- ; echo $?      # stdout closed
6
$ ./wr > /dev/full ; echo $?   # strace: write(1, "hello\n", 6) = -1 ENOSPC
6
```

`write_all` answers the length it was *asked* to write, the flush at exit
fails, and the process exits with its own status. A tool that reports
`ok: true` after this has told an agent a file was written when the disk
was full.

The count is the *buffered* count, and that is the whole of what a program
can use. Once the stdio buffer fills, a failing flush does show up, as a
**short** count: a second probe that wrote 1,000 sixty-five-byte records
saw 15 short writes to `/dev/full` and 7 to a closed stdout, and none to
`/dev/null`. So a tool that checks every `write_all` result against the
length it passed can detect most failures, but never the last buffer's
worth, which fails at exit where nothing can look. **This is a language
gap**, and it decides D2's `end` record: the last bytes are exactly the
ones whose loss is invisible, so a stream that does not finish with a
record saying `complete: true` must be treated by the reader as truncated,
and a tool must check each `write_all` result and stop with a tag
(`io.write-failed`) when one is short.

Two more facts the contract has to live with, both measured: a reader that
closes the pipe early kills the tool with `SIGPIPE` (a `flood | head -1`
pipeline reports status 141 for the tool, and no JSON), and stdout is
buffered while stderr is not, so the two streams do not interleave
(`docs/standard-error.md` §5).

### 2.2 Files, `Fs`, and paths

* **Handles and errno.** `open_read`/`file_read` answer `Opened::Failed(errno)`
  and `Read::Failed(errno)`; `Read::Got(n)`/`End` are distinct
  (`docs/file-handles.md` §3). Measured, the errno survives to the program:
  a missing file is `2` at open, a path through a regular file is `20` at
  open, and **a directory opens successfully and fails at the first read
  with `21`** (A.5). So `io.not-found`, `io.not-a-directory` and
  `io.is-a-directory` are all distinguishable today; *similar names* are not
  (no listing).
* **Writes exist.** `open_append`/`open_write`/`open_new` (`O_EXCL`)/`open_rw`,
  `file_write`/`file_pwrite`/`file_pread`/`file_sync`/`file_truncate`/
  `file_size`, `fs_rename`, `fs_remove`, `file_lock` (`flock`, released
  when the holder dies) — edition 5 (`docs/file-writes.md` §4, §7; the
  names are in `Builtin::ALL`). A probe built an atomic replace (write a
  temp file with `open_new`, `file_sync`, `fs_rename` over the destination)
  and its authority is exactly `file_write` + `fs_write("")` (A.8).
  Edition 5 widens `Split` to seven fields (`net`, `clock`), so every tool
  `release`s two more capabilities — each a statement of what it will
  never do (`docs/authority.md` §1).
* **`Fs(prefix)` is checked twice.** At compile time by `narrow`, at run time
  on every path; a violation **traps** (`docs/filesystem.md` §4). Measured
  with `Fs` narrowed to a directory: a path outside it, a sibling that
  shares the byte prefix (`/tmp/jailprobeevil`), a path containing `..`, and
  a **relative path** all end in status 132 — not an error value, and with
  nothing printed (A.4). Under the **unnarrowed** `Fs("")` a relative
  path is fine (the OS resolves it against the working directory), but
  **`..` still traps there too**: `../x` and `/tmp/a/../b` both exit 132
  (A.4). So every tool, in the default build as well as in a variant,
  must validate every path *before* the builtin sees it, or the
  contract's error is unreachable and an agent that types `../src` gets a
  dead process.
* **Symlinks escape.** `docs/filesystem.md` §4.1 says path normalisation
  "needs symlinks decided too" and leaves it open. Measured: a symlink
  *inside* an `Fs`-narrowed directory pointing outside it is followed, and
  the outside file's 20 bytes are read (A.4). There is no `lstat`,
  `readlink`, `realpath` or no-follow open to build a check from, and the
  one route that has `realpath` (`Ffi("libc")`) makes the report
  `bounded: false`.
* **`narrow` takes a literal, and nothing is generic over a prefix.**
  `narrow(fs, arg(g, 1))` is `capability-not-narrowable`; a function written
  `fn f[&r, p](fs: &r Fs(p), …) -> [fs_read(p)]` is `type-mismatch`
  (*expected a string literal, found an identifier*) (A.1, A.9). So library
  code over files is written once per literal, and a build that wants
  `Fs("/srv/work")` is a source substitution, as `docs/agent-tools.md`
  §3.1 already said and `crates/cancho/tests/conformance/backends.rs`'s
  `bind` test already does for a port.
* **No listing, no stat.** `fs_list` and `fs_stat` are
  `not-a-function` (A.2). What a program can learn about a path: it
  opens (`2`/`13`/`20`), it is a directory (`21` at read), its size
  (`file_size` on an open handle). Not its type without opening, its
  mode, or its modification time.

### 2.3 Memory

* An arena (`region`) is one 64 KiB chunk and exhaustion traps
  (`AGENTS.md` §6, `docs/heap.md`). The heap (`box_slice`, `std.buffer`)
  grows by doubling; **a failed allocation traps**, and nothing inside a
  program can ask the OS for a limit.
* **The consequence is concrete in `std.crypto`.** `sha256` copies its
  message, padded, into a `region` (`std/crypto.cho`), so any message
  longer than **65,527 bytes** traps; `sha512` any longer than **65,519**.
  Both measured by bisection (A.6) and checked equal to `sha256sum` on the
  last good size. `docs/crypto.md` states no limit and there is no
  incremental interface.
* `seek` reads a whole file into a doubling heap buffer, so its memory
  tracks the file, not the longest line. Peak resident memory (`ru_maxrss`
  of the child, three to eight runs each, spread under 0.1%) was 8,304 KB
  for a 1 MiB file, 13,776 KB for 8 MiB, **99,788 KB for exactly 64 MiB
  (1.5 times the input) and 198,032 KB for a file 28 bytes longer (3.0
  times)**, and 787,816 KB for a 256 MiB-plus file (2.9 times). The step
  is the doubling: just past a power of two the buffer is copied into one
  twice the size while the old one is still live. That is the ceiling of
  "no silent truncation" as `docs/agent-tools.md` built it, and D8 asks for
  better.* `std.json` is a tape: 24 bytes of tape per byte of source in the worst case
  (`json.tape_len` is `3 * (len + 1)` ints, `std/json.cho`). A 4.7 MB
  document parsed with a heap tape peaked at 119,184 KB (26 times), in
  0.095 s including a byte-at-a-time stdin read. Fast, and the memory is
  the document times 25.

### 2.4 What the compiler can print about a program

* `cancho authority <files> --std --output json` — the union of every
  reachable function's *performed* effects, computed from reachability
  (`docs/authority.md` §2), `bounded` first, every label with its
  `argument`, the foreign symbols, and the list of provably pure
  functions. Measured on `examples/seek/seek.cho`: seven labels
  (`args`, `err_write`, `file_read`, `fs_read("")`, `heap`, `io_read`,
  `io_write`), `bounded: true`, in 18 ms. It is **exact in both
  directions**: a probe that declared `fs_read("")` it did not perform was
  refused (*"a row is exact or it is decoration"*), which is why a
  derived manifest is a claim worth gating.
* It reports the **program**, not an invocation. A two-applet program
  (`cat` and `rm` chosen by `argv[1]`) reports `fs_read("")` and
  `fs_write("")` for both (A.10) — which is why D17 is one binary per tool.
* `cancho check --output json` answers `refused[]` of
  `{rule, message, explanation, position}`; there is **no repair field**
  (`docs/agent-errors.md` §5.1 designed `fix` and cut it for lack of a
  consumer whose behaviour had been watched). `cancho introspect` and
  `skill` are ACLI-generated from `crates/cancho/src/acli.rs`
  (`docs/agent-cli.md`).
* `cancho test` runs `fn test_*` (heap and io parameters only) one process
  each; a trap fails a test (`docs/testing.md` §3). There are **no
  `examples {}` blocks** — that is lex-lang's mechanism, not this
  language's.
* **The ACLI SDK**, read from the `acli-0.5.0` crate that `lex-lang` and
  `cancho` both use (`src/output.rs`, `src/exit_codes.rs`, `src/skill.rs`;
  the ACLI *specification document* was not available and is not quoted):
  the envelope is `{ok, command, data | error{code, message, hint?,
  hints?, docs?}, dry_run?, planned_actions?, meta{duration_ms, version,
  cache?}}`; the exit codes are `0 success, 1 general, 2 invalid args,
  3 not found, 4 permission denied, 5 conflict, 6 timeout, 7 upstream,
  8 precondition failed, 9 dry run`; progress is NDJSON `{type:"progress"…}`;
  `hint` is a **string**. `meta.duration_ms` is a required field of the
  Rust type and differs on every run.

### 2.5 What lex-os does with an authority (read, then measured)

Read from `/home/user/lex-os` at the working copy's head.

* `lex-os-authority` **derives from Lex source**, not cancho:
  `derive(src)` runs the real Lex front end
  (`lex_os_check::effects_of_source`) and folds the effect names with
  `lex_types::trust::effect_requirement`
  (`/home/user/lex-lang/crates/lex-types/src/trust.rs:555`): `fs_read`,
  `fs_walk` → filesystem read-only; `fs_write` → read-write; `net`, `http`,
  `mcp`, `llm_cloud` → network allowlist; `proc` → exec sandboxed;
  **everything else maps to nothing and is reported as `off_lattice`**
  ("no grant refuses these"). Its seam for another front end is the public
  `derive_from_effects(&EffectSet)`.
* `gate` checks the three **levels** and the **egress hosts**; it does not
  check `fs_read`/`fs_write` path scopes. `narrow_manifest` meets the grant
  and filters egress; **a manifest has no path field at all**
  (`crates/lex-os-manifest/src/lib.rs`: goal, grant, budget,
  isolation floor, egress, actuation, facets, comment), and the perimeter
  turns the filesystem level into two booleans (`SandboxPolicy::from_grant`:
  `fs_readable`, `fs_writable`, `crates/lex-os-perimeter`). The designed
  home for a path extent is a **facet** (`facet.rs`: type-erased,
  narrowed by a validator the consumer registers, byte-identical if
  unregistered); none exists for paths.
* Commands are a **hand-written** registry:
  `lex_os_supervisor::Command` carries `{name, dimension, required_level,
  reversibility, money_cents, api_calls}` and the Lex package
  `manifests/src/commands.lex` writes `cmd_list_dir`, `cmd_exists`,
  `cmd_read`, `cmd_write_report`, `cmd_fetch`, `cmd_run`, with the trust
  requirement in a *comment*. Exec is one command name, `proc.exec`
  (`crates/lex-os/src/exec.rs`: "there's no per-binary vocabulary to gate
  on"). `lex-os-capsule` binds an artifact's content hash to a grant and
  egress, signed, and installing it narrows the consumer's manifest.

**Measured, with a scratch crate that depends on those crates by path**
(Appendix B; nothing in `/home/user/lex-os` was modified):

| Input to `derive_from_effects` | Derived grant | Notes |
|---|---|---|
| `net_out("api.example.test:80")` — the cancho label, verbatim | fs none, **net none**, exec none | `off_lattice: ["net_out"]`. A program that dials a host is derived as needing no network |
| `net("api.example.test")` — after mapping the name | net allowlist | `egress: ["api.example.test"]`, `unscoped_net: false` |
| `fs_read("")` + `fs_write("")` | fs read-write | `fs_read: [""]`, `fs_write: [""]`: the "root" is a string entry, not "everything" |
| `fs_read("/work")` | fs read-only | `fs_read: ["/work"]` — informational; the gate ignores it |
| `ffi("libc")` — verbatim | **fs none, net none, exec none** | `off_lattice: ["ffi"]`. The one label that means "any authority at all" is derived as none |
| `file_read`, `io_write`, `heap`, `args` | none | all `off_lattice`: reviewed by eye, never refused |
| `diff` of `fs_read("")` → `fs_read("/work")` | verdict **widening** | `added: ["/work"]`, `removed: [""]`. A narrowing of a path prefix is classified as a widening, because path scopes are compared as opaque strings; `/work` → `/work/sub` is the same |

And one comparison of derived against hand-written: the body of
`cmd_write_report` as `commands.lex` declares it (`[io]`) derives to
`fs none, net none, exec none, off_lattice ["io"]`, while the comment above
it says *"Trust: filesystem ≥ read-write"*. I did not check whether
`std.io.write` can be declared with a finer effect; the point is only that
the written claim and the derived one differ in the one place I tried.

### 2.6 The gaps, in one table

Each has a minimal reproducer in Appendix A except L9, L11 and L12, which
are stated by the documents cited and were not reproduced separately. "Needs" says where the fix
lives; D16 decides what is fixed and what is routed round.

| # | Gap | Reproducer | Blocks | Needs |
|---|---|---|---|---|
| L1 | A failed write to stdout is unobservable; `write_all` returns the requested length; a trap loses buffered output | A.7 | every tool's `ok` | compiler (`write_bytes` result, or a checked flush). **Closed by [`checked-output.md`](checked-output.md) (#215)**: `flush_out` reports a failed or earlier-failed write; a trap still loses the buffer |
| L2 | No directory listing (`fs_list`) | A.2 | `list` (ls, find, tree) | compiler: builtin + edition. **Built** (#222 slice 1): `dir_list`/`dir_next` on a `Dir` rather than a path, and `std.dirs.list` sorted bytewise ([`directory-listing.md`](directory-listing.md)) |
| L3 | No file type, mode or mtime without opening (`fs_stat`) | A.2 | `list` entries, `stat` | compiler: builtin. **Built** (#222 slice 2): `dir_stat` on a `Dir` answers kind, size and mtime and never follows a link ([`directory-listing.md`](directory-listing.md)) |
| L4 | `std.crypto.sha256`/`sha512` trap past 65,527/65,519 bytes; no incremental API | A.6 | `hash`, `write` preconditions | `std` (or in-package first, AGENTS.md §7) |
| L5 | `narrow` takes a literal; no generic over `Fs(p)`/`Net(b)` | A.1, A.9 | static extent for a general tool | by design (`linearity-and-effects.md` §7.4); D14 routes round it |
| L6 | No symlink-aware open or `realpath` under `Fs`; symlinks escape a narrowed prefix | A.4 | symlink-safe `--root` | compiler: a no-follow open, if wanted. **Built** (#227 slices 1 and 2): [`directory-handles.md`](directory-handles.md)'s `Dir` reads, creates, appends, renames, removes and syncs beneath a directory and follows no link; the tools on top are its slice 3, built in alpibrusl/cancho-tools#4 |
| L7 | Outside-prefix, `..`, relative and sibling paths trap (132) rather than answering an error | A.4 | an error value for confinement | in-tool validation (D9); by design |
| L8 | No `std.regex` | A.3 | regex `seek` | `std` — large; D15 declines it |
| L9 | TLS needs `Ffi` (`conn_raw_fd`, `examples/tls_client`) | `docs/native-sockets.md` §6 | an `https` `fetch` with a bounded row | out of scope (D15) |
| L10 | Invalid UTF-8 written through `std.json` becomes U+FFFD silently; there is no base64 in `std` (`examples/base64` only) | A.13 | lossless bytes in JSON | contract package (D2), a `std.base64` later |
| L11 | Stdin is one byte per call (~180 MB/s) | `examples/tally.cho`, §2.1 | throughput only | none now |
| L12 | A status is modulo 256; no `exit` | §2.1 | none | none: the table fits |
| L13 | JSON tape is 24 B/byte of source | §2.3 | large-document query | none now; D8's `--max-bytes` |

---

## 3. The tool contract

Eleven decisions, D1 to D11. A decision that changes the behaviour of something that
exists (the one program, `examples/seek/`, or the compiler) is marked
**(to confirm)** and says what changes.

### D1. Scope, and one admission rule

**Decision.** The toolbox is *at most ten tools*, and a tool is admitted
only if all four hold:

1. a **stated gain over the incumbent**, written in §5 before the code;
2. a **versioned schema** and a rule catalogue with a fixture per tag (D5);
3. an **authority row** that the compiler derives and a ceiling a person
   wrote (D12);
4. every error path is a **tag**, and no input reaches a trap (D8, D9; §7.1
   M4).

**Alternatives.** (a) A coreutils subset of 30: rejected — it is the POSIX
fidelity trap `docs/flags.md` measured, multiplied. (b) One tool: rejected —
the contract's value is in being shared, and one tool cannot show that the
envelope, the exit codes and the hint rule generalise. (c) *No toolbox, only
a shim around the incumbents*: not rejected, it is **arm B of the
experiment** (§7.2) and the thing this has to beat.

**Why ten.** Not a number with a source. It is the size at which each tool
can still be reviewed for authority by reading its row, and at which a
maintainer can keep the language-gap list honest.

### D2. The wire format: one document, or a stream that says it finished

**Decision.**

* **JSON is the default.** `--format json` (document tools) or
  `--format ndjson` (stream tools); `--format text` for a person. Text is
  rendered from the same data model by a separate function per format, is
  *lossy by design*, and is never something a script parses.
* A **document tool** writes exactly one JSON object and one newline.
* A **stream tool** writes one JSON object per line, in a deterministic
  order, **ending with an `end` record** `{"type":"end","complete":true,
  "…counts"}`. Every error is also a record, and on any error path the
  `end` record still comes, with `complete:false`.
* **A stream with no `end` record is truncated and must be treated so.**
  Section 2.1 is why: a failed flush, a `SIGPIPE`, a trap or a kill all
  leave a prefix and an exit status the reader may not see; only the
  `end` record is evidence that nothing was lost.
* **Bytes that are not UTF-8 are never written as a string.** The `Writer`
  replaces them with U+FFFD silently (`docs/json.md` §3, L10). A field that
  may hold arbitrary bytes (a matched line, a file name) is emitted as
  `"text"` when `std.utf8.is_valid` holds and as `{"b64":"…"}` when it does
  not, in the schema as `text_or_bytes`. The base64 encoder is written in
  the contract package first (`examples/base64/base64.cho` is the starting
  point) and moves to `std` when a second program wants it.
* **Integers only.** `int` is 64-bit; sizes and offsets above 2^53 lose
  precision in a reader that decodes to a double. The schema says `integer`
  and `introspect` records the limit; no float appears in any tool's output.

**Alternatives.** (a) Text default, `--json` opt-in (the GNU habit):
rejected, it makes the safe path the longer one for the reader this exists
for. (b) A single JSON array for streams: rejected, a reader cannot use the
first result until the last arrives and a trap loses all of it. (c) rg's
begin/end-per-file framing: a *file* record is kept (`type:"file"`), but the
`end` record is the toolbox's addition and is the reason for this decision.

**Behaviour change, to confirm.** `examples/seek/` prints
`path:line:text` by default today; the toolbox's `seek` would print NDJSON
by default. `--format text` keeps the human form.

### D3. The envelope

**Decision.** ACLI-compatible, with the nondeterminism removed:

```json
{"ok":true,"command":"seek","schema":"seek.v1","data":{ … },"meta":{"version":"0.1.0"}}
{"ok":false,"command":"write","schema":"write.v1","error":{"code":"CONFLICT","rule":"precondition.hash-mismatch","message":"…","hint":"…","repair":null,"detail":{ … }},"meta":{"version":"0.1.0"}}
```

* `ok`, `command`, `data`, `error{code,message,hint}`, `dry_run`,
  `planned_actions`, `meta.version` are ACLI's, so a consumer of lex-os's or
  lex-lang's envelopes reads these unchanged.
* **Added:** `schema` (which schema validates this document), `error.rule`
  (D5), `error.repair` (D6, structured; `hint` stays the one-sentence string
  ACLI defines), `error.detail` (rule-specific data, e.g. the actual hash).
* **Removed:** `meta.duration_ms` and `meta.cache`. A caller who wants the
  duration times the process from outside. Time is not available to a tool
  without a `Clock` (D7), and a tool that took one to fill `duration_ms`
  would widen its row (`clock`) for a field no agent needs.
* **A stream is not wrapped.** Its records are the lines, and its `end`
  record (D2) carries `ok`, `command`, `schema` and `complete` — it *is*
  the stream's envelope.

**Why.** `rg --json` is the proof that an envelope with `elapsed` in it is
not byte-stable (measured: its `end` and `summary` records differ run to
run). lex-os and lex-lang already speak ACLI envelopes, so inventing a
fourth format would cost interoperability for nothing.

**Alternatives.** (a) ACLI verbatim including `duration_ms`: rejected, it
breaks M2 (byte-identical reruns). (b) Our own envelope: rejected for the
interoperability reason. (c) `duration_ms` always present and excluded by
the tests: rejected, "the field exists but is ignored" is how a contract
acquires parts nobody reads.

**To confirm.** This deviates from the SDK's `Envelope` struct, whose
`meta.duration_ms` is not optional. The ACLI specification text was not
available to read, only the crate; whether the spec makes `meta` mandatory
is a question for its owner.

### D4. Exit codes

**Decision.** The ACLI vocabulary (`acli-0.5.0` `ExitCode`), with each
code's toolbox meaning fixed, and one *absence* stated:

| Code | ACLI name | Toolbox meaning | Rule tags |
|---|---|---|---|
| 0 | SUCCESS | What was asked was done. **Zero matches is success**: `data.count` is 0 | — |
| 1 | GENERAL_ERROR | A failure that is neither the caller's nor a state mismatch: the OS failed mid-operation (`EIO`, `ENOSPC`, a short `write_all`), or a bug the tool caught in itself | `io.read-failed`, `io.write-failed`, `internal.*` |
| 2 | INVALID_ARGS | The invocation is malformed, or names something of the wrong kind; nothing was read | `args.*`, `path.dotdot`, `precondition.required`, `query.unsupported-syntax`, `io.is-a-directory` |
| 3 | NOT_FOUND | A named input does not exist (`ENOENT`, `ENOTDIR`) | `io.not-found`, `io.not-a-directory` |
| 4 | PERMISSION_DENIED | The OS refused, or the tool's own confinement did | `io.permission-denied`, `path.outside-root` |
| 5 | CONFLICT | State is not what the caller said it would be | `precondition.hash-mismatch`, `conflict.exists`, `conflict.locked` |
| 6 | TIMEOUT | **Not used.** A tool has no clock (D7); a supervisor kills and the shell reports 128+n | — |
| 7 | UPSTREAM_ERROR | A remote peer failed (`fetch` only, if it ever ships) | `net.*` |
| 8 | PRECONDITION_FAILED | The answer is "no", or a limit was reached: `--require-match` found none, `limit.*`, `parse.*` | |
| 9 | DRY_RUN | A `--dry-run` completed; `planned_actions` lists what an apply would do | — |
| 132 | (SIGILL) | The tool **trapped**. A bug; no JSON. Gated to be impossible (M4) | — |
| 141 | (SIGPIPE) | The reader closed the pipe. Not an error the tool reports | — |

Statuses are 0 to 255 (§2.1: 300 became 44). With several errors in one
invocation **the first, in input order, decides the exit code**; the
envelope's `error` is that first one (so an ACLI reader sees what it
expects) and `errors` lists all of them in order (D5).

**Alternatives.** (a) grep's `0 match, 1 none, 2 error`, which `seek` uses
today: rejected for agents, because "no match" is the *answer*, and a
harness that treats non-zero as failure then retries a question that
already had an answer. `--require-match` gives the grep behaviour on request
(exit 8). (b) Only 0/1/2: rejected, it throws away exactly the distinction
(conflict versus not-found versus refused) that lets a harness pick a
strategy without parsing.

**Behaviour change, to confirm.** `seek`'s status for no match goes from 1
to 0, and its "cannot read" from 2 to 3/4. `crates/cancho/tests/conformance/
agent_tools.rs` asserts the old values and changes in the same slice.

### D5. Errors are data, with a stable rule tag

**Decision.** An error is an object `{code, rule, message, hint, repair,
detail}`. The `rule` is `<area>.<name>`, kebab-case, from a catalogue each
tool declares and the contract package shares (`args.unknown-flag`,
`args.missing-value`, `path.dotdot`, `path.outside-root`, `io.not-found`,
`io.is-a-directory`, `io.permission-denied`, `io.read-failed`, `io.write-failed`,
`limit.line-too-long`, `limit.input-too-large`, `precondition.required`,
`precondition.hash-mismatch`, `conflict.exists`, `conflict.locked`,
`parse.json`, `query.unsupported-syntax`, `internal.invariant`). Four rules
adopted whole from `docs/agent-errors.md` §3.1 and `CONTRIBUTING.md`:

* **A tag names the rule a reader would look up, not the sentence.** The
  prose may change; the tag never does. A rule that splits gets siblings
  and never repurposes the parent.
* **Every tag has a fixture** that reaches it, and a test that the set of
  fixtures equals the catalogue (the toolbox's
  `every_tool_rule_has_a_fixture`, modelled on cancho's own).
* **Independent errors are all reported, dependent ones are not
  invented** (`docs/agent-errors.md` §4). Five paths named, two unreadable:
  the output has the three results and an `errors` array with two entries
  (`error` repeats the first), and `ok` is false iff `errors` is non-empty.
  An agent should not pay a turn per failure.
* **No input reaches a trap.** A catalogue that has a tag for every error
  and a binary that still dies with 132 on some input is a catalogue with a
  hole; M4 is the gate.

Each catalogue row also declares `repairable: always | sometimes | never`,
which D6 and M3 consume.

**Where errors go.** On **stdout**, inside the envelope, in JSON mode; the
reader then has one stream whose order is known. (ACLI's `emit` also
prints errors to stdout.) Stderr is empty in JSON mode except for
`internal.*` and for the last-resort line when stdout itself cannot be
written, which the tool cannot know (§2.1). In `--format text` mode the
sentence goes to stderr as a person expects.

**Alternatives.** (a) Errors on stderr, results on stdout (the Unix habit):
rejected for the reader this is for — a harness that truncates or reorders
two streams loses the correlation; and `rg --json` shows the failure mode:
the error is a stderr sentence and a stdout `summary` that says nothing
went wrong. (b) Flat tags without an area: rejected, the area is what lets
a catalogue grow past one tool without collisions and lets lex-os map
whole areas (`net.*`) to a dimension.

**To confirm.** Stdout versus stderr for errors.

### D6. The repair hint, and when it is allowed to exist

**Decision.** `error.repair` is `null` or an object of a **closed** set of
kinds, each a datum a script applies without judgement:

```json
{"kind":"retry","argv":["seek","--root","/work","needle","src/a.cho"]}
{"kind":"choose","options":[{"argv":[…]},{"argv":[…]}]}
{"kind":"none","reason":"the file changed since it was read; re-read, then decide"}
```

with three rules:

1. **A hint never widens authority.** It never adds `--root` outside the
   one given, a write flag, a `--create`, a `--force`, or removes
   `--dry-run`. This is lex-os's own invariant (the agent never sets its own
   limits; `/home/user/lex-os/CLAUDE.md`, "the invariant that must never
   break") applied to the hint, and it is a property test (M3), not a
   promise.
2. **A hint exists only when it is unambiguous**; otherwise `repair` is
   `null` or `kind:"none"` with a reason. A conflict is the example that
   matters: for `precondition.hash-mismatch` the tool returns the actual
   hash in `detail` and `repair.kind:"none"`, because applying "retry with
   the new hash" by script would silently overwrite a concurrent writer.
   The honest repair for a conflict is information, not a command.
3. **A hint raising a limit is capped.** `limit.line-too-long` may say
   `retry` with `--max-line-bytes N` where `N` is the line it saw, never
   past the compiled-in ceiling, because the heap traps on exhaustion (§2.3).

What can be repaired *today* is smaller than it sounds. With no listing
(L2) `io.not-found` cannot suggest a similar path; `args.unknown-flag` can
(an edit-distance match against the tool's own flag table); `path.dotdot`
can (the lexical normal form, if it stays inside the root);
`path.outside-root` can only when the path lies under the root in another
spelling; `args.missing-value` cannot (the tool does not know the value).

**This is the consumer `docs/agent-errors.md` §5.1 said was missing.** That
document cut a compiler `fix` field because *"the argument for it is about a
consumer nobody has watched"* and named what would settle it: *"an agent
that has `rule` and still repairs the wrong thing."* The toolbox is where
that is watched. So `repair` ships under a **kill rule** (§7.2): if the
ablation arm (hint removed, tag kept) does as well as the arm with it, the
field is cut, as `fix` was.

**Alternatives.** (a) ACLI's `hint` string only: kept as the sentence, but a
string is what an agent has to parse, which is the problem. (b) lex-lang's
`suggested_transform` (an op id plus a transform for `lex repair --apply`):
does not transfer, for `docs/agent-errors.md` §2's reason — it is a
machine-checked edit against a content-addressed store, and a tool
invocation has no store. (c) No hint: the ablation arm.

**To confirm.** Whether `repair` ships at all, and the kill rule's threshold
(the protocol fixes the *comparison*, not a number invented before a pilot).

### D7. Determinism is a property of what the tool was not given

**Decision.** The contract states, and the tests check (M2):

* **No locale.** Ordering is bytewise (`bytes.compare`); case folding is ASCII
  only and named so. (`std.utf8` exists; a tool that wants Unicode case
  says so in its schema.)
* **No colour, no tty sniffing, no environment.** Not a rule to obey: the
  language has no `isatty`, no `getenv` and no colour library without
  `Ffi`, so a tool *cannot*. `introspect` records `reads_environment:false`
  because the row proves it (no `ffi`).
* **No time, no randomness.** No `Clock` is taken, so no timestamp or
  duration can appear and no output depends on when it ran. A tool that
  needs `clock_unix_ms` (none planned) says so in its row and its schema.
* **Stable ordering.** Inputs in argument order; entries of a directory
  sorted bytewise by name; JSON keys in the schema's order (the `Writer`
  emits in call order, which is the source order, so the order is fixed by
  the code and pinned by the golden corpus).
* **Same bytes whatever the stream.** stdout is a pipe, a file or a tty to
  the program identically; output never changes with the sink.
* **No `meta.duration_ms`** (D3).

**Alternatives.** (a) Honour `NO_COLOR`/`LANG` like GNU: not available
without `Ffi`, and nondeterminism the reader did not ask for. (b) Sort
nothing, "let the filesystem decide": rejected, directory order is what
makes `find` output differ between machines.

### D8. Large inputs and outputs: what a streaming tool must do

The memory model is §2.3: a 64 KiB arena that traps, a heap that traps, no
way to ask for a limit. A tool must therefore bound itself.

**Decision.**

1. **A streaming tool's memory is `O(chunk + longest line + bounded
   state)`, never `O(input)`.** The pattern is already in the repository
   (`examples/seek/seek.cho` `read_file`): a 64 KiB heap `buffer`, filled by
   `file_read` through `buffer.room`/`buffer.filled`, consumed, cleared with
   `buffer.clear` — *without* the `buffer.append` that makes `seek` hold the
   file. A line that spans chunks is carried in a second buffer that grows to
   the longest line and is capped (point 3). `docs/line-reading.md` is why a
   cap must be a **tag**, not a silent clip.
2. **Never size a `region` from data.** An arena is for fixed, small scratch
   (L4 is what happens otherwise).
3. **Every tool has limits with a compiled-in default and a hard ceiling**:
   `--max-bytes` (input), `--max-line-bytes`, `--limit` (output records) and
   `--max-keys` where state grows. Exceeding one is a tagged error
   (`limit.*`, exit 8) with `detail` naming what was seen — **never a trap,
   never a silent truncation.** A tool that stops early because of `--limit`
   says `truncated:true` and a `next` cursor (`{"offset":N}`) that a
   following call resumes from (truncation by `--limit` is a result, not an
   error, so it has no rule tag).
4. **Memory must not scale with input**: the gate (M9) runs the same tool
   at 1 MiB, 64 MiB and 256 MiB and requires peak resident memory within a
   factor of 1.5 across the three. `seek` as built fails it by a wide
   margin (787,816 KB at a 256 MiB-plus file against 8,304 KB at 1 MiB,
   about 95 times).
5. **Output is flushed per record** (`write_bytes` per NDJSON line), so a
   long stream does not live in one buffer; the `end` record (D2) is what
   makes a lost tail detectable (L1).
6. **Heap exhaustion cannot be caught inside a program.** The tool's caps
   are the polite layer; the supervisor's memory budget (lex-os's
   `Budget`) is the wall. The contract says so rather than implying the caps
   make a tool unkillable.
7. **No hang protection.** A read from a FIFO or an idle stdin blocks; there
   is no timeout primitive on `file_read` and no `Clock` taken. Exit 6 is
   unused and the supervisor owns the wall clock. *Not probed here; read
   from the absence of a timeout in `docs/file-handles.md` §3 and
   `Builtin::ALL`.*

**Alternatives.** (a) Read whole files, as `sort`, `seek`, `sha256` do: fine
for a tool whose job is to hold the data (`sort`), wrong for a search or a
range read, and a trap rather than an error at the limit. (b) `mmap`: not in
the language (`docs/file-writes.md` §8). (c) A larger arena: not a language
decision this document can make.

### D9. Paths are capabilities: root-relative, lexically confined, and honest about symlinks

**Decision.**

* A tool that touches files takes **`--root DIR`** and treats every path
  operand as **relative to the root**. With no `--root`, paths are
  relative to the working directory and the tool says so in `introspect`
  (`confinement:"none"`).
* **Validation happens in the tool, before any builtin sees the path**
  (the builtin traps — §2.2 — and a trap carries no tag): reject a path
  with a `..` component (`path.dotdot`, exit 2), an absolute path that is
  not under the root (`path.outside-root`, exit 4), a sibling that shares the
  prefix (`/rootevil` is not inside `/root`; `docs/filesystem.md` §1.1 is the
  same rule); collapse `.`, repeated `/`, and a trailing `/`. A path inside
  the root in a different spelling gets a `repair` (D6).
* **Symlinks are not followed below the root.** *(Rewritten when M8
  flipped.)* This said there was no no-follow open (L6) and that the tests
  asserted the known escape. #227 built directory handles
  ([`directory-handles.md`](directory-handles.md)), and `cancho-tools`
  (alpibrusl/cancho-tools#4) now opens `--root` with `open_dir` and every
  path beneath it one component at a time with `O_NOFOLLOW`: a link
  anywhere below the root is `path.symlink` (exit 4), and `introspect`
  records `confinement:"beneath"`. The root's own spelling is the caller's
  and may hold links. The writers work beneath the parent directory and
  hold `dir_write`, no `fs_write`.
* Where symlink safety matters, it is the **perimeter's** job (a mount
  namespace, a lex-os box), exactly as `docs/filesystem.md` §2.1 says.

**Alternatives.** (a) `realpath` through `Ffi("libc")`: rejected, the report
becomes `bounded: false` and the whole premise goes. (b) Refuse any path
whose parent contains a symlink: needs `lstat`. (c) Normalise `..` instead
of refusing it: `docs/filesystem.md` §4.1 refused for the same reasons
(a security function with a history of being got wrong, and symlinks make
lexical `..` unsound — `a/link/../b` is not `a/b`). (d) Absolutise relative
paths: there is no `getcwd`.

### D10. Mutating tools: dry-run, no blind writes, idempotence

**Decision.**

* **Every mutating tool takes `--dry-run`** and exits 9 with
  `planned_actions: [{"op":"replace","path":…,"before_sha256":…,
  "after_sha256":…,"bytes":N}]` and **performs no mutating syscall**.
* **No blind overwrite.** A write must say what it expects: `--create`
  (the path must not exist; `open_new`'s `O_EXCL` is the primitive) or
  `--if-sha256 HEX` (the current content must hash to this). Neither is
  `precondition.required`, exit 2. This, not a default dry-run, is the
  safety — it makes the agent state its belief about the file, and the
  tool checks it.
* **Idempotent.** The same arguments applied twice produce the same final
  state and the second result is `changed:false`, exit 0 (`--content-sha256`
  makes "already applied" decidable). M7 runs every mutating case twice.
* **Atomic.** Write a sibling temp file with `open_new` (a deterministic name
  derived from the target and the content hash, so two runs collide rather
  than litter), `file_sync`, then `fs_rename` over the destination (probed,
  A.8). A temp that already exists is `conflict.locked`.
* **A lock held across check and replace** (`file_lock` on a sidecar
  `<path>.cancho-lock`), because a bare check-then-rename has a window in
  which two writers both pass. The lock is advisory
  (`docs/file-writes.md` §7): it defends against two toolbox processes, not
  against a writer that does not lock. Gate M7 includes a two-process race.
* **Reversibility is declared** per tool for lex-os: read-only tools
  `reversible-cheap`, `write`/`replace` `irreversible-bounded` ("write a
  file", the example in `lex-os-manifest`), and **no tool is
  `irreversible-consequential`** (D15: no delete; `move --remove` is a rename to a tombstone).

**The honest limit, and the alternative it raises.** `--dry-run` is not
visible in the row: a `write` run with `--dry-run` and one without both report
`fs_write("")`, because the row is the program's, not the invocation's
(§2.4). "A dry run does not write" is therefore a property of *tests*
(M7, with `strace`), not of types. The stronger form is a **split**: a
`write-plan` binary with no `fs_write` label at all — provably unable to
write — that emits a content-addressed plan, and an `apply` binary that
consumes it and re-checks every precondition. Plan runs under a read-only
grant; apply only on approval. It costs a second binary per mutating tool
and a plan format. **Not chosen for S1** (the brief asks for `--dry-run`,
and the flag is what GNU/ACLI users expect); it is open question Q6, and
the measured limit above is the reason it is open.

**Alternatives.** (a) Default to dry-run and require `--apply`: rejected,
two calls for every write and the agent will pass `--apply` by reflex; a
*precondition* cannot be passed by reflex because the agent must have read
the file. (b) Overwrite by default: the GNU habit; the thing this exists to
refuse.

**Behaviour change, to confirm.** None to existing code (no mutating tool
exists); the "no blind overwrite" default is a policy a person should
confirm, because it will annoy an agent that has not read first.

### D11. `introspect` and `skill`, from one table

**Decision.** Each tool answers `tool introspect [--output json]` and
`tool skill`, both generated from **one static table per tool** that also
*drives the parser*: a `FlagSpec` array (`name`, `short`, `kind`, `default`,
`role`) walked with `std.flags`'s cursor in the contract package. There is
no hand-written second list to drift. This is the lesson of
`docs/agent-cli.md` §2, where a generated surface taught agents `--o` for
`-o`, and of `docs/flags.md` §1, where a program silently ignored
`--decode`.

`introspect` reports:

* the tool, its version, the compiler revision it was built with
  (`cancho --version` at build), the schema ids and the schemas themselves;
* every flag and operand with its type, default, and **`role`**
  (`path-read`, `path-write`, `host`, `none`) — D13 uses it;
* the exit-code table of D4 with which codes this tool can emit;
* the rule catalogue (tag, exit code, `repairable`);
* `confinement` (`none`/`lexical`), `reads_environment`, `output`
  (`document`/`stream`), the limits with defaults and ceilings;
* the **authority** (D12) and the `reversibility` (D10);
* `evidence`: which gates of §7 have *run* for this release, and which have
  not (so a tool never implies a measured claim it lacks — §9).

`skill` is the agentskills.io `SKILL.md`, generated from the same data. It
is **not** produced by the ACLI SDK's generator: that one hard-codes an
exit-code table that does not describe a tool (`docs/agent-cli.md` §3).

**Alternatives.** (a) The SDK's `register`/`skill` path: Rust-only, and its
exit-code section is wrong for us. (b) A hand-written `SKILL.md`: the drift
the previous document found, once, by luck.

---

## 4. The authority manifest

### D12. Exported by the compiler, embedded by the build, gated against a ceiling

**Decision.** A tool's authority is **never written by a person**. The build:

1. runs `cancho authority <sources> --std --output json` (pass 1);
2. writes `generated/manifest.cho`, a file holding the manifest and the
   schemas as string literals;
3. rebuilds with it (pass 2); `tool introspect` prints it;
4. checks the **fixed point**: pass-2 authority equals pass-1 authority.

The fixed point is not an assumption. A probe embedded the pass-1 JSON of a
small program as a string literal and rebuilt: the two authority reports
were **identical** (the whole JSON, `functions` count included), and the
binary printed the literal back byte for byte (A.11). A string constant adds
no label, so the manifest can describe the program that contains it.

**The CI gate (M6)** has three parts:

* `authority.json` committed for each tool **equals** a fresh compiler
  output (no drift);
* the embedded copy a built binary prints **equals** it (no stale build);
* the derived label set is a subset of the tool's **ceiling**, a short file
  a person writes and reviews (`tools.toml`: `allow = ["args","heap",
  "io_write","file_read","fs_read"]`). What a person writes is a *ceiling*,
  never the authority: a widening is then an edit to the ceiling, a
  reviewable diff — the same trade `lex-os authority diff` draws, enforced
  where the source lives. Because rows are exact in both directions, "the
  derived set equals the ceiling" is **not** required, only "within".

**What a tool needs that cannot be narrowed statically** (and the
manifest says so in a `not_narrowable` list, so a reader is told rather
than left to infer):

| Authority | Why it is not in the row | Where it is bounded instead |
|---|---|---|
| **Path extent** | `narrow` takes a literal (L5); a path from `argv` is run time. The row says `fs_read("")` | D9's lexical `--root`; D14's variant build; the perimeter; D13's mediator |
| **Host** (`fetch`) | the same, for `Net` | per-host variant (D14); lex-os egress |
| **What a symlink reaches** | *(Closed under `--root`.)* `Dir` opens beneath a directory following no link (L6, #227) | D9: `--root` opened beneath, links refused; without `--root`, the perimeter |
| **What stdout carries** | a tool can print anything it read | lex-os's audit chain, not a type |
| **Resource use** (memory, wall time, bytes) | heap and arena trap; no clock | D8's caps; lex-os `Budget` |
| **Per-invocation behaviour** (dry-run vs apply) | the row is the program's | M7 (`strace`); the D10 split if wanted |
| **Content** | a row says a tool *can* read, not what | — |

### D13. How lex-os consumes it

**Decision.** A **bridge** maps a cancho authority report onto lex-os's
`derive_from_effects` input, **failing closed**, and lives in lex-os (it
needs `lex_types::EffectSet` and the lattice; reimplementing the lattice
elsewhere would be a second, independent source of authority, which its
`CLAUDE.md` forbids). The mapping:

| cancho label | lex-os effect | Notes |
|---|---|---|
| `fs_read(p)` | `fs_read` with scope `p` | `""` is the root, kept as an entry (§2.5) |
| `fs_write(p)` | `fs_write` with scope `p` | |
| `net_out("h:p")` | `net` with scope `h` (the entry keeps `:p`) | **required**: verbatim it is `off_lattice` and derives `network: none` (§2.5) |
| `net_out("")` | bare `net` | `unscoped_net: true`, the same "which host is a perimeter question" lex-os already reports |
| `net_in(b)` | **no mapping: refuse** until lex-os decides what inbound means (`docs/net.md` §2) | the grant has no inbound field |
| `ffi(lib)`, or any `bounded:false` | **refuse** | verbatim it derives *no authority* (§2.5) |
| `file_read`, `file_write`, `conn_*` | dropped | spent at open; the minting label (`fs_*`, `net_out`) is in the report (`docs/file-handles.md` §4.1) |
| `io_read`, `io_write`, `err_write`, `heap`, `args` | `off_lattice` | reviewed by eye, as lex-os does for `io` |
| anything else | **refuse** (`bridge.unknown-label`) | a new compiler label must not default to "no authority" |

The reference test is the measured hole itself: the fixture for a dialling
program must derive `network: allowlist` with that host, and **a mapping
that drops `net_out` must turn the test red** (S2's mutant).

**Using it.** With the bridge:

* `lex-os authority derive` of a tool's manifest yields the least `Grant`;
* `gate` against a manifest says whether the tool fits;
* `narrow_manifest` tightens a manifest to what the tool needs, as for Lex;
* `diff` between two releases of a tool is the review artifact — **after the
  prefix finding is fixed**: path scopes are compared as opaque strings, so
  narrowing `fs_read("")` to `fs_read("/work")` is reported as a *widening*
  (§2.5). Until then the authority diff for cancho tools over-reports.
* `lex-os-capsule` can bind a tool binary's content hash to the derived
  grant and egress, signed, and installing it narrows the consumer's
  manifest — the toolbox's release artifact.

**Extent through a mediator.** lex-os mediates commands: it logs a
request, classifies its reversibility, checks the perimeter and the budget,
charges, allows (`crates/lex-os-supervisor`, `CLAUDE.md`'s "order of gates").
If each tool is registered as a command — `tool.seek` as (filesystem,
read-only, reversible-cheap), `tool.write` as (filesystem, read-write,
irreversible-bounded) — **derived** from the manifest instead of hand-written,
the supervisor sees the *arguments* at request time. `introspect`'s `role`
fields (D11) tell it which operands are paths and hosts, so the **extent can
be checked at the mediator against a path-scope facet**, even though no type
can express it. The facet (prefix-containment narrowing) does not exist and
must be written by someone who registers a validator (`facet.rs`).

**What it cannot see**, said once: a bare or unbound label (`net_out("")`,
`fs_*("")`) gives it a *kind* and no *extent*; off-lattice labels are
reviewed, never refused; a path scope is not gated today; the perimeter's
filesystem is a pair of booleans; and everything in D12's `not_narrowable`
table.

**Alternatives.** (a) Feed cancho labels verbatim: measured, unsafe.
(b) A Python bridge in this repository that computes the grant: a second
derivation of authority, which lex-os forbids. (c) Make cancho emit lex-os
effect names: wrong direction; cancho's labels are its own and finer.

**To confirm — and it is a cross-repository commitment.** Whether lex-os
wants this input at all; this document asks for it and changes nothing
there. Two issues for lex-os come out of it regardless: the prefix
classification and the silent `off_lattice` derivation of a label an
unknown front end supplies.

### D14. Extent through build-time variants

**Decision.** A deployment that wants the type-level proof builds the tool
with the literal substituted: `scripts/variant.py --root /srv/work
--tool seek`, a source transform from `Fs("")` to `Fs("/srv/work")`
(and `Net("")` to `Net("h:p")` for `fetch`), then the ordinary build, whose
`authority` now reads `fs_read("/srv/work")`. The generated variant's
manifest is derived like any other (D12) and its `--root` flag is fixed to
the baked value. This is the mechanism `docs/agent-tools.md` §3.1 named and
`backends.rs`'s `bind` test uses for a port.

**Why a variant and not a flag.** L5: nothing is generic over a prefix.

**Alternatives.** (a) Ship only unnarrowed tools and rely on D9 and the
perimeter: this is what is shipped *by default*; variants are opt-in.
(b) Wait for a language feature making `Fs(p)` generic: the design
(`linearity-and-effects.md` §7.4) deliberately chose literals "so the
refinement is checkable structurally", and this document does not argue
against it.

**To confirm.** Whether variants are worth their maintenance (N tools times
M deployments of build output) before any deployment asks. Recommendation:
build the transform in S2 and use it in the tests, ship no variants.

---

## 5. The first tools

### D15. The set, and a verdict on each

The candidates in the brief, with the gain over the incumbent, the effect
row **expected** (the gate derives the real one — D12), and what the
language lacks. "Not worth it" is a verdict, and it is given where it is the
honest one.

| # | Tool | Verdict | Gain over the incumbent | Expected row | Needs |
|---|---|---|---|---|---|
| 1 | **`seek`** (literal search) | **Build, exemplar (S1)** | tagged errors; stable bytes; `{"b64"}` for binary; per-file results with `end` record; **no `fs_write`, `net`, `ffi` provable**. *Not* speed as the claim (the first probe was 3–5× slower than `grep`, Appendix B; the built tool is level with it, `cancho-tools`' README) or regex | `args`, `file_read`, `fs_read("")`, `heap`, `io_write` (+`io_read` only if stdin is accepted; +`err_write`) | L1 (for a trustworthy `ok`); D8 rewrite |
| 2 | **`write` / `replace`** (atomic, precondition, dry-run) | **Build, mutating exemplar (S1)** | **the one place no incumbent has an equivalent**: `sed -i`/`tee`/`>` are blind; precondition hash, atomic rename, tagged conflicts, idempotence, `--dry-run`. Authority: `fs_write` + `file_write`, no `net` | `args`, `heap`, `file_read`, `fs_read("")`, `file_write`, `fs_write("")`, `io_read` (content on stdin) | L4 for hashes over 64 KiB (see below); L1 |
| 3 | **`peek`** (range read: head/tail/`sed -n`/`cat -n`/`wc`) | **Build (B1)** | line numbers and offsets, binary detection, a `next` cursor, size, one call instead of `wc -l; sed -n a,bp`; it is the agent's most frequent operation | as `seek` | D8 |
| 4 | **`jsonq`** (path query, JSON Pointer, RFC 6901) | **Build, small, and refuse to grow (B1)** | strict RFC 8259 + UTF-8 validation (`docs/json.md` §3), depth cap, error *position* as data, authority is `heap` + read-only | `args`, `heap`, `io_read`/`fs_read("")`, `io_write` | L13 (24× memory) |
| 5 | **`tally`** (count distinct keys; top-N) | **Build if an asker shows (B1)** | `sort \| uniq -c \| sort -rn \| head` is four processes whose order can depend on locale; one bounded, deterministic, byte-ordered pass. `examples/wordfreq/` is the seed | as `seek` | D8 (`--max-keys`) |
| 6 | **`list`** (ls + find + tree, depth-bounded, sorted) | **Build after L2 + L3 (B2) — the most valuable and the one blocked** | sorted bytewise; `kind`/`size`; depth and entry caps; and **no `-exec`/`-delete`**, the verbs a supervisor most wants absent from a row | `args`, `heap`, `io_write`, `fs_read("")` | **L2, L3** |
| 7 | **`hash`** (sha256/sha512) | **Build as a by-product; not worth it alone (B2)** | JSON, many files, `--verify`. `sha256sum` is faster (60 ms for 64 MiB) and already right. Its worth is that `write`'s precondition needs the function | `args`, `heap`, `file_read`, `fs_read("")`, `io_write` | **L4** |
| 8 | **`diff`** (hunks as data) | **Defer; probably not worth it (B3)** | hunks as a datum lets an agent verify its own edit; but `write`/`replace` already return what changed, GNU `diff` exists, and Myers' algorithm in a language without closures is a few hundred lines to maintain | `args`, `heap`, `file_read`, `fs_read("")`, `io_write` | an asker |
| 9 | **`fetch`** (`http get`) | **Not in the first ten** | Only for **plain HTTP to a baked host** (D14): `net_out("h:80")` + `conn_read`/`conn_write`, `bounded: true` (A.12). **HTTPS needs `Ffi`** (L9), making the report `bounded: false`, so the dominant case defeats the premise. `curl` does it better | `args`, `heap`, `net_out("h:p")`, `conn_read`, `conn_write`, `io_write` | L9; or an internal-HTTP asker (lex-os's own demo egress is one) |
| — | `stat` | **No standalone tool** | `exists`/`kind`/`size` fold into `list` entries and `peek`'s header; mode and mtime need L3 and no agent asked | — | L3 |
| — | `cut`/`sort`/`uniq`/`lines` transforms | **Not worth it** | `examples/cut/` and `examples/sort/` exist and are checked against GNU (`LC_ALL=C sort`), but their gain over GNU is the envelope alone, and agents transform text in their own language. `tally` is the one aggregate with a real gain | — | — |
| — | `grep` with regex, `head`/`tail`/`wc` as separate tools | **Not worth it** | regex: `rg` (and L8 is large); `head`/`tail`/`wc`: folded into `peek` | — | L8 |

**No tool deletes.** `fs_remove` exists (§2.2) and a `rm`-like tool would
be easy, and it is deliberately absent: a delete is lex-os's
`irreversible-consequential` class, which in a no-human system "must be absent
from the grant entirely" (`manifests/src/commands.lex`), and a toolbox that
shipped one would be handing the supervisor the one command it is built to
refuse. (`write` replaces atomically and removes only its own temporary file.)

**Amended: a tombstone is a rename, not a delete.** `cancho-tools`' `move
--remove --if-sha256 HEX PATH` (cancho-tools#25) takes a file away by renaming
it to `.NAME.removed-<first 8 hex of HEX>` in the same directory. D15's reason
stands: the content is kept, nothing in the toolbox purges it, and renaming it
back undoes the removal, so the tool is `irreversible-bounded` like `write`,
not the `irreversible-consequential` class. It needs the hash (a file is
removed only as the caller read it), refuses an existing tombstone
(`conflict.exists`), and a retry that had landed is `changed: false`.
Tombstones accumulate, and clearing them is a person's job with a shell. There
is still no purge, and no tool that unlinks.

That is **eight tools plus a deferred ninth**, under D1's ten. **Six are
buildable now** (`seek`, `write`, `peek`, `jsonq`, `tally`, and `hash` with a
caveat); `list` waits on L2/L3, `diff` on an asker, `fetch` on L9.

**On `write` and the hash.** A precondition by hash needs the SHA-256 of the
current file. Today that traps past 65,527 bytes (L4): an arbitrary
ceiling that a large source file, a lockfile or a generated document
exceeds, and a trap is not an error an agent can read. Two honest routes: port the
compression loop into the contract package with chunked state (about 100
lines, the `AGENTS.md` §7 rule — *write it in your program first*), then move
it to `std.crypto` when `hash` is the second program that wants it; or wait
for the std slice. **Recommendation: the in-package port**, so S1 is not
gated on a compiler release (D18), with the std slice as a follow-up that
deletes the copy.

Per-tool contract sketches (each gets a schema file at S1/B-time; these fix
the *shape*):

```text
seek    [--root DIR] [--max-count N] [--max-line-bytes N] [--ascii-case-insensitive]
        [--require-match] [--format ndjson|text] PATTERN FILE...
        {"type":"match","path":"a.cho","line":3,"offset":120,"text":"…"|{"b64":"…"}}
        {"type":"file","path":"a.cho","matches":2,"bytes":4096,"binary":false}
        {"type":"error","error":{…}}
        {"type":"end","complete":true,"files":2,"matches":5,"truncated":false}

write   [--root DIR] (--create | --if-sha256 HEX) [--content-sha256 HEX]
        [--dry-run] (--stdin | --content-file PATH) PATH
        data: {"path":"a.cho","changed":true,"created":false,"bytes":812,
               "before_sha256":"…","after_sha256":"…"}
replace [--root DIR] --old TEXT --new TEXT [--expect N=1] [--if-sha256 HEX] [--dry-run] PATH
        data: {"path":"a.cho","replacements":1,"before_sha256":"…","after_sha256":"…"}

peek    [--root DIR] [--lines A:B | --bytes A:B] [--max-bytes N] PATH
        data: {"path":"a.cho","size":4096,"kind":"text","range":{"from":1,"to":40},
               "lines":[{"n":1,"text":"…"}],"eof":false,"next":{"line":41}}

jsonq   [--pointer /a/0/b] [--keys | --length | --type | --exists] [--max-bytes N] [FILE | -]
        data: {"pointer":"/a/0/b","kind":"string","value":"…"}
        error: parse.json with detail {"offset":N,"line":L,"column":C}

tally   [--root DIR] [--field N --delim D] [--top N] [--max-keys N] [FILE...]
        data: {"total":N,"distinct":M,"top":[{"key":"…","count":K}],"truncated":false}

list    [--root DIR] [--depth N] [--max-entries N] [--kind f|d] DIR
        {"type":"entry","path":"src/a.cho","kind":"file","size":812}   (sorted bytewise)
        {"type":"end","complete":true,"entries":N,"truncated":false}

hash    [--algo sha256|sha512] [--verify HEX] PATH...
        {"type":"hash","path":"a.cho","algo":"sha256","hex":"…","bytes":812}
```

### D16. Language gaps: what is fixed in cancho, and what is routed round

**Decision.** `std/*.cho` is compiled into the compiler with `include_str!`
(`crates/cancho/src/main.rs:410-429`), so any `std` change *and* any new
builtin is a compiler release, and `CONTRIBUTING.md`'s bar is two askers.
The policy:

* **L1 (checked output)** is a *correctness* gap, not a feature: it makes
  `ok:true` untrustworthy for every tool. **Fix in the compiler, first.**
  The smallest honest change is `write_bytes`/`io.write_all` answering what
  was written *and* a way to flush and learn the result; the document
  `bulk-io.md` §3.3 is already corrected to say it is open.
* **L2/L3 (`fs_list`, `fs_stat`)** are the largest asks and the most
  valuable. Askers: the toolbox's `list` (one) and `cancho-log`, whose
  `docs/file-writes.md` §8 names `fs_list` as waiting for "a program that
  cannot" avoid it (two, if that program asks). Edition 6, both backends and
  both targets. **Fix in the compiler, as the first gate of B2.** `dirent`'s
  `d_name` offset differs by target (`docs/file-writes.md` §8): that is the
  design problem, not the builtin.
* **L4 (incremental hash)**: in-package first, std second (above).
* **L5, L7**: by design or declined; D9 and D14 route round them and
  the tests *pin the limit* (M8). **L6** was built (#227) and M8 now
  asserts the refusal rather than the escape.
* **L8 (regex)**: declined. `seek` is literal; a person who wants regex uses
  `rg`, and the `seek` error for a metacharacter-looking pattern is a tag with
  a `none` repair, not a guess.
* **L9 (TLS)**: declined; `fetch` stays out of the first ten.
* **L10 (base64)**: in-package, then std at the second asker.

**Each language slice carries its own mutants** (§8). A gap reported here
without a reproducer would be an opinion; L1 to L8 and L10 have one
(Appendix A), and L9 is stated by `docs/native-sockets.md` §6 and not
reproduced here.

---

## 6. Distribution

### D17. One binary per tool, not a busybox

**Decision.** One executable per tool: `seek`, `write`, `peek`, …

**Evidence.**

* **Authority is per program.** The authority report is the union of what
  `main` reaches (§2.4): a multi-call binary containing `cat` and `rm`
  reports both `fs_read("")` and `fs_write("")` for every invocation (A.10).
  The single property this whole epic sells — *a read-only tool provably
  cannot write* — is destroyed by merging. A supervisor cannot know which
  applet will run.
* **Size and startup are no argument for merging.** Busybox exists to save
  space and `exec` cost. A one-tool cancho binary is 16 KB (`hello`) to
  22 KB (`seek`, 21,832 bytes); spawn cost is indistinguishable from
  `true` (0.98 ms against 1.01 ms for `/usr/bin/true`, 500 runs; `grep`
  1.30, `jq` 2.53, `rg` 4.56 — a probe, §B). `seek` builds in 0.2 s.
* **There is no dispatcher to write.** The language cannot `exec`.
  Discovery is a generated **index file** (`tools.json`: name, version,
  binary hash, authority, schema ids, reversibility), not a program.

**Alternatives.** (a) Multi-call binary: rejected on the first point.
(b) A multi-call binary *per authority class* (read-only, read-write):
legitimate and cheaper to distribute, defeats nothing the row says beyond
the class, and is a later optimisation if distribution count ever matters;
not needed at ten tools of 20 KB.

### D18. Where the code lives: a separate repository, `cancho-tools`

**Decision.** A separate repository, `cancho-tools`, with a `cancho.toml`
whose `[[bin]]` sections list one program per tool, the contract package as
a directory of modules inside it, `[package] cancho = "<commit>"`
pinning the compiler, and `[[test]]` sections for the unit tests
(`docs/package-system.md` §8.2, §8.8). Not `packages/` in this repository.

**Reasons, from the repository's own measurements.**

* **A tool's source breaks when the compiler moves.** `docs/hash-stability.md`
  measured that 71% of this repository's history stops type-checking under
  today's compiler (quoted in `docs/package-system.md` §8.1). Tools must
  therefore *pin* a compiler by commit, which is what the project file
  does; in-tree `packages/` are consumed at the same commit as the
  examples and never pin anything.
* **The precedent is the pattern the repository already set.** `cancho-hooks`,
  `cancho-log`, `cancho-web` are separate repositories using §7 and §8
  (`docs/package-system.md` §7.1); a tools repo is the same shape, and it
  can use `vcs lock --git` and `cancho install` unchanged.
* **Different change rates and different gates.** The compiler's file
  budget (`crates/cancho/tests/files.rs`), its two backends and its
  conformance suite are the wrong gate for a 20 KB tool; the tool repo has
  its own: M1–M9 (§7), which are slow and need GNU tools.
* **A `std` addition is a compiler release; a package is versioned apart.**
  Language gaps land in `cancho`; the tools wait for a compiler commit and
  bump a pin in a PR that shows the new authority diff
  (`docs/package-system.md` §7.5 step 6 is the missing half).

**What would argue for `packages/`**: the tools' tests would run in the
same PR as a compiler change that breaks them, instead of being found by a
canary. The mitigation is one **canary job in `cancho` CI** that builds the
pinned tools at the compiler's head and reports (not blocks) — cheap, and it
turns the 71% from a surprise into a number.

**Build requirement worth stating.** The compiler needs `clang` (≥ 15) and
`cc` at build time (`docs/package-system.md` §9.3). **No compiler release
has been published**, so a tools repo's CI builds the compiler from source
(about 1m23s, §9.1) until one is. A *user* of a tool needs only the binary.

**To confirm.** The repository, its name and ownership.

---

## 7. The measurement protocol

Two parts, and the line between them is the point. **Part 1 is offline,
deterministic and runnable in a sandbox like this one.** **Part 2 needs a
model in a loop; it cannot be run here and nothing downstream may be claimed
from it until it is.** This document reports **no result** from either part.

### 7.1 Offline and deterministic

Hermetic: a fresh temp directory, a fixed fixture tree, a pinned compiler
revision, a recorded environment. Conventions follow `scripts/bench.py`:
interleaved runs, minimum and median reported.

| | Measures | How | Gate |
|---|---|---|---|
| **M1** Schema conformance | every output is valid against its versioned schema | a corpus of invocations per tool (argv, stdin, fixture tree), each stdout validated with `jsonschema` (Draft 2020-12, `additionalProperties:false`); exit code in the declared table; a schema change classified additive/breaking by script | 100% valid, 0 undeclared fields, 0 codes outside the table; a breaking change without a major bump fails CI |
| **M2** Determinism | byte-identical reruns | every corpus case run twice and under varied `LANG`, `LC_ALL`, `TZ`, `TERM`, `NO_COLOR`, `PATH`, cwd, stdout as pipe, file and a pty | 100% byte-identical stdout and status |
| **M3** Rule coverage, soundness, actionability | every error path has a tag and a repair that works | one fixture per catalogued rule; run; check `error.rule`, exit code; if `repairable` is not `never`, **apply `repair.argv` by script, with no judgement, and rerun**; assert the declared outcome and that no flag it added increases authority | **coverage** (rules with a fixture / rules) = 100%; **hint soundness** (hints that succeed when applied / hints emitted) = 100% — a hint that fails is a bug; **actionability** (see below) is *reported* in S1 and gated once baselined, not before |
| **M4** Fault injection | no input reaches a trap | mutate argv, stdin and fixture state (missing/duplicate/oversized values, `..`, NULs in a file, a 1 GiB sparse file, a line longer than every cap, invalid UTF-8, a directory where a file is expected, EISDIR/ENOTDIR/ENOENT races) | **0** exits 132/139 over N cases (N fixed at S0 from a pilot of how fast new traps stop appearing); every case yields a valid envelope |
| **M5** Differential vs GNU | agreement on the shared semantic subset | per tool, a normaliser reduces both outputs to a comparable datum: `seek` vs `grep -F -n -b`; `peek` vs `sed -n`/`wc`; `jsonq` vs `jq -c`; `hash` vs `sha256sum`/`sha512sum`; `write` end-state vs `cp`; `list` vs `find -printf '%P\t%y\t%s\n' \| LC_ALL=C sort`; `tally` vs `LC_ALL=C sort \| uniq -c \| sort -k1,1nr -k2`; `diff`: apply our hunks with `patch` and compare the *result* (not the text — minimal diffs are not unique) | 0 divergences over N seeded cases plus the hand-written edge corpus (CRLF, no trailing newline, NUL, long lines, invalid UTF-8, empty file). The precedent is `both_ports_match_gnu_on_every_spelling` (`docs/flags.md` §1.1) |
| **M6** Authority | manifest equals the compiler's, within the ceiling | D12's three checks; the bridge totality check (every label the compiler can emit for these tools is in D13's table) | all three, for every tool; the label-mapping table is total |
| **M7** Mutation behaviour | dry-run does not write; apply is idempotent; atomic; two writers race | run `--dry-run` under `strace -f -e trace=file,rename,write` and a before/after hash of the tree; every mutating case twice; 200 trials of two concurrent writers with the same `--if-sha256` | dry-run: 0 mutating syscalls, tree unchanged; second apply `changed:false`; race: exactly one success, never two |
| **M8** Confinement | `--root` holds, lexically and through links | `..`, absolute, `//`, `./`, trailing `/`, sibling prefix, empty, 4096+ bytes, Unicode, a link to a file and a link to a directory outside the root | every case a tag, none a trap; *(flipped, #227)* every tool refuses both links with `path.symlink` and leaves the outside untouched |
| **M9** Performance and memory | startup, throughput, **memory flatness** | see below | no gate on speed vs GNU; a regression gate against the previous release's minimum; **memory flatness is a gate** |

**Actionability, defined.** Fault-injection cases from M4 that produce an
error are the denominator for *coverage of hints* (how many carry a
`repair`); the numerator for *soundness* is hints that work when applied.
These are two different numbers: a tool can have perfect soundness and
cover no case. Both are reported per tool and per rule; neither is set
before a baseline exists. (The temptation to write "≥ 90%" here is exactly
what this document should not do: a threshold from nowhere is the claim
outrunning the evidence.)

**Performance, as a method, not a result.**

```text
python3 scripts/toolbench.py --tool seek --size 64MiB --runs 15 --sink pipe
# per run: [tool, incumbent] interleaved; report min and median;
# record: machine, kernel, `cancho --version`, incumbent versions,
# locale, page-cache state (warm), sink.
# incumbents: grep -F, rg -F, sha256sum, jq, sed -n, LC_ALL=C sort|uniq -c, find
# startup: 500 spawns of an empty workload per binary, mean ms
```

Three pitfalls were met *in this document's own probes* and the harness
must defend against each:

* **GNU `grep` short-circuits when stdout is `/dev/null`.** `grep -c gamma`
  on 64 MiB took 3 ms with output to `/dev/null` and 100–160 ms through a
  pipe: a 40× illusion. The harness's sink is a pipe or file, and a self-test
  asserts the two differ for `grep` so a future change cannot silently
  reintroduce the shortcut.
* **The operation must match.** `wc` (all counts) took about 0.9–1.0 s on the
  64 MiB file, `wc -l` 15 ms, `wc -c` 3 ms: "faster than `wc`" means nothing
  without saying which. Each comparison names the exact incumbent command.
* **Locale.** `sort` and `wc -w` depend on it; the harness pins `LC_ALL=C`
  for the incumbent and states it. (Not demonstrable on this machine, which
  has only the `C` locales.)

**Memory flatness (the one performance gate).** Run each streaming tool on a
1 MiB, 64 MiB and 256 MiB input and read peak resident memory
(`ru_maxrss` of the child); require `max / min ≤ 1.5`. It is deterministic,
it tests D8's rule directly, and the existing `seek` fails it.

**Mutants for the harness itself** are in S3.

### 7.2 Agent in the loop (cannot be run in this sandbox)

> **This part has not been run, cannot be run here (there is no agent
> loop, model access or budget in this environment), and no sentence in
> this document, the README or any tool's `introspect` may say the toolbox
> is "more agent-friendly" or "improves task success" until it has.**
> What follows is the format, the harness, the metrics, the baselines and
> the gate. No number appears because none exists.

**Three arms**, because the contract and the implementation are separable
(§0):

| Arm | What the agent is given | What it isolates |
|---|---|---|
| **A. Incumbents** | GNU coreutils, `rg`, `jq`, `sed`, `find`/`ls`, as a shell | the baseline |
| **B. Incumbents + shim** | the same programs behind a *conforming* envelope: JSON, the D4 exit codes, rule tags, D6 hints, D10 preconditions — a wrapper in any language | the value of the **contract** alone |
| **C. Toolbox** | the cancho tools | B versus C is the value of the **implementation** (authority, determinism, defined behaviour) |
| **D. Toolbox, hints removed** (ablation) | C with `repair` always `null`, tags kept | the **kill rule** for D6 (`docs/agent-errors.md` §5.1) |

**Task suite format.** One JSON file per task, a directory of fixtures, a
verifier:

```json
{"id":"edit-precondition-03","category":"edit","goal":"…text the agent sees…",
 "fixture":{"tarball_sha256":"…","setup":["…seeded mutations…"]},
 "tools":{"A":["sed","grep"],"B":["…"],"C":["seek","peek","write"]},
 "faults":[{"after_call":2,"inject":"concurrent-edit","path":"src/a.cho"}],
 "oracle":{"cmd":"python3 verify.py","success":"exit 0","unsafe":"exit 7"},
 "budget":{"max_tool_calls":30,"max_tokens":60000,"max_wall_s":600},
 "seeds":[1,2,3,4,5]}
```

Categories (each with a *safety* twin where it makes sense): orient (find a
file), search, read-a-range, edit-with-precondition, **concurrent edit** (the
fixture changes the file between the agent's read and write: the right
behaviour is to notice, not to overwrite), **recover-from-error** (a fault is
injected: a mistyped flag, a `..`, a missing file), **refuse-unsafe** (the
goal tempts a write outside the root: success is *not* doing it), json-extract,
hash-verify.

**Harness.** One runner executes (task, arm, seed) in a clean sandbox (a
lex-os box where available, otherwise a container), with the model, version
and sampling parameters pinned and recorded, the tool descriptions generated
from each arm's own `introspect`/`--help` (so an arm is not advantaged by
better prose), and every call, response, token count and wall time logged to
a transcript. The oracle, not the agent, decides success and unsafe actions.

**Metrics.**

* *Effectiveness*: **success within budget** (the pre-registered primary
  metric); tool calls to success; tokens; wall time.
* *Recovery*: after an injected fault, was the next call a success
  (`recovery@1`)? how many calls to recover?
* *Hint use*: was `repair.argv` applied verbatim, adapted, or ignored? did
  the agent misparse an envelope (schema-misread rate)?
* *Safety*: **unsafe-action rate** (the oracle's `unsafe` exit), measured
  in a box whose grant is derived from the tools' manifests versus a box with
  broad exec.

**Analysis.** Paired by (task, seed); bootstrap confidence intervals; the
primary metric, the sample size (from a pilot) and the decision rule written
down **before** the run. Three claims are separate and each may fail:

1. *The contract helps*: B beats A on success or cost.
2. *The implementation helps safety*, not necessarily success: C beats B on
   unsafe-action rate under a derived grant. An honest expectation is that
   C and B tie on success — authority is not task success — and the benefit,
   if any, is in the safety column and in what a supervisor can *check*.
3. *Repair hints help*: C beats D. If not, `repair` is cut.

**Gate for S-last.** The protocol and the suite are reviewed and frozen; a
pilot sizes the sample; the run happens; the result is reported with its
intervals whichever way it goes. A claim is added to `README.md` only for
the comparison that cleared its pre-registered rule.

### 7.3 What may be said when

| Statement | Allowed after |
|---|---|
| "Every tool's output validates against its published schema" | M1 green |
| "Every error path carries a rule tag; every hint was applied by script and worked" | M3 green |
| "The authority a tool reports is the compiler's" | M6 green |
| "The tools do not crash on adversarial input" | M4 green *with its N stated* |
| "…and are as fast as / faster than GNU" | never claimed: not a goal |
| "More agent-friendly", "improves task success", "recovers from errors better" | **S-last, and only the comparisons that cleared their rule** |

---

## 8. Slices

Effort figures are **judgement, not measurement**, in working days for one
person who knows the repository; they are there so a person can see the
shape, not to be held to.

**S0. The contract package** (about 5–8 days). In `cancho-tools`, a
directory of modules, none over 2,000 lines: the error type and rule
catalogue; the envelope `Writer` over `std.json`; the D4 exit codes; the
table-driven flag parser over `std.flags`; D9's path validation; limits; the
chunked reader of D8; `text_or_bytes` with the base64 encoder; the
`introspect`/`skill` emitter; the incremental SHA-256 port.
*Gate:* `cancho test` green; `fmt --check` and `check` clean; envelope and
catalogue corpus validated (M1 for the contract itself); fuzz of the path
validator and the flag parser with 0 traps (M4 slice); `authority` of a
contract-only program within `{args, heap, io_write}`.
*Mutants (each must be killed by a named test):* envelope omits `schema`;
`INVALID_ARGS` and `NOT_FOUND` swapped; a tag renamed without its fixture;
validation accepts `..`; accepts `a//../b`; accepts an absolute path outside
the root; accepts `/rootevil` for `/root`; a limit check `>` for `>=`;
invalid UTF-8 written as a string instead of `{"b64"}`; the table says a flag
is boolean and the parser takes a value; `--` not honoured; the `end`
record omitted on an error path; `complete:true` after a limit stop; no
trailing newline; the hint adds `--root /`; `introspect` prints a stale table.

**L1. Checked output** (compiler; about 2–4 days). A way to learn that a
write failed. *Gate:* `cargo test --workspace` plus new conformance tests:
a program writing to `/dev/full` and to a closed descriptor observes the
failure on both backends; `bulk-io.md` §3.3's correction updated. *Mutants:*
the result ignored; the failure swallowed on one backend; flush at exit
errors dropped.

**S1. Two tools end to end** (about 8–12 days): `seek` v1 (read-only) and
`write`/`replace` (mutating). *Gate:* M1–M5, M7, M8 for both, M9 memory
flatness for `seek`, and the contract's rule-fixture equality.
*Mutants — `seek`:* the `--max-line-bytes` guard removed (M4 must hit a
limit); line numbers off by one; CRLF handled differently; a NUL-bearing file
treated as text; `--max-count` per file instead of total; the `end` record
dropped on `--max-count`. *`write`:* precondition skipped; `fsync` skipped;
rename before the data is written; the lock removed (the race must fail
within N trials; N is calibrated by running this mutant until it fails in
at least 95% of batches); a second apply not idempotent; temp not removed on
failure; hash taken over the wrong range; **dry-run writes** (M7's `strace`
must fail it).

**S2. The authority manifest, and the lex-os derivation** (about 5 days
here, plus a lex-os change of about 3–5 days). *In the tools repo:* D12's
two-pass embed and three-part gate; the ceiling file; D14's variant
transform. *In lex-os:* D13's bridge, its fixtures, and the two issues
(prefix classification; unknown labels defaulting to no authority).
*Gate:* M6; and, across the repositories, the bridge derives the expected
grant for every tool's manifest (hand-written expectations: `seek` fs
read-only, `write` fs read-write, none net or exec), refuses `ffi`/
`bounded:false`/unknown labels, and **the measured `net_out` hole is a
fixture**. *Mutants:* the mapping drops `net_out` (must turn the fixture
red); `ffi` mapped to nothing; an unknown label accepted; the `:port`
dropped from egress; the embedded manifest patched after link
(`introspect` differs from the compiler); two tools' manifests swapped;
the ceiling widened without a diff.

**S3. The benchmark harness, offline part** (about 5 days). `scripts/
toolbench.py`, the seeded corpus generator, the report format. *Gate:* the
harness is reproducible (two runs, same machine, same corpus, same
selection of incumbents); it has **power** (a planted 1.5× slowdown is
detected in at least 95% of 20 trials and identical reruns are flagged in at
most 5%); the `/dev/null` self-test of §7.1 passes. No result is published
from it. *Mutants:* sink switched to `/dev/null`; runs not interleaved; the
minimum replaced by the first run; the memory probe reading the parent's
`ru_maxrss`; locale not pinned.

**B1. Batch one** (about 3–5 days per tool): `peek`, `jsonq`, `tally`. Each
passes the S1 gate set with its own M5 normaliser and mutants (`peek`: an
off-by-one in `next`; binary detection removed. `jsonq`: depth cap off;
`--max-bytes` ignored; a pointer escape (`~0`/`~1`) mishandled. `tally`:
`--max-keys` ignored; ties not broken bytewise).

**L2/L3. `fs_list` and `fs_stat`** (compiler; about 5–10 days: two
backends, two targets, an edition, the `dirent` layout). *Gate:* both
backends; a hostile directory (10⁵ entries, non-UTF-8 names, a name 255
bytes long, a dangling link) lists identically on both and in bytewise order;
the row is `fs_read(p)`; the must-reject fixtures. *Mutants:* unsorted;
`d_name` read at the wrong offset on one target; the prefix check skipped;
`.`/`..` leaked.

**B2. Batch two:** `list` (after L2/L3), `hash` (after the std slice or with
the in-package copy). `list`'s M5 is `find -printf … | LC_ALL=C sort`.

**B3. Batch three:** `diff` only if an asker appears; `fetch` only as a D14
variant for internal HTTP, if one asks.

**S-last. The agent-in-the-loop evaluation** (about 15–25 days, dominated
by building the suite and by the run's cost; **cannot be started in this
sandbox**). *Gate:* §7.2's, including the pre-registered primary metric,
decision rule and sample size.

---

## 9. Risks

| Risk | Why it is real here | Mitigation |
|---|---|---|
| **Scope creep** | Every Unix tool is "ten lines" in the head and a `grep` clone in the diff; `jsonq` wants to be `jq` | D1's admission rule; `query.unsupported-syntax` with a `none` repair that names `jq`; the ten-tool ceiling is in `tools.toml`, so exceeding it is a diff |
| **POSIX expectations** | An agent will type `grep -r`. `docs/flags.md` found silent ignoring is the dangerous failure; `docs/agent-cli.md` found a generated surface teaching a wrong spelling | table-driven parsing (one table drives parse and `introspect`); an unknown flag is `args.unknown-flag` with the nearest declared flag as the hint, never ignored |
| **The claim outruns the evidence** | The easy sentence ("a safer, agent-friendly `grep`") is already in this document's own prior art | §7.3's table; `introspect.evidence` lists which gates have run; the README gets nothing from S-last until S-last runs |
| **Authority theatre** | `fs_read("")` with `bounded: true` reads as reassuring and means the whole filesystem | `not_narrowable` in every manifest (D12); §0 amendment 2; extent comes from D9/D13/D14 and the perimeter |
| **A trap is the failure mode** | Overflow, bounds, `Writer` misuse (a bare value in an object is a trap, `docs/json.md` §3), arena and heap exhaustion all kill with no JSON, and unflushed stdout is lost | M4; the `end` record; no `region` sized from data; caps with tags |
| **Maintenance** | 71% of this repository's history stops type-checking under today's compiler (`docs/hash-stability.md`); three targets' worth of `Split` and edition churn | compiler pin per release; a canary in `cancho` CI (D18); each tool is small (`seek` is 336 lines; `cut` 309; `sort` 359) |
| **Two truths about a tool's flags** | A flag table and a parser written separately will drift | D11: the table *is* the parser's input; M1 runs every declared flag |
| **Silent output failure** | L1 | the `end` record now; the compiler fix first (D16) |
| **Cost of linear types in the tools** | `out = f(h, out, x)` three tokens longer every time (`AGENTS.md` §1) and a `res` consumed on every path | a shared contract package so each tool is mostly its own logic; reported as lines per tool against the shim's, as a *cost* in S-last |
| **Cross-repository coupling** | The bridge lives in lex-os; the tools repo pins a compiler; the compiler gains builtins for the tools | each dependency is a named slice with its own gate and an owner (Q7) |
| **Benchmarks that flatter** | `/dev/null`, mismatched operations, locale — all three bit in the probes | §7.1's pitfalls and S3's self-tests |

---

## 10. Open questions for a person

| # | Question | Why it is not decided here |
|---|---|---|
| Q1 | Is JSON the default (D2), and does `seek`'s no-match exit change from 1 to 0 (D4)? | Both change the one existing tool's behaviour and a test that asserts it |
| Q2 | May the envelope drop `meta.duration_ms` and add `schema`/`rule`/`repair` (D3), and are errors on stdout (D5)? | A deviation from the ACLI SDK's `Envelope`; the spec text was not read |
| Q3 | Does `repair` ship, under the kill rule (D6)? What is the comparison's tolerance? | The previous round cut `fix` for lack of evidence; this one produces the evidence, and a person should agree the rule before the run |
| Q4 | Is "no blind overwrite" (D10) acceptable policy? | It will make an agent that has not read the file fail once |
| Q5 | Should cancho add `fs_list`/`fs_stat` (D16), and does `cancho-log` count as the second asker? | A language-scope decision with a two-asker bar and an edition bump |
| Q6 | Dry-run flag now, or the plan/apply split (D10)? | The flag is not provable by the row; the split is, and costs a binary and a plan format per mutating tool |
| Q7 | Does lex-os want a cancho authority input (D13), and will it fix path-scope narrowing and unknown-label handling? Who owns the bridge? | A cross-repository commitment; lex-os is not changed by this document |
| Q8 | Separate repository and name (D18); whether to register tools as lex-os commands with a path-scope facet (D13) | Ownership; and a facet nobody has written |
| Q9 | Which model, harness and budget for S-last; is a lex-os box available to run it? | Not available here, and the sample size depends on a pilot |
| Q10 | Should `cancho`'s own `check`/`authority` output be retrofitted to this envelope, since they are already agent-facing tools whose JSON has no `ok`? | Changes an existing, tested surface for consistency |

---

## Appendix A. Reproducers

Every one was run with the prebuilt compiler of Appendix B. Each is the
smallest program that shows the thing, written to a scratch directory.

**A.1. `narrow` takes a literal** (L5). A program that narrows `fs` to
`arg(g, 1)` is refused:

```text
{"rule":"capability-not-narrowable",
 "message":"`narrow` takes a literal, so the refinement can be checked where it is written"}
```

**A.2. No listing, no stat** (L2, L3). `fs_list(f, "/tmp")` and
`fs_stat(f, "/tmp")` inside a `borrow fs as &f` block are each refused:

```text
{"rule":"not-a-function","message":"`fs_list` is not a function in this program"}
```

Through `Ffi`, `opendir` and `readdir` type-check (`c_ptr` results) and the
report becomes `"bounded": false`, `foreign_symbols: ["opendir","readdir"]`;
a `c_ptr` is never dereferenced, so no name comes out. *I did not try
`getdents64` through `syscall`; it would be `ffi("libc")` as well.*

**A.3. No regex** (L8). `import std.regex;` is `unknown-name`
(*no module `std.regex` in this program*).

**A.4. `Fs` confinement** (L6, L7). `Fs` narrowed to `"/tmp/jailprobe"`,
path from `argv[1]`, `fs_read` into a 64-byte slice, exit status = bytes read
(100 for -1):

```text
$ ln -s /tmp/outside/secret.txt /tmp/jailprobe/link.txt
/tmp/jailprobe/in.txt                       -> 14        (14 bytes: ok)
/tmp/jailprobe/missing.txt                  -> 100       (-1: missing is a value)
/tmp/jailprobe/link.txt                     -> 20        (the OUTSIDE file's bytes)
/tmp/jailprobe/../outside/secret.txt        -> 132       (SIGILL, nothing printed)
/tmp/outside/secret.txt                     -> 132
in.txt                                      -> 132       (relative path)
/tmp/jailprobeevil/x                        -> 132       (sibling prefix)
/tmp/jailprobe                              -> 100       (a directory: -1)
```

With `Fs` left unnarrowed (`Fs("")`, same program shape): a relative path
that exists is read (150 = 5014 mod 256, 14 bytes), a relative path that
does not exist is a value (`ENOENT`), and **`../x` and
`/tmp/jailprobe/../jailprobe/in.txt` both exit 132**.

**A.5. Errno survives** (`open_read` then `file_read`, status = a code
mod 256: 1000+errno at open, 2000+errno at read, 5000+bytes): a regular
file `5014`; `/nonexistent` `1002` (ENOENT); `/tmp/jailprobe/in.txt/x`
`1020` (ENOTDIR); `/tmp` `2021` (**open succeeds, read is EISDIR**); `""`
`1002`.

**A.6. `sha256` past 64 KiB** (L4). Read stdin into a `std.buffer`, call
`crypto.sha256(buffer.bytes(t), contents(o))`, print the digest:

```text
bytes 1000, 60000, 65527   -> exit 0  (65527: first 16 hex digits equal sha256sum's)
bytes 65528, 70000, 8e6    -> exit 132
sha512: 65519 ok, 65520 traps
```

The bound is `(n + 9 + 63) / 64 * 64 <= 65536`: the padded copy is a `region`.

**A.7. Output failure is invisible** (L1):

```text
fn main(world: World) -> [] int {
    …release the rest…
    var got = 0;
    borrow mut io as &!i in { got = io.write_all(i, "hello\n"); }
    release(io);
    return got;
}
```

`./wr` exits 6 and prints `hello`; `./wr >&-` exits 6; `./wr > /dev/full`
exits 6 and `strace -e trace=write` shows `write(1, "hello\n", 6) = -1
ENOSPC`. A loop of 1,000 sixty-five-byte `write_all` calls counts the
short answers: 15 to `/dev/full`, 7 to a closed stdout, 0 to `/dev/null`
(the failure surfaces only once the buffer fills). A program that writes `{"partial":` and calls `trap()` prints
nothing through a pipe (status 132). `./flood | head -1` reports the
producer's status as 141.

**A.8. An atomic replace is expressible.** `open_new` a temp, `file_write`,
`file_sync`, `file_close`, `fs_rename` over the destination (edition 5):
`authority` is `["file_write","fs_write"]`, `fs_write("")`, `bounded: true`;
running it twice exits 0 both times and leaves the destination intact. A
first draft that declared `fs_read("")` it did not use was refused
(*"a row is exact or it is decoration"*).

**A.9. Nothing is generic over a prefix.**
`fn probe[&f, p, &q](fs: &f Fs(p), path: &q [byte]) -> [fs_read(p)] int` is
`type-mismatch`: *expected a string literal, found an identifier*.

**A.10. A multi-call binary has the union.** A program that picks a
`cat`-like function (`fs_read("")`) or an `rm`-like one (`fs_remove`,
`fs_write("")`) from `argv[1]` reports `args, fs_read(""), fs_write("")`;
deleting the `rm` branch gives `args, fs_read("")`.

**A.11. The manifest fixed point.** A program returning a string constant;
`authority --output json` → JSON `J1`; rebuild with the constant set to
`J1`'s canonical text (223 bytes); `authority` again → `J2`. `J1 == J2`, and
the built binary prints the constant byte for byte.

**A.12. A narrow network row exists.** `edition 5;` with `Net` narrowed to
`"api.example.test:80"` and `tcp_connect`, `conn_write`, `conn_read`:
`effects ["conn_read","conn_write","net_out"]`, `net_out` argument
`"api.example.test:80"`, `bounded: true`. The row's label is `net_out`, not
lex-os's `net` — the reason for D13's table.

**A.13. Invalid UTF-8 is replaced silently** (L10). `json.put_string` of the
three bytes `61 FF 61` writes `{"text":"a\ufffda"}` and nothing signals that
a byte was lost; `std` has no base64, so a tool must pre-check with
`std.utf8.is_valid` and encode the bytes itself.

## Appendix B. Probe record

* Compiler: `cancho 0.0.0 (rev 052e623fb55a909f531456cd941bd375ab202dfc,
  host x86_64-unknown-linux-gnu)`, `/home/user/cancho/target/release/cancho`;
  `std/` and `crates/` identical to commit `ab332d3` (only tests and scripts
  differ).
* Machine: Linux 6.18, x86_64, 4 cores, a shared sandbox. **Noisy.**
  Locales installed: `C`, `C.utf8`, `POSIX`. GNU `grep` 3.11, coreutils 9.4,
  `rg` 14.1.0, `jq` 1.7, `curl` 8.5.0, Python 3.11 with `jsonschema` 4.26.
  **Not installed:** `fd`, `nu`, `hyperfine`.
* Fixtures: a 64 MiB text file (67,108,892 bytes, 1,307,997 lines, random
  words from a seeded generator) and a 4,675,827-byte JSON array of 60,000
  objects.
* Timings, one probe each, **not protocol results**: spawn, mean of 500:
  `hello` 0.98 ms, `/usr/bin/true` 1.01, `grep -c x /dev/null` 1.30,
  `jq -n 1` 2.53, `rg x /dev/null` 4.56. Search for `gamma` on 64 MiB
  through a pipe, 5 runs: `seek -c` 0.42–0.57 s, `grep -c` 0.12–0.17,
  `grep -F -c` 0.11–0.18, `rg -c` 0.08–0.11; `seek -c` on stdin 0.54–0.71
  (3 runs). `cat` 14 ms, `wc -c` 2.5 ms, `wc -l` 15 ms, `wc -w` 0.9–1.0 s,
  `sha256sum` 60 ms, `sort` 0.37–0.55 s. `examples/tally.cho` 0.35–0.38 s.
  JSON: parse of the 4.7 MB document 0.095 s with 119,184 KB peak; `jq length`
  0.153 s. `seek` peak resident memory: 8,304 KB at 1 MiB, 13,776 KB at 8 MiB, 99,788 KB at 64 MiB exactly, 198,032 KB at 64 MiB + 28 bytes, 787,816 KB at 256 MiB + 112 bytes. `cancho build`
  of `seek.cho` 0.206 s, a 21,832-byte executable; `hello` 16,056 bytes.
* **The lex-os probe** is a scratch crate outside both repositories, with
  `lex-os-authority` and `lex-os-manifest` as path dependencies and
  `lex-types` at the tag lex-os pins (`v0.11.0`), built with
  `--ignore-rust-version` because the local `rustc` is 1.94.1 and those
  crates declare 1.95. It calls `derive_from_effects` and `diff` on
  hand-built effect sets and `derive` on the body of `cmd_write_report`. It
  is not committed (it would be a second derivation of authority) and is
  described in §2.5 precisely enough to rebuild.
* The cited `lex-lang` source (`trust.rs:555`) is the working copy at
  `/home/user/lex-lang`, which may be newer than the `v0.11.0` tag the probe
  compiled; the probe's *measurements* are of the tag.
* No tool, library or compiler change was made. No test was run: the only
  changes are documents, and `every_documentation_link_resolves` is the only
  test that reads them.

## Appendix C. What this changed in other documents

Corrected in place, each with a note naming this document, as
`CONTRIBUTING.md` asks:

* **`docs/bulk-io.md` §3.3**: *"Output has no such question: it wrote the
  bytes or the process is gone."* False (§2.1, A.7).
* **`docs/agent-tools.md` §1**: *"No silent truncation."* True of what `seek`
  reads, false of what it writes (`/dev/full` exits 0), and `seek` holds
  the file (between 1.5 and 3 times its size in memory) rather than a line
  (§2.1, §2.3).
* **`docs/agent-cli.md` §3**: *"cancho's real codes are 0/1/2/3"*. There is a
  fourth, `4`, for `cancho test` (`crates/cancho/src/main.rs` header,
  `docs/testing.md` §3).
* **`docs/README.md`, "cancho code under a lex-os grant"**: *"`network` and
  `exec` are not [enforceable], because both are libc."* Not true of
  `network` for a program built on `Net` (A.12); and the sentence's
  *"enforceable through it"* does not yet hold for either dimension, because
  lex-os cannot read the labels (§2.5).

Found and **not** corrected (not documents' claims, or not this change's to
make): `examples/tally.cho` prints `wc`-style padded columns that run together
when a count exceeds the field width (`1307997 11774082 67108892` printed as
`13079971177408267108892`; GNU `wc` separates them), which is the plainest
argument for D2 in this repository; and the two lex-os findings of D13
(prefix narrowing classified as widening; `off_lattice` as the default for a
label the front end does not know).
