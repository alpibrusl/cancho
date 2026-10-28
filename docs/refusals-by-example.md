# Refusals by example

A path through the compiler's refusals in the order a newcomer meets them. Each chapter is one rule: a program the compiler refuses, the
`rule` tag and sentence it gives, and the same program corrected. Every refusal in cancho carries a stable `rule` tag (`CLAUDE.md`), and
`cancho check --output json` prints it with the message and the explanation (`docs/agent-errors.md`); that is what the blocks headed
`text` below are.

**Nothing here is typed by hand.** Each refused program is a fixture, `tests/reject/walkthrough_*.cho`, and the test
`docs::refusals_by_example_is_what_the_compiler_says` (`crates/cancho/tests/conformance/docs.rs`) fails when

* a refused block differs from its fixture, or the compiler no longer refuses it with exactly the one rule named in its heading
  (a rule renamed in the compiler fails here);
* the `rule`, `message` or `explanation` lines of a chapter differ from what `cancho check --output json` prints for that fixture;
* a corrected block is refused, or does not pass `cancho check`.

To learn the language rather than its refusals, read `AGENTS.md` first; to see a refusal for every rule, `tests/reject/` has a program
per rule.

Programs are shown without the two `//~` header lines the fixtures begin with (they say what the test expects). The word `res` marks a
struct whose values must be ended; `[]` after `->` is the effect row, empty here because these functions do nothing to the world.

## Types and control flow

These are the refusals every typed language has. They come first because the rest of the document uses the same shape: a program, the rule that refuses it, the corrected program.

### 1. `type-mismatch`: A value of the wrong type

A function states what it takes and what it gives, and every use is checked against that. Here `double` takes an `int` and is handed a `bool`.

```cancho refused tests/reject/walkthrough_type_mismatch.cho
fn double(n: int) -> [] int {
    return n * 2;
}

fn main(world: World) -> [] int {
    release(world);
    return double(true);
}
```

The compiler refuses it:

```text
rule: type-mismatch
message: expected `int`, found `bool`
explanation: An expression's type does not match what the context requires — a return type, an annotation, a parameter, or the mode of a reference.
```

Corrected. The argument is an `int`.

```cancho fixed
fn double(n: int) -> [] int {
    return n * 2;
}

fn main(world: World) -> [] int {
    release(world);
    return double(21);
}
```

### 2. `assign-to-immutable`: Changing a `let`

`let` binds a name once. A binding that changes is declared with `var`.

```cancho refused tests/reject/walkthrough_assign_to_immutable.cho
fn main(world: World) -> [] int {
    release(world);
    let total = 1;
    total = total + 1;
    return total;
}
```

The compiler refuses it:

```text
rule: assign-to-immutable
message: `total` is immutable; declare it with `var` to assign to it
explanation: A `let` binding never changes after it is bound; `var` is the declaration that allows assignment.
```

Corrected. `let` became `var`.

```cancho fixed
fn main(world: World) -> [] int {
    release(world);
    var total = 1;
    total = total + 1;
    return total;
}
```

### 3. `missing-return`: A path that returns nothing

Every path out of a function returns the declared type. Falling off the end is not a path.

```cancho refused tests/reject/walkthrough_missing_return.cho
fn sign(n: int) -> [] int {
    if n > 0 {
        return 1;
    }
}

fn main(world: World) -> [] int {
    release(world);
    return sign(5);
}
```

The compiler refuses it:

```text
rule: missing-return
message: function `sign` can finish without returning a value
explanation: Every path out of a function returns a value of its declared type; falling off the end is not one of them.
```

Corrected. The path where `n` is not positive returns too.

```cancho fixed
fn sign(n: int) -> [] int {
    if n > 0 {
        return 1;
    }
    return 0;
}

fn main(world: World) -> [] int {
    release(world);
    return sign(5);
}
```

### 4. `match-not-exhaustive`: A `match` that forgets a case

A `match` names every variant of its enum, or has a `_` arm. There is no runtime failure for a value that matched nothing, so the compiler asks for the case now.

```cancho refused tests/reject/walkthrough_match_not_exhaustive.cho
enum Shape { Circle(int), Rect(int, int) }

fn area(s: Shape) -> [] int {
    match s {
        Shape::Circle(r) => { return r * r * 3; }
    }
    return 0;
}

fn main(world: World) -> [] int {
    release(world);
    return area(Shape::Circle(2));
}
```

The compiler refuses it:

```text
rule: match-not-exhaustive
message: this `match` does not cover `Shape::Rect`
explanation: A `match` covers every variant of its enum, or has a `_`. There is no runtime failure for a value that matched nothing.
```

Corrected. The `Rect` arm exists, and the `return 0` after the `match`, which could now never run, is gone.

```cancho fixed
enum Shape { Circle(int), Rect(int, int) }

fn area(s: Shape) -> [] int {
    match s {
        Shape::Circle(r) => { return r * r * 3; }
        Shape::Rect(w, h) => { return w * h; }
    }
}

fn main(world: World) -> [] int {
    release(world);
    return area(Shape::Circle(2));
}
```

## Ownership: `res` values

Cancho's central idea. A `res` value (a file, a connection, a handle, a struct declared `res`) must be ended exactly once, by the function that owns that job. The compiler follows each one through the function.

### 5. `linear-value-unconsumed`: Dropping a `res` value

A `res` value is something that must be ended: a file, a connection, a handle. It is consumed exactly once on every path. Assigning over a live one would drop it without anything having ended it.

```cancho refused tests/reject/walkthrough_linear_value_unconsumed.cho
res struct Ticket { fd: int }

fn open(fd: int) -> [] Ticket {
    return Ticket { fd: fd };
}

fn close(t: Ticket) -> [] int {
    let Ticket { fd } = t;
    return fd;
}

fn main(world: World) -> [] int {
    release(world);
    var t = open(1);
    t = open(2);
    return close(t);
}
```

The compiler refuses it:

```text
rule: linear-value-unconsumed
message: assigning to `t` would discard the `res` value it still holds; consume it first
explanation: A `res` value is consumed exactly once on every path: not dropped at the end of a block, not discarded by an assignment or a statement, and not left to differ between branches.
```

Corrected. Each ticket is closed by the function that owns that job, then the name is free.

```cancho fixed
res struct Ticket { fd: int }

fn open(fd: int) -> [] Ticket {
    return Ticket { fd: fd };
}

fn close(t: Ticket) -> [] int {
    let Ticket { fd } = t;
    return fd;
}

fn main(world: World) -> [] int {
    release(world);
    let first = open(1);
    let a = close(first);
    let second = open(2);
    return a + close(second);
}
```

### 6. `linear-use-after-move`: Using a `res` value after it is gone

Passing a `res` value to a function that takes it moves it. After that there is nothing left to borrow or use.

```cancho refused tests/reject/walkthrough_linear_use_after_move.cho
res struct Ticket { fd: int }

fn close(t: Ticket) -> [] int {
    let Ticket { fd } = t;
    return fd;
}

fn audit(t: Ticket) -> [] int {
    let fd = close(t);
    borrow t as &r in {
        return fd;
    }
}

fn main(world: World) -> [] int {
    release(world);
    return 0;
}
```

The compiler refuses it:

```text
rule: linear-use-after-move
message: `t` has already been consumed; there is nothing left to borrow
explanation: A `res` value is used exactly once. Once it has been moved or consumed there is nothing left to use, borrow or read.
```

Corrected. The borrow comes first; `close` takes the ticket last.

```cancho fixed
res struct Ticket { fd: int }

fn close(t: Ticket) -> [] int {
    let Ticket { fd } = t;
    return fd;
}

fn audit(t: Ticket) -> [] int {
    var seen = 0;
    borrow t as &r in {
        seen = 1;
    }
    return close(t) + seen;
}

fn main(world: World) -> [] int {
    release(world);
    return 0;
}
```

## Borrows

A `borrow` lends a reference for the length of a block. While it is lent, the reference is the only way to reach the value, and a shared reference is a read.

### 7. `borrow-conflict`: Changing what is lent

While a value is borrowed, the borrow is the only way to reach it. Assigning to `c` inside the block would change the value underneath the reference `r`.

```cancho refused tests/reject/walkthrough_borrow_conflict.cho
struct Counter { n: int }

fn main(world: World) -> [] int {
    release(world);
    var c = Counter { n: 1 };
    borrow c as &r in {
        c = Counter { n: 2 };
    }
    return c.n;
}
```

The compiler refuses it:

```text
rule: borrow-conflict
message: `c` is borrowed by an enclosing `borrow`, so it cannot be assigned to here
explanation: A unique reference `&!` is the only way to reach its referent while it lives, so nothing else may read, write, move or consume that value meanwhile.
```

Corrected. The assignment waits until the borrow is over.

```cancho fixed
struct Counter { n: int }

fn main(world: World) -> [] int {
    release(world);
    var c = Counter { n: 1 };
    borrow c as &r in {
        let seen = r.n;
    }
    c = Counter { n: 2 };
    return c.n;
}
```

### 8. `shared-reference-written`: Writing through a shared reference

A shared reference `&` is a read. A write goes through a unique reference `&!`, which is what `borrow mut` makes.

```cancho refused tests/reject/walkthrough_shared_reference_written.cho
struct Counter { n: int }

fn main(world: World) -> [] int {
    release(world);
    var c = Counter { n: 1 };
    borrow c as &r in {
        r.n = 2;
    }
    return c.n;
}
```

The compiler refuses it:

```text
rule: shared-reference-written
message: this is a shared reference `&`, which promises its referent will not change; `borrow mut` binds one that may be written through
explanation: A shared reference `&` promises its referent will not change, so it cannot be written through or turned into a unique one.
```

Corrected. `borrow mut` gives a unique reference.

```cancho fixed
struct Counter { n: int }

fn main(world: World) -> [] int {
    release(world);
    var c = Counter { n: 1 };
    borrow mut c as &!r in {
        r.n = 2;
    }
    return c.n;
}
```

## Regions

An arena (`region`) and a borrow have the same rule: nothing that points into them lives longer than they do.

### 9. `reference-escapes-region`: A reference that outlives its arena

A reference may not outlive the region it points into. The arena `a` is freed when its block ends, so a reference into it cannot be returned.

```cancho refused tests/reject/walkthrough_reference_escapes_region.cho
struct Node { value: int }

fn first[&q](fallback: &q Node) -> [] &q Node {
    region a {
        return alloc[a](Node { value: 1 });
    }
}

fn main(world: World) -> [] int {
    release(world);
    return 0;
}
```

The compiler refuses it:

```text
rule: reference-escapes-region
message: this returns a reference into `a`, which is an arena in this function; a reference may not outlive its region
explanation: A reference may not outlive the region it points into — an arena, a `borrow` block, or a caller's region parameter.
```

Corrected. The function returns the number, which does not point into the arena.

```cancho fixed
struct Node { value: int }

fn first[&q](fallback: &q Node) -> [] int {
    region a {
        let fresh = alloc[a](Node { value: 1 });
        return fresh.value + fallback.value;
    }
}

fn main(world: World) -> [] int {
    release(world);
    return 0;
}
```

## Effects

The bracket after `->` is a function's effect row: everything it does to the world. It is exact in both directions, so a signature is a contract and `[]` means pure.

### 10. `effect-not-declared`: An effect the row does not admit

The bracket after `->` is the effect row: what the function does to the world. It is exact. Reading the command line is an effect, so `verbose` has to say so.

```cancho refused tests/reject/walkthrough_effect_not_declared.cho
fn verbose[&a](args: &a Args) -> [] bool {
    return arg_count(args) > 1;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(io);
    var loud = false;
    borrow args as &a in {
        loud = verbose(a);
    }
    release(args);
    if loud {
        return 1;
    }
    return 0;
}
```

The compiler refuses it:

```text
rule: effect-not-declared
message: `verbose` performs `args`, which its row [] does not declare; narrow the body or widen the row
explanation: Every effect a function's body performs appears in its row, and a function performs what the capability it was lent authorises.
```

Corrected. The row names `args`.

```cancho fixed
fn verbose[&a](args: &a Args) -> [args] bool {
    return arg_count(args) > 1;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(io);
    var loud = false;
    borrow args as &a in {
        loud = verbose(a);
    }
    release(args);
    if loud {
        return 1;
    }
    return 0;
}
```

### 11. `effect-declared-not-performed`: An effect the row promises and never performs

The row is exact in the other direction too: a label the function never performs is refused. A row that may be too wide would no longer mean anything, and `[]` would stop meaning pure.

```cancho refused tests/reject/walkthrough_effect_declared_not_performed.cho
fn quiet() -> [io_write] int {
    return 1;
}

fn main(world: World) -> [] int {
    release(world);
    return quiet();
}
```

The compiler refuses it:

```text
rule: effect-declared-not-performed
message: `quiet` declares `io_write` but never performs it; a row is exact or it is decoration
explanation: An effect row is exact in both directions: a label a function declares but never performs makes the row decoration rather than a contract.
```

Corrected. The row is empty because the function does nothing to the world.

```cancho fixed
fn quiet() -> [] int {
    return 1;
}

fn main(world: World) -> [] int {
    release(world);
    return quiet();
}
```

## Capabilities

Authority is a value. `main` is handed the `World`, takes it apart, and passes the parts down. A capability is narrowed or released, never widened and never discarded.

### 12. `capability-not-narrowable`: Widening a capability

A capability can be narrowed, never widened. The scope of an `Ffi` is the set of libraries it names; a narrowing keeps some of them. Asking a capability that holds `libc` to hold `libc,libssl` is the one direction there is not.

```cancho refused tests/reject/walkthrough_capability_not_narrowable.cho
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(io);
    let one = narrow(ffi, "libc");
    let wider = narrow(one, "libc,libssl");
    release(wider);
    return 0;
}
```

The compiler refuses it:

```text
rule: capability-not-narrowable
message: `libc` cannot be narrowed to `libc,libssl`: a capability is attenuated, never widened, and a program must not be able to grant itself a library it was not given
explanation: `narrow` attenuates and never widens: the target must be strictly inside what the capability already grants, spelled as a literal so it can be checked where it is written.
```

Corrected. The wide grant is made first and narrowed after.

```cancho fixed
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(io);
    let both = narrow(ffi, "libc,libssl");
    let one = narrow(both, "libc");
    release(one);
    return 0;
}
```

### 13. `capability-misused`: Taking a capability apart

A capability is ended by `release`, and by nothing else. Taking one apart would destroy authority without naming the function that knows how.

```cancho refused tests/reject/walkthrough_capability_misused.cho
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(ffi);
    let Io { } = io;
    return 0;
}
```

The compiler refuses it:

```text
rule: capability-misused
message: `Io` is a capability and is destroyed by `release`, not by being taken apart; authority is a resource and the function that ends one is named
explanation: A capability is a value with exactly one way to use it: hold it, lend it, or end it with `release`. It is not taken apart, not written as a literal, and an operation it authorises reaches it through the parameter that names it.
```

Corrected. `release(io)` ends it.

```cancho fixed
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(ffi);
    release(io);
    return 0;
}
```

## What is not here

13 of the 58 rules that have a fixture in `tests/reject/` (counted by their `//~ RULE` lines). The chapters are the ones met first in `examples/`; the others are in
`tests/reject/`. A new chapter is a fixture `tests/reject/walkthrough_NAME.cho`, a block in this document in the same three parts, and
nothing else: the test finds the chapters by their `cancho refused` blocks.
