//~ ERROR is still live at the end of this block
//~ RULE linear-value-unconsumed

// `docs/native-sockets.md` §3: a `Listener` is a `res` value, so leaking
// one is a compile error with no new machinery -- as `File`'s is.

edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(clock);
    let bound = narrow(net, "8080");
    borrow bound as &n in {
        match tcp_listen(n, 8080, 8, 0) {
            Listening::Ok(l) => {
            }
            Listening::Failed(e) => {
            }
        }
    }
    release(bound);
    return 0;
}
