module std.x25519;
import std.field25519;

// `std.x25519` — the X25519 Diffie-Hellman function (RFC 7748 §5, §6.1).
// `docs/x25519.md` is the design; this is sub-issue 3 (#200) of the pure
// TLS 1.3 client (#197). Not independently reviewed (#209).
//
// The scalar is secret. The ladder below walks all 255 bits whatever
// they are, and each bit decides only a `cswap` (a masked XOR), never a
// branch or an index (`docs/x25519.md` §3).

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_length() -> [] int {
    return -1;
}

pub fn refused_zero_secret() -> [] int {
    return -2;
}

// The stable name of a refusal code.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "x25519-length";
    }
    if code == -2 {
        return "x25519-zero-secret";
    }
    return "unknown";
}

// The scalar's bit `i` after RFC 7748's clamping: bits 0 to 2 clear, bit
// 254 set, bit 255 clear. Clamped here, bit by bit, rather than in a
// copy, so the caller's scalar is never written. The index is the loop
// counter, which is public; the bit is secret and goes only to `cswap`.
fn scalar_bit[&k](k: &k [byte], i: int) -> [] int {
    if i < 3 || i == 255 {
        return 0;
    }
    if i == 254 {
        return 1;
    }
    return int_of(k[i >> 3]) >> (i & 7) & 1;
}

// `out[0..32] = X25519(k, u)`: the Montgomery ladder of RFC 7748 §5 on
// the 32-byte scalar `k` and u-coordinate `u`. An all-zero result (a
// low-order `u`) is refused, as RFC 7748 §6.1 and RFC 8446 §7.4.2 say a
// TLS client must, and `out` is then all zeros.
pub fn scalarmult[&k, &u, &o](k: &k [byte], u: &u [byte], out: &!o [byte]) -> [] int {
    if len(k) != 32 || len(u) != 32 || len(out) < 32 {
        return refused_length();
    }
    var diff = 0;
    region r {
        let t = alloc_slice[r](field25519.scratch_len(), 0);
        let x1 = alloc_slice[r](16, 0);
        let x2 = alloc_slice[r](16, 0);
        let z2 = alloc_slice[r](16, 0);
        let x3 = alloc_slice[r](16, 0);
        let z3 = alloc_slice[r](16, 0);
        let a = alloc_slice[r](16, 0);
        let b = alloc_slice[r](16, 0);
        let c = alloc_slice[r](16, 0);
        let d = alloc_slice[r](16, 0);
        let e = alloc_slice[r](16, 0);
        let da = alloc_slice[r](16, 0);
        let cb = alloc_slice[r](16, 0);
        let a24 = alloc_slice[r](16, 0);
        field25519.unpack(x1, u);
        field25519.set_small(x2, 1);
        field25519.set_small(z2, 0);
        field25519.copy(x3, x1);
        field25519.set_small(z3, 1);
        // a24 = (486662 - 2) / 4 = 121665, which is 0x1db41: two limbs.
        field25519.set_small(a24, 0xdb41);
        a24[1] = 1;

        var swap = 0;
        var i = 254;
        while i >= 0 {
            let bit = scalar_bit(k, i);
            swap = swap ^ bit;
            field25519.cswap(x2, x3, swap);
            field25519.cswap(z2, z3, swap);
            swap = bit;

            field25519.add(a, x2, z2);
            field25519.square(e, a, t);
            field25519.sub(b, x2, z2);
            field25519.square(d, b, t);
            field25519.sub(c, e, d);
            // e = AA, d = BB, c = E = AA - BB.
            field25519.add(x2, x3, z3);
            field25519.sub(z2, x3, z3);
            field25519.mul(da, z2, a, t);
            field25519.mul(cb, x2, b, t);
            field25519.add(x3, da, cb);
            field25519.square(x3, x3, t);
            field25519.sub(z3, da, cb);
            field25519.square(z3, z3, t);
            field25519.mul(z3, z3, x1, t);
            field25519.mul(x2, e, d, t);
            field25519.mul(z2, c, a24, t);
            field25519.add(z2, z2, e);
            field25519.mul(z2, c, z2, t);
            i = i - 1;
        }
        field25519.cswap(x2, x3, swap);
        field25519.cswap(z2, z3, swap);

        field25519.invert(z2, z2, t);
        field25519.mul(x2, x2, z2, t);
        field25519.pack(out, x2, t);

        var j = 0;
        while j < 32 {
            diff = diff | int_of(out[j]);
            j = j + 1;
        }
        // The ladder's secrets, best effort (`docs/chacha20.md` §3.2).
        j = 0;
        while j < 64 {
            t[j] = 0;
            j = j + 1;
        }
    }
    if diff == 0 {
        return refused_zero_secret();
    }
    return ok();
}

// The u-coordinate of the base point, 9 (RFC 7748 §4.1).
static base_u: [byte] {
    let u = alloc_slice[static](32, byte_of(0));
    u[0] = byte_of(9);
    return u;
}

// The public key for the private key `k`: X25519(k, 9).
pub fn public_key[&k, &o](k: &k [byte], out: &!o [byte]) -> [] int {
    return scalarmult(k, base_u, out);
}
