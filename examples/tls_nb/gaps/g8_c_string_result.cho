// A C string that C returns cannot be read: `SSL_get_version(ssl)` answers `const char *`, which is an integer here and has no way
// into a byte slice (c_ptr is opaque, nothing dereferences an int). The names of the protocol and the cipher come out only through
// functions that fill a buffer the caller owns (`SSL_CIPHER_description`), and the error text through `ERR_error_string_n`.
// This reproducer declares the call and shows what is left: a number that is an address.
edition 5;
import std.io;

extern fn TLS_client_method[&f](ffi: &f Ffi("tls")) -> [ffi("tls")] int;

extern fn SSL_CTX_new[&f](ffi: &f Ffi("tls"), m: int) -> [ffi("tls")] int;

extern fn SSL_new[&f](ffi: &f Ffi("tls"), c: int) -> [ffi("tls")] int;

extern fn SSL_get_version[&f](ffi: &f Ffi("tls"), s: int) -> [ffi("tls")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let tls = narrow(ffi, "tls");
    var text = 0;
    borrow tls as &f in {
        text = SSL_get_version(f, SSL_new(f, SSL_CTX_new(f, TLS_client_method(f))));
    }
    borrow mut io as &!i in {
        io.write_all(i, "the protocol name is at address ");
        io.print_nat(i, text);
        io.write_all(i, "; there is no way to read it\n");
    }
    release(tls);
    release(io);
    return 0;
}
