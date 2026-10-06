//~ ERROR performs `dir_read`, which its row
//~ RULE effect-not-declared

// `docs/directory-handles.md` §2: a step beneath a directory performs
// `dir_read`, and a function that borrows a `Dir` has to say so. The row is
// `[dir_read]` and never `[fs_read("/some/path")]`: the path was spent at
// `open_dir`, so the handle is the authority.

edition 6;

fn peek[&d](dir: &d Dir) -> [] int {
    match dir_open_read(dir, "x") {
        Opened::Ok(f) => {
            file_close(f);
            return 0;
        }
        Opened::Failed(e) => {
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
