//~ ERROR unknown type `Exec`
//~ RULE unknown-name

// `docs/processes.md` §3: `Exec` is edition 7, so an edition-6 file cannot
// name it (`docs/editions.md` §5: an addition is absent from older editions).

edition 6;
fn start[&x](exec: &x Exec("/bin")) -> [] int {
    return 0;
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    return 0;
}
