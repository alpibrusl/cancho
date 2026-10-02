//~ EXIT 0

// `join(t)` is its own IR node and the LLVM backend's scalar-kind inference had no arm for it, so a `join` that was an
// operand of arithmetic (`join(a) + join(b)`) failed with "the compiler failed to generate code". Found building
// `fork_heap_workers.ls`; the kind of a `join` is its thread's result type.

edition 5;

fn one(x: int) -> [] int {
    return x;
}

fn two(x: int) -> [] int {
    return x + 1;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    release(heap);
    let f = one;
    let g = two;
    let a = spawn(1, f);
    let b = spawn(1, g);
    return join(a) + join(b) - 3;
}
