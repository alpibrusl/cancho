//~ STDOUT 42
//~ EXIT 0

// `docs/threads.md` §2: `spawn`/`join`, the single-leaf slice
// (`docs/threads.md` §5 step 2, scoped down from arbitrary payloads --
// this language has no compiler-synthesised trampoline function yet, so
// `body`'s own compiled entry point becomes `pthread_create`'s start
// routine directly, and every leaf `pthread_create`'s `void *(*)(void
// *)` can carry is one pointer-width value: `int`, `bool`, `c_ptr`, a
// reference, or `()`). `worker` takes and returns a plain `int`, the
// simplest instance of that shape -- no capability, no reference, just
// a value moved onto a real OS thread and a value moved back.

edition 4;

import std.io;

fn worker(x: int) -> [] int {
    return x * 2;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let w = worker;
    let h = spawn(21, w);
    let result = join(h);

    borrow mut io as &!i in {
        io.print_int(i, result);
        io.newline(i);
    }
    release(io);
    return 0;
}
