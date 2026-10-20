# CI speed

> **Status: measured; sections 4 and 5. A build cache does not help, and none is added; the jobs are split instead (section 5).** Written to
> decide whether to add `Swatinem/rust-cache@v2` to the jobs of `.github/workflows/ci.yml`. A claim that turns
> out false is corrected here, in place (section 3 does so for the first version of this document).

## 1. The question

Every job rebuilds the workspace from nothing. A dependency cache is the usual remedy. Whether it helps depends
on how much of a job is compiling, so that is measured first.

## 2. Where the time goes (before the change)

Job seconds on `main` at 7e4088b (run 2328, all jobs green):

| job | seconds |
|---|---:|
| `tls-assurance (examples)` | 2090 |
| `selfhost (linux-x86_64)` | 1858 |
| `darwin-aarch64` | 1436 |
| `selfhost (darwin-aarch64)` | 1439 |
| `linux-x86_64` | 1326 |
| `selfhost (linux-aarch64)` | 1279 |
| `tls-assurance` | 933 |
| `tls-assurance (server)` | 623 |
| `stability-plateau` | 400 |
| `tls-perf` | 136 |
| `semantic-index` | 89 |
| `satisfy-contract` | 58 |

Inside `linux-x86_64`, the `Tests` step is 1278 s of 1326; formatting is 1 s, clippy 22 s, the smoke program 13 s.

**What compiling costs.** `cargo test --workspace --no-run` from an empty target directory, on the machine this was
written on (not a CI runner): 76 s wall, 166 s of CPU. A hosted runner has two cores, so call it two to three
minutes. Against a 1278 s test step that is **at most about 10 %**, and it is the only part a build cache can
restore. The rest is running the tests: the conformance suite builds and runs hundreds of programs, which no cache
of Rust artifacts holds.

## 3. Measured with the cache, and against the noise

`Swatinem/rust-cache@v2` was added to all nine jobs on this branch and run twice (the first run fills the
caches; the second restores them). Job seconds, `main` at 7e4088b (the single run section 2 used), the first
cached run and the second (warm) cached run; restore plus save cost 6 to 27 s a job:

| job | main 7e4088b | cached, first | cached, warm |
|---|---:|---:|---:|
| `tls-assurance (examples)` | 2090 | not recorded | 2850 |
| `selfhost (linux-x86_64)` | 1858 | 1513 | 1517 |
| `selfhost (darwin-aarch64)` | 1439 | 1298 | 1289 |
| `darwin-aarch64` | 1436 | 1476 | 1470 |
| `linux-x86_64` | 1326 | 2152 | 2166 |
| `selfhost (linux-aarch64)` | 1279 | 1296 | 1221 |
| `tls-assurance` | 933 | 969 | 936 |
| `tls-assurance (server)` | 623 | 735 | 742 |
| `stability-plateau` | 400 | 360 | 383 |
| `tls-perf` | 136 | 96 | 96 |
| `semantic-index` | 89 | 78 | 44 |
| `satisfy-contract` | 58 | 95 | 34 |

Read alone, `linux-x86_64` looks 63 % slower with the cache. It is not. **The same job without any cache** ran
39 times in the 3 days before (the API's `ci` runs, pull-request and push events, 2026-10-16 to 10-19):

| job | runs | min | median | max | std. dev. |
|---|---:|---:|---:|---:|---:|
| `linux-x86_64` | 39 | 1244 | 1961 | 2366 | 317 |
| `darwin-aarch64` | 39 | 578 | 1405 | 1770 | 275 |
| `selfhost (linux-x86_64)`, pull requests | 17 | 1106 | 1817 | 1960 | 273 |

So 1326 s, the figure section 2 started from, was one of the lucky runs (its minimum is 1244), and 2152 and 2166
are ordinary. **A runner-to-runner spread of about 300 s on one job is larger than anything a build cache could
save, so one before and one after cannot show a gain of this size, and the first version of this document, which
read the single 7e4088b run as the baseline, was wrong to.**

What the cache did change, on the evidence of the two cached runs:

* `semantic-index` 89 to 44 s, `satisfy-contract` 58 to 34 s, `tls-perf` 136 to 96 s: the three jobs that are
  mostly one `cargo build --release`. About 100 s of runner time saved in all.
* Every other job: no change that stands out from the spread; about 20 s of restore and save added to each of
  the nine.

The longest job, which sets how long a pull request waits (`tls-assurance (examples)`: 2090 s once, 2850 s with
the cache, no uncached history read for it), is not helped by compiling faster.

## 4. Conclusion

**A Rust build cache does not make this CI faster, and this change does not add one.** Compiling is at most
about a tenth of the long jobs; the rest is the conformance suite, which builds and runs hundreds of cancho
programs with the compiler under test (a local `cargo test --workspace` was still inside that one test binary
after twelve minutes when this was written). That is work for the test run itself, not for a dependency cache.
The workflow is unchanged by this pull request; only this measurement is kept.

What would shorten a pull request's wait, in the order the data suggests, none of it done here:

1. **Shard the conformance binary across jobs** (`cargo test --test conformance -- <filter>`), which needs a
   per-test timing first. That timing does not exist yet; it is the next measurement, and `--report-time` is
   unstable, so it needs `cargo nextest` or a wrapper.
2. **Stop running the same programs on three backends in the pull-request path** if the main-branch run
   already covers the others. This is a trade of coverage for time and is the maintainer's to make.
3. **Keep the cache for the three short jobs only**, if about 100 s of runner time is worth nine fewer lines:
   it is the only place the data shows a gain.

## 5. What was done: the long steps split across jobs

The wait of a pull request is its longest job. On `main` at 2526701 (run 39341822725, all green), by job:

| job | minutes | what the time is |
|---|---:|---|
| `tls-assurance (examples)` | 46.5 | the HTTP client's mutants 18.2, the streaming-body mutants 12.1, the byte-fed mutants 4.3, the rest of the steps about 10 |
| `linux-x86_64` | 34.5 | `cargo test --workspace` 33.5 |
| `selfhost (linux-x86_64)` | 32.5 | the conformance suite |
| `darwin-aarch64`, `selfhost (darwin-aarch64)`, `selfhost (linux-aarch64)` | 22, 18, 21 | the same two |

* **The three mutant scripts are their own jobs** (`tls-assurance (mutants, ...)`): they run `cancho test` on mutated sources and need
  the compiler and nothing else, so they do not have to follow the other steps. The examples job is left with about 10 minutes of
  steps plus its compile.
* **The conformance suite runs in four shards** (`scripts/conformance_shard.py`), in the default (`conformance (...)`) and the strict
  (`selfhost (...)`) modes. The shard script lists the tests from the test binary, so a new test lands in a shard by itself, and packs
  whole modules into shards by `.github/conformance-weights.txt`, heaviest first. `build` keeps formatting, lints, every other test
  and the smoke program.

**Measured (Linux x86-64, a 4-core machine, debug build, each module's tests run on their own):** the 100 modules sum to 1,501 s,
dominated by `selfhost` (382 s) and `floats` (203 s); no other is above 50 s. Packed in four, the shards weigh 382, 376, 376, 376 s,
so a shard is a quarter of the suite and **the heaviest module, `selfhost`, is the floor**: more than four shards cannot help until
that module is split. `scripts/conformance_shard.py --check 4` says every one of the 742 tests (the five known differences left out)
is in exactly one shard.

**Measured on CI, once (this change's own run, 39388628935, all 32 checks green; the same job list before is run 39341822725).**
Wall time from the first job starting to the last finishing: **23.3 minutes, against 46.5.** What each part came to:

| job | before (min) | after (min) |
|---|---:|---:|
| `tls-assurance (examples)` | 46.5 | 11.4 |
| `tls-assurance (mutants, HTTP client)` / `(streaming body)` / `(byte-fed server)` | inside the 46.5 (18.2 / 12.1 / 4.3) | 21.9 / 17.4 / 6.3 |
| `linux-x86_64` (formatting, lints, the other tests, smoke) | 34.5 | 3.3 |
| `conformance (linux-x86_64, 1..4/4)` | inside the 34.5 (33.5 with the rest) | 9.8, 5.4, 8.0, 7.1 |
| `selfhost (linux-x86_64, 1..4/4)` | 32.5 | 15.5, 7.8, 4.5, 5.5 |
| `selfhost (linux-aarch64, 1..4/4)` | 21.2 | 10.3, 6.2, 5.1, 4.9 |
| `selfhost (darwin-aarch64, 1..4/4)` | 17.7 | 10.8, 4.9, 4.4, 4.9 |

* **The last job to finish was `selfhost (darwin-aarch64, 1/4)`, at 23.3 minutes, and it ran for 10.8 of them**: it started 12.5 minutes
  in, because the macOS jobs wait for a runner (eight of them, with the conformance ones). The longest job by its own time is
  the HTTP client's mutants, 21.9 minutes (18.2 as a step before); the 3.7 minutes' difference is within the spread below, and
  I did not look for another cause.
* **Runner-minutes went up**: 245 summed over the 32 jobs, against 210 over the 12 before, 16 % more, for the compiling every shard
  does. The wall time is the gain, not the cost.
* **One run each.** Section 3 measured a spread of about 300 s on one job from runner to runner, so a figure here is good to about
  that; the halving (23 against 46.5 minutes) is outside it.
* **The longest remaining jobs are the HTTP client's mutants (21.9 minutes) and the streaming-body mutants (17.4)**, each a script of
  hundreds of independent `cancho test` runs; they can be split by mutant the way the suite was split by module, and nothing here
  does it. The `selfhost` module sets the floor of a conformance shard (`selfhost (linux-x86_64, 1/4)`, 15.5 minutes).

The names of the checks change (`conformance (...)`, `selfhost (..., n/4)`, `tls-assurance (mutants, ...)`); a branch protection rule
that names the old ones has to be changed with this.

**A hang found after the merge of the shards.** On two runs the byte-fed mutants job did not end after its script had (6eb9975e and
5f2571e9; on a third, a342146c, it ended its script at 8 minutes and the job at 30). The process listing the job prints
(`ps`, after the script) shows why: 35 `apport --from-systemd-coredump` processes, two to three minutes old, running at once. The
mutants make test programs crash; each core dump starts `apport` on a runner, and the runner's job clean-up then took 15 minutes
(09:32 to 09:47 in that log). That is also the likeliest reason for the one argued-equivalent mutant that was "killed" in one run
(`5f2571e9`'s parent): a crash-loaded runner, not the mutation; it is a reading of the log, not tested apart. The mutant jobs now
run with `ulimit -c 0`. The 23.3-minute wall time above is the first run, which did not have the stall; the one with it was 30.
