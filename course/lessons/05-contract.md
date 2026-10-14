# Lesson 5 — The contract is the specification

**Part B · the agent builds, you decide · about 90 minutes**

```sh
start.py 5               # --offline if you have no agent: see below
```

From here the agent writes the code. You write what it is held to, and you
decide whether it has been. Every Part B lesson has the same shape: **you
do the hard thing by hand first, then the agent is unlocked.** The order is
not negotiable, and it is not a rule about honesty. A checklist handed to
someone who has never been burned gets run, passes, and teaches the
performance of diligence. You are about to get burned on purpose.

## By hand first

Read `brief.md`. It is four lines about a function called `count_words`.

**1. Examples, on paper.** Write down six inputs and what `count_words`
should answer for each. At least three should be awkward ones — the ones
you would be annoyed to get wrong.

**2. Be the lazy implementer.** `count_words.contract.cho` has two example
tests. Write `candidate.cho`, by hand, in under five minutes: the *laziest*
function you can that passes those two tests. It does not have to be
right. It has to be allowed.

```sh
cancho satisfy count_words.contract.cho candidate.cho --std
```

If that verdict says *satisfied*, you have a function that is wrong about
almost everything and the compiler is happy. Write down one input it gets
wrong.

**3. Predict.** Write down three places an agent, given only those two
tests, is likely to make a decision you did not. Fold the paper. It gets
opened at the end.

## The idea

A **contract** is a `.cho` file whose `test_*` functions call the thing
you want. It is not a new format: it is the only language the compiler
already reads, so a contract is checkable the day it is written.
`cancho satisfy` runs the contract's tests against a candidate and answers
a verdict — *satisfied*, or the refusal, or the failing test.

```sh
cancho satisfy count_words.contract.cho candidate.cho --std
```

Three things you need to know, because each cost somebody a confusing
afternoon:

- The candidate has **no `main`**: `satisfy` supplies its own, and a candidate
  that brings one is refused ("`main` is defined twice"). Mark the function
  you are offering `pub`, as the library does; the verdict works without it,
  but the file then says less about what it is for.
- The contract and the candidate are checked as **one program**, so if
  both import `std.test` under the same name the compiler refuses it. A
  contract imports with `as` (`import std.test as t;`) so it cannot fight
  whatever the candidate imports. The starter already does.
- *Satisfied* is exactly as strong as your tests. It prints the identity
  (`sig_id`) of what you now depend on, which is a fact about the function's
  signature, not about whether it is right.

## Delegate

Open your agent in this directory. Give it `BUILDER.md`, `brief.md` and
`count_words.contract.cho` — the **thin** contract, exactly as it is — and
ask for `candidate.cho`. Do not improve the contract first. You want to
see what comes back.

No agent today? `start.py 5 --offline` put a stand-in in `offline/`. Copy it
to `candidate.cho` and treat it exactly as you would a real one.

Then run the three questions, in this order, and **write down what each
said**:

```sh
cancho check candidate.cho stub_main.cho --std --output json     # 1. does it check?
cancho satisfy count_words.contract.cho candidate.cho --std   # 2. does it keep its promise?
cancho authority candidate.cho stub_main.cho --std               # 3. what can it reach?
```

(`stub_main.cho` is there because a candidate is only functions and `check` and
`authority` refuse a program with no `main`. Never hand it to `satisfy`.)

For a pure function the third answer should be *performs nothing*. If it is
anything else, the agent took authority nobody offered it.

## Verify: the part the agent cannot do for you

Now get the six inputs from your paper. Turn each into a test, **add the
tests to the contract**, and run `satisfy` again.

Three outcomes are possible. The agent's code passes the strengthened
contract: the machine's version is often better than yours would have
been, and that is the uncomfortable part, not a problem with the lesson.
Or it fails, and you now know exactly which of your six inputs it got
wrong. Or it passes and you wrote a test that could not have failed.

Either way, write the list this lesson is for: **every decision the agent
made that your two examples did not make.** Not "is it good" — that is a
review. *What did it choose?* Each entry names a choice and the missing
sentence that would have prevented it. At least three.

Open your folded paper and score the three predictions.

<details>
<summary>Only after you have your list: the planted case</summary>

`fixtures/05-contract/attempt.candidate.cho` — `offline/attempt.candidate.cho`
if you ran offline — counts the spaces and adds one. It passes `"hello
world"` and `"one"`. It gets wrong the empty string, a string of only blanks,
two spaces in a row, a leading or trailing blank, and a tab or newline as a
separator. Every claim its author would make about it is true, and every
listed test passes.

`solutions/05-contract/count_words.contract.cho` is a contract it cannot
satisfy: six tests, each one a decision. The one that surprises people is
the empty string. Nothing in the brief says that no text has no words.

</details>

## The questions

> **Does it keep its promise?** — as strong as the contract.
>
> **What did I not say?** — the silent decisions. The contract is where you
> say them.

## Checkpoint

- Your strengthened contract fails a function you wrote on purpose to be
  wrong — you watched it fail.
- A written list of at least three decisions the agent made silently, each
  tied to a gap in your own words.
- You filled in a [trace sheet](../trace-sheet.md) for the delegation, and
  box 6 ("what I did not check") is not empty.

Next: [Lesson 6 — Reading what the compiler says back](06-refusals.md)
