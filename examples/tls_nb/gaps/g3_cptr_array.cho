// c_ptr in a slice: does `[c_ptr]` work as a table of handles?
edition 5;
extern fn fdopen[&f, &m](ffi: &f Ffi("libc"), fd: int, mode: &m [byte]) -> [ffi("libc")] c_ptr;

extern fn fclose[&f](ffi: &f Ffi("libc"), stream: c_ptr) -> [ffi("libc")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let libc = narrow(ffi, "libc");
    var status = 0;
    borrow libc as &f in {
        region r {
            let a = alloc_slice[r](4, null_ptr());
            a[1] = fdopen(f, 1, "w");
            if a[1] == null_ptr() {
                status = 1;
            }
            if a[0] != null_ptr() {
                status = 2;
            }
            fclose(f, a[1]);
        }
    }
    release(libc);
    return status;
}
