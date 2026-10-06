//~ ERROR performs `poll`, which its row
//~ RULE effect-not-declared

// `docs/processes.md` §4.8: registering a child with a poller performs `poll`,
// as registering a connection does, and the function has to say so.

edition 7;
fn watch[&p, &c](poller: &!p Poller, child: &c Child) -> [] int {
    return poller_add_child(poller, child, 1);
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals); release(exec);
    return 0;
}
