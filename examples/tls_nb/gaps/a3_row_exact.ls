// A caller that borrows the capability and passes it on must say so in its row; one that does not borrow it cannot reach OpenSSL.
edition 5;
extern fn SSL_new[&f](ffi: &f Ffi("openssl"), ctx: int) -> [ffi("openssl")] int;

fn leaf[&f](ffi: &f Ffi("openssl")) -> [ffi("openssl")] int {
    return SSL_new(ffi, 0);
}

// REFUSED (effect-not-declared): calls `leaf` and its row says nothing
fn middle[&f](ffi: &f Ffi("openssl")) -> [] int {
    return leaf(ffi);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(ffi);
    return 0;
}
