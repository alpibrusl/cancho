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
