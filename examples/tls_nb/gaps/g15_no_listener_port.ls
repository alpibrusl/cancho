// `tcp_listen(net, 0, ...)` binds an ephemeral port, and nothing reports which: there is no `listener_port`. A program that wants a
// loopback channel to itself (a doorbell between a worker thread and the poller) must therefore agree a fixed port in advance.
edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(clock);
    var port = 0;
    borrow net as &n in {
        match tcp_listen(n, 0, 4, 0) {
            Listening::Ok(l) => {
                port = listener_port(l);
                listener_close(l);
            }
            Listening::Failed(e) => {
            }
        }
    }
    release(net);
    release(heap);
    return port;
}
