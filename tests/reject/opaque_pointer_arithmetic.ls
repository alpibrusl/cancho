//~ ERROR `c_ptr` has no arithmetic (`int` and `float` do)
//~ RULE operator-type-mismatch

// `docs/opaque-pointers.md` §3: `c_ptr` is comparable for equality and
// nothing else. Arithmetic on a handle would claim to know something
// about what it points at or how far it extends, and the checker knows
// neither -- that is exactly why accepting the handle at all is sound
// (`docs/reach.md` §3.1).

edition 3;

fn main() -> [] int {
    if null_ptr() + null_ptr() == null_ptr() {
        return 0;
    }
    return 1;
}
