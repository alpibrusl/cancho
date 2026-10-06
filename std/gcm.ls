module std.gcm;
import std.aes;
import std.bignum;

// `std.gcm` -- AES-GCM (NIST SP 800-38D) with a 96-bit nonce, the only
// form TLS uses (`docs/tls-parity.md` §3.1). Sub-issue 10 (#207) of the
// self-contained TLS client (#197). Not independently reviewed (#209).
//
// GHASH is a port of BearSSL's `ghash_ctmul32` (Thomas Pornin, MIT
// licence, as `std/aes.ls` quotes it): a carry-less multiply from
// integer multiplies of operands with every fourth bit kept, so the
// carries land in the holes and are masked away. No table is indexed by
// `H` or the data, and nothing branches on them. Each product is of two
// values under 0x88888889, so under 2^63: the checked multiply never
// traps. Every word is in [0, 2^32), as in `std.aes`.
//
// The API is `std.chacha20`'s: `seal` and `open`, the tag after the
// ciphertext, and nothing written on a tag mismatch.

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_key_length() -> [] int {
    return -1;
}

pub fn refused_nonce_length() -> [] int {
    return -2;
}

pub fn refused_output_length() -> [] int {
    return -3;
}

pub fn refused_too_long() -> [] int {
    return -4;
}

pub fn refused_too_short() -> [] int {
    return -5;
}

pub fn refused_tag_mismatch() -> [] int {
    return -6;
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "gcm-key-length";
    }
    if code == -2 {
        return "gcm-nonce-length";
    }
    if code == -3 {
        return "gcm-output-length";
    }
    if code == -4 {
        return "gcm-too-long";
    }
    if code == -5 {
        return "gcm-too-short";
    }
    if code == -6 {
        return "aead-tag-mismatch";
    }
    return "unknown";
}

fn m32(x: int) -> [] int {
    return x & 0xffffffff;
}

fn be32[&s](s: &s [byte], at: int) -> [] int {
    return int_of(s[at]) << 24 | int_of(s[at + 1]) << 16 | int_of(s[at + 2]) << 8 | int_of(s[at + 3]);
}

fn put_be32[&o](o: &!o [byte], at: int, x: int) -> [] int {
    o[at] = byte_of(x >> 24 & 255);
    o[at + 1] = byte_of(x >> 16 & 255);
    o[at + 2] = byte_of(x >> 8 & 255);
    o[at + 3] = byte_of(x & 255);
    return 0;
}

// The low 32 bits of the carry-less product of `x` and `y`.
fn bmul32(x: int, y: int) -> [] int {
    let x0 = x & 0x11111111;
    let x1 = x & 0x22222222;
    let x2 = x & 0x44444444;
    let x3 = x & 0x88888888;
    let y0 = y & 0x11111111;
    let y1 = y & 0x22222222;
    let y2 = y & 0x44444444;
    let y3 = y & 0x88888888;
    let z0 = (x0 * y0 ^ x1 * y3 ^ x2 * y2 ^ x3 * y1) & 0x11111111;
    let z1 = (x0 * y1 ^ x1 * y0 ^ x2 * y3 ^ x3 * y2) & 0x22222222;
    let z2 = (x0 * y2 ^ x1 * y1 ^ x2 * y0 ^ x3 * y3) & 0x44444444;
    let z3 = (x0 * y3 ^ x1 * y2 ^ x2 * y1 ^ x3 * y0) & 0x88888888;
    return z0 | z1 | z2 | z3;
}

// `x`'s 32 bits in reverse order.
fn rev32(x0: int) -> [] int {
    var x = x0;
    x = m32((x & 0x55555555) << 1) | x >> 1 & 0x55555555;
    x = m32((x & 0x33333333) << 2) | x >> 2 & 0x33333333;
    x = m32((x & 0x0f0f0f0f) << 4) | x >> 4 & 0x0f0f0f0f;
    x = m32((x & 0x00ff00ff) << 8) | x >> 8 & 0x00ff00ff;
    return m32(x << 16) | x >> 16;
}

// Where GHASH keeps its words in the one array `w` (`WORDS` of them):
// the Karatsuba operands `a` and `b` and products `c` (18 each), the
// unreduced product `zw` (8), H as 4 words and bit-reversed (4 each),
// and the state `y` (4, word 3 the most significant).
fn A() -> [] int {
    return 0;
}

fn B() -> [] int {
    return 18;
}

fn C() -> [] int {
    return 36;
}

fn ZW() -> [] int {
    return 54;
}

fn HW() -> [] int {
    return 62;
}

fn HWR() -> [] int {
    return 66;
}

fn Y() -> [] int {
    return 70;
}

fn WORDS() -> [] int {
    return 74;
}

// Big-endian word at `at` of `data`, zero past its end: GHASH's padding
// of the last block. Branches on the length only.
fn be32_padded[&s](s: &s [byte], at: int) -> [] int {
    var x = 0;
    var k = 0;
    while k < 4 {
        var b = 0;
        if at + k < len(s) {
            b = int_of(s[at + k]);
        }
        x = x << 8 | b;
        k = k + 1;
    }
    return x;
}

// GHASH: the state `y` absorbs `data`, zero-padded to 16 bytes, under H.
fn ghash[&w, &d](w: &!w [int], data: &d [byte]) -> [] int {
    let n = len(data);
    let a = A();
    let b = B();
    let c = C();
    let zw = ZW();
    let y = Y();
    var at = 0;
    while at < n {
        w[y + 3] = w[y + 3] ^ be32_padded(data, at);
        w[y + 2] = w[y + 2] ^ be32_padded(data, at + 4);
        w[y + 1] = w[y + 1] ^ be32_padded(data, at + 8);
        w[y] = w[y] ^ be32_padded(data, at + 12);
        // Karatsuba: three 64x64 products, each three 32x32 ones,
        // done on the words and on their bit-reversals.
        var i = 0;
        while i < 4 {
            w[a + i] = w[y + i];
            w[a + 9 + i] = rev32(w[y + i]);
            w[b + i] = w[HW() + i];
            w[b + 9 + i] = w[HWR() + i];
            i = i + 1;
        }
        i = 0;
        while i < 2 {
            let o = 9 * i;
            w[a + o + 4] = w[a + o] ^ w[a + o + 1];
            w[a + o + 5] = w[a + o + 2] ^ w[a + o + 3];
            w[a + o + 6] = w[a + o] ^ w[a + o + 2];
            w[a + o + 7] = w[a + o + 1] ^ w[a + o + 3];
            w[a + o + 8] = w[a + o + 6] ^ w[a + o + 7];
            w[b + o + 4] = w[b + o] ^ w[b + o + 1];
            w[b + o + 5] = w[b + o + 2] ^ w[b + o + 3];
            w[b + o + 6] = w[b + o] ^ w[b + o + 2];
            w[b + o + 7] = w[b + o + 1] ^ w[b + o + 3];
            w[b + o + 8] = w[b + o + 6] ^ w[b + o + 7];
            i = i + 1;
        }
        i = 0;
        while i < 18 {
            w[c + i] = bmul32(w[a + i], w[b + i]);
            i = i + 1;
        }
        w[c + 4] = w[c + 4] ^ w[c] ^ w[c + 1];
        w[c + 5] = w[c + 5] ^ w[c + 2] ^ w[c + 3];
        w[c + 8] = w[c + 8] ^ w[c + 6] ^ w[c + 7];
        w[c + 13] = w[c + 13] ^ w[c + 9] ^ w[c + 10];
        w[c + 14] = w[c + 14] ^ w[c + 11] ^ w[c + 12];
        w[c + 17] = w[c + 17] ^ w[c + 15] ^ w[c + 16];
        let d0 = w[c];
        let d1 = w[c + 4] ^ rev32(w[c + 9]) >> 1;
        let d2 = w[c + 1] ^ w[c] ^ w[c + 2] ^ w[c + 6] ^ rev32(w[c + 13]) >> 1;
        let d3 = w[c + 4] ^ w[c + 5] ^ w[c + 8] ^ rev32(w[c + 10] ^ w[c + 9] ^ w[c + 11] ^ w[c + 15]) >> 1;
        let d4 = w[c + 2] ^ w[c + 1] ^ w[c + 3] ^ w[c + 7] ^ rev32(w[c + 13] ^ w[c + 14] ^ w[c + 17]) >> 1;
        let d5 = w[c + 5] ^ rev32(w[c + 11] ^ w[c + 10] ^ w[c + 12] ^ w[c + 16]) >> 1;
        let d6 = w[c + 3] ^ rev32(w[c + 14]) >> 1;
        let d7 = rev32(w[c + 12]) >> 1;
        w[zw] = m32(d0 << 1);
        w[zw + 1] = m32(d1 << 1) | d0 >> 31;
        w[zw + 2] = m32(d2 << 1) | d1 >> 31;
        w[zw + 3] = m32(d3 << 1) | d2 >> 31;
        w[zw + 4] = m32(d4 << 1) | d3 >> 31;
        w[zw + 5] = m32(d5 << 1) | d4 >> 31;
        w[zw + 6] = m32(d6 << 1) | d5 >> 31;
        w[zw + 7] = m32(d7 << 1) | d6 >> 31;
        // Reduction modulo x^128 + x^7 + x^2 + x + 1.
        i = 0;
        while i < 4 {
            let lw = w[zw + i];
            w[zw + i + 4] = w[zw + i + 4] ^ lw ^ lw >> 1 ^ lw >> 2 ^ lw >> 7;
            w[zw + i + 3] = w[zw + i + 3] ^ m32(lw << 31) ^ m32(lw << 30) ^ m32(lw << 25);
            i = i + 1;
        }
        i = 0;
        while i < 4 {
            w[y + i] = w[zw + 4 + i];
            i = i + 1;
        }
        at = at + 16;
    }
    return 0;
}

// The largest plaintext under one nonce: 2^32 - 2 blocks (SP 800-38D
// §5.2.1.1).
pub fn max_text() -> [] int {
    return (0xffffffff - 1) * 16;
}

fn lengths_ok[&k, &n](key: &k [byte], nonce: &n [byte], text: int) -> [] int {
    if aes.rounds(len(key)) == 0 {
        return refused_key_length();
    }
    return shape_ok(nonce, text);
}

fn shape_ok[&n](nonce: &n [byte], text: int) -> [] int {
    if len(nonce) != 12 {
        return refused_nonce_length();
    }
    if text > max_text() {
        return refused_too_long();
    }
    return ok();
}

// ---- A prepared key (`docs/crypto-builtins.md` §6, step 2) ----
//
// What depends only on the key: the round count, the expanded key and
// GHASH's H = AES(K, 0) (its words and their bit-reversals). A caller
// that seals or opens many messages under one key, as a TLS record
// layer does, prepares it once: the expansion and H were 25% and 14% of
// a 64-byte seal (`docs/tls-parity.md` §3.1).
//
//     [0] rounds   [1 .. 1 + aes.skey_len()] the expanded key
//     then H's four words, then their bit-reversals.
fn c_skey() -> [] int {
    return 1;
}

fn c_h() -> [] int {
    return 1 + aes.skey_len();
}

pub fn context_len() -> [] int {
    return c_h() + 8;
}

// Prepares `key` (16 or 32 bytes) into `ctx` (at least `context_len()`
// words): `ok()`, or `refused_key_length()`.
pub fn prepare[&k, &c](key: &k [byte], ctx: &!c [int]) -> [] int {
    if aes.rounds(len(key)) == 0 || len(ctx) < context_len() {
        return refused_key_length();
    }
    region t {
        let q = alloc_slice[t](aes.scratch_len(), 0);
        let zeros = alloc_slice[t](16, byte_of(0));
        let blk = alloc_slice[t](16, byte_of(0));
        let nr = aes.expand(key, ctx[c_skey()..c_h()]);
        ctx[0] = nr;
        aes.encrypt_block_with(nr, ctx[c_skey()..c_h()], zeros, blk, q);
        var i = 0;
        while i < 4 {
            ctx[c_h() + 3 - i] = be32(blk, 4 * i);
            i = i + 1;
        }
        i = 0;
        while i < 4 {
            ctx[c_h() + 4 + i] = rev32(ctx[c_h() + i]);
            blk[4 * i] = byte_of(0);
            blk[4 * i + 1] = byte_of(0);
            blk[4 * i + 2] = byte_of(0);
            blk[4 * i + 3] = byte_of(0);
            i = i + 1;
        }
    }
    return ok();
}

// Overwrites a prepared key.
pub fn forget[&c](ctx: &!c [int]) -> [] int {
    bignum.zero(ctx);
    return 0;
}

// Whether `ctx` holds a prepared key.
fn prepared[&c](ctx: &c [int]) -> [] bool {
    return len(ctx) >= context_len() && (ctx[0] == 10 || ctx[0] == 14);
}

// The tag of `ciphertext` and `aad` under the nonce: GHASH over both
// and their lengths, XORed with the encryption of J0 = nonce || 1. Its
// scratch is the caller's: `q` for `std.aes`, `w` (`WORDS()`), and a
// 16-byte block `blk`. Leaves `w` and `blk` zero.
fn tag_of[&s, &n, &a, &c, &o, &q, &w, &b](ctx: &s [int], nonce: &n [byte], aad: &a [byte], ciphertext: &c [byte], tag: &!o [byte], q: &!q [int], w: &!w [int], blk: &!b [byte]) -> [] int {
    let nr = ctx[0];
    let skey = ctx[c_skey()..c_h()];
    var i = 0;
    while i < 4 {
        w[HW() + i] = ctx[c_h() + i];
        w[HWR() + i] = ctx[c_h() + 4 + i];
        w[Y() + i] = 0;
        i = i + 1;
    }
    ghash(w, aad);
    ghash(w, ciphertext);
    // The lengths block: both lengths in bits, 64 bits each.
    let abits = len(aad) * 8;
    let cbits = len(ciphertext) * 8;
    put_be32(blk, 0, abits >> 32);
    put_be32(blk, 4, abits & 0xffffffff);
    put_be32(blk, 8, cbits >> 32);
    put_be32(blk, 12, cbits & 0xffffffff);
    ghash(w, blk);
    i = 0;
    while i < 4 {
        put_be32(blk, 4 * i, w[Y() + 3 - i]);
        i = i + 1;
    }
    aes.ctr32_with(nr, skey, nonce, 1, blk, tag, q);
    i = 0;
    while i < 16 {
        blk[i] = byte_of(0);
        i = i + 1;
    }
    i = 0;
    while i < WORDS() {
        w[i] = 0;
        i = i + 1;
    }
    return 0;
}

// AES-GCM encryption: `out` must be exactly `len(plaintext) + 16`
// bytes, and receives the ciphertext followed by the 16-byte tag. `key`
// is 16 or 32 bytes, `nonce` 12. A (key, nonce) pair must never seal two
// messages: the nonce is the caller's to make unique.
pub fn seal[&k, &n, &a, &p, &o](key: &k [byte], nonce: &n [byte], aad: &a [byte], plaintext: &p [byte], out: &!o [byte]) -> [] int {
    let checked = lengths_ok(key, nonce, len(plaintext));
    if checked != 0 {
        return checked;
    }
    var code = 0;
    region t {
        let ctx = alloc_slice[t](context_len(), 0);
        prepare(key, ctx);
        code = seal_with(ctx, nonce, aad, plaintext, out);
        forget(ctx);
    }
    return code;
}

// `seal` under a key `prepare` made.
pub fn seal_with[&c, &n, &a, &p, &o](ctx: &c [int], nonce: &n [byte], aad: &a [byte], plaintext: &p [byte], out: &!o [byte]) -> [] int {
    if !prepared(ctx) {
        return refused_key_length();
    }
    let text = len(plaintext);
    let checked = shape_ok(nonce, text);
    if checked != 0 {
        return checked;
    }
    if len(out) != text + 16 {
        return refused_output_length();
    }
    // One region, so one allocation, for all of the call's scratch.
    region t {
        let q = alloc_slice[t](aes.scratch_len(), 0);
        let w = alloc_slice[t](WORDS(), 0);
        let blk = alloc_slice[t](16, byte_of(0));
        aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, plaintext, out[0..text], q);
        tag_of(ctx, nonce, aad, out[0..text], out[text..text + 16], q, w, blk);
    }
    return ok();
}

// AES-GCM decryption: `sealed` is ciphertext followed by the 16-byte
// tag, and `out` must be exactly `len(sealed) - 16` bytes. The tag is
// checked first, over every byte, and on a mismatch `out` is not
// written at all.
pub fn open[&k, &n, &a, &s, &o](key: &k [byte], nonce: &n [byte], aad: &a [byte], sealed: &s [byte], out: &!o [byte]) -> [] int {
    var text = len(sealed) - 16;
    if text < 0 {
        text = 0;
    }
    let checked = lengths_ok(key, nonce, text);
    if checked != 0 {
        return checked;
    }
    var code = 0;
    region t {
        let ctx = alloc_slice[t](context_len(), 0);
        prepare(key, ctx);
        code = open_with(ctx, nonce, aad, sealed, out);
        forget(ctx);
    }
    return code;
}

// `open` under a key `prepare` made.
pub fn open_with[&c, &n, &a, &s, &o](ctx: &c [int], nonce: &n [byte], aad: &a [byte], sealed: &s [byte], out: &!o [byte]) -> [] int {
    if !prepared(ctx) {
        return refused_key_length();
    }
    if len(sealed) < 16 {
        let shape = shape_ok(nonce, 0);
        if shape != 0 {
            return shape;
        }
        return refused_too_short();
    }
    let text = len(sealed) - 16;
    let checked = shape_ok(nonce, text);
    if checked != 0 {
        return checked;
    }
    if len(out) != text {
        return refused_output_length();
    }
    var diff = 0;
    region t {
        let q = alloc_slice[t](aes.scratch_len(), 0);
        let w = alloc_slice[t](WORDS(), 0);
        let blk = alloc_slice[t](16, byte_of(0));
        let want = alloc_slice[t](16, byte_of(0));
        tag_of(ctx, nonce, aad, sealed[0..text], want, q, w, blk);
        // Every byte is compared whatever the earlier ones were.
        var i = 0;
        while i < 16 {
            diff = diff | int_of(want[i]) ^ int_of(sealed[text + i]);
            want[i] = byte_of(0);
            i = i + 1;
        }
        if diff == 0 {
            aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, sealed[0..text], out, q);
        }
    }
    if diff != 0 {
        return refused_tag_mismatch();
    }
    return ok();
}
