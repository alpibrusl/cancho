//~ ERROR needs `edition 6;`
//~ RULE literal-form

// `docs/f32.md` §6: `f32` is edition 6. Before it existed the lexer
// refused `1.5f32`, so no earlier file can contain a literal; an earlier
// file that now writes one is told which edition it wants.

edition 5;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let x = 1.5f32;
    return 0;
}
