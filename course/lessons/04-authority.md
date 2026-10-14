# Lesson 4 — What a function can reach

**Part A · by hand · agent: tutor, if you are stuck · about 75 minutes**

```sh
start.py 4
```

This is the last lesson you do alone. After it the agent writes the code
and you are the one who decides whether it is allowed to stand, and this is
the lesson that decides what "allowed" means.

## By hand first: predict, then read

`a.cho`, `b.cho`, `c.cho` and `d.cho` are four small complete programs.
For each, **write down what you think `cancho authority` will report** —
the list of things the program can reach. Use words: "the console, write
only". Then:

```sh
cancho authority a.cho --std
cancho authority b.cho --std --output json
```

Score yourself, honestly, with a tally. The report has two halves:
*performs* (what it reaches) and *never touches* (what it provably cannot).
The second half is worth predicting too: it is the half that makes the report
something you can rely on, because it is a list of things the program
*cannot* do.

## The idea

You saw in lesson 1 that `say` has `[io_write]` after its arrow and `main`
has `[]`. The part in brackets is an **effect row**: the list of things this
function does to the outside world. It is exact in both directions — a
label must appear if the body performs it, and a label that appears must be
performed. Both refusals are real:

```cancho-refused
//~ RULE effect-not-declared
import std.io;

fn greet[&i](io: &!i Io) -> [] int {
    return io.write_all(io, "hello\n");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    borrow mut io as &!i in { greet(i); }
    release(io);
    return 0;
}
```

```cancho-refused
//~ RULE effect-declared-not-performed
import std.io;

fn greet[&i](io: &!i Io) -> [io_write, heap] int {
    return io.write_all(io, "hello\n");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    borrow mut io as &!i in { greet(i); }
    release(io);
    return 0;
}
```

The second one is the one to remember. A row that is *wider* than the body
is refused — "a row is exact or it is decoration" — because if it were
allowed, `cancho authority` would report authority the program does not
use, and a report you cannot trust is not worth running.

> **Owning discharges. Borrowing declares.**

`main` *owns* the capabilities `split` gave it, so its own row is empty —
it has the authority, rather than reaching for someone else's. `greet` only
*borrows* `io`, so its row says what it did with the loan. And the report
for the whole program is the union of every row, which is why a program
that prints and nothing else comes back `io_write`, even though `main`'s
own row says `[]`.

## Two ways the program gets smaller

`cancho authority` also tells you what a program *cannot* do. In program C
(`c.cho`) every capability was released at the top of `main`, and the
report says `performs nothing`: no function in it has anything to reach
with. That is not a claim about style. It is a claim a reviewer can check
in a second, on a program they did not write.

Now `shrink.cho`:

```sh
check.py shrink.cho
```

It prints `HELLO` — and the checker still fails it, on the `AUTHORITY`
line in its header. The output is right and the authority is not: it uses
the heap, and it does not need to. **Change the program, not the header,**
until the report is `io_write` and nothing else. The comment at the top
names the one library function you need, and the other place bytes can
live.

This is the exact shape of the next four lessons, in miniature: correct
behaviour, and a report that says it can do more than it should.

## Never widen the row

When a row is refused, the temptation is to edit the row until the checker
stops complaining. **That is the one fix that is always wrong.** The row is
a promise about the body; if the row and the body disagree, change the
body, or ask why the body needed it. You are going to tell your agent this
in the next lesson, because agents are very good at making a checker
stop complaining.

## The question

> **What can it reach?**

Not "what does it do" — what *could* it do. Ask it of every program,
including your own. The answer is one command.

## Checkpoint

- `check.py shrink.cho` says `ok`, with both lines.
- You can explain why `main` has the row `[]` and the program still prints.
- You predicted four reports and can say which half of each you got wrong.

---

**Part A is done.** You have written, by hand: a program that prints, two
functions with tests, a resource kept and ended, and a program whose
authority you cut. You know four refusals by name. You have asked three
questions: *where did this get permission, who ends this, what can it
reach*.

Now you stop typing the code.

Next: [Lesson 5 — The contract is the specification](05-contract.md)
