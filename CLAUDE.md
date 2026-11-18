# CLAUDE.md — cancho

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before changing the compiler,
and [`AGENTS.md`](AGENTS.md) before writing cancho programs. The rules
that matter most:

- **The gate:** `scripts/gate.sh` (stages fmt, files, seed, walk,
  tests, reject), passing, before anything is called done. While Rust
  crates remain on disk, also `cargo fmt --all --check`,
  `cargo clippy --all-targets -- -D warnings` and
  `cargo test --workspace`.
- **Design before code**, in `docs/`, with claims measured. A claim that
  turns out false is corrected in place, in the document that made it.
- **No source file over 2,000 lines** (stage `files` of the gate, `scripts/files-budget.cho`).
  Split by concern; never raise a ceiling.
- **Every refusal has a rule tag**, and no input may reach a panic.
