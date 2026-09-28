//~ STDOUT main and t1 differ
//~ STDOUT main and t2 differ
//~ STDOUT t1 and t2 differ
//~ EXIT 0

// `docs/threads.md` §5 step 2's other half: not just that `spawn`
// type-checks, but that it hands back a *real* OS thread -- checked
// directly, the same "measured, not argued" standard `reach.md` itself
// is held to, rather than trusted from the type checker alone.
// `pthread_self()` read from `main` and from each worker gives three
// thread IDs; a correct `spawn` produces three genuinely distinct
// values, not three copies of the same one a sequential call would.
//
// This also exercises §3's "shared reference crosses, and the spawning
// side still has it after `join`" case: `f`, the borrowed `Ffi("libc")`
// capability, is the payload for *two* spawns and is read again by
// `main` itself (for `mine`) once both threads have been joined --
// exactly the aliasing the escape check is built to allow (two shared
// readers, never a writer) rather than something bolted on for threads.

edition 4;

import std.io;

extern fn pthread_self[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int;

fn worker[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    return pthread_self(ffi);
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let libc = narrow(ffi, "libc");
    borrow libc as &f in {
        let mine = pthread_self(f);
        let w = worker;
        let h1 = spawn(f, w);
        let h2 = spawn(f, w);
        let t1 = join(h1);
        let t2 = join(h2);

        borrow mut io as &!i in {
            if t1 != mine {
                io.write_all(i, "main and t1 differ\n");
            } else {
                io.write_all(i, "unexpected: main and t1 are the same thread\n");
            }
            if t2 != mine {
                io.write_all(i, "main and t2 differ\n");
            } else {
                io.write_all(i, "unexpected: main and t2 are the same thread\n");
            }
            if t1 != t2 {
                io.write_all(i, "t1 and t2 differ\n");
            } else {
                io.write_all(i, "unexpected: t1 and t2 are the same thread\n");
            }
        }
        release(io);
    }
    release(libc);
    return 0;
}
