# Learn cancho by hand, then by agent

A course in eight lessons. You write the first four programs yourself.
From the fifth on, an AI agent writes the code and **you** are the one who
says what it must do, and whether it has.

It is built on the method of
[Prompt to Academy](https://github.com/alpibrusl/prompt-to-academy): do the
thing small and by hand first, so the failure is a memory and not a
lecture; only then delegate it; and when you delegate, learn the question
that checks the result. Cancho suits that method unusually well, because it
has a referee that cannot be argued with. The compiler refuses with a
*named rule*, `cancho satisfy` holds code to a contract you wrote, and
`cancho authority` says what a program can reach. The agent can be wrong in
a way the first two find, in a way the third finds, and in a way none of them
can — the thing you forgot to say.

```
Part A — by hand, no agent         Part B — the agent builds, you decide
  1  Hello, and who said you could    5  The contract is the specification
  2  Numbers, and tests that can      6  Reading what the compiler says back
     fail                             7  Audit what it can reach
  3  A thing you must end once        8  Capstone: defend it
  4  What a function can reach
```

## The five questions

Each is introduced in the lesson where you are first burned by not asking
it, and every Part B lesson ends by asking them of an agent's work.

| # | Question | Command | Lesson |
|---|---|---|---|
| 1 | Does it **check**? | `cancho check f.cho --std --output json` | 1, 6 |
| 2 | Does it **keep its promise**? | `cancho satisfy contract.cho f.cho --std` | 2, 5 |
| 3 | What can it **reach**? | `cancho authority f.cho --std` | 4, 7 |
| 4 | What did I **not say**? | *you* | 5, 6, 8 |
| 5 | Could I **defend** it? | the [trace sheet](trace-sheet.md) | 8 |

Questions 1–3 have a command. Question 4 has none, and it is the one the
course is for.

## The lessons

| | Lesson | Agent | By hand first | Ends on |
|---|---|---|---|---|
| 1 | [Hello, and who said you could](lessons/01-hello.md) | none | break it three ways, predicting each | a named refusal |
| 2 | [Numbers, and tests that can fail](lessons/02-numbers.md) | none | predict which tests pass an empty stub | a test you watched fail |
| 3 | [A thing you must end exactly once](lessons/03-own-it-once.md) | tutor | predict the compiler's complaint | `linear-value-unconsumed` |
| 4 | [What a function can reach](lessons/04-authority.md) | tutor | predict four authority reports | a program with its authority cut |
| 5 | [The contract is the specification](lessons/05-contract.md) | **builder** | be the lazy implementer | a contract an agent cannot cheat |
| 6 | [Reading what the compiler says back](lessons/06-refusals.md) | **builder** | write it yourself first, count refusals | a failure the compiler cannot see |
| 7 | [Audit what it can reach](lessons/07-audit.md) | **builder** | write the authority budget | a report that matches it |
| 8 | [Capstone: defend it](lessons/08-capstone.md) | **builder** | spec, "done", known answers | a defence, to a person |

*Agent* is the mode, and the order is enforced by the lessons rather than
trusted to you: **none** — do it alone; **tutor** — ask for help, but the
agent asks questions back and writes nothing
([`agent/skills/cancho-tutor`](agent/skills/cancho-tutor/SKILL.md)); **builder** — the agent
writes the code from your brief and contract
([`agent/builder.md`](agent/builder.md)). Every Part B lesson starts with
something you do by hand *before* the agent is unlocked, and the reason is
the lesson, not a rule.

About 10–12 hours in all. Nothing here needs a classroom.

## Set up

```sh
cargo build --release -p cancho          # needs clang and cc
export PATH="$PWD/target/release:$PWD/course:$PATH"
cancho --version
```

`course/start.py` and `course/check.py` are plain Python 3 with no
dependencies.

```sh
start.py 1                 # copies lesson 1's files to ~/cancho-course/01-hello
cd ~/cancho-course/01-hello
cancho run liftoff.cho --std
check.py liftoff.cho       # reads the //~ header in the file and tells you if you're there
```

**Work outside the repository.** `start.py` copies each lesson's exercise to
`~/cancho-course/`, on purpose. The reference solutions live in
`course/solutions/`, and the agent you hand a lesson to can read whatever its
directory reaches. Run it in the copy and it cannot see them. (You can see
them. Don't, until your own passes.)

## With an agent, and without

Part B wants an agent that can read files and run shell commands (Claude
Code, or any equivalent). Open it in the lesson's directory, paste
`BUILDER.md` (it is copied there), and give it the brief and the contract as
the lesson says.

**No agent?** `start.py N --offline` also copies `offline/`: the stand-in
for what an agent would have handed back, with every comment stripped from it.
Each fixture is a *planted* result — it compiles, it passes the first
contract, and it is wrong in one specific way the lesson is about. Treat it
exactly as you would the real thing, and write down what you find before you
open the spoiler.

An honest warning: a real agent will sometimes get the planted thing right
and give you nothing to catch. That is a result worth a line in your trace
sheet. The fixtures exist so the lesson does not depend on luck.

## What is here

```
lessons/      the eight lessons
exercises/    starter files, one directory per lesson; start.py copies them
solutions/    a reference answer for each; read after yours passes
fixtures/     planted agent output, one wrong thing each; --offline uses them
agent/        builder.md (Part B) and the tutor skill (Part A)
trace-sheet.md  the delegation record, one page; one per delegation
rubric.md     what lesson 8 is judged on
check.py      reads a file's //~ header and checks it; start.py sets a lesson up
```

## What the checker is

`check.py` is the course's only moving part. It reads the same `//~`
header the compiler's own test fixtures use, and does what the header says:
`STDOUT`/`EXIT` (run it), `RULE` (it must be refused, with these rules),
`AUTHORITY` (`cancho authority` must report exactly this), `SATISFIES` and
`UNSATISFIED` (a contract). It never edits a file and never prints a solution.
The Rust test suite runs it over `solutions/` and `fixtures/` and also
checks that every starter **fails** — an exercise that already passes teaches
nothing — so the course cannot rot without a red build
(`crates/cancho/tests/conformance/course.rs`).

## What was decided here

Read these before changing the course; each is a place where the method
met this language and something had to give.

**The agent builds; it does not run the console.** In lesson 8 the program
is split so that the agent writes pure counting functions and a given
`main.cho` holds all the authority. The compiler can then say something
strong about the agent's work: it can make the output wrong, and it cannot
make the program reach anything. A course where the agent also wrote the
I/O would have taught "review everything" and nothing sharper.

**A contract is the spec, and `satisfy` is the check.** Prompt to Academy
teaches "specify precisely" with prose against a person. Here the
specification is a file the compiler reads, so "your words ran out" is a
failing test instead of a discussion. The first contract in lessons 5, 6 and
8 is deliberately thin — the exercise is to find out how thin.

**The compiler is not the only judge, and the course says so.** Lesson 7's
planted case passes `check`, passes `satisfy`, and reports the right
authority after the obvious fix, while still containing a read the brief
never allowed. `cancho authority` reports what a program *can* reach by its
types; it does not read the body for you. The course would be wrong to
teach otherwise, and `docs/filesystem.md` §4 is the reason.

**Imports collide between a contract and a candidate.** They are checked as
one program, so both importing `std.test` under the same name is refused.
This was found writing the course, not designed into it, and the starters'
`import std.test as t;` is the fix. It is the kind of thing an agent hits
and a learner needs to be told once.

## What is not here

**It has not been run on a learner.** Everything above is measured in the
narrow sense that every solution compiles and passes, every fixture fails
the way its lesson says it does, and every starter fails until it is
changed. Whether the lessons teach is a different claim, and nothing here
supports it yet. The first cohort's trace sheets are the evidence the course
needs, and the lessons that read worst should be rewritten from them.

**Part A is a start, not a language tutorial.** It covers what lesson 5 and
later *need*: printing, loops, tests, ownership, borrowing, rows. It does
not teach structs, enums, `match`, regions or the standard library beyond
what is used. [`AGENTS.md`](../AGENTS.md) and [`examples/tour.cho`](../examples/tour.cho)
are where those are, and the agent is reading the first of them in Part B.

**No score, no certificate, no server.** Progress is files on your disk.
