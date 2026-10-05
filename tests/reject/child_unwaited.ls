//~ ERROR `child` is still live at the end of this block
//~ RULE linear-value-unconsumed

// `docs/processes.md` §4.7: a started child is reaped on every path. Returning
// without `child_wait` leaves a zombie, and here that is a compile error.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    var started = 0;
    borrow exec as &x in {
        match exec_spawn(x, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
            Spawned::Ok(child) => { started = 1; }
            Spawned::Failed(e) => { }
        }
    }
    release(exec);
    return started;
}
