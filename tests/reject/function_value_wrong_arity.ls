//~ ERROR takes 1 argument, but 2 were given
//~ RULE arity-mismatch

// `docs/function-values.md` §4.2: a call through a value is checked
// against the value's own type exactly as a named call is checked
// against a declaration -- arity included.

fn double(x: int) -> [] int {
    return x + x;
}

fn main() -> [] int {
    let h = double;
    return h(1, 2);
}
