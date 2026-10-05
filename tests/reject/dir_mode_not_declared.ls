//~ ERROR performs `dir_read`, which its row
//~ RULE effect-not-declared

// `docs/directory-listing.md` §3.5: reading a file's permission bits beneath a
// directory performs `dir_read`, as `dir_stat` does, and a function that
// borrows a `Dir` has to say so.

edition 7;

fn permissions_of[&d](dir: &d Dir) -> [] int {
    match dir_mode(dir, "x") {
        Done::Ok(bits) => {
            return bits;
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
