//~ ERROR `exec_spawn_in` takes 8 arguments
//~ RULE arity-mismatch

// `docs/processes.md` §4.10: `exec_spawn_in` is `exec_spawn` with the `Dir`
// after the capability; `exec_spawn`'s seven arguments are one short.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    borrow exec as &x in {
        match exec_spawn_in(x, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
            Spawned::Ok(child) => {
                child_wait(child);
            }
            Spawned::Failed(e) => {
            }
        }
    }
    release(exec);
    return 0;
}
