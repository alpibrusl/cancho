# Lesson 8 — Capstone: defend it

**Part B · the agent builds, you decide, you answer for it · about 2 hours**

```sh
start.py 8               # --offline if you have no agent
```

You build a small real tool, with an agent, and then you are asked about
it. The tool is `loglens`: it reads a log on standard input and prints how
many lines it has, how many of them contain `ERROR`, and how long the
longest is. It is small on purpose. The work is in what the brief does not
say.

## The shape

`main.cho` is **given** and is the only file with authority in it: it reads
standard input into a buffer and prints three lines. The counting is three
pure functions in `logstats.candidate.cho`, which the **agent** writes, and
which are held to `logstats.contract.cho`, which **you** write. The split is
the point: the agent never touches the console, so an agent's mistake in the
counting can make the output wrong but cannot make the program reach
anything.

```
brief.md                  what loglens is for, and the authority budget
main.cho                  given: reads stdin, prints the report
logstats.contract.cho     yours: the first draft
logstats.candidate.cho    the agent's
```

## By hand first (agent: none)

All of this is written before any agent sees the brief, and in the trace
sheet as you go:

1. **The spec.** Read `brief.md`. Under it, answer every question it asks
   at the bottom, as a decision. A log with no final newline: how many
   lines? An empty line? Two `ERROR`s in one line? Does the length of a line
   include the newline? Write each as a test in `logstats.contract.cho`.
   Do not leave one for the agent.
2. **"Done."** In trace sheet box 2, write what done means in checkable
   terms. Box 3, the way it could pass everything and still be wrong.
3. **Known answers.** Make a small input of your own — a log with an awkward
   shape — and work out the three numbers by hand. Do it on paper. Do not
   run anything yet.
4. **The budget.** `heap`, `io_read`, `io_write`, and nothing else. It is
   already in `main.cho`'s header; do not edit it.

## Delegate

Give the agent `BUILDER.md`, `brief.md` and your contract; ask for
`logstats.candidate.cho`. Run all of it:

```sh
cancho check logstats.candidate.cho --std --output json
cancho satisfy logstats.contract.cho logstats.candidate.cho --std
check.py main.cho                    # builds main + the candidate, runs the sample log
cancho authority main.cho logstats.candidate.cho --std
```

`check.py main.cho` runs the whole program on the sample log in `main.cho`'s
header, compares its output, and holds its authority to the budget in the
header. Put the actual output of each command in box 5 of your trace sheet.

If you are offline, `offline/logstats.candidate.cho` is the stand-in.

## The change request

Your logs now come from a Windows machine, and every line ends in `\r\n`.
A `\r` is not part of what anyone means by the length of a line.

Now, in this order:

1. Change **the contract**. Make `longest_line` ignore a `\r` before the
   newline. Run `satisfy` against the agent's *unchanged* candidate. It must
   **fail**. If it does not, either the candidate already ignores the `\r`
   or your new test is not testing what you think; find out which.
2. Only then hand the agent the new contract and one sentence: "this
   contract changed; make the candidate satisfy it." Not what to change.
3. Run everything again. Fill in a new trace sheet; do not edit the old one.

An order of operations the lesson insists on: the test fails first, because
a change you cannot show failing is a change you cannot show fixed.

<details>
<summary>Only after your contract is written: the planted case</summary>

`fixtures/08-capstone/logstats.candidate.cho` counts newlines to count lines.
It satisfies the first-draft contract and runs the sample log perfectly —
`main.cho` prints the right three lines with it — because every log in the
first draft ends in a newline. A log cut off mid-line (a crash; `head -c`)
loses its last line. The strengthened contract in
`solutions/08-capstone/logstats.contract.cho` catches it with one test, and
makes five more decisions you will have made differently.

</details>

## Defend it

Answer each of these **in writing**, from your trace sheets and not from
memory, before you read the rubric:

1. What did you ask for? Quote it.
2. Which three decisions did the brief leave open, and which test pins each?
3. The compiler checked some things, you checked some, and nobody checked
   the rest. List one of each.
4. Show me the test that would have caught the agent's worst mistake, and
   show me it failing against the version where that mistake exists.
5. The agent says it only uses the console and the heap. How do you know?
6. What would you check differently next time, and what about the process
   told you so?
7. **A question you could not have prepared for:** have someone who was not
   there pick one line of the agent's code and ask what happens if the log
   is ten gigabytes. If you cannot answer, say what you would run to find out.

Then find someone who was not part of this — a friend, a colleague, anyone
with no reason to be kind — hand them [the rubric](../rubric.md), and
defend it to them out loud. Not to a mirror; not to your agent. The
rubric's third row, *honesty about what was not checked*, is the one that
matters, and it is the one you cannot make up for on the other three.

## Checkpoint

- All five commands pass with your contract and an agent's candidate.
- You can show the contract failing, before the change request and after it.
- Two trace sheets, with box 6 ("what I did not check") filled in honestly
  on both.
- Defended to a person, once, with the rubric in their hands.

---

**That is the course.** You wrote four programs by hand, so you know what
the compiler refuses and why. Then you handed the work to a machine and
found out what a compiler can tell you about it (`check`, `satisfy`,
`authority`) and what only you can (*what did I not say*). The compiler is
the one party in the room that cannot be argued with. Everything else — the
brief, the contract, the budget, the account — is yours.

Where next: [`AGENTS.md`](../../AGENTS.md) is the page you have been pasting
into agents; read it as a person now. [`examples/`](../../examples/) are
programs meant to be read. [`docs/README.md`](../../docs/README.md) indexes
the design documents, and every one of them says what it measured.
