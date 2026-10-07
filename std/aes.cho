module std.aes;

// `std.aes` -- AES-128 and AES-256 (FIPS 197), bitsliced, with no table
// and no branch or index that depends on the key or the data
// (`docs/tls-parity.md` §3.1). Sub-issue 10 (#207) of the self-contained
// TLS client (#197). Not independently reviewed (#209).
//
// A port of BearSSL's `aes_ct` (Thomas Pornin, MIT licence: "Permission
// is hereby granted, free of charge, to any person obtaining a copy of
// this software ... The above copyright notice and this permission
// notice shall be included in all copies or substantial portions of the
// Software."). Two blocks are encrypted at once, as eight 32-bit words
// whose bits are the blocks' bits regrouped (`ortho`); the S-box is
// Boyar and Peralta's circuit of 113 gates
// (https://eprint.iacr.org/2009/191.pdf).
//
// Every word here is in [0, 2^32): a left shift is masked back with
// `m32`, so the right shift of a word is the logical one
// (`docs/sha512.md` §2).

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_key_length() -> [] int {
    return -1;
}

pub fn refused_counter_exhausted() -> [] int {
    return -2;
}

pub fn refused_scratch_length() -> [] int {
    return -3;
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "aes-key-length";
    }
    if code == -2 {
        return "aes-counter-exhausted";
    }
    if code == -3 {
        return "aes-scratch-length";
    }
    return "unknown";
}

fn m32(x: int) -> [] int {
    return x & 0xffffffff;
}

fn not32(x: int) -> [] int {
    return x ^ 0xffffffff;
}

fn le32[&s](s: &s [byte], at: int) -> [] int {
    return int_of(s[at]) | int_of(s[at + 1]) << 8 | int_of(s[at + 2]) << 16 | int_of(s[at + 3]) << 24;
}

fn swap32(x: int) -> [] int {
    return (x & 255) << 24 | (x >> 8 & 255) << 16 | (x >> 16 & 255) << 8 | x >> 24 & 255;
}

// The S-box on eight bitsliced words `q[at..at + 8]`.
fn sbox[&q](q: &!q [int], at: int) -> [] int {
    let x0 = q[at + 7];
    let x1 = q[at + 6];
    let x2 = q[at + 5];
    let x3 = q[at + 4];
    let x4 = q[at + 3];
    let x5 = q[at + 2];
    let x6 = q[at + 1];
    let x7 = q[at];
    // Top linear transformation.
    let y14 = x3 ^ x5;
    let y13 = x0 ^ x6;
    let y9 = x0 ^ x3;
    let y8 = x0 ^ x5;
    let t0 = x1 ^ x2;
    let y1 = t0 ^ x7;
    let y4 = y1 ^ x3;
    let y12 = y13 ^ y14;
    let y2 = y1 ^ x0;
    let y5 = y1 ^ x6;
    let y3 = y5 ^ y8;
    let t1 = x4 ^ y12;
    let y15 = t1 ^ x5;
    let y20 = t1 ^ x1;
    let y6 = y15 ^ x7;
    let y10 = y15 ^ t0;
    let y11 = y20 ^ y9;
    let y7 = x7 ^ y11;
    let y17 = y10 ^ y11;
    let y19 = y10 ^ y8;
    let y16 = t0 ^ y11;
    let y21 = y13 ^ y16;
    let y18 = x0 ^ y16;
    // Non-linear section.
    let t2 = y12 & y15;
    let t3 = y3 & y6;
    let t4 = t3 ^ t2;
    let t5 = y4 & x7;
    let t6 = t5 ^ t2;
    let t7 = y13 & y16;
    let t8 = y5 & y1;
    let t9 = t8 ^ t7;
    let t10 = y2 & y7;
    let t11 = t10 ^ t7;
    let t12 = y9 & y11;
    let t13 = y14 & y17;
    let t14 = t13 ^ t12;
    let t15 = y8 & y10;
    let t16 = t15 ^ t12;
    let t17 = t4 ^ t14;
    let t18 = t6 ^ t16;
    let t19 = t9 ^ t14;
    let t20 = t11 ^ t16;
    let t21 = t17 ^ y20;
    let t22 = t18 ^ y19;
    let t23 = t19 ^ y21;
    let t24 = t20 ^ y18;
    let t25 = t21 ^ t22;
    let t26 = t21 & t23;
    let t27 = t24 ^ t26;
    let t28 = t25 & t27;
    let t29 = t28 ^ t22;
    let t30 = t23 ^ t24;
    let t31 = t22 ^ t26;
    let t32 = t31 & t30;
    let t33 = t32 ^ t24;
    let t34 = t23 ^ t33;
    let t35 = t27 ^ t33;
    let t36 = t24 & t35;
    let t37 = t36 ^ t34;
    let t38 = t27 ^ t36;
    let t39 = t29 & t38;
    let t40 = t25 ^ t39;
    let t41 = t40 ^ t37;
    let t42 = t29 ^ t33;
    let t43 = t29 ^ t40;
    let t44 = t33 ^ t37;
    let t45 = t42 ^ t41;
    let z0 = t44 & y15;
    let z1 = t37 & y6;
    let z2 = t33 & x7;
    let z3 = t43 & y16;
    let z4 = t40 & y1;
    let z5 = t29 & y7;
    let z6 = t42 & y11;
    let z7 = t45 & y17;
    let z8 = t41 & y10;
    let z9 = t44 & y12;
    let z10 = t37 & y3;
    let z11 = t33 & y4;
    let z12 = t43 & y13;
    let z13 = t40 & y5;
    let z14 = t29 & y2;
    let z15 = t42 & y9;
    let z16 = t45 & y14;
    let z17 = t41 & y8;
    // Bottom linear transformation.
    let t46 = z15 ^ z16;
    let t47 = z10 ^ z11;
    let t48 = z5 ^ z13;
    let t49 = z9 ^ z10;
    let t50 = z2 ^ z12;
    let t51 = z2 ^ z5;
    let t52 = z7 ^ z8;
    let t53 = z0 ^ z3;
    let t54 = z6 ^ z7;
    let t55 = z16 ^ z17;
    let t56 = z12 ^ t48;
    let t57 = t50 ^ t53;
    let t58 = z4 ^ t46;
    let t59 = z3 ^ t54;
    let t60 = t46 ^ t57;
    let t61 = z14 ^ t57;
    let t62 = t52 ^ t58;
    let t63 = t49 ^ t58;
    let t64 = z4 ^ t59;
    let t65 = t61 ^ t62;
    let t66 = z1 ^ t63;
    let s0 = t59 ^ t63;
    let s6 = t56 ^ not32(t62);
    let s7 = t48 ^ not32(t60);
    let t67 = t64 ^ t65;
    let s3 = t53 ^ t66;
    let s4 = t51 ^ t66;
    let s5 = t47 ^ t65;
    let s1 = t64 ^ not32(s3);
    let s2 = t55 ^ not32(t67);
    q[at + 7] = s0;
    q[at + 6] = s1;
    q[at + 5] = s2;
    q[at + 4] = s3;
    q[at + 3] = s4;
    q[at + 2] = s5;
    q[at + 1] = s6;
    q[at] = s7;
    return 0;
}

fn swapn[&q](q: &!q [int], cl: int, ch: int, s: int, x: int, y: int) -> [] int {
    let a = q[x];
    let b = q[y];
    q[x] = a & cl | m32((b & cl) << s);
    q[y] = (a & ch) >> s | b & ch;
    return 0;
}

// Regroups eight words `q[at..at + 8]` into the bitsliced form, and back
// (the transformation is its own inverse).
fn ortho[&q](q: &!q [int], at: int) -> [] int {
    swapn(q, 0x55555555, 0xaaaaaaaa, 1, at, at + 1);
    swapn(q, 0x55555555, 0xaaaaaaaa, 1, at + 2, at + 3);
    swapn(q, 0x55555555, 0xaaaaaaaa, 1, at + 4, at + 5);
    swapn(q, 0x55555555, 0xaaaaaaaa, 1, at + 6, at + 7);
    swapn(q, 0x33333333, 0xcccccccc, 2, at, at + 2);
    swapn(q, 0x33333333, 0xcccccccc, 2, at + 1, at + 3);
    swapn(q, 0x33333333, 0xcccccccc, 2, at + 4, at + 6);
    swapn(q, 0x33333333, 0xcccccccc, 2, at + 5, at + 7);
    swapn(q, 0x0f0f0f0f, 0xf0f0f0f0, 4, at, at + 4);
    swapn(q, 0x0f0f0f0f, 0xf0f0f0f0, 4, at + 1, at + 5);
    swapn(q, 0x0f0f0f0f, 0xf0f0f0f0, 4, at + 2, at + 6);
    swapn(q, 0x0f0f0f0f, 0xf0f0f0f0, 4, at + 3, at + 7);
    return 0;
}

fn rcon(k: int) -> [] int {
    if k < 8 {
        return 1 << k;
    }
    if k == 8 {
        return 0x1b;
    }
    return 0x36;
}

// The S-box on each byte of one word (for the key schedule).
fn sub_word(x: int) -> [] int {
    var r = 0;
    region t {
        let q = alloc_slice[t](8, x);
        ortho(q, 0);
        sbox(q, 0);
        ortho(q, 0);
        r = q[0];
    }
    return r;
}

// The number of rounds for a key of `len` bytes: 10 for AES-128, 14 for
// AES-256, else 0. AES-192 is not offered by any TLS suite and is not
// here.
pub fn rounds(len: int) -> [] int {
    if len == 16 {
        return 10;
    }
    if len == 32 {
        return 14;
    }
    return 0;
}

// The words of an expanded key: `skey_len()` of them, enough for AES-256.
pub fn skey_len() -> [] int {
    return 120;
}

// Expands `key` (16 or 32 bytes) into `skey` (at least `skey_len()`
// words), in the bitsliced form `encrypt_sliced` takes. The number of
// rounds, or `refused_key_length()`. Branches only on the key's length.
// Works in `skey` alone: the schedule is built there, made bitsliced in
// place, then compressed and expanded back word pair by word pair.
pub fn expand[&k, &s](key: &k [byte], skey: &!s [int]) -> [] int {
    let nr = rounds(len(key));
    if nr == 0 || len(skey) < skey_len() {
        return refused_key_length();
    }
    let nk = len(key) / 4;
    let nkf = (nr + 1) * 4;
    var tmp = 0;
    var i = 0;
    while i < nk {
        tmp = le32(key, 4 * i);
        skey[2 * i] = tmp;
        skey[2 * i + 1] = tmp;
        i = i + 1;
    }
    var j = 0;
    var k = 0;
    i = nk;
    while i < nkf {
        if j == 0 {
            tmp = m32(tmp << 24) | tmp >> 8;
            tmp = sub_word(tmp) ^ rcon(k);
        } else if nk > 6 && j == 4 {
            tmp = sub_word(tmp);
        }
        tmp = tmp ^ skey[2 * (i - nk)];
        skey[2 * i] = tmp;
        skey[2 * i + 1] = tmp;
        j = j + 1;
        if j == nk {
            j = 0;
            k = k + 1;
        }
        i = i + 1;
    }
    tmp = 0;
    i = 0;
    while i < nkf {
        ortho(skey, 2 * i);
        i = i + 4;
    }
    // The compressed key, expanded straight back (BearSSL's
    // `skey_expand`): each word's even bits from one copy, odd bits
    // from the other.
    i = 0;
    while i < nkf {
        let c = skey[2 * i] & 0x55555555 | skey[2 * i + 1] & 0xaaaaaaaa;
        let x = c & 0x55555555;
        let y = c & 0xaaaaaaaa;
        skey[2 * i] = x | m32(x << 1);
        skey[2 * i + 1] = y | y >> 1;
        i = i + 1;
    }
    return nr;
}

// The expanded key in FIPS 197's own form, for the hardware path
// (`docs/crypto-builtins.md` §3): `rounds + 1` round keys of 16 bytes
// into `out` (at least that long), the words `expand` makes before it
// bitslices them, each stored in byte order. The number of rounds, or
// `refused_key_length()`. The same schedule as `expand`, so the same
// constant-time `sub_word`; branches only on the key's length.
pub fn round_keys[&k, &o](key: &k [byte], out: &!o [byte]) -> [] int {
    let nr = rounds(len(key));
    if nr == 0 || len(out) < (nr + 1) * 16 {
        return refused_key_length();
    }
    let nk = len(key) / 4;
    let nkf = (nr + 1) * 4;
    var i = 0;
    while i < 4 * nk {
        out[i] = key[i];
        i = i + 1;
    }
    var tmp = le32(key, 4 * (nk - 1));
    var j = 0;
    var k = 0;
    i = nk;
    while i < nkf {
        if j == 0 {
            tmp = m32(tmp << 24) | tmp >> 8;
            tmp = sub_word(tmp) ^ rcon(k);
        } else if nk > 6 && j == 4 {
            tmp = sub_word(tmp);
        }
        tmp = tmp ^ le32(out, 4 * (i - nk));
        out[4 * i] = byte_of(tmp & 0xff);
        out[4 * i + 1] = byte_of(tmp >> 8 & 0xff);
        out[4 * i + 2] = byte_of(tmp >> 16 & 0xff);
        out[4 * i + 3] = byte_of(tmp >> 24 & 0xff);
        j = j + 1;
        if j == nk {
            j = 0;
            k = k + 1;
        }
        i = i + 1;
    }
    return nr;
}

fn add_round_key[&q, &s](q: &!q [int], skey: &s [int], at: int) -> [] int {
    var i = 0;
    while i < 8 {
        q[i] = q[i] ^ skey[at + i];
        i = i + 1;
    }
    return 0;
}

fn shift_rows[&q](q: &!q [int]) -> [] int {
    var i = 0;
    while i < 8 {
        let x = q[i];
        q[i] = x & 0x000000ff | (x & 0x0000fc00) >> 2 | (x & 0x00000300) << 6 | (x & 0x00f00000) >> 4 | (x & 0x000f0000) << 4 | (x & 0xc0000000) >> 6 | (x & 0x3f000000) << 2;
        i = i + 1;
    }
    return 0;
}

fn rotr8(x: int) -> [] int {
    return x >> 8 | m32(x << 24);
}

fn rotr16(x: int) -> [] int {
    return x >> 16 | m32(x << 16);
}

fn mix_columns[&q](q: &!q [int]) -> [] int {
    let q0 = q[0];
    let q1 = q[1];
    let q2 = q[2];
    let q3 = q[3];
    let q4 = q[4];
    let q5 = q[5];
    let q6 = q[6];
    let q7 = q[7];
    let r0 = rotr8(q0);
    let r1 = rotr8(q1);
    let r2 = rotr8(q2);
    let r3 = rotr8(q3);
    let r4 = rotr8(q4);
    let r5 = rotr8(q5);
    let r6 = rotr8(q6);
    let r7 = rotr8(q7);
    q[0] = q7 ^ r7 ^ r0 ^ rotr16(q0 ^ r0);
    q[1] = q0 ^ r0 ^ q7 ^ r7 ^ r1 ^ rotr16(q1 ^ r1);
    q[2] = q1 ^ r1 ^ r2 ^ rotr16(q2 ^ r2);
    q[3] = q2 ^ r2 ^ q7 ^ r7 ^ r3 ^ rotr16(q3 ^ r3);
    q[4] = q3 ^ r3 ^ q7 ^ r7 ^ r4 ^ rotr16(q4 ^ r4);
    q[5] = q4 ^ r4 ^ r5 ^ rotr16(q5 ^ r5);
    q[6] = q5 ^ r5 ^ r6 ^ rotr16(q6 ^ r6);
    q[7] = q6 ^ r6 ^ r7 ^ rotr16(q7 ^ r7);
    return 0;
}

// Encrypts two blocks at once: `q` (8 words) holds them in the bitsliced
// form; `nr` rounds of `skey` (from `expand`).
fn encrypt_sliced[&s, &q](nr: int, skey: &s [int], q: &!q [int]) -> [] int {
    add_round_key(q, skey, 0);
    var u = 1;
    while u < nr {
        sbox(q, 0);
        shift_rows(q);
        mix_columns(q);
        add_round_key(q, skey, 8 * u);
        u = u + 1;
    }
    sbox(q, 0);
    shift_rows(q);
    add_round_key(q, skey, 8 * nr);
    return 0;
}

// The scratch `encrypt_block_with` and `ctr32_with` take, in words.
pub fn scratch_len() -> [] int {
    return 8;
}

// Byte `k` (0 to 31) of the two blocks `q` holds once out of the
// bitsliced form: the first block's in the even words, the second's in
// the odd ones, each word little-endian. `k` is a position, never data.
fn byte_at[&q](q: &q [int], k: int) -> [] int {
    return q[2 * ((k & 15) >> 2) + (k >> 4)] >> 8 * (k & 3) & 255;
}

// One block: `input` (16 bytes) encrypted into `out` (16 bytes) under
// the expanded key `skey` with `nr` rounds (`expand`'s answer), using
// `q` (`scratch_len()` words) and leaving it zero. Allocates nothing.
pub fn encrypt_block_with[&s, &i, &o, &q](nr: int, skey: &s [int], input: &i [byte], out: &!o [byte], q: &!q [int]) -> [] int {
    if len(q) < scratch_len() {
        return refused_scratch_length();
    }
    var i = 0;
    while i < 4 {
        q[2 * i] = le32(input, 4 * i);
        q[2 * i + 1] = 0;
        i = i + 1;
    }
    ortho(q, 0);
    encrypt_sliced(nr, skey, q);
    ortho(q, 0);
    var k = 0;
    while k < 16 {
        out[k] = byte_of(byte_at(q, k));
        k = k + 1;
    }
    k = 0;
    while k < 8 {
        q[k] = 0;
        k = k + 1;
    }
    return 0;
}

// `encrypt_block_with`, with its scratch from a region of its own.
pub fn encrypt_block[&s, &i, &o](nr: int, skey: &s [int], input: &i [byte], out: &!o [byte]) -> [] int {
    var code = 0;
    region t {
        let q = alloc_slice[t](scratch_len(), 0);
        code = encrypt_block_with(nr, skey, input, out, q);
    }
    return code;
}

// Counter mode as GCM uses it (NIST SP 800-38D §6.5): the keystream is
// the encryption of `iv` (12 bytes) followed by a 32-bit big-endian
// counter, starting at `counter`; `input` XORed with it into `output`
// (the same length). Answers 0, or `refused_counter_exhausted()` when
// the counter would pass 2^32 - 1. Uses `q` (`scratch_len()` words),
// leaves it zero, and allocates nothing.
pub fn ctr32_with[&s, &v, &i, &o, &q](nr: int, skey: &s [int], iv: &v [byte], counter: int, input: &i [byte], output: &!o [byte], q: &!q [int]) -> [] int {
    let n = len(input);
    let blocks = (n + 15) / 16;
    if counter < 0 || counter + blocks - 1 > 0xffffffff {
        return refused_counter_exhausted();
    }
    if len(q) < scratch_len() {
        return refused_scratch_length();
    }
    let iv0 = le32(iv, 0);
    let iv1 = le32(iv, 4);
    let iv2 = le32(iv, 8);
    var cc = counter;
    var at = 0;
    while at < n {
        q[0] = iv0;
        q[1] = iv0;
        q[2] = iv1;
        q[3] = iv1;
        q[4] = iv2;
        q[5] = iv2;
        q[6] = swap32(cc);
        // The second block's counter may be one past the last one
        // used; it is computed, never used, and kept in 32 bits.
        q[7] = swap32(m32(cc + 1));
        ortho(q, 0);
        encrypt_sliced(nr, skey, q);
        ortho(q, 0);
        var k = 0;
        while k < 32 && at + k < n {
            output[at + k] = byte_of(int_of(input[at + k]) ^ byte_at(q, k));
            k = k + 1;
        }
        at = at + 32;
        cc = cc + 2;
    }
    var k = 0;
    while k < 8 {
        q[k] = 0;
        k = k + 1;
    }
    return 0;
}

// `ctr32_with`, with its scratch from a region of its own.
pub fn ctr32[&s, &v, &i, &o](nr: int, skey: &s [int], iv: &v [byte], counter: int, input: &i [byte], output: &!o [byte]) -> [] int {
    var code = 0;
    region t {
        let q = alloc_slice[t](scratch_len(), 0);
        code = ctr32_with(nr, skey, iv, counter, input, output, q);
    }
    return code;
}
