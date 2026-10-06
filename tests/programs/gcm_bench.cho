// `docs/tls-parity.md` §3.1: how fast `std.gcm` seals.
//
//     gcm_bench <rounds> <size> <key bytes, 16 or 32>
//
// Seals one `<size>`-byte message `<rounds>` times, each round under the
// nonce the round number makes and over the previous round's ciphertext,
// so no round can be skipped, and prints the last tag's first byte.
// Timing is done from outside, as the difference between one round and
// many.
import std.gcm;
import std.io;

fn number_of[&s](text: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(text) {
        n = n * 10 + (int_of(text[i]) - '0');
        i = i + 1;
    }
    return n;
}

fn run[&i](io: &!i Io, rounds: int, size: int, key_len: int) -> [io_write] int {
    var last = 0;
    region r {
        let key = alloc_slice[r](key_len, byte_of(7));
        let nonce = alloc_slice[r](12, byte_of(0));
        let text = alloc_slice[r](size, byte_of(1));
        let out = alloc_slice[r](size + 16, byte_of(0));
        var n = 0;
        while n < rounds {
            nonce[0] = byte_of(n & 0xff);
            nonce[1] = byte_of(n >> 8 & 0xff);
            nonce[2] = byte_of(n >> 16 & 0xff);
            gcm.seal(key, nonce, out[0..0], text, out);
            var j = 0;
            while j < size {
                text[j] = out[j];
                j = j + 1;
            }
            n = n + 1;
        }
        last = int_of(out[size]);
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
    var rounds = 1;
    var size = 16384;
    var key_len = 16;
    borrow args as &g in {
        if arg_count(g) > 1 {
            rounds = number_of(arg(g, 1));
        }
        if arg_count(g) > 2 {
            size = number_of(arg(g, 2));
        }
        if arg_count(g) > 3 {
            key_len = number_of(arg(g, 3));
        }
    }
    release(args);
    borrow mut io as &!i in {
        run(i, rounds, size, key_len);
    }
    release(io);
    return 0;
}
