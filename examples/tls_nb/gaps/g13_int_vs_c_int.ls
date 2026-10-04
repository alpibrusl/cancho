// A C `int` result declared as `int` is only right by luck: the upper half of rax is not defined by the ABI.
// `close(-1)` returns -1 (EBADF). Declared `-> int` the program may read 4294967295, and `if close(fd) < 0` is then false.
// `examples/tls_client/tls_client.ls` declares SSL_connect/SSL_write/SSL_read `-> int` and tests `<= 0`.
edition 5;
import std.io;

extern fn close[&f](ffi: &f Ffi("libc"), fd: int) -> [ffi("libc")] int;

extern fn dup2[&f](ffi: &f Ffi("libc"), old: int, new: int) -> [ffi("libc")] c_int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let libc = narrow(ffi, "libc");
    var wide = 0;
    var narrow_ = 0;
    borrow libc as &f in {
        wide = close(f, 0 - 1);
        narrow_ = dup2(f, 0 - 1, 0 - 1);
    }
    borrow mut io as &!i in {
        io.print_int(i, wide);
        io.newline(i);
        io.print_int(i, narrow_);
        io.newline(i);
    }
    release(libc);
    release(io);
    return 0;
}
