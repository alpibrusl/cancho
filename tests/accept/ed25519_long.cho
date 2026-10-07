// `docs/hkdf.md` §2: `ed25519.sign` copied `prefix || msg` into one 64 KiB
// arena and trapped on a message of 65,000 bytes. It streams the hash now.
// 70,000 bytes of `a` signed with a seed of 32 bytes of 7: the signature is
// the system OpenSSL's (through pyca/cryptography), it verifies, and a
// tampered one does not.
//~ STDOUT b55b27ffa0eeaebd239a832e3cd798699eb0a4f8ecb7b38517135afca0d6df2f85206239e05ed6cd5b44eeb3e65d43126e107ff5b4015a2ec3ac5eea7e797208
//~ STDOUT 1
//~ STDOUT 0
//~ EXIT 0

import std.io;
import std.ed25519;

fn print_hex[&i, &d](io: &!i Io, digest: &d [byte]) -> [io_write] int {
    let alphabet = "0123456789abcdef";
    var n = 0;
    while n < len(digest) {
        let b = int_of(digest[n]);
        putchar(io, int_of(alphabet[b >> 4 & 0xf]));
        putchar(io, int_of(alphabet[b & 0xf]));
        n = n + 1;
    }
    io.newline(io);
    return 0;
}

fn check[&i, &m](io: &!i Io, msg: &m [byte]) -> [io_write] int {
    region r {
        let seed = alloc_slice[r](32, byte_of(7));
        let sig = alloc_slice[r](64, byte_of(0));
        ed25519.sign(seed, msg, sig);
        print_hex(io, sig);
        let pk = alloc_slice[r](32, byte_of(0));
        ed25519.public_key_from_seed(seed, pk);
        putchar(io, 48 + ed25519.verify(pk, msg, sig));
        io.newline(io);
        sig[63] = byte_of(int_of(sig[63]) ^ 1);
        putchar(io, 48 + ed25519.verify(pk, msg, sig));
        io.newline(io);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    release(ffi);
    borrow mut heap as &!h in {
        let msg = box_slice(h, 70000, byte_of(97));
        borrow msg as &m in {
            borrow mut io as &!i in {
                check(i, contents(m));
            }
        }
        unbox_slice(h, msg);
    }
    release(heap);
    release(io);
    return 0;
}
