//~ ERROR performs `dir_read`, which its row
//~ RULE effect-not-declared

// `docs/directory-listing.md` §3.3: reading a listing performs `dir_read`,
// and a function handed only a `DirList` has to say so. The path was spent
// at `open_dir`, so the row names none.

edition 6;

fn next[&l, &b](list: &!l DirList, name: &!b [byte]) -> [] int {
    match dir_next(list, name) {
        Listed::Name(n, kind) => {
            return n;
        }
        Listed::End => {
            return 0;
        }
        Listed::Failed(e) => {
            return e;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    release(fs);
    return 0;
}
