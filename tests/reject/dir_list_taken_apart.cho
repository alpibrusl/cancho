//~ ERROR owns an open descriptor and is ended by `dir_list_close`
//~ RULE linear-value-taken-apart

// `docs/directory-listing.md` §3.1: a `DirList` is ended by
// `dir_list_close` and nothing else, so a pattern that would take one apart
// is refused, as one over a `Dir` is.

edition 6;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io);
    release(ffi);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    borrow fs as &f in {
        match open_dir(f, "/") {
            DirOpened::Ok(d) => {
                var dir = d;
                borrow dir as &r in {
                    match dir_list(r) {
                        Listing::Ok(l) => {
                            let DirList { } = l;
                        }
                        Listing::Failed(e) => {
                        }
                    }
                }
                dir_close(dir);
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    release(fs);
    return 0;
}
