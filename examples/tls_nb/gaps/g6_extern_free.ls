// A foreign declaration of libc's own `free` is accepted by `check` and then refused by the backend as an internal error:
// the LLVM module the compiler emits already declares `free` for its heap, with a different signature.
edition 5;
extern fn free[&f](ffi: &f Ffi("libc"), p: int) -> [ffi("libc")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let libc = narrow(ffi, "libc");
    var status = 0;
    borrow libc as &f in {
        status = free(f, 0);
    }
    release(libc);
    return 0;
}
