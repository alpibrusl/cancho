# CI speed

> **Status: measured; the conclusion is section 4: a build cache does not help, and none is added.** Written to
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
