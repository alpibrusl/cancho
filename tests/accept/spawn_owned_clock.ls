//~ EXIT 0

// `docs/parallelism.md` T2: an owned `Clock`, moved into a spawned thread and read there. `Clock` is declared with
// no fields (`defs.rs`, and `leaf_free`), so it crosses `pthread_create`'s one pointer exactly as `Io` does: zero
// leaves, no new codegen -- only `crosses_to_a_thread`'s allowlist, which had left it off. The worker's row is `[]`:
// owning the capability outright discharges the label, as `spawn_owned_io.ls` found for `Io`.

edition 5;

fn worker(c: Clock) -> [] int {
    var now = 0;
    borrow c as &k in {
        now = clock_ms(k);
    }
    release(c);
    if now > 0 {
        return 0;
    }
    return 1;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(heap);
    release(io);
    let w = worker;
    let h = spawn(clock, w);
    return join(h);
}
