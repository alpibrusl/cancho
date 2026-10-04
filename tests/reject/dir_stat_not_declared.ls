//~ ERROR performs `dir_read`, which its row
//~ RULE effect-not-declared

// `docs/directory-listing.md` §3.3: a status beneath a directory performs
// `dir_read`, as every other step beneath one does, and a function that
// borrows a `Dir` has to say so.

edition 6;

fn size_of[&d](dir: &d Dir) -> [] int {
    match dir_stat(dir, "x") {
        DirStat::Ok(kind, size, mtime) => {
            return size;
        }
        DirStat::Failed(e) => {
            return 0 - e;
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
