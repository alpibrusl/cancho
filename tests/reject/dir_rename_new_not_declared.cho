//~ ERROR performs `dir_write`, which its row
//~ RULE effect-not-declared

// `docs/directory-handles.md` §3, slice 4: a rename that refuses to replace
// still changes what is beneath the directory, so it performs `dir_write`, as
// `dir_rename` does, and a function that borrows a `Dir` has to say so.

edition 7;

fn publish[&d](dir: &d Dir) -> [dir_read] int {
    match dir_rename_new(dir, "tmp", "final") {
        Done::Ok(n) => {
            return n;
        }
        Done::Failed(e) => {
            return 0 - e;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals); release(exec);
    return 0;
}
