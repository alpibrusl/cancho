// Can a program reach the descriptor of a connection in a std.conns Table without conn_raw_fd?
// (conn_raw_fd is designed in docs/native-sockets.md section 6 and not built.)
edition 5;
import std.conns;
import std.vec;

fn fd_of[&t](table: &t conns.Table, slot: int) -> [] int {
    return vec.get(table.tickets, slot) & 4294967295;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    return 5;
}
