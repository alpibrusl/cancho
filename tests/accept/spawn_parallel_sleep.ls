//~ EXIT 0

// `docs/threads.md` §5 step 2's wall-clock half: four threads, each
// blocked in a real `usleep(200ms)`, joined. A `spawn` that actually
// hands off to the OS scheduler finishes this program in about one
// sleep's worth of wall time; one that silently ran the four sleeps
// sequentially (on the calling thread, say, as a disguised ordinary
// call) would take roughly four times as long. `crates/lex-sys/tests/
// conformance/backends.rs`'s own
// `spawn_and_join_run_concurrently_not_sequentially` is where that
// timing is actually asserted -- this fixture carries no `//~ STDOUT`
// because the property under test is wall-clock time, not output.

edition 4;

import std.io;

extern fn usleep[&f](ffi: &f Ffi("libc"), usec: int) -> [ffi("libc")] int;

fn worker[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    return usleep(ffi, 200000);
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let libc = narrow(ffi, "libc");
    borrow libc as &f in {
        let w = worker;
        let h1 = spawn(f, w);
        let h2 = spawn(f, w);
        let h3 = spawn(f, w);
        let h4 = spawn(f, w);
        join(h1);
        join(h2);
        join(h3);
        join(h4);
    }
    release(libc);
    return 0;
}
