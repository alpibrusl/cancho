//~ ERROR owns an open descriptor and is ended by `dir_close`
//~ RULE linear-value-taken-apart

// `docs/directory-handles.md` §2: a `Dir` is ended by `dir_close` and
// nothing else, so a pattern that would take one apart is refused, as one
// over a `File` is.

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
                let Dir { } = d;
            }
            DirOpened::Failed(e) => {
            }
        }
    }
    release(fs);
    return 0;
}
