module std.test;

// `std.test` — the smallest primitive a program needs to state what it
// expects and have the compiler-inserted machinery enforce it.
//
// `docs/testing.md` §2 is the argument: this repository's own answer to
// "an assumption failed" has been a trap since M0 — no message, no
// exception, `SIGILL` on both targets, the same as an overflowing `+`
// or an out-of-range index (`docs/defined-behaviour.md` §1). `assert`
// is that trap, reached deliberately rather than by an arithmetic or
// bounds check the compiler emitted on its own, and it is a library
// function rather than a builtin because deciding *when* to trap is
// the caller's policy, not this module's — the same reason
// `std.buffer`'s doubling is a library and not a builtin
// (`docs/boxed-slices.md` §4).
//
// There is no test runner here, and no way to discover a `test_*`
// function automatically: this language has no macros and no
// reflection, so nothing can enumerate a program's own declarations
// from inside it. `docs/testing.md` §3 names the runner as open,
// separate work. What this closes is narrower and load-bearing on its
// own: a program — an agent's or a person's — had no way to state
// "this must be true" at all, not even that, before today.

pub fn assert(condition: bool) -> [] int {
    if condition {
        return 0;
    }
    return trap();
}

// `a == b`, asserted. The common case spelled out, the way
// `docs/collections.md`'s own `unwrap_or` spells out a two-line match:
// not a new primitive, a name for the one line every caller would
// otherwise write for itself.
pub fn assert_eq(a: int, b: int) -> [] int {
    return assert(a == b);
}

pub fn assert_ne(a: int, b: int) -> [] int {
    return assert(a != b);
}
