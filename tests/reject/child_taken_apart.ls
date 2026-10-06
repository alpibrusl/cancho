//~ ERROR is ended by `child_wait`, not by being taken apart
//~ RULE linear-value-taken-apart

// `docs/processes.md` §4.7: a pattern cannot end a child; only `child_wait`
// reaps one.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    borrow exec as &x in {
        match exec_spawn(x, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
            Spawned::Ok(child) => { let Child { } = child; }
            Spawned::Failed(e) => { }
        }
    }
    release(exec);
    return 0;
}
