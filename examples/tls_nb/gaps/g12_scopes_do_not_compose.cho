// `Ffi("libc")` and `Ffi("tls")` are different types, `split` hands out ONE Ffi and `narrow` consumes it: a program has one scope.
// net.sockets and net.connect hard-code Ffi("libc"), so a program that also has its own scope for OpenSSL cannot call both: the
// second `narrow` is a use after move, and a function taking both capabilities can never be called.
edition 5;
extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;

extern fn SSL_free[&f](ffi: &f Ffi("tls"), ssl: int) -> [ffi("tls")] int;

fn both[&a, &b](libc: &a Ffi("libc"), tls: &b Ffi("tls")) -> [ffi("libc"), ffi("tls")] int {
    return labs(libc, 0 - 3) + SSL_free(tls, 0);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let libc = narrow(ffi, "libc");
    let tls = narrow(ffi, "tls");
    borrow libc as &a in {
        borrow tls as &b in {
            both(a, b);
        }
    }
    release(libc);
    release(tls);
    return 0;
}
