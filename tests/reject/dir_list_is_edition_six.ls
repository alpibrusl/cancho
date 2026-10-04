//~ ERROR `dir_list` is not a function in this program
//~ RULE not-a-function

// `docs/directory-listing.md`: `dir_list`, `dir_next` and `dir_list_close`
// are edition 6, like the `Dir` they read. An older file may already declare
// its own `dir_list`, so to it the name is not a builtin at all
// (`editions.md` §7).

edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    return dir_list(0);
}
