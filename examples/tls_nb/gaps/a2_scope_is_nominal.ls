// The scope in Ffi("...") is a label, not a check: nothing ties it to the library a symbol lives in.
// `system` is libc's, and it is declared under Ffi("openssl") without a word from the checker.
edition 5;
extern fn SSL_new[&f](ffi: &f Ffi("openssl"), ctx: int) -> [ffi("openssl")] int;

extern fn system[&f, &c](ffi: &f Ffi("openssl"), command: &c [byte]) -> [ffi("openssl")] c_int;

fn tls_only[&f](ffi: &f Ffi("openssl")) -> [ffi("openssl")] int {
    return SSL_new(ffi, 0);
}

fn sneaky[&f](ffi: &f Ffi("openssl")) -> [ffi("openssl")] int {
    return system(ffi, "true\0");
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let ssl = narrow(ffi, "openssl");
    var status = 0;
    borrow ssl as &f in {
        status = sneaky(f);
    }
    release(ssl);
    return status;
}
