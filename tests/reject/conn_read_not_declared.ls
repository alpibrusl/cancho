//~ ERROR performs `conn_read`, which its row [] does not declare
//~ RULE effect-not-declared

// `docs/native-sockets.md` §3: reading a connection is a path-free label
// (`conn_read`) a function that borrows a `Conn` must declare, so the
// authority report says what a function can do with one.

edition 5;
fn pull[&c, &b](conn: &!c Conn, buf: &!b [byte]) -> [] int {
    match conn_read(conn, buf) {
        Received::Data(n) => {
            return n;
        }
        Received::End => {
            return 0;
        }
        Received::Again => {
            return 0;
        }
        Received::Failed(e) => {
            return 0 - 1;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    return 0;
}
