# Lesson 3 — A thing you must end exactly once

**Part A · by hand · agent: tutor, if you are stuck · about 60 minutes**

```sh
start.py 3
```

## By hand first

Open `laugh.cho`. The header says two things are wrong with it. The first,
the compiler will tell you. **Before you run it, write down what you expect
it to say, and which line.**

```sh
cancho check laugh.cho --std
```

Read the refusal — the *rule*, then the sentence, then the line it points
at — and fix it. Then run it:

```sh
check.py laugh.cho
```

The second thing wrong is that `repeat` is a stub; the output is wrong.
Write the loop. The file's own comment tells you what to build; the shape of
the loop body is the whole lesson, and it is the next section.

## The idea

A `Buffer` is memory you asked the heap for, so somebody has to give it
back. In most languages "somebody" is a convention, a destructor you hope
runs, or a garbage collector. Here it is a rule the compiler enforces:

> **A resource is consumed exactly once, on every path.**

You get one from `buffer.empty`. You consume it by passing it to a function
that *takes it*: `buffer.drop` ends it; `buffer.append` takes it and hands a
new one back. So a loop that fills one has this shape, and you will see it
on every page of cancho you ever read:

```cancho
import std.buffer;

fn twice[&h, &r](heap: &!h Heap, s: &r [byte]) -> [heap] buffer.Buffer {
    var out = buffer.empty(heap, 16);
    out = buffer.append(heap, out, s);
    out = buffer.append(heap, out, s);
    return out;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(args);
    borrow mut heap as &!h in {
        let text = twice(h, "ha");
        buffer.drop(h, text);
    }
    release(heap);
    return 0;
}
```

`out = f(heap, out, x)` — take it, hand it back. It is longer than
`out.push(x)`, and the extra length is the compiler's guarantee, written
down where you can see it.

There are two ways to get it wrong, and you have already met the first:

**Never consuming it** is `linear-value-unconsumed`:

```cancho-refused
//~ RULE linear-value-unconsumed
import std.buffer;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(args);
    borrow mut heap as &!h in {
        let b = buffer.empty(h, 8);
    }
    release(heap);
    return 0;
}
```

**Consuming it twice** is `linear-use-after-move` — after `drop`, `b` is
gone, and naming it again is naming something that no longer exists:

```cancho-refused
//~ RULE linear-use-after-move
import std.buffer;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(args);
    borrow mut heap as &!h in {
        let b = buffer.empty(h, 8);
        buffer.drop(h, b);
        buffer.drop(h, b);
    }
    release(heap);
    return 0;
}
```

(You should recognise `release(io)` now: it is the same rule. Authority is
just a resource whose job is "being allowed".)

## Borrowing: look without taking

To *read* a buffer you do not need to consume it. `borrow text as &t in
{ ... }` lends it out for the length of the block and gives it back at the
closing brace; `borrow mut` lends it *uniquely*, so nobody else can look
while you are writing. That is why `laugh.cho` writes the text inside a
`borrow text` block and drops it *after* — the borrow has ended by then.

Try dropping it inside the block. Predict the rule first.

## The question

> **Who ends this?**

For every value that is a resource — a buffer, a capability, later a file or
a socket — there is exactly one answer, and the answer has a line number.
When you ask an agent for a function that takes or returns a resource, this
is the first thing to find in what it hands back.

## If you are stuck

Ask your agent, but tell it to be a tutor: copy
`course/agent/skills/cancho-tutor/SKILL.md` into its instructions first. It
should ask you questions and not write the loop. If it writes the loop, the
lesson did not happen.

## Checkpoint

- `check.py laugh.cho` says `ok`.
- You can name the rule for "forgot to end it" and the rule for "ended it
  twice", and say what each looks like in a program.
- You can say, for `repeat`, which variable holds the buffer at each point
  of the loop, and who ends it afterwards.

Next: [Lesson 4 — What a function can reach](04-authority.md)
