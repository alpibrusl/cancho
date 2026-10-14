# Lesson 1 — Hello, and who said you could

**Part A · by hand · agent: none · about 45 minutes**

```sh
start.py 1            # copies the exercise out of the repository
```

## By hand first

Open `liftoff.cho`. It is a complete program. Run it:

```sh
cancho run liftoff.cho --std
```

It prints `Hello, world!`. Now read it once, top to bottom, and **don't
look anything up**. Write down, on paper, one line for each of these:

- which function prints?
- how does it know *where* to print?
- where does `main` get that from?

Then break it three ways. Before each, write down what you think the
compiler will say. Then run `cancho check liftoff.cho --std` and compare.

1. Delete `io: &!i Io` from `say`'s parameters (and `i` from the call).
2. Delete the line `release(io);`.
3. Delete `release(heap);`.

The point is not to get the prediction right. The point is that you made
one, so the compiler's answer is something you were *surprised by or
not*, instead of something that just happened.

## Now make it do something

Change the program until `check.py liftoff.cho` is happy. It wants this
exact output:

```
Hello, cancho!
1
2
3
4
5
liftoff
```

You will need `io.write_all(io, "...")`, a `while` loop, and
`io.print_nat(io, n)` followed by `io.newline(io)`. They all take the `io`
you were handed. Nothing in this lesson takes anything else.

## The idea

A cancho program is handed one thing by the outside world: `World`.
`split(world)` takes it apart into the *capabilities* — `io` for the
console, `fs` for files, `heap` for memory, `args`, `ffi` — and after that
there is no other way to get one. Nothing is global. Nothing is ambient.

```cancho
import std.io;

fn say[&i](io: &!i Io) -> [io_write] int {
    return io.write_all(io, "Hello, cancho!\n");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    borrow mut io as &!i in { say(i); }
    release(io);
    return 0;
}
```

Three facts fall out of that, and each is a refusal you just met:

**A function that was not handed `io` cannot print.** You did not need a
rule against it; there is nothing to print *with*. The compiler's word for
that is `unknown-name`.

```cancho-refused
//~ RULE unknown-name
import std.io;

fn say() -> [io_write] int {
    return io.write_all(io, "Hello, cancho!\n");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    release(io);
    return 0;
}
```

**Authority has to end somewhere.** `io` is a *resource*, not a number:
whoever holds it must `release` it, exactly once, on every path. Leave
one out and the program does not compile — not as a warning. The compiler's
word for that is `linear-value-unconsumed`.

```cancho-refused
//~ RULE linear-value-unconsumed
import std.io;

fn say[&i](io: &!i Io) -> [io_write] int {
    return io.write_all(io, "Hello, cancho!\n");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    borrow mut io as &!i in { say(i); }
    return 0;
}
```

**`main`'s row is `[]` and `say`'s is `[io_write]`, and that is not a
contradiction.** Hold that thought; lesson 4 is about it.

You do not need to follow every symbol in `[&i]` and `&!i` yet. `&i` names
how long a borrow lasts; `&!` means the borrow is *unique*. Copy the
pattern, and you will have it by lesson 3.

## Read a refusal as data

The compiler writes a sentence for you. It also writes a *name*:

```sh
cancho check liftoff.cho --std --output json
```

Find the `"rule"` field. There are about sixty of them and they never
change; `docs/agent-errors.md` is the list. In Part B you will hand work to
an agent, and an agent that reads the rule fixes the right thing while one
that reads the sentence rewrites the file. Learn to read the rule now, on
errors you made.

## The question

> **Where did this get permission to do that?**

Every line that touches the outside world has an answer, and the answer is
a parameter you can point at. If you cannot point at it, the program does
not compile. Keep asking it. In lesson 4 it grows an answer for the whole
program.

## Checkpoint

- `check.py liftoff.cho` says `ok`.
- You wrote down three predictions before running three breakages, and can
  say which one you got wrong, if any.
- You can say what `unknown-name` and `linear-value-unconsumed` each mean,
  without opening this page.

Next: [Lesson 2 — Numbers, and tests that tell the truth](02-numbers.md)
