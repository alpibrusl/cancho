edition 5;
extern fn fdopen[&f, &m](ffi: &f Ffi("libc"), fd: int, mode: &m [byte]) -> [ffi("libc")] c_ptr;

extern fn fclose[&f](ffi: &f Ffi("libc"), stream: c_ptr) -> [ffi("libc")] int;

fn get[T, &a](tab: &a [T], i: int) -> [] T {
    return tab[i];
}

fn put[T, &a](tab: &!a [T], i: int, v: T) -> [] int {
    tab[i] = v;
    return 0;
}

struct Cell[T] {
    v: T,
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
    var status = 0;
    borrow libc as &f in {
        region r {
            let a = alloc_slice[r](4, null_ptr());
            put(a, 1, fdopen(f, 1, "w"));
            let p = get(a, 1);
            if p == null_ptr() {
                status = 1;
            }
            let c = Cell { v: p };
            if c.v == null_ptr() {
                status = 3;
            }
            fclose(f, c.v);
        }
    }
    release(libc);
    return status;
}
