//~ ERROR `theirs` is still live at the end of this block
//~ RULE linear-value-unconsumed

// `docs/processes.md` §4.4: the child's end of a channel is handed to a child
// (`exec_spawn` consumes it) or closed; a parent that kept it is the classic
// pipe that never sees end-of-file.

edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    release(exec);
    match pipe_open() {
        Piped::Ok(mine, theirs) => {
            pipe_close(mine);
        }
        Piped::Failed(e) => { }
    }
    return 0;
}
