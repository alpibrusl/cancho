//~ ERROR is not a borrowed `Exec`
//~ RULE capability-misused

// `docs/processes.md` §3.2: a program is started through the capability that
// names which programs may be, and nothing else stands in for it.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    release(exec);
    match exec_spawn(io, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
        Spawned::Ok(child) => { child_wait(child); }
        Spawned::Failed(e) => { }
    }
    release(io);
    return 0;
}
