// A function value cannot be written with a module qualifier: `spawn(payload, mod.worker)` / `let f = mod.worker;` is refused
// (unknown-name: the module is "not bound here"), so a module that offers a thread body has to be wrapped by a function in the
// file that spawns it.
edition 5;
import std.vec;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let f = vec.size;
    return 0;
}
