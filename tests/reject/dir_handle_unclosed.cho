//~ ERROR is still live at the end of this block
//~ RULE linear-value-unconsumed

// `docs/directory-handles.md` §2: a `Dir` is a `res` value, so leaking one
// is a compile error with no new machinery -- as `File`'s is.

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
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    release(fs);
    return 0;
}
