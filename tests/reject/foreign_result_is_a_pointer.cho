// `docs/reach.md` §3.1: a foreign *result* is a scalar, and that is the
// rule the whole reach argument turns on. C's `getenv` returns a `char *`
// -- a pointer with no length, no region and no provenance the checker
// can name -- so there is nothing here for it to come back as `&n [byte]`.
//
// `docs/opaque-pointers.md` §3 (corrected here: an OpenSSL or a libpq
// client is *not* out of reach any more, only this declaration's own
// choice is wrong) opened exactly one foreign-result shape for a pointer:
// `c_ptr`, an opaque handle that is never dereferenced. `getenv` really
// does return a NUL-terminated string, not a handle, so `c_ptr` is not
// the fix here either -- the fix is that this program cannot say what it
// wants (a length-free, checker-verified pointer into C's own memory) at
// all, by design.
//
// The message names `int`, `bool` and `c_ptr` and nothing else. It used
// to offer `()` as a fourth, which sent a reader to write `()` and be
// told there is no `()` -- `tuples.md` §4 keeps unit out of the grammar
// on purpose.
//~ ERROR a foreign result is `int`, `bool` or `c_ptr`
//~ RULE foreign-boundary-type

extern fn getenv[&f, &n](ffi: &f Ffi("libc"), name: &n [byte])
    -> [ffi("libc")] &n [byte];

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(io);
    release(ffi);
    return 0;
}
