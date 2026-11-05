# `cancho profile`: a profiler whose answer is a number an agent can gate on

> **Status: slice 1 built (§9): the profile, the allocator section and the baseline. Gates 1, 4, 5 and 7 of §7 are not met.** Written before the code. Measured
> facts are marked; everything else is a **gate** in §7, not a result.
>
> **Written in cancho.** `cancho profile` is a slice of the command line in
> cancho (`examples/selfhost/cli*.cho`), driving `valgrind` as a child. It
> does not live in `crates/cancho/src/`, and it replaces nothing in
> `scripts/tls_perf.py`, which stays as the oracle the new tool is checked
> against (§7.1) and is not extended.

## 1. The question

Every profile in this repository was made by hand. `tls-performance.md` and
`record-in-place.md` ran `perf record --call-graph dwarf`, fed the samples
to a script, and read the result. The numbers were good and the process was
a person's: choose a tool, find that the clock moves by a third under load
(`p256-fast.md` records `powersave`), switch to instruction counts, write a
parser. An agent asked "why is this slow?" has none of that, and a profile
it cannot repeat is a profile it cannot gate on.

So the design starts from what the repository learned the hard way:

> **Count instructions, not seconds.** On the machine of
> `tls-performance.md` §2 the time of one operation moved by a factor of
> two within a minute; the instruction count did not.

A profile that is deterministic is one run, not five, and one run is what an
agent has the patience for.

## 2. Measured here (2026-11-05)

* `valgrind` is present on this host; `perf` is not. `scripts/tls_perf.py
  check` already prefers callgrind's `Ir` and falls back to time only where
  valgrind is absent, with a wider factor (`--time-factor 2.0` against
  `--factor 1.3`).
* The compiled code keeps no frame pointers (`tls-performance.md` §3;
  searched `cancho-codegen-llvm/src`: no frame-pointer attribute). Callgrind
  does not unwind with frame pointers, so this costs nothing here, and it
  is the reason `perf` needed `--call-graph dwarf`.
* `-O2` is always on (`cancho-codegen-llvm/src/lib.rs`). A profile of the
  shipped binary is a profile of `-O2`, which is what is wanted, and the
  opposite of `trace`'s choice (`agent-debug.md` §3.2).

## 3. The command

```sh
cancho profile PROG.cho [--std-dir DIR] [-- ARGS...]
    --top N                        functions listed (default 20)
    --baseline FILE                compare, and exit 1 on a regression
    --record FILE                  write FILE from this run
    --factor F                     default 1.3, as tls_perf.py
    --stdin FILE                   the program's input (default: empty)
    --output json                  the only form
```

It builds with line tables (`-g`, `agent-debug.md` §3, **without** `-O0`),
runs the binary under `valgrind --tool=callgrind` as a child, reads
`callgrind.out` (a text format, deterministic) and answers once:

```json
{
  "metric": "instructions",
  "total": 482113907,
  "functions": [
    { "name": "bigmod.mul", "file": "std/bigmod.cho", "line": 120,
      "self": 191004211, "inclusive": 191004211, "calls": 90112 },
    { "name": "main", "file": "app.cho", "line": 1,
      "self": 1203, "inclusive": 482113907, "calls": 1 }
  ],
  "allocations": { "calls": 112, "bytes": 7340032, "by": [
    { "name": "region", "calls": 96, "file": "std/tls.cho", "line": 211 }
  ]},
  "other": { "self": 1209334, "functions": 214 }
}
```

* **`self` and `inclusive`**, as callgrind's `Ir`. Callgrind's `fn=` names
  are the emitted symbols (`lexs_<item>`); with line tables the file and
  line are the cancho ones, and the name is the cancho name. A symbol that
  cannot be mapped is reported under its symbol, never dropped.
* **`other`** is everything below `--top`, so the list sums to `total` and
  an agent need not guess what was left out.
* **`allocations`** is a first-class section, not an afterthought, because
  it is where the largest finding of the TLS work came from: *every
  `region` was a 64 KiB `malloc` and `free`, 6 to 10% of a handshake and
  45 to 53% of a request on a kept connection* (`tls-performance.md`
  summary in `ROADMAP.md`, #391). It counts calls to the allocator and
  attributes them to the first cancho frame above.

## 4. The gate: `--baseline`

A baseline is a small committed file recorded by `--record`, one function
per line (a regression is one changed line in a diff):

```json
{ "format": 1, "metric": "instructions", "total": 482113907,
  "functions": { "bigmod.mul": 191004211, "main": 1203 } }
```

| verdict | when | tag | exit |
|---|---|---|---|
| regressed | a function's `self`, or the `total`, is above `factor` times the baseline's | `profile-regressed` | 1 |
| improved | below `1/factor` times | `profile-improved` | 0, or 1 with `--strict` |
| new | a function over 1% of `total` that the baseline lacks | `profile-new-hot` | 1 |
| gone | in the baseline, not in the run | `profile-gone` | 0 (a function inlined away is not a regression) |
| no baseline | `--baseline` names a file that is absent | `profile-baseline-missing` | 1 |
| unreadable | not JSON or unknown `format` | `profile-baseline-unreadable` | 1 |

`improved` asks for a re-record in the same change, which is
`tls-performance.md` §1.3's rule for a primitive that got cheaper: a
baseline that is loose in one direction hides the next regression.

The factor default is `tls_perf.py`'s 1.3, taken because it was chosen
against real noise, not because it is right here. §7.2 measures the noise
of *this* tool before the default is kept.

## 5. Findings and exits

| tag | when | exit |
|---|---|---|
| `profile-unavailable` | no `valgrind` on the host | 3 |
| `profile-unsupported-host` | valgrind exists and does not run the binary (it does not support darwin-aarch64) | 3 |
| `profile-failed` | the program exited non-zero or trapped; the report is **not** given, because a profile of a crash measures the crash | 1 |
| `profile-timeout` | the run passed the bound | 1 |
| `profile-unsupported` | the program is one the cancho compiler has no code for | 3 |

There is **no silent wall-clock fallback.** `tls_perf.py` has one and says
so in its docstring; an agent reading JSON would not see the docstring, so
here the absence of valgrind is a finding, and a time-based mode is a flag
the caller asks for (`--metric time`, with `metric: "time"` in the answer
and a wider default factor), never a substitution.

Exit **0** is a successful profile; exit 1 is only for a gate that failed or
a program that did not complete. A slow program is not a failure.

## 6. What it costs, and does not claim

* **Callgrind is 20 to 50 times slower than the program** (a rule of thumb;
  *not measured here*). A program whose natural run is a minute is not
  profilable. `--timeout` defaults to 10 minutes and the finding says so.
* **Instructions are not time.** A program that stalls on memory or on a
  syscall spends its time where the count does not look. This is the one
  thing `perf stat` cycle counts give that this does not, and `bulk-io.md`
  §4.1's lesson applies: *volume is not time*. The report says
  `"metric": "instructions"` and nothing in it claims seconds.
* **Not on every host.** No valgrind on darwin-aarch64. The repository's
  CI has both (`ci.yml`), so a baseline recorded on linux-x86_64 is not
  comparable with one recorded on linux-aarch64: `format` carries a
  `machine` tag (`tls_baseline.json` has one entry per machine), and a
  mismatch is `profile-baseline-unreadable`, not a comparison.
* **Only programs the compiler in cancho accepts** (`cli-in-cancho.md`
  §14), as `agent-ci.md` §7 says for `ci`.

## 7. Gates

1. **Agreement with the oracle.** On the operations of
   `tests/programs/tls_prims_bench.cho`, `cancho profile` gives total
   instruction counts within 2% of `scripts/tls_perf.py check`'s, which is
   a different parser over the same callgrind output. Disagreement is a
   defect in one of them and is found before either is trusted.
2. **Noise.** The same program profiled 20 times gives the same counts
   exactly (the claim of §1). If it does not (ASLR, a clock read, a hash
   seed), the cause is named here and the factor is chosen from the
   measured spread, not from `tls_perf.py`.
3. **Line tables change nothing.** The count with `-g` is within 0.1% of
   without (debug info is metadata). If not, the profile is of a different
   program.
4. **Attribution.** For 10 hand-built programs with a known hot function
   (a loop that calls one function a known number of times), `calls` is
   exact and `self` is within 1% of the analytical count.
5. **Allocations.** On `https_hello`'s per-request path the `region`
   finding of #391 is reproduced by `allocations.by` without being told
   where to look.
6. **No input reaches a trap.** A fuzzer over `callgrind.out` and over the
   baseline file (truncated, a function named with a newline, a 1 GiB file)
   yields only a tagged finding.
7. **Authority.** `cancho authority` of the tool shows `Exec` narrowed to
   `valgrind` and the one built binary, `Fs` to a scratch directory, no
   `Net`; the report is quoted here once it exists.
8. **Value to an agent** is a hypothesis, as `agent-debug.md` §8.8: the
   evidence is an agent given the TLS handshake and asked what to change.

## 8. Open

| Question | Why it waits |
|---|---|
| A flame graph | an agent reads JSON; a picture is for a person, and `callgrind_annotate` already prints one |
| Sampling (`perf`) on hosts without valgrind | no PMU in the VMs the project used; a `--metric time` fallback is what exists |
| Memory high-water (`massif`) | `allocations` is calls and bytes; peak is a different question with no asker |
| Profile-guided optimisation | the backend always runs `-O2`, and a profile consumed by the compiler is a design of its own |

## 9. Slice 1, as built (`cliprofile.cho`)

```sh
cancho profile [--std] [--std-dir DIR] [--unoptimized] [--top N] [--record FILE] [--baseline FILE]
               [--factor F] [--strict] FILE... [-- ARGS...]
```

In cancho (`examples/selfhost/cliprofile.cho`, about 800 lines, a dispatch in
`cli.cho`); no Rust. It builds the program with `-g` as a child of this command
line, runs it under `valgrind --tool=callgrind --compress-strings=no
--compress-pos=no`, reads the file callgrind wrote, and answers once. The
program and the profile are written beside where it is run and taken away, also
when the run fails (the test looks).

**What building it changed in this document** (corrected in place):

* **Callgrind's function names say nothing, and its inclusive costs are
  gone.** The optimiser puts the whole program into the C `main` (the call
  tree of `spin.cho` is `main` and libc, measured), so `fn=` names the
  entry and nothing else, and `--read-inline-info=yes` changed nothing for
  callgrind. What survives inlining is the *line*: each cost line carries a
  `.cho` file and a line. So the profile attributes the cost of a line to
  **the function whose `fn` it is under**, found by reading the source file
  (`fn ` or `pub fn ` at the start of a line; the library's files are read
  from the copy in this binary). Consequences: §3's `inclusive` and `calls` per
  function are **not** in the answer (they would need the call tree), and
  neither is `bytes` in `allocations` (callgrind does not count them).
  A function with the same name twice in a file is two entries.
* **At `-O2` most of a hot loop has no line.** Measured on `spin.cho` (a
  loop of `i % 7`): of 15,118,961 instructions, **11,001,012 (73%) are at
  line 0** or in the support file, which is where the optimiser puts what it
  hoists, merges or vectorises; 4,000,000 are attributed. §2's "a profile of
  the shipped binary is what is wanted" is true and does not help when the
  shipped binary has lost its lines. So the answer has an `unattributed`
  section, the finding `profile-mostly-unattributed` when it is over half, and
  `--unoptimized`, which builds with `-O0` (`build -O0`, a flag of this command
  line only): the same program is 44,123,661 instructions, 6,000,809 (14%)
  unattributed, and `inner` is 38,001,600 of them. The counts then differ from
  the shipped binary's by a factor (2.9 here), and a baseline is only
  comparable with one recorded the same way.
* **A baseline records `name@file`** (a line number would change with every
  edit above it) and the total, one function to a line.
* **Run it by a path**, not by a bare name or one with `..` (the finding
  `profile-self-path`, exit 3), as `ci` (`agent-ci.md` §12).

**Rules** (`rule` in `findings`; standard error as `RULE: sentence` for the
ones with no answer to give): `profile-regressed`, `profile-improved`
(a failure under `--strict`), `profile-new-hot` (a function over a hundredth of
the run that the baseline lacks), `profile-baseline-missing`,
`profile-baseline-unreadable`, `profile-mostly-unattributed`; on standard
error `profile-unavailable` (no `/usr/bin/valgrind`, exit 3),
`profile-failed` (the program did not end with 0, or no profile was written,
exit 1), `profile-unwritable` (3), `profile-self-path` (3). A function is
compared when it is a hundredth of the run or more; `--factor` is in hundredths
(`1.3`, default; at least 1).

**Measured (this document, Linux x86-64,
`profile_in_cancho_counts_instructions_by_function_and_gates_on_a_baseline`,
64 s, valgrind 3.22):**

| claim | result |
|---|---|
| §7.2 the same program counts the same | two runs of `spin.cho` give byte-identical answers |
| the parts add up | functions + `other` + `unattributed` + `external` equals the total, at `-O0`, at `-O2`, and with the library |
| §7.4 attribution | `inner` is the first function, at its line, with over 80% of the run (a hand count of the loop is not made) |
| §7.5 allocations | `std.buffer` grown to 100,000 bytes: 18 allocator calls, **17 at `reserve`** of `<std>/buffer.cho` and 1 at `empty` (doubling), found without being told where to look; the `region` finding of #391 is *not* reproduced (§7.5 stands open) |
| baseline | the same run: no finding; twice the work: `profile-regressed` for `inner@spin.cho` and the total, exit 1; half: `profile-improved`, exit 0, exit 1 under `--strict`; `--factor 3` lets the slower run pass; a missing file, `not json`, `{}`, an empty file, a file with no functions and one with metric `seconds` are findings, never a crash |
| `-g` finding that this needed | with the library, `clang` dropped all debug information (a call in the crypto support code had no `!dbg`: that text is not indented); `cidebug.cho` now annotates every instruction that is not a comment or a label (`agent-debug.md` §3.4) |

**Not done:** §7.1 (agreement with `tls_perf.py`), §7.3 (counts with and
without `-g`: the disassembly is the same, `agent-debug.md` §3.4, but the
counts were not compared), §7.6 beyond the six bad baselines (no fuzzer, no
`callgrind.out` fuzz), §7.7 (`cancho authority` of the tool), §7.8; `--metric
time`; a machine tag in the baseline (a baseline recorded on aarch64 is
read on x86-64 and says everything regressed or improved); programs that
read standard input (`--stdin`); a timeout (a program that does not end is
waited for, up to the day `run_capture` allows). Callgrind makes a program
20 to 50 times slower (not measured here: `spin.cho` is 15 million
instructions and took 0.55 s with the build).

`profile` is in `introspect` and `skill` of the command line in cancho (registered with `ci` and `trace`; `agent-ci.md` §10), and not in `--help`.
