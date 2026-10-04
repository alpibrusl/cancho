//~ EXIT 7

// `docs/foreign-authority.md` section 5.2: one capability over two libraries,
// each symbol declared under its own, lent down to what each helper needs.
// The conformance suite (`conformance/foreign_authority.rs`) reads the
// authority report of the same shape.
edition 5;

extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;

extern fn pthread_self[&f](ffi: &f Ffi("libpthread")) -> [ffi("libpthread")] int;

fn magnitude[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int {
    return labs(ffi, n);
}

fn thread[&f](ffi: &f Ffi("libpthread")) -> [ffi("libpthread")] bool {
    return pthread_self(ffi) != 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let native = narrow(ffi, "libpthread,libc");
    var status = 0;
    borrow native as &f in {
        status = magnitude(f, 0 - 7);
        if !thread(f) {
            status = 1;
        }
    }
    release(native);
    return status;
}
