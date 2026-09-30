# Testing a program written here, not the compiler that builds it

> **Status: §2's primitive is built. §3's runner is not.** A program —
> an agent's or a person's — had no way to state "this must be true"
> at all before this, not even that. What exists now is `trap()`
> (`crates/lex-sys-ir/src/builtin.rs`) and `std.test`'s `assert`/
> `assert_eq`/`assert_ne` built on it, on both backends. There is no
> `lex-sys test` command, no test discovery, and no way to run many
> assertions in one process and see which failed rather than the
> first: §3 names that, and does not build it.

## 1. What was missing, and how it was found

An agent-usability survey this session — not a hunt for a specific
bug, a direct question about what the toolchain still lacked for real
use — found that `cargo test` (`crates/lex-sys/tests/conformance/`)
is purely internal to developing the compiler itself, and that nothing
comparable exists for a program written *in* lex-sys. The real path
to testing one's own function, before this document, was: write
another `.ls` file, compare values with `if`/`==`, and return a
distinct `int` as the process exit status. No assertion, no failure
message, no way to tell "this test failed" from "this program
computed 1" apart from reading the number back and remembering what
it meant.

That gap outranked several stdlib holes found the same session
(`std.vec` missing `pop`/`remove`/`insert`, no `regex`, no `readdir`)
because it is not a missing convenience — it is the thing that would
let an agent (or anyone) trust code it just wrote, here, rather than
eyeballing it.

## 2. `trap()`, and `assert` built on it

This repository's own answer to "an assumption failed" has been a trap
since M0: no message, no exception, `SIGILL` on both targets, exactly
like an overflowing `+` or an out-of-range index
(`docs/defined-behaviour.md` §1). Every existing trap is the
compiler's own arithmetic or bounds check firing on its own. Nothing
let a *program* reach that outcome deliberately — there was no
primitive to build `assert` out of.

`trap()` is that primitive: no arguments, `[] int` like `byte_of`'s
own fixed signature (the `int` is never actually produced, since the
call never returns, but the type checker needs a result type to check
the call site the ordinary way), no capability, because deciding to
trap is not an effect on the world. Both backends already carried the
one instruction this needs — Cranelift's `trapnz`/`trap`, the LLVM
backend's own `trap_if` (`ud2` on x86-64, `udf #0xc11f` on aarch64,
the same instruction `docs/llvm-backend.md` §3.2/§3.3 already measured
for every checked operation) — so this is the first builtin that
reaches it *unconditionally* rather than behind a check the compiler
emits on its own, and adding it was mechanical on both: one
`trap(TrapCode::unwrap_user(1))` plus a fresh dead block in
`lex-sys-codegen`, one `trap_if("true")` in `lex-sys-codegen-llvm`.

`std.test.assert(condition: bool) -> [] int` is a library function
built on it, not a second builtin:

```
pub fn assert(condition: bool) -> [] int {
    if condition {
        return 0;
    }
    return trap();
}
```

A library function rather than a builtin for the same reason
`std.buffer`'s doubling policy is a library and not a builtin
(`docs/boxed-slices.md` §4): deciding *when* to trap is the caller's
policy — `trap()` is the primitive, `assert` is one way to use it, and
nothing stops a program from writing its own. `assert_eq`/`assert_ne`
are `assert(a == b)`/`assert(a != b)` spelled out, over `int`, the same
way `docs/collections.md`'s own `unwrap_or` spells out a two-line
`match`: not a new primitive, a name for the line every caller would
otherwise write for itself.

`tests/accept/assert.ls` is the pass side, checked against its own
`//~ STDOUT`/`//~ EXIT` the ordinary way. The fail side cannot be: a
trap is not a value a program's `return` can produce, so
`traps.rs::assert_fails_the_same_way_every_other_trap_does` checks it
the way every other trap fixture here is checked — built, run, and
confirmed killed by a signal (`run.status.code() == None`), not
exited.

## 3. What this does not build: a runner

This language has no macros and no reflection (`README.md`'s own
"what's deliberately not here"), so nothing can enumerate a program's
own declarations from inside it. A `lex-sys test some_file.ls` that
found every `test_*` function and ran each one, reporting `ok`/`FAILED`
the way `cargo test`'s own output does, would need the *compiler* to
do that enumeration — parse the file, walk its `Item::Fn`s for a name
convention and a plain (`[] int`, no parameters, no generics) shape,
and build a synthetic program per match, one process each, since a
trap kills the process it happens in and a runner has to survive one
test's failure to report the next.

None of that is built. It is a real, separate piece of work — a new
CLI subcommand, a naming convention, a synthesized `main` per test,
one subprocess per test for isolation — and building it without first
having something to *call* inside each test would have been building
the frame before the primitive. §2 is that primitive. The runner is
next, not done here.

## 4. What this does not propose

Not a message on a failed assertion. Every trap in this language is
message-less by design (§2's whole argument), and `assert` inherits
that rather than becoming the one trap that carries text — a caller
that wants to know *which* assertion failed reaches for
`check --output json`'s located diagnostics at compile time, or reads
the source at the line a debugger stops on at run time, the same way
any other trap here is diagnosed.

Not `assert_eq`/`assert_ne` over anything but `int`. A generic
version wants `==` over an arbitrary `T`, which this language does not
have for every type (`docs/collections.md` never built one either) —
open, not answered here, the same way `docs/collections.md` §7 leaves
`map`/`and_then` open for want of closures.

## 5. Open

| Question | Why it waits |
|---|---|
| The test runner (§3) | Real, separate work — a CLI subcommand, a naming convention, one subprocess per test. Not built here |
| A message on a failed assertion | Would make `assert` the one trap with text, against every other trap's own design (§4) |
| `assert_eq`/`assert_ne` over a generic `T` | Wants `==` over an arbitrary type, which nothing here has built yet |
| Capability-carrying tests (a test that needs `Heap`/`Io` to do anything) | The runner would need to synthesize a `main` that splits `World` and threads the right capability in — open until §3 is |
