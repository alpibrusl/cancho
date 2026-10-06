//~ ERROR performs `dir_write`, which its row
//~ RULE effect-not-declared

// `docs/directory-handles.md` §3: creating, renaming, removing and syncing
// beneath a directory perform `dir_write`, a label of their own, so a
// function that may only read beneath a `Dir` says `[dir_read]` and is
// refused the moment it changes anything.

edition 6;

fn tidy[&d](dir: &d Dir) -> [dir_read] int {
    match dir_remove(dir, "stale.tmp") {
        Done::Ok(n) => {
            return 0;
        }
        Done::Failed(e) => {
            return e;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    return 0;
}
