// A pointer-to-pointer out-parameter that is not the last parameter cannot be declared. OpenSSL's
//   int BIO_new_bio_pair(BIO **bio1, size_t writebuf1, BIO **bio2, size_t writebuf2)
// has two. A byte slice supplies a pointer AND a length (so `bio1` would get the right pointer and `writebuf1` the slice's length),
// and the second out-parameter has no register of its own. getaddrinfo's `struct addrinfo **res` is last, and can be had as a
// slice the call writes eight bytes into -- if the other pointers (node, service, hints) could be passed, which they cannot (g7).
// This declares the closest honest shape and shows what C would receive: two integers where it expects two pointers.
edition 5;
extern fn BIO_new_bio_pair[&f, &a, &b](ffi: &f Ffi("tls"), out1: &a [byte], size1: int, out2: &b [byte], size2: int) -> [ffi("tls")] c_int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let tls = narrow(ffi, "tls");
    var r = 0;
    borrow tls as &f in {
        region s {
            let a = alloc_slice[s](8, byte_of(0));
            let b = alloc_slice[s](8, byte_of(0));
            r = BIO_new_bio_pair(f, a, 4096, b, 4096);
        }
    }
    release(tls);
    return r;
}
