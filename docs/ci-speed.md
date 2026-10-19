# CI speed

> **Status: measured; the conclusion is at the end of section 3.** Written alongside the change that adds
> `Swatinem/rust-cache@v2` to every job of `.github/workflows/ci.yml`. A claim that turns out false is
> corrected here, in place.

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

## 3. Measured with the cache

The first run on a branch fills the cache, so it measures only what the cache costs. From the first run of this
change (head 54bdab0 plus the fix of the stability job; `Run Swatinem/rust-cache` and its post step):

| job | restore | save | job seconds | before |
|---|---:|---:|---:|---:|
| `linux-x86_64` | 12 | 8 | 2152 | 1326 |
| `darwin-aarch64` | 10 | 9 | 1476 | 1436 |
| `selfhost (linux-x86_64)` | 12 | 8 | 1513 | 1858 |
| `selfhost (darwin-aarch64)` | 8 | 9 | 1298 | 1439 |
| `selfhost (linux-aarch64)` | 6 | 7 | 1296 | 1279 |
| `tls-assurance` | 13 | 4 | 969 | 933 |

The cache itself costs 10 to 25 s a job. The job times moved by -19 % to +62 %, in both directions, with the same
sources: on `linux-x86_64` the `Tests` step went from 1278 s to 2080 s with nothing in it touched by the cache.
**Run to run, the tests vary by more than the compile time the cache could save**, so a single before and after
cannot show a gain of this size.

WARM-RUN RESULT PENDING: filled in from the second run.
