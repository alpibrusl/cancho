//~ ERROR expected `int`, found `c_ptr`
//~ RULE type-mismatch

// `docs/opaque-pointers.md` §2: unlike `c_int`, `c_ptr` does not collapse
// to another type -- it stays `Type::CPtr` everywhere, so this is an
// ordinary type mismatch rather than something that happens to be
// meaningless. The positive mirror of `docs/reach.md` §3.1.1's `malloc`
// example: there, a smuggled `int` cannot become a reference; here, a
// real handle cannot become an `int`.

edition 3;

fn main() -> [] int {
    let x: int = null_ptr();
    return x;
}
