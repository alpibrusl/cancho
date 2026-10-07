//~ ERROR cannot be compared with `==`
//~ RULE operator-type-mismatch

// `docs/function-values.md` §4.2 says nothing about equality, on
// purpose: a function value's only meaning is "call through it", and
// nothing here invents an identity for one to compare -- so `==`
// stays exactly the list of scalar types it always was
// (`docs/opaque-pointers.md` §3 grew that list by one, `c_ptr`; this
// is not a second).

fn double(x: int) -> [] int {
    return x + x;
}

fn triple(x: int) -> [] int {
    return x + x + x;
}

fn main() -> [] int {
    let a = double;
    let b = triple;
    if a == b {
        return 1;
    }
    return 0;
}
