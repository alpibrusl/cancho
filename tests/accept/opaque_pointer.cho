// `docs/opaque-pointers.md` §3: `c_ptr`, an opaque handle that crosses a
// foreign boundary in both directions without ever being dereferenced.
// `fdopen`/`fclose` are the smallest real C pair with this shape --
// `FILE *` (or `NULL`) out, `int` back in -- that every libc this
// project targets links with nothing extra, unlike OpenSSL's
// `SSL_CTX *`/`SSL *`, which is the real motivating case
// (`docs/opaque-pointers.md` §1) but needs a live server to open a
// handshake against.
//
// `fd`, not a path: `fdopen`'s one string parameter is `mode`, and it is
// the *last* parameter, which matters here for a reason that has
// nothing to do with `c_ptr` and everything to do with how a `&r [byte]`
// parameter already crosses (`docs/strings.md` §6) -- as a pointer
// *and* a separate length, both real arguments. A byte-slice parameter
// followed by anything else silently shifts every later parameter into
// the wrong register, because the real C callee was never declared to
// receive that length at all. `fd: int` first and `mode: &m [byte]`
// last (matching `fdopen(int, const char *)` exactly) is the same shape
// `write`/`read`'s own existing declarations already use; two
// string-shaped parameters in one declaration -- `fopen(path, mode)`,
// for instance -- has no correct spelling in this language today, a
// pre-existing gap this fixture works around by construction rather
// than by accident.
//~ STDOUT stdin opened
//~ STDOUT closed 0
//~ STDOUT bad fd is null
//~ EXIT 0

edition 3;

import std.io;

extern fn fdopen[&f, &m](ffi: &f Ffi("libc"), fd: int, mode: &m [byte]) -> [ffi("libc")] c_ptr;

extern fn fclose[&f](ffi: &f Ffi("libc"), stream: c_ptr) -> [ffi("libc")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(fs);
    release(heap);
    release(args);
    release(net);

    let libc = narrow(ffi, "libc");
    borrow mut io as &!i in {
        borrow libc as &f in {
            region scratch {
                let mode = alloc_slice[scratch](2, byte_of(0));
                mode[0] = byte_of('r');

                // fd 0 (stdin) is always open in a process this
                // harness starts.
                let handle = fdopen(f, 0, mode);
                if handle == null_ptr() {
                    io.write_all(i, "unexpected: stdin did not open\n");
                } else {
                    io.write_all(i, "stdin opened\n");
                    io.write_all(i, "closed ");
                    io.print_int(i, fclose(f, handle));
                    io.newline(i);
                }

                // A descriptor nothing has open: `fdopen` hands back the
                // null handle, comparable against `null_ptr()` and
                // nothing else -- no arithmetic, no dereference,
                // exactly `docs/opaque-pointers.md` §3's whole point.
                let bad = fdopen(f, 999, mode);
                if bad == null_ptr() {
                    io.write_all(i, "bad fd is null\n");
                } else {
                    io.write_all(i, "unexpected: bad fd opened\n");
                    fclose(f, bad);
                }
            }
        }
    }
    release(libc);
    release(io);
    return 0;
}
