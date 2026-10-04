// A foreign function with two string parameters cannot be declared: a byte slice crosses as a pointer AND a length, so the second
// parameter's register holds the first string's length. `strcmp(a, b)` reads `b` as the integer 6 and crashes (SIGSEGV).
// OpenSSL has the shape in SSL_CTX_load_verify_locations(ctx, CAfile, CApath) and libc in res_query/getaddrinfo's node.
edition 5;
extern fn strcmp[&f, &a, &b](ffi: &f Ffi("libc"), x: &a [byte], y: &b [byte]) -> [ffi("libc")] c_int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let libc = narrow(ffi, "libc");
    var r = 0;
    borrow libc as &f in {
        r = strcmp(f, "hello\0", "hello\0");
    }
    release(libc);
    return r;
}
