//~ ERROR `f32_of` is not a function in this program
//~ RULE not-a-function

// `docs/f32.md` §6: `f32_of` and its three siblings are edition 6, like
// `value_barrier`: an earlier file may already declare its own, so to it
// the name is not a builtin at all (`editions.md` §7).

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
    let x = f32_of(1.5);
    return 0;
}
