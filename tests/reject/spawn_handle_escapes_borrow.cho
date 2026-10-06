//~ ERROR a reference may not outlive its region
//~ RULE reference-escapes-region

// `docs/threads.md` §3: the whole soundness argument, checked. `Thread[T,
// R]` carries `T` (the payload's own type) as a type argument purely so
// its region is tracked -- no new checker code, just `Type::Named`'s
// already-generic `mentions`/`regions_into` walk into its arguments,
// the same escape check that already refuses a plain reference leaving
// its `borrow` block. `h`'s type here is `Thread[&r int, int]`, and `r`
// is `spawn_it`'s own inner `borrow` block, not the `q` its return type
// is allowed to name -- so returning `h` unjoined is refused exactly
// like returning the borrowed reference itself would be, before `h`
// ever reaches a `join` call that could legally consume it.

edition 4;

fn worker[&r](x: &r int) -> [] int {
    return *x;
}

fn spawn_it[&q](n: int) -> [conc] Thread[&q int, int] {
    borrow n as &r in {
        let w = worker;
        let h = spawn(r, w);
        return h;
    }
}

fn main() -> [] int {
    return 0;
}
