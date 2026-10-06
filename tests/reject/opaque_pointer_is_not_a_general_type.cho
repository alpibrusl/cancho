//~ ERROR unknown type `c_ptr`
//~ RULE unknown-name

// `docs/opaque-pointers.md` §4, matching `c_int`'s own existing rule
// (`docs/reach.md` §3.4): `c_ptr` is recognised by name at exactly one
// position, an `extern fn`'s parameter or return type. It is never
// registered as a general type, so writing it anywhere else -- a
// written function's own return type, here -- is an ordinary unresolved
// name rather than something this check has to refuse specially.

edition 3;

fn main() -> [] c_ptr {
    return null_ptr();
}
