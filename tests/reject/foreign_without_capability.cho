//~ ERROR borrows 0 `Ffi` capabilities
//~ RULE foreign-declaration

// `docs/foreign-authority.md` section 5.1: a foreign function is reached
// through exactly one `Ffi`. This declaration names no effect and takes no
// capability, so it was accepted, ran a shell, and left the authority
// report saying `bounded: true` and *never touches foreign code*.

extern fn system[&c](command: &c [byte]) -> [] c_int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(heap);
    release(fs);
    release(ffi);
    release(io);
    return system("true\0");
}
