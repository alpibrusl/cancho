// Can the payload be a SHARED reference to an Ffi, so the spawning thread keeps using it too?
edition 5;
extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;

fn worker[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    return labs(ffi, 0 - 5);
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(heap);
    let libc = narrow(ffi, "libc");
    var code = 1;
    borrow libc as &f in {
        let w = worker;
        let t = spawn(f, w);
        let mine = labs(f, 0 - 7);
        let theirs = join(t);
        code = mine * 10 + theirs - 75;
    }
    release(libc);
    return code;
}
