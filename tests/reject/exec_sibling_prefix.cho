//~ ERROR a path prefix extends at a `/`
//~ RULE capability-not-narrowable

// `docs/processes.md` §4.1: `Exec`'s prefix is a path, with `Fs`'s rule --
// `/opt/toolsevil` is the directory next door, not inside `/opt/tools`.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    let tools = narrow(exec, "/opt/tools");
    let next_door = narrow(tools, "/opt/toolsevil");
    release(next_door);
    return 0;
}
