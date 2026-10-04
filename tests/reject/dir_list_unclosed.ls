//~ ERROR is still live at the end of this block
//~ RULE linear-value-unconsumed

// `docs/directory-listing.md` §3.1: a `DirList` is a `res` value, so a
// listing that is never closed is a compile error, as an unclosed `Dir` is.

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
