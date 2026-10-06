edition 6;
module std.field25519;

// `std.field25519` — arithmetic modulo p = 2^255 - 19, the field under
// X25519 (RFC 7748) and Ed25519 (RFC 8032). `docs/x25519.md` is the
// design; this is part of sub-issue 3 (#200) of the pure TLS 1.3 client
// (#197). Not independently reviewed (#209).
//
// An element is a `[int]` of 16 limbs, limb `i` holding bits `16i` to
// `16i + 15`: TweetNaCl's representation (`docs/x25519.md` §2). Every
// function here has the same instruction sequence whatever the values:
// no `if` or loop bound depends on an element, and the one selection,
// `cswap`, is a masked XOR. That is what lets X25519's secret scalar
// pass through it (`docs/x25519.md` §3).
//
// Arithmetic is `wrapping_*`: none of it can overflow (the bounds are
// in `docs/x25519.md` §2), and a checked operation would be an overflow
// test computed from secret limbs.
//
// Functions that need room take a scratch slice `t` the caller owns, so
// no arena is opened per multiplication (`docs/hkdf.md` §2 measured what
// one costs). `mul`, `square`, `carry` and the rest write their output
// only after reading their inputs, so an output may be the same slice
// as an input.

// The words of scratch every function here needs at most.
pub fn scratch_len() -> [] int {
    return 64;
}

pub fn copy[&o, &a](o: &!o [int], a: &a [int]) -> [] int {
    var i = 0;
    while i < 16 {
        o[i] = a[i];
        i = i + 1;
    }
    return 0;
}

// `o = v`, for a small non-negative `v` (under 2^16).
pub fn set_small[&o](o: &!o [int], v: int) -> [] int {
    o[0] = v;
    var i = 1;
    while i < 16 {
        o[i] = 0;
        i = i + 1;
    }
    return 0;
}

// Brings every limb back to `[0, 2^16)`, folding the carry out of the
// top limb back into the bottom one as `38 = 2 * 19` (2^256 = 38 mod p).
// Arithmetic shifts, so a negative limb borrows from the next one.
fn carry[&o](o: &!o [int]) -> [] int {
    var i = 0;
    while i < 15 {
        let v = wrapping_add(o[i], 65536);
        let c = v >> 16;
        o[i + 1] = wrapping_add(o[i + 1], wrapping_sub(c, 1));
        o[i] = wrapping_sub(v, c << 16);
        i = i + 1;
    }
    let v = wrapping_add(o[15], 65536);
    let c = v >> 16;
    o[0] = wrapping_add(o[0], wrapping_mul(38, wrapping_sub(c, 1)));
    o[15] = wrapping_sub(v, c << 16);
    return 0;
}

pub fn add[&o, &a, &b](o: &!o [int], a: &a [int], b: &b [int]) -> [] int {
    var i = 0;
    while i < 16 {
        o[i] = wrapping_add(a[i], b[i]);
        i = i + 1;
    }
    return 0;
}

pub fn sub[&o, &a, &b](o: &!o [int], a: &a [int], b: &b [int]) -> [] int {
    var i = 0;
    while i < 16 {
        o[i] = wrapping_sub(a[i], b[i]);
        i = i + 1;
    }
    return 0;
}

// `o = a * b`. The 31 partial-product words go in `t[0..31]`; the top
// 15 fold down as `* 38`; two carries bring the limbs back in range.
pub fn mul[&o, &a, &b, &t](o: &!o [int], a: &a [int], b: &b [int], t: &!t [int]) -> [] int {
    var i = 0;
    while i < 31 {
        t[i] = 0;
        i = i + 1;
    }
    i = 0;
    while i < 16 {
        var j = 0;
        let ai = a[i];
        while j < 16 {
            t[i + j] = wrapping_add(t[i + j], wrapping_mul(ai, b[j]));
            j = j + 1;
        }
        i = i + 1;
    }
    i = 0;
    while i < 15 {
        t[i] = wrapping_add(t[i], wrapping_mul(38, t[i + 16]));
        i = i + 1;
    }
    i = 0;
    while i < 16 {
        o[i] = t[i];
        i = i + 1;
    }
    carry(o);
    carry(o);
    return 0;
}

pub fn square[&o, &a, &t](o: &!o [int], a: &a [int], t: &!t [int]) -> [] int {
    return mul(o, a, a, t);
}

// Swaps `p` and `q` when `bit` is 1 and leaves them when it is 0, with
// the same work either way: `mask` is all ones or all zeros, never a
// branch (RFC 7748 §5's `cswap`). The mask passes through
// `value_barrier` where it is made (`docs/value-barrier.md` §4): `bit` is
// a bit of X25519's secret scalar, and without it LLVM may prove the
// mask 0 or -1, fold the XOR into a select and hoist that as a branch
// (#316).
pub fn cswap[&p, &q](p: &!p [int], q: &!q [int], bit: int) -> [] int {
    let mask = value_barrier(wrapping_sub(0, bit));
    var i = 0;
    while i < 16 {
        let x = mask & (p[i] ^ q[i]);
        p[i] = p[i] ^ x;
        q[i] = q[i] ^ x;
        i = i + 1;
    }
    return 0;
}

// `o = a^(p-2) = 1/a` (and 0 for 0). The exponent is a constant, so the
// sequence of squarings and multiplications is the same for every `a`.
// `t` needs `scratch_len()` words: the running power in `t[32..48]`.
pub fn invert[&o, &a, &t](o: &!o [int], a: &a [int], t: &!t [int]) -> [] int {
    var i = 0;
    while i < 16 {
        t[32 + i] = a[i];
        i = i + 1;
    }
    var bit = 253;
    while bit >= 0 {
        square_at(t, 32);
        if bit != 2 && bit != 4 {
            mul_at(t, 32, a);
        }
        bit = bit - 1;
    }
    i = 0;
    while i < 16 {
        o[i] = t[32 + i];
        i = i + 1;
    }
    return 0;
}

// `o = a^((p-5)/8) = a^(2^252 - 3)`, the exponent of Ed25519's square
// root (RFC 8032 §5.1.3).
pub fn pow2523[&o, &a, &t](o: &!o [int], a: &a [int], t: &!t [int]) -> [] int {
    var i = 0;
    while i < 16 {
        t[32 + i] = a[i];
        i = i + 1;
    }
    var bit = 250;
    while bit >= 0 {
        square_at(t, 32);
        if bit != 1 {
            mul_at(t, 32, a);
        }
        bit = bit - 1;
    }
    i = 0;
    while i < 16 {
        o[i] = t[32 + i];
        i = i + 1;
    }
    return 0;
}

// The element in `t[at..at + 16]`, squared in place; `t[0..31]` is the
// product's room and `t[48..64]` a copy of the input. The exponent loops
// above use these so that one scratch slice holds everything.
fn square_at[&t](t: &!t [int], at: int) -> [] int {
    var i = 0;
    while i < 16 {
        t[48 + i] = t[at + i];
        i = i + 1;
    }
    product_into(t, at, 48, 48);
    return 0;
}

fn mul_at[&t, &a](t: &!t [int], at: int, a: &a [int]) -> [] int {
    var i = 0;
    while i < 16 {
        t[48 + i] = a[i];
        i = i + 1;
    }
    product_into(t, at, at, 48);
    return 0;
}

// `t[out..out+16] = t[x..x+16] * t[y..y+16]`, the partial products in
// `t[0..31]`. The same arithmetic as `mul`, over one slice: the inputs
// are read into the products before `out` is written.
fn product_into[&t](t: &!t [int], out: int, x: int, y: int) -> [] int {
    var i = 0;
    while i < 31 {
        t[i] = 0;
        i = i + 1;
    }
    i = 0;
    while i < 16 {
        let xi = t[x + i];
        var j = 0;
        while j < 16 {
            t[i + j] = wrapping_add(t[i + j], wrapping_mul(xi, t[y + j]));
            j = j + 1;
        }
        i = i + 1;
    }
    i = 0;
    while i < 15 {
        t[i] = wrapping_add(t[i], wrapping_mul(38, t[i + 16]));
        i = i + 1;
    }
    i = 0;
    while i < 16 {
        t[out + i] = t[i];
        i = i + 1;
    }
    carry_at(t, out);
    carry_at(t, out);
    return 0;
}

fn carry_at[&t](t: &!t [int], at: int) -> [] int {
    var i = 0;
    while i < 15 {
        let v = wrapping_add(t[at + i], 65536);
        let c = v >> 16;
        t[at + i + 1] = wrapping_add(t[at + i + 1], wrapping_sub(c, 1));
        t[at + i] = wrapping_sub(v, c << 16);
        i = i + 1;
    }
    let v = wrapping_add(t[at + 15], 65536);
    let c = v >> 16;
    t[at] = wrapping_add(t[at], wrapping_mul(38, wrapping_sub(c, 1)));
    t[at + 15] = wrapping_sub(v, c << 16);
    return 0;
}

// 32 little-endian bytes into an element. The top bit of byte 31 is
// ignored, as RFC 7748 §5 requires of a u-coordinate; a value of p or
// more is taken mod p by the arithmetic that follows, which is also what
// RFC 7748 asks for (non-canonical inputs are accepted).
pub fn unpack[&o, &s](o: &!o [int], s: &s [byte]) -> [] int {
    var i = 0;
    while i < 16 {
        o[i] = int_of(s[2 * i]) | int_of(s[2 * i + 1]) << 8;
        i = i + 1;
    }
    o[15] = o[15] & 0x7fff;
    return 0;
}

// The canonical 32-byte encoding, fully reduced below p, into
// `out[0..32]`. Two trial subtractions of p, each kept or dropped by
// `cswap`, never by a branch. `t` needs `scratch_len()` words.
pub fn pack[&o, &a, &t](out: &!o [byte], a: &a [int], t: &!t [int]) -> [] int {
    var i = 0;
    while i < 16 {
        t[i] = a[i];
        i = i + 1;
    }
    carry_at(t, 0);
    carry_at(t, 0);
    carry_at(t, 0);
    var pass = 0;
    while pass < 2 {
        // `t[16..32] = t[0..16] - p`, limb by limb with the borrow.
        t[16] = wrapping_sub(t[0], 0xffed);
        i = 1;
        while i < 15 {
            t[16 + i] = wrapping_sub(wrapping_sub(t[i], 0xffff), t[16 + i - 1] >> 16 & 1);
            t[16 + i - 1] = t[16 + i - 1] & 0xffff;
            i = i + 1;
        }
        t[31] = wrapping_sub(wrapping_sub(t[15], 0x7fff), t[30] >> 16 & 1);
        let under = t[31] >> 16 & 1;
        t[30] = t[30] & 0xffff;
        // No borrow: the difference is the reduced value; take it.
        cswap_at(t, 0, 16, 1 - under);
        pass = pass + 1;
    }
    i = 0;
    while i < 16 {
        out[2 * i] = byte_of(t[i] & 0xff);
        out[2 * i + 1] = byte_of(t[i] >> 8 & 0xff);
        i = i + 1;
    }
    return 0;
}

// `cswap` within one array: `pack`'s choice of the reduced value, which
// depends on the secret it packs, so its mask is barriered too (#316).
fn cswap_at[&t](t: &!t [int], x: int, y: int, bit: int) -> [] int {
    let mask = value_barrier(wrapping_sub(0, bit));
    var i = 0;
    while i < 16 {
        let d = mask & (t[x + i] ^ t[y + i]);
        t[x + i] = t[x + i] ^ d;
        t[y + i] = t[y + i] ^ d;
        i = i + 1;
    }
    return 0;
}

// 1 if `a` and `b` are the same element (compared canonically), 0
// otherwise. Every byte is compared whatever the earlier ones were.
pub fn equal[&a, &b, &t](a: &a [int], b: &b [int], t: &!t [int]) -> [] int {
    var diff = 0;
    region r {
        let pa = alloc_slice[r](32, byte_of(0));
        let pb = alloc_slice[r](32, byte_of(0));
        pack(pa, a, t);
        pack(pb, b, t);
        var i = 0;
        while i < 32 {
            diff = diff | int_of(pa[i]) ^ int_of(pb[i]);
            i = i + 1;
        }
    }
    // `diff` is in [0, 255]: `diff - 1` is negative, and its bit 8 set,
    // exactly when `diff` is 0.
    return wrapping_sub(diff, 1) >> 8 & 1;
}

// The low bit of `a`'s canonical encoding: its "sign" in RFC 8032.
pub fn parity[&a, &t](a: &a [int], t: &!t [int]) -> [] int {
    var bit = 0;
    region r {
        let pa = alloc_slice[r](32, byte_of(0));
        pack(pa, a, t);
        bit = int_of(pa[0]) & 1;
    }
    return bit;
}
