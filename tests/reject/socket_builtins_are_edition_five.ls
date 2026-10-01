//~ ERROR `tcp_listen` is not a function in this program
//~ RULE not-a-function

// `docs/native-sockets.md` §3: the socket builtins are edition 5. An older
// file may already declare its own `tcp_listen`, so to it the name is not a
// builtin at all -- the rule every edition-gated builtin follows
// (`editions.md` §7).

edition 4;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    let bound = narrow(net, "8080");
    borrow bound as &n in {
        match tcp_listen(n, 8080, 8, 0) {
            Listening::Ok(l) => {
                listener_close(l);
            }
            Listening::Failed(e) => {
            }
        }
    }
    release(bound);
    return 0;
}
