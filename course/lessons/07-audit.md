# Lesson 7 — Audit what it can reach

**Part B · the agent builds, you decide · about 75 minutes**

```sh
start.py 7               # --offline if you have no agent
```

Until now the agent's code was pure: it read a string and answered an
integer, and the only interesting thing it could reach was nothing. This
lesson is about a program that touches the filesystem, because that is
where "it works" and "it is allowed" come apart, and `cancho authority` is
the command that tells them apart.

## By hand first: the budget

Before an agent writes a line, you write the **authority budget**: one
sentence that says what the program may reach and nothing more. The brief
for this lesson, written for you, is:

> Print the length in bytes of `/etc/hostname`, then a newline.
> **Authority:** it may read that one file and write to the console.
> Nothing else.

Translate that into the exact thing `cancho authority` should print. You
know the labels (`io_write`, `fs_read`, `heap`, `args`, …). A file path
carries its path as an argument — `fs_read("/var/log")` — and the empty
path `fs_read("")` is **every file**. Write down your budget, in the
program's own vocabulary, on paper. The header of `hostname_len.cho` has
one; do not read it yet.

## Delegate

Hand an agent `BUILDER.md` and that brief, and ask for `candidate.cho`. This
one is a whole program with its own `main`, so ignore `stub_main.cho` here.
Add your budget as a header, so the checker can hold the agent's program to
it:

```
//~ AUTHORITY fs_read("/etc/hostname"); io_write
```

Then run the questions, in order:

```sh
cancho check candidate.cho --std --output json   # does it check?
cancho run candidate.cho --std                   # does it do the job?
cancho authority candidate.cho --std             # what can it reach?
check.py candidate.cho                           # ...and is that the budget?
```

If your agent got it exactly right, good: you have an audit that passed,
which is worth one line in the trace. If it did not, you have what this
lesson is about. Either way, then do the same to `hostname_len.cho`, which
is a planted example — an agent will not always make the mistake it
carries, which is why it is planted. If you are working without an agent,
`hostname_len.cho` *is* the agent's output.

It works. It prints a number. Any test you would reasonably write for it
passes. Now read the report against your budget, line by line.
`check.py hostname_len.cho` fails and says which labels it found. Your job
is to change **the program** — not the header, and not by editing the row —
until the report equals the budget.

You need `narrow(fs, "/etc/hostname")`, which turns the broad `Fs` that
`split` handed `main` into a capability that names one file. Everything
that borrows it then carries that path in its signature, and so in the
report.

## Verify: a report is not the whole story

When `check.py` says `ok`, stop and read the function the agent wrote
again. The program has a branch the budget never allowed. The report now
says `fs_read("/etc/hostname")`, and the report is *right*, and the program
still contains a read of a different file.

Narrow a capability and the report shrinks. It does not remove code. A
read of another path through a narrowed `Fs` is **stopped at run time** —
`docs/filesystem.md` §4 — which is a better failure than a leak, and still
a failure: a program the brief called correct that now dies on a machine
where the first file is empty. Run it with the narrowed capability and the
first path swapped for another, and watch what `cancho run` does.

So the audit has three steps, and the compiler does one of them:

1. The report equals the budget. *(the compiler tells you)*
2. The body does nothing the budget did not allow. *(you read it)*
3. The agent's account of itself matches 1 and 2. *(you compare)*

Ask the agent what authority its program has. It will tell you. **Do not
use its answer for anything except a comparison.** The report is a fact; the
account is a claim. A model that says "I only read the one file you named"
is saying something that `cancho authority` can contradict, and the
interesting case is the one where it does so honestly, because it believes
what it wrote.

<details>
<summary>Only after you have the report matching: the planted case</summary>

The starter reads `/etc/hostname` and, if that is empty, falls back to
`/etc/machine-id` — "a hostname file can be empty on some machines". That is
a reasonable thing for a careful engineer to want and the brief did not
allow it. Written against `Fs("")`, it compiles, passes, and reports
`fs_read("")`: every file on the machine. Narrowed to the one file the
brief named, the report is exact, and the fallback is dead code that kills
the program if it ever runs.

The right response is not "narrow it and keep the fallback". It is to notice
that **the agent made a decision for you** — the same thing as lesson 5,
about authority instead of counting — and to put it back in your hands:
either the budget gets a second file, because you want the fallback, or the
fallback goes. The solution deletes it. `solutions/07-audit/hostname_len.cho`.

</details>

## Pin it

A budget you checked once is a budget that will drift. Keep the report:

```sh
cancho authority candidate.cho --std --output json > authority.expected
```

Next time anything changes — the agent, the file, you — run the same
command and `diff` it against that file. A new `fs_read` shows up as one
added line. This is how a review of an agent's change becomes five seconds
long.

## The questions

> **What can it reach?** — compared to a budget you wrote first.
>
> **Is that what it told me?** — the agent's account is a claim.

## Checkpoint

- `check.py hostname_len.cho` says `ok` with both lines.
- You wrote the budget before you read the report, and it matched or you
  can say how it differed.
- You can say what the report *cannot* see, and what you did about it.
- A saved `authority.expected` and the one-line command that diffs it.

Next: [Lesson 8 — Capstone: defend it](08-capstone.md)
