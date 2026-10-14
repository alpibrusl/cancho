# The rubric

The capstone (lesson 8) is judged on these four, and each is judged on
what you *wrote down at the time*, not on how the program turned out. A
program that works and a record that is silent about what was not checked
is a **Not yet**.

| | Not yet | Meets the bar | Exemplary |
|---|---|---|---|
| **The spec** | The contract is the brief's examples, restated. | Every decision the brief left open is made, in a test, and the ones deliberately left out are named. | Also anticipates a result that would pass every test and still be wrong, and writes the test that stops it. |
| **The check** | "It compiled and the tests passed." | `check`, `satisfy` and `authority` were each run, the authority was compared to a budget written *before* the agent started, and the contract was shown able to fail. | Also found something the first contract missed by reading the candidate, and fixed the contract, not just the code. |
| **Honesty about what was not checked** | The record is silent about coverage, or was written afterwards. | The trace says what was and was not verified, specifically, and was filled in while working. | Also says why the line was drawn where it was, and names the assumption that made it feel safe. |
| **What did I forget to specify** | Stops when the stated requirements are met. | Asks the question of its own work unprompted, and reports at least one thing it found. | Gives an account that names a gap nobody prompted. |

The third row cannot be made up for by the others. Everything else can be
coached; that one is the actual outcome.
