//~ STDOUT 30
//~ EXIT 0

// `docs/threads.md` §5 step 3's other named case: `File`. Unlike
// `Io`, `File` is one real leaf -- the fd, `abi::leaves_into`'s own
// `PRELUDE_FILE` arm on both backends (`types::I64` / `LKind::I64`)
// -- and it crosses exactly the way a plain `int` payload already
// does, since Cranelift's `pthread_create` call and LLVM's `inttoptr`
// conversion both work off the value's actual machine width, not its
// surface `Type`. `main` opens the file (needs `Fs`), then moves the
// resulting handle into the spawned thread, which reads it and closes
// it -- real I/O performed by a real second OS thread, not the one
// that opened the file.

edition 4;

import std.io;

fn worker(file: File) -> [] int {
    var result = 0;
    region a {
        var buffer = alloc_slice[a](64, byte_of(0));
        var f = file;
        borrow mut f as &!h in {
            match file_read(h, buffer) {
                Read::Got(n) => {
                    result = n;
                }
                Read::End => {
                    result = -1;
                }
                Read::Failed(e) => {
                    result = 0 - 100 - e;
                }
            }
        }
        file_close(f);
    }
    return result;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(ffi);
    release(heap);
    release(args);
    release(net);

    let path = "/tmp/lex-sys-spawn-handle.txt";
    var status = 0;
    borrow fs as &c in {
        fs_write(c, path, "hello from a spawned file read");
        match open_read(c, path) {
            Opened::Ok(f) => {
                let w = worker;
                let h = spawn(f, w);
                let n = join(h);
                borrow mut io as &!i in {
                    io.print_int(i, n);
                    io.newline(i);
                }
            }
            Opened::Failed(e) => {
                status = e;
            }
        }
    }
    release(io);
    release(fs);
    return status;
}
