// `docs/x25519.md` §6: how fast `std.ed25519` signs and verifies, and
// `std.x25519` computes, before and after the field module.
//
//     ed25519_bench <op> <rounds>      op: 0 sign, 1 verify, 2 x25519
//
// Each round feeds the previous result into the next, so no round can
// be skipped; prints one byte of the last result. Timing is done from
// outside, as the difference between one round and many.
import std.ed25519;
import std.io;
import std.x25519;

fn number_of[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn run[&i](io: &!i Io, op: int, rounds: int) -> [io_write] int {
    var last = 0;
    region r {
        let seed = alloc_slice[r](32, byte_of(7));
        let msg = alloc_slice[r](64, byte_of(1));
        let sig = alloc_slice[r](64, byte_of(0));
        let pk = alloc_slice[r](32, byte_of(0));
        ed25519.public_key_from_seed(seed, pk);
        ed25519.sign(seed, msg, sig);
        let u = alloc_slice[r](32, byte_of(9));
        let out = alloc_slice[r](32, byte_of(0));
        var n = 0;
        while n < rounds {
            if op == 0 {
                ed25519.sign(seed, msg, sig);
                msg[0] = sig[0];
                last = int_of(sig[0]);
            } else if op == 1 {
                last = last + ed25519.verify(pk, msg, sig);
            } else {
                x25519.scalarmult(seed, u, out);
                u[0] = out[0];
                last = int_of(out[0]);
            }
            n = n + 1;
        }
    }
    io.print_int(io, last);
    io.newline(io);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(fs);
    release(ffi);
    release(heap);
    var op = 0;
    var rounds = 1;
    borrow args as &g in {
        op = number_of(arg(g, 1));
        rounds = number_of(arg(g, 2));
    }
    release(args);
    borrow mut io as &!i in {
        run(i, op, rounds);
    }
    release(io);
    return 0;
}
