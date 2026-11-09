# Pre-registration: E2 and E3 (written 2026-10-15, before any run beyond the smoke test)

**Question E2.** Can a current model write correct cancho for the pilot tasks given only `cancho agent-guidelines`?
**Question E3.** For the same tasks, models and iteration budget, how does the cost to a correct program in cancho compare with Rust, Python and C, and how many defects survive the language's own checker?

**Design.** 9 pilot tasks (`harness/tasks.py`), languages cancho, Rust, Python, C. Cancho context conditions: A (the guidelines only), B (A plus five fixed examples). Condition C is not run in the pilot. Up to 10 iterations; the model gets its language's checker output and the visible examples' failures after each try; the run ends at the first program that builds and passes the visible examples. Hidden cases (3 to 6 per task) score the result. Five runs per cell (task, language, model, condition) at the default temperature of the provider.

**Models.** To be named in the commit that starts the runs, with exact version strings; at least one Anthropic model and one other family reached through opencode, plus one local model through Ollama if its use is agreed.

**Metrics.** pass@1 (green on the first try), pass@10 (green within the budget), iterations to green, input and output tokens, wall time, and *survivors*: hidden cases failed by a program that passed the visible examples.

**Thresholds fixed now.**
- E2 fails (the claim "any current model can write cancho from the guide" cannot be made) if condition A is below 30% pass@10 on the strongest model tried.
- E3 fails (the claim "cheaper for an agent than a language designed for humans" cannot be made) if Rust has fewer median iterations to green *and* fewer survivors than cancho on this suite, on the strongest model tried.
- Expected, and reported whichever way it goes: cancho loses on pass@1 against Rust and Python.

**Not claimed.** Anything about tasks outside this suite; anything about a model not run; anything from fewer than five runs a cell.

**Amendments** are appended below with a date, never edited in place.

## Amendment 1 (2026-11-09, before any run beyond the smoke test)

**Why.** The baselines were weaker than ordinary good practice: Python was checked with `py_compile` only and Rust with `rustc` only. A cancho result against those would flatter cancho. This amendment strengthens the comparison languages and changes nothing else. The tasks, metrics, thresholds and iteration budget above stand.

**Changes to the checkers** (`harness/langs.py`; the checker's output is still what goes back to the model):
- Python: `mypy --strict --no-incremental sol.py`. A program is accepted by the checker only if it type-checks; it is then run with `python3`.
- Rust: `cargo clippy --release -- -D warnings` in a one-file Cargo project (edition 2021), then `cargo build --release`. Every rustc and clippy warning is an error.
- cancho and C are unchanged.

**Pinned versions.** mypy 2.4.1; clippy 0.1.98 (Rust 1.98); clang as installed on the run machine. The exact versions of every tool and of the cancho compiler (commit hash) are recorded in the commit that starts the runs.

**Consequence for the thresholds.** None. E3 still fails if Rust has fewer median iterations to green and fewer survivors than cancho. A stricter Rust and Python can only make that harder for cancho to avoid, which is the point.

**Still open before the first run.** The exact model strings (at least two families), and freezing the guide (`cancho agent-guidelines`) at a commit. Both go in the commit that starts the runs, not here.
