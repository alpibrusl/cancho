//~ ERROR cannot be narrowed to `/opt`
//~ RULE capability-not-narrowable

// `docs/processes.md` §4.1: `Exec`'s prefix narrows one way only, as `Fs`'s does;
// `/opt/tools` cannot become `/opt`.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    let tools = narrow(exec, "/opt/tools");
    let wider = narrow(tools, "/opt");
    release(wider);
    return 0;
}
