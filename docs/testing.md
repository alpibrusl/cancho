# Testing a program written here, not the compiler that builds it

> **Status: built.** §2's primitive (`trap()`, and `std.test`'s
> `assert`/`assert_eq`/`assert_ne` on it) is on both backends, and §3's
> runner is `cancho test`. A program — an agent's or a person's — had
> no way to state "this must be true" at all before this, not even
> that. What is *not* built is listed in §5: no per-test timeout, no
> message on a failed assertion, nothing generic over `T`.

## 1. What was missing, and how it was found

An agent-usability survey this session — not a hunt for a specific
bug, a direct question about what the toolchain still lacked for real
use — found that `cargo test` (`crates/cancho/tests/conformance/`)
is purely internal to developing the compiler itself, and that nothing
comparable exists for a program written *in* cancho. The real path
to testing one's own function, before this document, was: write
another `.cho` file, compare values with `if`/`==`, and return a
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
`cancho-codegen`, one `trap_if("true")` in `cancho-codegen-llvm`.

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

`tests/accept/assert.cho` is the pass side, checked against its own
`//~ STDOUT`/`//~ EXIT` the ordinary way. The fail side cannot be: a
trap is not a value a program's `return` can produce, so
`traps.rs::assert_fails_the_same_way_every_other_trap_does` checks it
the way every other trap fixture here is checked — built, run, and
confirmed killed by a signal (`run.status.code() == None`), not
exited.

## 3. The runner: `cancho test`

This language has no macros and no reflection (`README.md`'s own
"what's deliberately not here"), so nothing can enumerate a program's
own declarations from inside it. `cancho test some_file.cho` does it
from outside: it parses each named file, takes every `fn test_*`, writes
a `main` that runs whichever one `argv[1]` names, builds **once**, and
runs the executable **once per test**, because a trap kills the process
it happens in and a runner has to survive one test's failure to report
the next (`crates/cancho/src/test_cli.rs`).

```
$ cancho test --std ok.cho
running 4 tests
test test_add ... ok
test test_returns_nonzero ... FAILED
test test_traps ... FAILED
test test_with_caps ... ok

failures:

---- test_returns_nonzero ----
returned 7, and a test answers 0 to pass

---- test_traps ----
trapped: killed by signal 4 (SIGILL, the trap every checked operation ends with)

test result: FAILED. 2 passed; 2 failed
```

**The shape of a test.** `fn test_x[regions](caps) -> [row] int`: no
type parameters, returns `int`, and every parameter is a *unique*
reference to `Heap` or to `Io`, at most one of each. It answers `0` to
pass; anything else, or a signal, is a failure. A `fn test_*` of any
other shape is refused with exit 2 rather than skipped, because a test
that silently did not run reads as a pass. A test in a module other than
the root must be `pub`, since the runner reaches it from outside.

**Capabilities (the row this closes in §5).** A test that needs a heap
or the console declares it, exactly as `main` would: the synthesized
`main` splits `World`, releases `ffi`/`fs`/`args`, borrows `heap` and
`io` uniquely, passes each test the ones it names, and releases them
after. The row is exact, as everywhere here (`[heap]` on a test that
never touches the heap is a refusal), so a test that asks for `Io`
has to use it.

**No `main` in a test file.** The runner supplies its own, so a file
that declares a root-module `main` is refused with exit 2.

**Exit codes.** `0` every test passed; `4` the program built and at
least one test failed (a new code, `test` only, so a caller can tell a
red test from `1`, "the program was refused", a broken build); `2` no
`test_*` functions were found — a run that found none is not a pass —
or one has the wrong shape; `3` the environment failed. A status that
is a multiple of 256 would read back as `0` as a process exit, so the
synthesized `main` answers `1` for it instead.

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
| A per-test timeout | A test that loops forever hangs the run. One process per test makes a kill easy to add, but nothing has asked for it yet |
| A message on a failed assertion | Would make `assert` the one trap with text, against every other trap's own design (§4) |
| `assert_eq`/`assert_ne` over a generic `T` | Wants `==` over an arbitrary type, which nothing here has built yet |
| Running tests in parallel | One process per test makes it possible; output would need buffering per test, which the runner already does |
| Filtering (`cancho test f.cho -- name`) | Not needed until a file has enough tests to want one |
