// A thread handle is a resource and std's containers hold only copyable values, so a pool of workers cannot live in a Vec: every
// worker is a named local that main (or one function) spawns and joins, and the pool's size is fixed in the program text.
edition 5;
import std.vec;

fn work(n: int) -> [] int {
    return n;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    var h = heap;
    var code = 0;
    borrow mut h as &!hh in {
        let w = work;
        let t = spawn(1, w);
        var v = vec.empty(hh, 4, t);
        code = 1;
        vec.drop(hh, v);
    }
    release(h);
    return code;
}
