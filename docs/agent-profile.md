# `cancho profile`: a profiler whose answer is a number an agent can gate on

> **Status: slice 1 built (§9, on `valgrind`) and replaced as the default by an instrumented build (§10); `--machine` keeps §9.** Written before the code. Measured
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

## 10. The default is an instrumented build (`ciinstr.cho`, `cliprofile.cho`)

§9 found that at `-O2` most of a hot loop has no line (73% of `spin.cho` was
unattributed), so a profile of the shipped binary under `valgrind` could not say
which function was hot, and that the tool needed `valgrind`, which CI installs in
one job. The compiler can count what the program does itself
(`agent-debug.md` §11 describes the build): each statement counts its
executions; its weight is the number of IR instructions the emitter wrote for it
(a static count); a function's cost is the sum over its statements of executions
times weight; and the wrappers `cancho_i_malloc`, `cancho_i_calloc` and
`cancho_i_region_take` count each call against the statement that made it. At
exit the run time writes one line per function to the file named by
`CANCHO_TRACE_OUT`.

```sh
cancho profile [--std] [--top N] [--record F] [--baseline F] [--factor F] [--strict] FILE... [-- ARGS]
cancho profile --machine [--unoptimized] ...        # §9: machine instructions under valgrind
```

The answer is the one of §3 (`functions`, `other`, `unattributed`, `external`,
`allocations`, `findings`), with **`"metric": "ir-instructions"`**, `unattributed`
and `external` always 0 (the run time attributes every statement, and does not
see the C library), and the baseline file in the same shape. **A baseline carries
its metric** and is not read as the other (`profile-baseline-unreadable`), as
the test checks.

**Measured (this document, Linux x86-64):**

| claim | result |
|---|---|
| attribution at `-O2` | `spin.cho`: `inner` 33,002,000 of 33,006,217 (2,000,800 statements), `outer` 4,209, `main` 8; **nothing unattributed**, where §9 had 73% |
| §7.2, the same program counts the same | two runs give identical answers, and so do `-O2` and `--unoptimized`: the count is of the program and not of what `clang` made of it |
| §7.5, allocations | `std.buffer` grown to 100,000 bytes: 17 calls at `reserve`, 1 at `empty` — the same as `valgrind` found; the `region` finding of #391 is still not reproduced (the wrapper for `cancho_region_take` exists and is not exercised) |
| cost | `spin.cho` builds and profiles in 0.26 s in all; a run of 200 × 5,000 iterations takes 5 ms |
| baseline | recorded, passes, twice the work is `profile-regressed` for `inner@spin.cho` and the total, as in §9 |
| `--machine` | the answer of §9, unchanged; when `valgrind` is not there the test says so and does not try it |

**What this metric is, and is not:**

* **It is the work the program asks for, in the units of the IR the emitter
  writes before `clang`.** It is deterministic, portable (no `valgrind`, no PMU,
  macOS and aarch64 alike) and does not change with the optimiser. It does **not**
  see what `clang` does (a loop it vectorises is as dear as one it does not), the
  C library (`memcpy`, `write`) or the kernel. A regression in the *compiler's*
  code quality (#489: 1.26 to 1.32 times on the P-256 rows) is invisible to it;
  that is what `--machine` is for.
* **Weights are static**: a statement in a loop that the program runs a million
  times costs a million times its weight, but a call's cost is the callee's, not
  the call's.
* The counts are not atomic; threaded programs' are approximate.

**Not done:** §7.1 (agreement with `tls_perf.py`; the two metrics differ by
design), §7.6 beyond the bad baselines, §7.7, §7.8; the cost of the
instrumentation on a larger program than `spin.cho` (the program runs 5 ms
here); a `region` count on `https_hello`; a machine tag in a baseline.

## 11. `--advise`: the facts for deciding what to optimise, and no judgement

```sh
cancho profile --advise [--std] [--top N] FILE... [-- ARGS]      # N is 5 unless given
```

The question was whether the profiler could say how much effort an optimisation would be. **It does not, by decision (the user's, 2026-11-05):** the agent
estimates the effort from the output, as it reads the code; the profiler measures. So the report has no `effort`, no advice and no verdict, and a test
fails if one of those words appears in it. What it has, for each of the functions with the most instructions of their own:

| field | what it is |
|---|---|
| `function`, `file`, `line`, `size_lines` | where it is and how long it is (to the first line that is only `}`) |
| `self`, `share` | its instructions, and `self / total` |
| `max_speedup`, `if_halved` | `total / (total - self)` and `total / (total - self/2)`: **arithmetic on the counts**, the most the program gains on this metric if the function cost nothing, or half |
| `calls`, `statements`, `statements_per_call`, `ir_per_call` | a loop shows as a large number of statements a call; a function called a million times for little shows as many calls and a small `ir_per_call` |
| `allocations`, `allocations_per_call` | allocator calls made from its statements |
| `hot_lines` | up to three lines with the most cost inside it (`executions`, `cost`, `share_of_function`) and **the text of the line**, so the agent need not open the file |

and, for the whole report: `total`, `functions` (how many ran), `covered` (the share the listed functions make up, so a flat profile is visible as a small number), `basis` (what each field means,
in a sentence) and `limits` (what is not measured: the C library and the kernel, what `clang` does to the code, and the threads' races).

It is the same run as `profile`, with one more thing from the run time: `CANCHO_TRACE_STATEMENTS` in the environment makes the exit dump also write a line for each statement that ran (`st`
lines), and the call counter of each function (`cancho_i_enter`) is in the `fn` line. `--advise` does not go with `--machine`, `--record` or `--baseline` (exit 2).

**Measured (this document):** `spin.cho`: `inner` is 0.9998 of the run, `max_speedup` 7826.94, 200 calls, 10,004.0 statements a call, its two hottest lines are `total = total + i % 7;` (1,000,000
executions, 0.6969 of the function) and `i = i + 1;`; `grow.cho` with the library: `push`, `fill` and `reserve` each take 0.30 to 0.39, `reserve` makes 17 allocations in 100,000 calls (so `allocations_per_call`
has five decimals), and no function is dominant (`covered` 0.9999 with three listed). The test checks the bound against the counts, the share, the lines and their text, the library's names and that the report
contains no word of advice.

**Not done:** an `ci` step that runs `--advise` when the baseline regresses (the diagnosis with the failure); callers (which function called a hot function most), which needs an edge counter in
`cancho_i_enter`; the cost of the instrumentation on a program larger than `spin.cho`.
