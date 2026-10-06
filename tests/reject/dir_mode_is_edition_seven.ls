//~ ERROR `dir_mode` is not a function in this program
//~ RULE not-a-function

// `docs/directory-listing.md` §3.5: `dir_mode` and `dir_own_mode` are edition
// 7. An older file may already declare its own `dir_mode`, so to it the name
// is not a builtin at all (`editions.md` §7).

edition 6;

fn permissions_of[&d](dir: &d Dir) -> [dir_read] int {
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
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    return 0;
}
