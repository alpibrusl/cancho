//~ ERROR `dir_rename_new` is not a function in this program
//~ RULE not-a-function

// `docs/directory-handles.md` §3, slice 4: `dir_rename_new` is edition 7. An
// older file may already declare its own `dir_rename_new`, so to it the name
// is not a builtin at all (`editions.md` §7).

edition 6;

fn publish[&d](dir: &d Dir) -> [dir_write] int {
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
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    return 0;
}
