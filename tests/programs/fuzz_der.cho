edition 5;

// `docs/tls-assurance.md` §3.3: one certificate's DER, from standard
// input, to `x509.parse`. Any answer is fine; a trap is the failure.
import fuzz_common;
import std.io;
import x509;

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] int {
    var input = box_slice(heap, 65536, byte_of(0));
    var view = box_slice(heap, x509.view_len(), 0);
    borrow mut input as &!d in {
        borrow mut view as &!v in {
            let n = fuzz_common.read_all(io, contents(d));
            // Exactly as long as the input, so a read past its end traps.
            x509.parse(contents(d)[0..n], contents(v));
        }
    }
    unbox_slice(heap, input);
    unbox_slice(heap, view);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(args);
    release(ffi);
    release(fs);
    release(net);
    release(clock);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
