//~ ERROR expected
//~ RULE type-mismatch

// `docs/processes.md` §4.10: the directory a child starts in is lent, as the
// capability is: `exec_spawn_in` takes `&Dir`, and a `Dir` by value is a type
// error rather than a handle the spawn silently consumes.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io);
    release(ffi);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    borrow exec as &x in {
        borrow fs as &f in {
            match open_dir(f, "/tmp") {
                DirOpened::Ok(d) => {
                    match exec_spawn_in(x, d, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
                        Spawned::Ok(child) => {
                            child_wait(child);
                        }
                        Spawned::Failed(e) => {
                        }
                    }
                }
                DirOpened::Failed(e) => {
                }
            }
        }
    }
    release(exec);
    release(fs);
    return 0;
}
