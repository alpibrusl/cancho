# eval: evidence for the claims made about cancho

Each claim made about cancho (performance, agent-friendliness, safety, stability, the ecosystem) has an experiment that could
refute it. This directory holds the harness, the pre-registrations and the raw data. It is a **working record**, not a result:
nothing here has been reproduced by anyone outside the project, and the authors designed both the language and the tasks.

## Status (2026-10-15)

| # | Claim | Status | Where |
|---|---|---|---|
| E1 | Close to C and Rust on kernels | Part A on one machine (Linux x86-64, i7-1260P, governor `powersave`, one core): 4 kernels, 9 rounds, **minimum** wall time as `scripts/three.py` reports it (no spread). Mac, Raspberry Pi 5, RSS, binary size and the system benchmarks not done. | `data/e1_three_gram_linux_x86_64.txt` |
| E2/E3 | A model can write cancho from the guide; cost to green against Rust, Python and C | Harness and 9 pilot tasks only; one smoke run (claude-haiku-5-5, `crc32`, condition A, 6 iterations: not green; n = 1, no finding). Pre-registration below. | `harness/`, `PREREGISTRATION-E2-E3.md` |
| E4 | Authority cannot be escaped | Not started | |
| E5 | Verification instead of comprehension | Not started (needs reviewers) | |
| E6 | Small surface | `AGENTS.md` 944 lines, 5,024 words, 33,406 bytes; 24 keywords plus `_`; 17 CLI subcommands. **No token counts, no comparison with other languages.** | below |
| E7 | What is the compiler's own | 8 crates, 103,817 lines of Rust; 126 external crates; Rust 1.98.1 pinned; code generation is LLVM through `clang`, or Cranelift. | below |
| E8 | Identity stability | `scripts/history.py` on the full history (405 commits, 2026-09-19 to 2026-10-14): 383 of 399 revisions read today (96.0%), 382 under Cranelift. Not comparable with `docs/hash-stability.md`'s older figures. Ids unchanged by reformatting on 131 of 176 sample files (45 not testable alone; every file was already formatted, so the check is of mangled whitespace). The 16 failures are not classified into regressions and language moves. | `data/e8_*` |
| E9 | The Python toll | Not started | |
| E10 | Entry friction | Not started (needs people) | |
| E11 | Agent using `cancho-tools` | Not started | |
| E12 | Ecosystem compatibility | 13 product repos, their own CI steps, current compiler. First pass (single `cancho` lines) and second pass (whole steps, steps needing services skipped). Failures are not yet separated into harness problems and compiler problems except by the class counts below. | `data/e12_*.jsonl` |

### E6, E7 (measured on a fresh clone at 8d996b0, 2026-10-14)

- `AGENTS.md`: 944 lines, 5,024 words, 33,406 bytes. Keywords of the lexer: `fn let var if else while return defer true false struct enum match res extern module import pub val borrow region as in mut where`, and `_`. CLI: `add agent-guidelines authority build check fmt ids ingest install introspect layout print run satisfy skill test vcs` and `--version`.
- Workspace: `cancho` 46,407 lines, `cancho-ir` 20,950, `cancho-codegen-llvm` 13,848, `cancho-syntax` 9,016, `cancho-codegen` (Cranelift) 8,460, `cancho-id` 2,433, `cancho-vcs` 1,900, `cancho-types` 803. 814 `.cho` files in the repository, among them the compiler written in cancho (`examples/selfhost`).
- Direct external crates: acli, blake3, cranelift-codegen/-frontend/-module/-native/-object, ed25519-dalek, hex, httparse, object, serde, serde_json, target-lexicon, tempfile, toml. Outside the repository: `clang` and `cc`; `wasm-ld` and a wasi sysroot for the wasm target.

### E12 failure classes (second pass, 365 step runs, 122 failed)

Missing file or directory 46; a Python module the test needs (`dns`, `standardwebhooks`) 28; the project's pinned compiler revision refused (without `--ignore-compiler-rev`) 18; the environment (ports, permissions) 3; a missing tool 4; **other 23**. The "other" are the ones to read: stale embedded authority reports (the report was recorded by another compiler revision), `cancho-log`'s crash sweep (2 failures of 8,874 checks), one failure each in `cancho-gateway`'s rate-limit and authentication suites, `cancho-web`'s guarded example, `cancho-dns`'s RFC coverage table, and services that were not installed (Mosquitto, Redis, Postgres). Each of the 23 needs a look before it is called a compiler problem.

With `--ignore-compiler-rev`: tools, table, mqtt, gateway and hooks (test), gpu (build, 7 of 7 tests), log (build) pass their build and test steps.

## Protocol

- Pre-register tasks, metrics and thresholds in a dated file in this repository before running; commit it.
- Publish the raw per-run data (one JSON line per run: model, task, language, context, iterations, tokens, outcome) and the harness.
- Tasks are outside what the agent can see: nothing in `examples/` or the product repositories is a visible task.
- Oracles are independent: expected outputs come from Python's standard library, never from code written for this project.
- At least five runs per task and model; report the spread, not only the mean.
- Report losses.
- Threats to validity, stated up front: the authors designed the language and the tasks; the guide (`AGENTS.md`) and much of the code were written with the help of agents; every repository has one maintainer and no outside review.

## Harness

`harness/run.py` runs one (task, language, model, context, seed) to the end and appends a JSON line. `tasks.py` has the 9 pilot tasks (pure: crc32, sha256, base64, sort_numbers, word_freq, hmac_sha256; product-like: csv_sum, dns_query_name, mqtt_connect_encode), each a program that reads standard input and writes standard output, with visible and hidden cases from the Python standard library. `langs.py` has each language's own checker (cancho: `cancho check --output json`; Rust: `rustc`; C: `clang -fsanitize=address,undefined`; Python: `py_compile`). `models.py` has the providers (Anthropic, Ollama, opencode); keys are read from the environment or a key file and never written anywhere (`ANTHROPIC_KEY_FILE`, `ANTHROPIC_WORKSPACE_ID`). `run_e12.py` runs a repository's own CI steps.

Not done in the pilot: an authority group of tasks, context condition C (the whole repository), Rust with clippy, more than one model family.
