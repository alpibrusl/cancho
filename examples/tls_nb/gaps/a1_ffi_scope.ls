// What the checker allows for an Ffi scope: any name, one per `narrow`, and the row says it exactly. `lex-sys authority` on this
// prints `ffi("libssl") <- unbounded` and lists the foreign symbols it calls (docs/under-a-grant.md: a library is not a domain).
edition 5;
extern fn TLS_client_method[&f](ffi: &f Ffi("libssl")) -> [ffi("libssl")] c_ptr;

fn only_ssl[&f](ssl: &f Ffi("libssl")) -> [ffi("libssl")] int {
    let m = TLS_client_method(ssl);
    if m == null_ptr() {
        return 1;
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let ssl = narrow(ffi, "libssl");
    var status = 0;
    borrow ssl as &f in {
        status = only_ssl(f);
    }
    release(ssl);
    return status;
}
