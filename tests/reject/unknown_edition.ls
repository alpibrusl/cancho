//~ ERROR unknown edition 4; the only editions today are 1, 2 and 3
//~ RULE unknown-edition

// `docs/editions.md` §6.1, §7.
//
// A file's `edition N;` marker names one of the editions this compiler
// knows. Edition 1 is the language as it is today and needs no marker
// at all; edition 2 adds `Net`; edition 3 adds `c_ptr`/`null_ptr`
// (`docs/opaque-pointers.md` §4). There is nothing later than that yet
// to opt into.

edition 4;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);
    release(heap);
    release(io);
    return 0;
}
