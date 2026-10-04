//~ ERROR is not a library name
//~ RULE foreign-scope

// `docs/foreign-authority.md` section 5.2: the scope of an `Ffi` is a set of
// library names, and `libc:statx` is a symbol, not a library. A scope that
// tried to name one would be a claim the report could not print as a pair.

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(io);
    let held = narrow(ffi, "libc:statx");
    release(held);
    return 0;
}
