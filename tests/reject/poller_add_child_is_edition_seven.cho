//~ ERROR `poller_add_child` is not a function in this program
//~ RULE not-a-function

// `docs/processes.md` §4.8: watching a child is edition 7, so an edition-6
// file cannot call it -- `poller_add_child` is a name such a file may declare.

edition 6;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    return poller_add_child(0, 0, 0);
}
