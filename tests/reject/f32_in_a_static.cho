//~ ERROR a `static` holds scalars
//~ RULE static-item

// `docs/f32.md` §5: a `static` holds `int`, `byte`, `bool` or `float`; an
// `f32` table is deferred with the rest of `compile-time-data.md` §6's
// width question.

edition 6;

static table: [f32] {
    let out = alloc_slice[static](1, 0.0f32);
    return out;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    return 0;
}
