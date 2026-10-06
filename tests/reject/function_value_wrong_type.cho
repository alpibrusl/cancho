//~ ERROR expected `int`, found `bool`
//~ RULE type-mismatch

// `docs/function-values.md` §4.2: a call through a value checks each
// argument against the value's own type, the same `expect_type` a
// named call's arguments are checked with.

fn double(x: int) -> [] int {
    return x + x;
}

fn main() -> [] int {
    let h = double;
    return h(true);
}
