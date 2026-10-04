//~ ERROR `open_dir` is not a function in this program
//~ RULE not-a-function

// `docs/directory-handles.md`: `open_dir` and the `dir_*` builtins are
// edition 6. An older file may already declare its own `open_dir`, so to it
// the name is not a builtin at all -- the rule every edition-gated builtin
// follows (`editions.md` §7).

edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(ffi);
    release(heap);
    release(args);
    release(net);
    release(clock);
    var status = 0;
    borrow fs as &f in {
        status = open_dir(f, "/");
    }
    release(fs);
    return status;
}
