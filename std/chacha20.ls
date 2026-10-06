edition 6;
module std.chacha20;
import std.bytes;

// `std.chacha20` — ChaCha20, Poly1305 and the ChaCha20-Poly1305 AEAD
// (RFC 8439). `docs/chacha20.md` is the design; this is sub-issue 2
// (#199) of the pure TLS 1.3 client (#197). It is a file of its own,
// not part of `std/crypto.ls`, so it can move to a package without a
// rewrite (`docs/chacha20.md` §1). Not independently reviewed (#209).
//
// Secret data flows through every function here (the key, the
// keystream, the one-time Poly1305 key, the plaintext), so the rule is
// `docs/chacha20.md` §3's: no `if`, `while` condition or slice index
// depends on a secret value. Lengths, counters and the *result* of the
// tag comparison are public and may be branched on.

// The refusals, each a code and a tag (`refusal_tag`). 0 is success.
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

// The stable name of a refusal code, for a log line or a test.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "chacha20-key-length";
    }
    if code == -2 {
        return "chacha20-nonce-length";
    }
    if code == -3 {
        return "chacha20-output-length";
    }
    if code == -4 {
        return "chacha20-counter-exhausted";
    }
    if code == -5 {
        return "aead-too-short";
    }
    if code == -6 {
        return "aead-tag-mismatch";
    }
    return "unknown";
}

fn m32(x: int) -> [] int {
    return x & 0xffffffff;
}

// Every word this rotates is in `[0, 2^32)`, so `x >> 32 - n` is the
// logical shift (`docs/sha512.md` §2) and `x << n` stays under 2^48.
fn rotl32(x: int, n: int) -> [] int {
    return m32(x << n | x >> 32 - n);
}

fn le32[&s](s: &s [byte], at: int) -> [] int {
    return int_of(s[at]) | int_of(s[at + 1]) << 8 | int_of(s[at + 2]) << 16 | int_of(s[at + 3]) << 24;
}

// RFC 8439 §2.1, on the working copy in `x[16..32]`. The indices are
// constants of the round structure, never data.
fn quarter[&x](x: &!x [int], wa: int, wb: int, wc: int, wd: int) -> [] int {
    let a = wa + 16;
    let b = wb + 16;
    let c = wc + 16;
    let d = wd + 16;
    x[a] = m32(wrapping_add(x[a], x[b]));
    x[d] = rotl32(x[d] ^ x[a], 16);
    x[c] = m32(wrapping_add(x[c], x[d]));
    x[b] = rotl32(x[b] ^ x[c], 12);
    x[a] = m32(wrapping_add(x[a], x[b]));
    x[d] = rotl32(x[d] ^ x[a], 8);
    x[c] = m32(wrapping_add(x[c], x[d]));
    x[b] = rotl32(x[b] ^ x[c], 7);
    return 0;
}

// RFC 8439 §2.3: the 64-byte keystream block for (`key`, `counter`,
// `nonce`), into `out`. Lengths are the caller's (`block`, `xor`).
fn block_into[&k, &n, &w, &o](key: &k [byte], counter: int, nonce: &n [byte], work: &!w [int], out: &!o [byte]) -> [] int {
    work[0] = 0x61707865;
    work[1] = 0x3320646e;
    work[2] = 0x79622d32;
    work[3] = 0x6b206574;
    var i = 0;
    while i < 8 {
        work[4 + i] = le32(key, i * 4);
        i = i + 1;
    }
    work[12] = counter;
    work[13] = le32(nonce, 0);
    work[14] = le32(nonce, 4);
    work[15] = le32(nonce, 8);
    i = 0;
    while i < 16 {
        work[16 + i] = work[i];
        i = i + 1;
    }
    var round = 0;
    while round < 10 {
        quarter(work, 0, 4, 8, 12);
        quarter(work, 1, 5, 9, 13);
        quarter(work, 2, 6, 10, 14);
        quarter(work, 3, 7, 11, 15);
        quarter(work, 0, 5, 10, 15);
        quarter(work, 1, 6, 11, 12);
        quarter(work, 2, 7, 8, 13);
        quarter(work, 3, 4, 9, 14);
        round = round + 1;
    }
    i = 0;
    while i < 16 {
        bytes.store_le32(out, i * 4, m32(wrapping_add(work[16 + i], work[i])));
        i = i + 1;
    }
    return 0;
}

// The one ChaCha20 block for (`key`, `counter`, `nonce`), into the
// first 64 bytes of `out`. `counter` must be in `[0, 2^32)`.
pub fn block[&k, &n, &o](key: &k [byte], counter: int, nonce: &n [byte], out: &!o [byte]) -> [] int {
    if len(key) != 32 {
        return refused_key_length();
    }
    if len(nonce) != 12 {
        return refused_nonce_length();
    }
    if counter < 0 || counter > 0xffffffff {
        return refused_too_long();
    }
    if len(out) < 64 {
        return refused_output_length();
    }
    region r {
        let work = alloc_slice[r](32, 0);
        block_into(key, counter, nonce, work, out);
    }
    return ok();
}

// RFC 8439 §2.4: `output = input XOR keystream`, starting at block
// `counter`. `output` must be exactly as long as `input`. The 32-bit
// block counter is not allowed to wrap: a message that would need it
// to is refused, not encrypted under a repeated keystream.
pub fn xor[&k, &n, &i, &o](key: &k [byte], counter: int, nonce: &n [byte], input: &i [byte], output: &!o [byte]) -> [] int {
    if len(key) != 32 {
        return refused_key_length();
    }
    if len(nonce) != 12 {
        return refused_nonce_length();
    }
    let total = len(input);
    if len(output) != total {
        return refused_output_length();
    }
    if counter < 0 || counter > 0xffffffff {
        return refused_too_long();
    }
    let blocks = (total + 63) / 64;
    if blocks > 0x100000000 - counter {
        return refused_too_long();
    }
    region r {
        let work = alloc_slice[r](32, 0);
        let stream = alloc_slice[r](64, byte_of(0));
        var at = 0;
        var c = counter;
        while at < total {
            block_into(key, c, nonce, work, stream);
            var j = 0;
            while j < 64 && at + j < total {
                output[at + j] = byte_of(int_of(input[at + j]) ^ int_of(stream[j]));
                j = j + 1;
            }
            at = at + 64;
            c = c + 1;
        }
    }
    return ok();
}

// Poly1305 (RFC 8439 §2.5) in five 26-bit limbs, the layout of
// poly1305-donna's 32-bit code. `st` is ten words: `r0..r4`, then the
// accumulator `h0..h4`. With a message limb added every limb of `h` is
// under 2^28 and `5 * r` is under 2^29, so a product is under 2^57 and
// a sum of five under 2^60: nothing overflows, and the arithmetic is
// `wrapping_*` only so that no overflow check is emitted
// (`docs/chacha20.md` §2).
fn poly_init[&s, &k](st: &!s [int], key: &k [byte]) -> [] int {
    st[0] = le32(key, 0) & 0x3ffffff;
    st[1] = le32(key, 3) >> 2 & 0x3ffff03;
    st[2] = le32(key, 6) >> 4 & 0x3ffc0ff;
    st[3] = le32(key, 9) >> 6 & 0x3f03fff;
    st[4] = le32(key, 12) >> 8 & 0x00fffff;
    var i = 5;
    while i < 10 {
        st[i] = 0;
        i = i + 1;
    }
    return 0;
}

// `a*b + c*d + e*f + g*h + i*j`, none of which can overflow (the bound
// above); wrapping so that no overflow check, which would be a branch on
// secret data, is emitted (`docs/chacha20.md` §3).
fn dot5(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int, j: int) -> [] int {
    let ab = wrapping_mul(a, b);
    let cd = wrapping_mul(c, d);
    let ef = wrapping_mul(e, f);
    let gh = wrapping_mul(g, h);
    let ij = wrapping_mul(i, j);
    return wrapping_add(wrapping_add(wrapping_add(ab, cd), wrapping_add(ef, gh)), ij);
}

// One 16-byte block. `hibit` is `1 << 24` for a full block (the 2^128
// bit) and 0 for the final partial one, which the caller has already
// padded with its `0x01` byte.
fn poly_block[&s, &m](st: &!s [int], m: &m [byte], hibit: int) -> [] int {
    let r0 = st[0];
    let r1 = st[1];
    let r2 = st[2];
    let r3 = st[3];
    let r4 = st[4];
    let s1 = wrapping_mul(r1, 5);
    let s2 = wrapping_mul(r2, 5);
    let s3 = wrapping_mul(r3, 5);
    let s4 = wrapping_mul(r4, 5);

    let h0 = wrapping_add(st[5], le32(m, 0) & 0x3ffffff);
    let h1 = wrapping_add(st[6], le32(m, 3) >> 2 & 0x3ffffff);
    let h2 = wrapping_add(st[7], le32(m, 6) >> 4 & 0x3ffffff);
    let h3 = wrapping_add(st[8], le32(m, 9) >> 6 & 0x3ffffff);
    let h4 = wrapping_add(st[9], le32(m, 12) >> 8 | hibit);

    let d0 = dot5(h0, r0, h1, s4, h2, s3, h3, s2, h4, s1);
    var d1 = dot5(h0, r1, h1, r0, h2, s4, h3, s3, h4, s2);
    var d2 = dot5(h0, r2, h1, r1, h2, r0, h3, s4, h4, s3);
    var d3 = dot5(h0, r3, h1, r2, h2, r1, h3, r0, h4, s4);
    var d4 = dot5(h0, r4, h1, r3, h2, r2, h3, r1, h4, r0);

    var c = d0 >> 26;
    var n0 = d0 & 0x3ffffff;
    d1 = wrapping_add(d1, c);
    c = d1 >> 26;
    let n1 = d1 & 0x3ffffff;
    d2 = wrapping_add(d2, c);
    c = d2 >> 26;
    st[7] = d2 & 0x3ffffff;
    d3 = wrapping_add(d3, c);
    c = d3 >> 26;
    st[8] = d3 & 0x3ffffff;
    d4 = wrapping_add(d4, c);
    c = d4 >> 26;
    st[9] = d4 & 0x3ffffff;
    n0 = wrapping_add(n0, wrapping_mul(c, 5));
    c = n0 >> 26;
    st[5] = n0 & 0x3ffffff;
    st[6] = wrapping_add(n1, c);
    return 0;
}

// Feeds `data` as full 16-byte blocks, the last one zero-padded: the
// AEAD's `pad16` (RFC 8439 §2.8), which is a full block, not
// Poly1305's own `0x01` padding.
fn poly_padded[&s, &d, &b](st: &!s [int], data: &d [byte], scratch: &!b [byte]) -> [] int {
    let n = len(data);
    var at = 0;
    while at + 16 <= n {
        poly_block(st, data[at..at + 16], 0x1000000);
        at = at + 16;
    }
    if at < n {
        var j = 0;
        while j < 16 {
            scratch[j] = byte_of(0);
            j = j + 1;
        }
        j = 0;
        while at + j < n {
            scratch[j] = data[at + j];
            j = j + 1;
        }
        poly_block(st, scratch[0..16], 0x1000000);
    }
    return 0;
}

// Full carry, `h mod p` by a masked select of `h` or `h - p` (no
// branch on the accumulator), `+ s mod 2^128`, little-endian out.
fn poly_finish[&s, &k, &o](st: &!s [int], key: &k [byte], tag: &!o [byte]) -> [] int {
    var h0 = st[5];
    var h1 = st[6];
    var h2 = st[7];
    var h3 = st[8];
    var h4 = st[9];

    var c = h1 >> 26;
    h1 = h1 & 0x3ffffff;
    h2 = wrapping_add(h2, c);
    c = h2 >> 26;
    h2 = h2 & 0x3ffffff;
    h3 = wrapping_add(h3, c);
    c = h3 >> 26;
    h3 = h3 & 0x3ffffff;
    h4 = wrapping_add(h4, c);
    c = h4 >> 26;
    h4 = h4 & 0x3ffffff;
    h0 = wrapping_add(h0, wrapping_mul(c, 5));
    c = h0 >> 26;
    h0 = h0 & 0x3ffffff;
    h1 = wrapping_add(h1, c);

    // `g = h + 5 - 2^130`: negative exactly when `h < p`.
    var g0 = wrapping_add(h0, 5);
    c = g0 >> 26;
    g0 = g0 & 0x3ffffff;
    var g1 = wrapping_add(h1, c);
    c = g1 >> 26;
    g1 = g1 & 0x3ffffff;
    var g2 = wrapping_add(h2, c);
    c = g2 >> 26;
    g2 = g2 & 0x3ffffff;
    var g3 = wrapping_add(h3, c);
    c = g3 >> 26;
    g3 = g3 & 0x3ffffff;
    let g4 = wrapping_sub(wrapping_add(h4, c), 0x4000000);

    // All ones when `g4 < 0` (keep `h`), zero otherwise (take `g`). The
    // mask is secret, so it passes through `value_barrier` where it is
    // made (`docs/value-barrier.md` §4; review finding B-1, #209): LLVM
    // can prove `x >> 63` is 0 or -1 and turn the select into a branch.
    let keep = value_barrier(g4 >> 63);
    let take = ~keep;
    h0 = h0 & keep | g0 & take;
    h1 = h1 & keep | g1 & take;
    h2 = h2 & keep | g2 & take;
    h3 = h3 & keep | g3 & take;
    h4 = h4 & keep | g4 & take;

    let w0 = m32(h0 | h1 << 26);
    let w1 = m32(h1 >> 6 | h2 << 20);
    let w2 = m32(h2 >> 12 | h3 << 14);
    let w3 = m32(h3 >> 18 | h4 << 8);

    var f = wrapping_add(w0, le32(key, 16));
    bytes.store_le32(tag, 0, m32(f));
    f = wrapping_add(wrapping_add(w1, le32(key, 20)), f >> 32);
    bytes.store_le32(tag, 4, m32(f));
    f = wrapping_add(wrapping_add(w2, le32(key, 24)), f >> 32);
    bytes.store_le32(tag, 8, m32(f));
    f = wrapping_add(wrapping_add(w3, le32(key, 28)), f >> 32);
    bytes.store_le32(tag, 12, m32(f));
    return 0;
}

// The Poly1305 one-time authenticator (RFC 8439 §2.5): a 16-byte tag
// over `msg` under the 32-byte one-time `key`, into `tag[0..16]`. A key
// must never be used for two messages; the AEAD below derives a fresh
// one per nonce.
pub fn poly1305[&k, &m, &o](key: &k [byte], msg: &m [byte], tag: &!o [byte]) -> [] int {
    if len(key) != 32 {
        return refused_key_length();
    }
    if len(tag) < 16 {
        return refused_output_length();
    }
    region r {
        let st = alloc_slice[r](10, 0);
        poly_init(st, key);
        let n = len(msg);
        var at = 0;
        while at + 16 <= n {
            poly_block(st, msg[at..at + 16], 0x1000000);
            at = at + 16;
        }
        if at < n {
            let last = alloc_slice[r](16, byte_of(0));
            var j = 0;
            while at + j < n {
                last[j] = msg[at + j];
                j = j + 1;
            }
            last[j] = byte_of(1);
            poly_block(st, last, 0);
        }
        poly_finish(st, key, tag);
    }
    return ok();
}

// RFC 8439 §2.8's tag: the one-time key is block 0's first 32 bytes,
// then `aad || pad16 || ciphertext || pad16 || le64(len aad) ||
// le64(len ciphertext)`.
fn aead_tag[&k, &n, &a, &c, &o](key: &k [byte], nonce: &n [byte], aad: &a [byte], ct: &c [byte], tag: &!o [byte]) -> [] int {
    region r {
        let work = alloc_slice[r](32, 0);
        let otk = alloc_slice[r](64, byte_of(0));
        block_into(key, 0, nonce, work, otk);
        let st = alloc_slice[r](10, 0);
        poly_init(st, otk);
        let scratch = alloc_slice[r](16, byte_of(0));
        poly_padded(st, aad, scratch);
        poly_padded(st, ct, scratch);
        let lens = alloc_slice[r](16, byte_of(0));
        bytes.store_le32(lens, 0, m32(len(aad)));
        bytes.store_le32(lens, 4, len(aad) >> 32);
        bytes.store_le32(lens, 8, m32(len(ct)));
        bytes.store_le32(lens, 12, len(ct) >> 32);
        poly_block(st, lens, 0x1000000);
        poly_finish(st, otk[0..32], tag);
        // The one-time key is secret; this overwrite is best effort
        // (`docs/chacha20.md` §3.2 says what is not guaranteed).
        var i = 0;
        while i < 64 {
            otk[i] = byte_of(0);
            i = i + 1;
        }
    }
    return 0;
}

// Checks shared by `seal` and `open`. 2^32 - 1 blocks from counter 1
// is RFC 8439 §2.8's own ceiling on one message.
fn aead_lengths[&k, &n](key: &k [byte], nonce: &n [byte], text: int) -> [] int {
    if len(key) != 32 {
        return refused_key_length();
    }
    if len(nonce) != 12 {
        return refused_nonce_length();
    }
    if (text + 63) / 64 > 0xffffffff {
        return refused_too_long();
    }
    return ok();
}

// AEAD_CHACHA20_POLY1305 encryption (RFC 8439 §2.8): `out` must be
// exactly `len(plaintext) + 16` bytes, and receives the ciphertext
// followed by the 16-byte tag. A (key, nonce) pair must never seal two
// messages: the nonce is the caller's to make unique.
pub fn seal[&k, &n, &a, &p, &o](key: &k [byte], nonce: &n [byte], aad: &a [byte], plaintext: &p [byte], out: &!o [byte]) -> [] int {
    let text = len(plaintext);
    let checked = aead_lengths(key, nonce, text);
    if checked != 0 {
        return checked;
    }
    if len(out) != text + 16 {
        return refused_output_length();
    }
    xor(key, 1, nonce, plaintext, out[0..text]);
    aead_tag(key, nonce, aad, out[0..text], out[text..text + 16]);
    return ok();
}

// AEAD_CHACHA20_POLY1305 decryption: `sealed` is ciphertext followed by
// the 16-byte tag, and `out` must be exactly `len(sealed) - 16` bytes.
// The tag is checked first, over every byte, and on a mismatch `out`
// is not written at all (`docs/chacha20.md` §3.1): the plaintext of a
// forged message is never handed to the caller, not even in part.
pub fn open[&k, &n, &a, &s, &o](key: &k [byte], nonce: &n [byte], aad: &a [byte], sealed: &s [byte], out: &!o [byte]) -> [] int {
    if len(sealed) < 16 {
        let shape = aead_lengths(key, nonce, 0);
        if shape != 0 {
            return shape;
        }
        return refused_too_short();
    }
    let text = len(sealed) - 16;
    let checked = aead_lengths(key, nonce, text);
    if checked != 0 {
        return checked;
    }
    if len(out) != text {
        return refused_output_length();
    }
    var diff = 0;
    region r {
        let want = alloc_slice[r](16, byte_of(0));
        aead_tag(key, nonce, aad, sealed[0..text], want);
        // Every byte is compared whatever the earlier ones were: the
        // time this takes says nothing about where the tags differ.
        var i = 0;
        while i < 16 {
            diff = diff | int_of(want[i]) ^ int_of(sealed[text + i]);
            i = i + 1;
        }
    }
    if diff != 0 {
        return refused_tag_mismatch();
    }
    xor(key, 1, nonce, sealed[0..text], out);
    return ok();
}
