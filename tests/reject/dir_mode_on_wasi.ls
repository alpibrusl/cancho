//~ ERROR permission bits do not exist on `wasm32-wasip1`
//~ RULE unsupported-on-target
//~ TARGET wasm32-wasip1

// `dir_own_mode` is fine for the host and refused for WebAssembly. WASI's stat has
// no permission bits: wasi-libc's `st_mode` carries the file type and nothing else,
// so on a WASI build this *compiled, ran and answered 0 for every file*, as if
// nothing were readable. A wrong answer is worse than a refusal, and it was found
// only by running the builtin that no accept fixture reaches (`docs/wasm.md`, W0.6).
//
// The same file passes `lex-sys check` with no `--target`.

edition 7;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(io);
    release(ffi);
    release(heap);
    release(args);
    release(net);
    release(clock);
    release(signals);
    release(exec);
    var code = 0;
    borrow fs as &f in {
        match open_dir(f, "/tmp") {
            DirOpened::Ok(opened) => {
                var dir = opened;
                borrow dir as &r in {
                    match dir_own_mode(r) {
                        Done::Ok(bits) => { code = bits; }
                        Done::Failed(e) => { code = e; }
                    }
                }
                dir_close(dir);
            }
            DirOpened::Failed(e) => { code = e; }
        }
    }
    release(fs);
    return code;
}
