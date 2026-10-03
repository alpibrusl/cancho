edition 5;

import std.io;
import sha1;
import b64;

// `probe sha1 <hex>` and `probe b64 <hex>`: the hash, or the base64, of the bytes the hex names, on standard output. The
// differential test (`scripts/ws_codec_check.py`) drives it with random inputs and compares with `hashlib` and `base64`.

fn hexval(c: int) -> [] int {
    if c >= '0' && c <= '9' {
        return c - '0';
    }
    if c >= 'a' && c <= 'f' {
        return c - 'a' + 10;
    }
    return 0 - 1;
}

fn hexdigit(n: int) -> [] int {
    if n < 10 {
        return '0' + n;
    }
    return 'a' + n - 10;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(net);
    release(clock);
    var status = 2;
    region a {
        let data = alloc_slice[a](2048, byte_of(0));
        var n = 0;
        var op = 0;
        borrow args as &g in {
            if arg_count(g) == 3 {
                let h = arg(g, 2);
                if len(h) % 2 == 0 && len(h) / 2 <= 2048 {
                    n = len(h) / 2;
                    var i = 0;
                    while i < n {
                        data[i] = byte_of(hexval(int_of(h[2 * i])) * 16 + hexval(int_of(h[2 * i + 1])));
                        i = i + 1;
                    }
                    let name = arg(g, 1);
                    if len(name) == 4 && int_of(name[0]) == 's' {
                        op = 1;
                    } else if len(name) == 3 && int_of(name[0]) == 'b' {
                        op = 2;
                    }
                }
            }
        }
        if op == 1 {
            let out = alloc_slice[a](20, byte_of(0));
            sha1.digest(data[0..n], out);
            let text = alloc_slice[a](41, byte_of(0));
            var i = 0;
            while i < 20 {
                text[2 * i] = byte_of(hexdigit(int_of(out[i]) / 16));
                text[2 * i + 1] = byte_of(hexdigit(int_of(out[i]) % 16));
                i = i + 1;
            }
            text[40] = byte_of('\n');
            borrow mut io as &!w in {
                io.write_all(w, text);
            }
            status = 0;
        } else if op == 2 {
            let out = alloc_slice[a](3000, byte_of(0));
            let m = b64.encode(data[0..n], out);
            out[m] = byte_of('\n');
            borrow mut io as &!w in {
                io.write_all(w, out[0..m + 1]);
            }
            status = 0;
        }
    }
    release(args);
    release(io);
    return status;
}
