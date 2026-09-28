# CLAUDE.md — lex-sys

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before changing the compiler,
and [`AGENTS.md`](AGENTS.md) before writing lex-sys programs. The rules
that matter most:

- **The gate:** `cargo fmt --all --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`,
  and `cargo run -p lex-sys -- docsync --check`, all passing, before
  anything is called done. The last one catches a generated fact in
  `README.md` (crate/example/doc counts, the `Net` tally) that fell out
  of sync with the repository — fix with `cargo run -p lex-sys --
  docsync`, never by hand-editing the block
  (`crates/lex-sys/src/docsync.rs`).
- **Design before code**, in `docs/`, with claims measured. A claim that
  turns out false is corrected in place, in the document that made it.
- **No source file over 2,000 lines** (`crates/lex-sys/tests/files.rs`).
  Split by concern; never raise a ceiling.
- **Every refusal has a rule tag**, and no input may reach a panic.
