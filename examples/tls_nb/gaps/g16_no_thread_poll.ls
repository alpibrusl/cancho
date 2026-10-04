// `join` waits. There is no way to ask whether a thread has finished, and a `Thread` is not something a `Poller` can watch, so a
// poller loop cannot learn that a worker is done without a channel the worker writes to (a loopback `Conn`, see g15).
edition 5;
fn work(n: int) -> [] int {
    return n;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let w = work;
    let t = spawn(1, w);
    if thread_done(t) {
        return join(t);
    }
    return join(t);
}
