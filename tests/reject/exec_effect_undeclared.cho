//~ ERROR performs `exec("/bin")`, which its row
//~ RULE effect-not-declared

// `docs/processes.md` §3.2: starting a program performs `exec(p)` with the
// prefix the capability was narrowed to, and a function that borrows an `Exec`
// has to say so.

edition 7;
fn start[&x](exec: &x Exec("/bin")) -> [] int {
    match exec_spawn(exec, "/bin/true", "", "", Stdio::Null, Stdio::Null, Stdio::Null) {
        Spawned::Ok(child) => {
            match child_wait(child) {
                Exited::Code(n) => { return n; }
                Exited::Signaled(s) => { return 1; }
                Exited::Failed(e) => { return 1; }
            }
        }
        Spawned::Failed(e) => { return e; }
    }
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(net); release(clock); release(signals);
    let bin = narrow(exec, "/bin");
    var n = 0;
    borrow bin as &x in { n = start(x); }
    release(bin);
    return n;
}
