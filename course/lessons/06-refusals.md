# Lesson 6 — Reading what the compiler says back

**Part B · the agent builds, you decide · about 90 minutes**

```sh
start.py 6               # --offline if you have no agent
```

An agent that has never seen cancho will write cancho the way it writes
everything else. Some of what comes back is wrong *about the language*, some
is wrong *about your request*, and both look the same until you read the
rule. This lesson is about telling them apart, and about a mistake that the
compiler cannot see at all.

## By hand first

Read `brief.md`: `parse_nat` reads a natural number from decimal digits and
answers `Ok(n)` or `Err(reason)`. The contract is `parse_nat.contract.cho`.

**Write `parse_nat` yourself, without help**, in `candidate.cho`. Twenty
minutes. Your language habits are about to be tested by a compiler that has
none of your assumptions, and that is exactly what an agent is up against.

Count your refusals. Run `cancho check` after every change and keep a
tally: how many times were you refused, and for which `rule`? Write that
number down. It is the baseline the agent will be measured against.

*(You will want `import std.result;`, `result.Result::Ok(n)` — a variant,
not a function — and `std.bytes` for `is_digit`. Look at `docs/` if you must;
that is allowed in Part A and in the by-hand half of Part B alike. What is
not allowed is asking the agent yet.)*

When it satisfies the contract, move on.

## Delegate — twice

You are going to give the same brief to the same agent twice and compare.

**Attempt 1: tell it nothing about cancho.** Open the agent. Do *not* give
it `BUILDER.md`'s first section, and do not mention `agent-guidelines`. Ask
for `attempt1.cho`. Give it the brief and the contract. Run:

```sh
cancho check attempt1.cho stub_main.cho --std --output json
```

Count the refusals and write down every `rule`. (`check` reports *every*
independent refusal in one go, not the first, so the count is meaningful.)

**Attempt 2: the first page.** Same agent, a fresh conversation. Give it all
of `BUILDER.md` this time — its first line makes it read `cancho
agent-guidelines` — and ask for `attempt2.cho`. Same count, same list.

If you are offline, `offline/attempt_1.cho` and `offline/attempt_2.candidate.cho`
are stand-ins for those two.

Now compare: your baseline, attempt 1, attempt 2. In `AGENTS.md` this is
the entire experiment of a first page: what does an agent do when it has
read twelve hundred words about the language it is about to write?

## The refusals, sorted

For every refusal in attempt 1, decide which kind it is. There are two:

- **The agent's.** A language habit: comparing a byte with an integer
  (`type-mismatch`), reaching for `break` (`unknown-name`), calling
  `result.ok(...)` as though it were a function (`not-a-function`). The
  fix is *information*. Give the agent the first page, or paste the rule
  name back and let it read what that rule means. Do not rewrite its code.
- **Yours.** A refusal that says your request was one thing and your
  contract another. The fix is *your words*.

And a rule for what the agent may do about a refusal, which is in
`BUILDER.md` and is worth saying aloud: **it may change the body; it may not
widen the row, delete a test, or take an authority it was not offered.** A
checker that stops complaining is not the same as a program that is right,
and an agent is very good at the first.

## Verify: the one the compiler cannot see

Attempt 2 compiles. It satisfies the contract. Now do what lesson 2 taught
you: ask *could this have failed?* — and ask what the contract **did not
say**.

The brief says "a natural number", and an `int` is 64 bits. What should
`parse_nat("99999999999999999999")` answer?

A modern agent may already have handled this. One did, in the trial run of
this lesson, and said so in its list of decisions. Then the finding is the
other half of lesson 5: the decision was made *for* you, you only know
because it told you, and the contract still does not say it. Write the test
anyway; if it passes, pin the decision.

Write that test. If it does not pass, make it fail honestly: run it against the attempt, and
watch what happens. Then decide the answer, put it in the contract, and give
the agent *only the contract and one sentence*: "this contract changed; make
the candidate satisfy it". Do not tell it what was wrong.

<details>
<summary>Only after you have run it: what happened</summary>

`fixtures/06-refusals/attempt_2.candidate.cho` multiplies its running total
by ten for every digit. A twenty-digit number overflows an `int`, and an
`int` does not wrap — it **traps**: no message, no `Err`, the test process
is killed. Everything the brief said is true of the candidate. It is correct
for every number that fits and fatal for the rest, and nothing in the first
contract could tell the two apart.

The strengthened contract is `solutions/06-refusals/parse_nat.contract.cho`.
Its last test adds the boundary on both sides:
`9223372036854775807` parses; `9223372036854775808` is an `Err`.

</details>

## The questions

> **Does it check?** — read the `rule`, not the sentence.
>
> **What did I not say?** — a trap is a decision nobody made.

## Checkpoint

- Three numbers: your refusal count, attempt 1's, attempt 2's. And one
  sentence: what the first page changed.
- Every refusal in attempt 1 classified as the agent's or yours.
- A contract with a test that **fails** against the attempt before it is
  fixed, and passes after. You watched both.
- A trace sheet whose box 3 ("one way it could pass all of that and still
  be wrong") names an input you did not expect to need.

Next: [Lesson 7 — Audit what it can reach](07-audit.md)
