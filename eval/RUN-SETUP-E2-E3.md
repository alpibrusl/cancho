# E2/E3 run setup (2026-11-09)

Recorded in the commit that starts the runs, as the pre-registration requires.

- cancho compiler: commit 7684baa2 (release build, `cargo build --release -p cancho`); guide = `cancho agent-guidelines`, sha256 prefix 799c4cd3537c8351.
- Models: `anthropic:claude-haiku-5-5` (Anthropic) and `opencode:openai/gpt-5.6-luna` (OpenAI, through the opencode CLI).
- Checkers: mypy 2.4.1, clippy 0.1.98 (Amendment 1); clang as installed.
- Known gap: the opencode provider reports no token counts (recorded as 0), so token cost is measured for the Anthropic model only.
- Trial before the runs: crc32, one seed, all four of cancho/Rust/Python and gpt-5.6-luna on Rust ran green. Not findings (n = 1).
