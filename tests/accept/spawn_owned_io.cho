//~ STDOUT hello from a thread
//~ EXIT 0

// `docs/threads.md` §5 step 3: an *owned* capability, moved into a
// spawned thread and used there for real I/O. `Io` is declared with
// no fields at all (`defs.rs`'s own `prelude_types`), so it costs
// zero leaves crossing to `pthread_create`'s `void *arg` -- the same
// path this backend already built for a `()` payload, extended in
// `crosses_to_a_thread` (`lower/conc.rs`) from "no leaves" meaning
// only `Unit` to "no leaves" meaning any zero-field capability too.
// No new codegen: both backends already handle the zero-leaf case.
//
// `worker`'s own row is `[]`, not `[io_write]`: owning `Io` outright
// *discharges* every stream of the console (`defs.rs`'s
// `discharged_by`), the same reason `main` itself is allowed to
// declare `[]` while still performing `io_write` through a capability
// it holds. Declaring `io_write` explicitly here is the wrong row --
// checked while writing this fixture, not assumed.

edition 4;

import std.io;

fn worker(io: Io) -> [] int {
    borrow mut io as &!i in {
        io.write_all(i, "hello from a thread\n");
    }
    release(io);
    return 7;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let w = worker;
    let h = spawn(io, w);
    let result = join(h);
    return result - 7;
}
