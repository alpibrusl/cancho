# Lesson 2 — Numbers, and tests that can fail

**Part A · by hand · agent: none · about 60 minutes**

```sh
start.py 2
```

## By hand first

Open `numbers.cho`. There are three functions: `triangle` is done, so you
can see the shape; `is_prime` and `collatz_steps` are stubs. Below them are
the tests, and **the tests are the specification**. Read them before you
write a line.

Now, before you run anything, predict: of the four tests, **which will pass
against the stubs?** Write it down. Then:

```sh
cancho test numbers.cho --std
```

Look at what passed. One of the tests passes against a function that does
nothing but `return false;`. A test that passes against a stub proves
nothing about the function it is named after. We will come back to that
within the hour, in your own hands.

## Now write the two functions

```sh
check.py numbers.cho
```

wants `is_prime` and `collatz_steps` right. Two things to know about this
language that you would not guess:

**There is no `break` and no `continue`.** A loop stops because its
condition says so. If you want "stop early", the condition is where it goes,
with a flag:

```cancho
fn first_space[&r](s: &r [byte]) -> [] int {
    var i = 0;
    var found = false;
    while i < len(s) && !found {
        if int_of(s[i]) == 32 {
            found = true;
        } else {
            i = i + 1;
        }
    }
    return i;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(args); release(io); release(heap);
    return first_space("abc def") - 3;
}
```

Try writing `break` anyway, in a scratch file, and read the `rule` the
compiler gives you. (It is the same one you met in lesson 1. That is
informative.)

**`int` is 64 bits and it *traps* on overflow.** It does not wrap. There is
no wrong answer, only a program that stops.

```cancho
fn doubled(times: int) -> [] int {
    var x = 1;
    var k = 0;
    while k < times {
        x = x * 2;
        k = k + 1;
    }
    return x;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return doubled(62) % 256;
}
```

Change `62` to `63` and run it with `cancho run`. Nothing is printed and the
run ends in status 3. That 3 is not the program's answer: `run` reports a
program killed by a signal as 3 ("the environment failed"), and a trap *is*
a signal. (`cancho build` it and run the binary yourself and your shell will
say `Illegal instruction`, which is the same thing said plainly.) (`wrapping_add` and its two
siblings are how you *ask* for wraparound when that is what you mean.) A
program that cannot produce a wrong number is a program that sometimes
produces no number. Keep that in mind for lesson 6.

## The idea

A function with the row `[]` is **pure**: it reaches nothing, so the same
input gives the same answer, so a test of it is a complete test. That is
why this lesson has no `io` in it, and why `cancho test` supplies its own
`main` — a file it runs must not declare one.

`std.test` has three tools: `assert(bool)`, `assert_eq(a, b)` and
`assert_ne(a, b)`. A failing one *traps*: there is no message. So **one
concern per test function**, named for it, because the name is all you get
back (`test_small_primes ... FAILED`).

## Where the empty stub taught you something

You saw `test_not_primes` pass against `return false;`. Fix `is_prime`. Then
**break your finished version on purpose**, one change at a time, and run
the tests after each. Predict which test fails *before* you run it.

- change `if n < 2` to `if n < 1`
- change `d * d <= n` to `d * d < n`

Each makes `is_prime` wrong, and each is caught — by `test_not_primes`,
which checks `1` and `9`. Now ask the harder version of the question: what
does `collatz_steps(0)` do? The brief says nothing about it, so the tests
say nothing about it, so a test run will never tell you. Try it, and
**use a timeout**:

```sh
timeout 5 cancho run your-scratch-file.cho --std
```

It does not trap and it does not answer: it never finishes. There is no
per-test timeout either (`docs/testing.md` §5), so a hanging function
hangs the whole `cancho test`. Decide what `collatz_steps(0)` *should* do,
write the test that says so, and change the function until it passes.
That is the whole job of a specification, done once on a small thing.

## The question

> **Could this test have failed?**

A passing test only means something if there was a version of the program
under which it would have failed. You will ask this of an agent's tests,
and of your own, for the rest of the course.

## Checkpoint

- `check.py numbers.cho` says `ok`.
- You can say, for each of the four tests, what *wrong* function it would
  catch — and name one it would not.
- You have seen a trap, and can say why `cancho run` shows it as status 3.

Next: [Lesson 3 — A thing you must end exactly once](03-own-it-once.md)
