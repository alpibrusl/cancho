# satisfy: a contract a candidate must meet

> **Status: designed, then the smallest slice built** (#406). What exists
> and is composed, unchanged: the checker (`cancho check`, every refusal
> as data), the test runner (`cancho test`, `docs/testing.md` §3), and
> `cancho-id`'s per-declaration identity. What is new here is the
> *composition* being a command with a verdict, and one addition to it:
> the contract pins the `SigId` the caller depends on, so satisfaction
> is against a specific identity, not a name.

---

## 1. What a contract is

A contract is a `.cho` file — the only language the checker already
reads, so a contract is checkable the day it is written and needs no
new format, parser or schema:

```cancho
// contract: max
// sig: 8f13...   (optional: the pinned SigId, `cancho ids` of the declaration)

fn test_max_picks_the_larger() -> [] int {
    if max(3, 7) != 7 { return 1; }
    ...
}
```

Three parts, two of them free:

1. **The signature** — implied by the contract's own tests: they call
   the candidate by name, so a candidate with the wrong arity or
   parameter types is refused by the ordinary checker, with the rule
   tag and position `check --output json` already answers (`unknown-name`,
   arity mismatches, type mismatches). A candidate that does not check
   against the contract's calls does not get as far as running.
2. **The examples** — `test_*` functions with concrete inputs, the
   shape `cancho test` already runs. Satisfaction includes them
   passing.
3. **The properties** — the same `test_*` functions, written as
   witnesses over ranges rather than points (`max(i, 100-i)` against
   `max(100-i, i)` for a hundred values). A property that is not
   decidable at compile time is *witnessed*, which is what a test can
   honestly say; a property that is decidable (`min(a,a) == a`) is
   witnessed too, and the difference is the writer's to state, not the
   tool's to guess.

## 2. What `satisfy` does

`cancho satisfy <contract.cho> <candidate.cho> [--std]`:

1. **Checks** the two files as one program — the contract's tests
   against the candidate's declarations. A refusal here is the
   ordinary refusal vocabulary, located, as data with `--output json`.
2. **Runs** the contract's tests against the candidate, exactly as
   `cancho test` does, one process per test.
3. **Answers** the verdict as data: `satisfied` (with the
   recomputed `SigId` of each contract-named declaration, so the
   caller can see what it now depends on), or the refusals /
   failures that stand between the candidate and the contract.

Exit codes: 0 satisfied, 1 refused (checker), 4 failed (tests) — the
same codes `check` and `test` already answer, because satisfy *is*
those two commands composed, with the verdict named.

## 3. What the slice deliberately does not do

* **No generation.** The candidate is written by an agent or a person;
  the compiler is the gate, never the author. `agent-errors.md` §2's
  finding holds here too: `suggested_transform` needs op ids, and this
  slice does not add them.
* **No new trust machinery.** The verdict is `check` plus `test`,
  composed. An attestation of satisfaction is the vcs gate's job
  (`docs/vcs.md`), reached through `vcs publish` of the candidate
  once it satisfies — nothing here duplicates it.
* **No property language.** A property is a test function. When the
  language can express a decidable property *as data* (a term, not a
  program), that is a new slice with its own design; today's honest
  answer is the witness.

## 4. The committed artifact

`tests/satisfy/` holds the first real contract and its candidate —
`std.math`'s `max`, contract with point examples and a commutativity
witness, candidate the ordinary `if` — and CI runs
`cancho satisfy tests/satisfy/max.contract.cho tests/satisfy/max.candidate.cho --std`
so the loop's first end-to-end run is a checked property of the
repository, per the issue's own verification ask.
