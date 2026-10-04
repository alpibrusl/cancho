// GAP 2: c_ptr cannot be a struct field or an array element.
edition 5;
struct Holder {
    p: c_ptr,
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    return 2;
}
