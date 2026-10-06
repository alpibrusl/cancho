// GAP 1: c_ptr cannot be the parameter or return type of an ordinary function.
edition 5;
extern fn SSL_CTX_new[&f](ffi: &f Ffi("libc"), method: c_ptr) -> [ffi("libc")] c_ptr;

fn keep(p: c_ptr) -> [] bool {
    return p == null_ptr();
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
    return 1;
}
