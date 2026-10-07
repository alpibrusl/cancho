module std.rsa;
import std.bigmod;
import std.crypto;

// `std.rsa` -- RSA signature verification: RSASSA-PKCS1-v1_5 and
// RSASSA-PSS (RFC 8017 §8.2.2 and §8.1.2) over SHA-256, SHA-384 and
// SHA-512 (`docs/rsa.md` §3). Sub-issue 6 (#203) of the pure TLS 1.3
// client (#197). Not independently reviewed (#209).
//
// Verification only, on public data, so nothing here is constant time.
// The hash is named by its digest length, as in `std.hmac`. The modulus
// and exponent are big-endian bytes, as `packages/x509` locates them
// (leading zero bytes allowed). Every function answers 0 for a valid
// signature or a negative code whose name is `refusal_tag(code)`.

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_modulus_size() -> [] int {
    return -10;
}

pub fn refused_even_modulus() -> [] int {
    return -11;
}

pub fn refused_exponent() -> [] int {
    return -12;
}

// The longest public exponent verified, in bits (`docs/rsa.md` §3.1).
pub fn max_exponent_bits() -> [] int {
    return 64;
}

pub fn refused_hash() -> [] int {
    return -13;
}

pub fn refused_digest_length() -> [] int {
    return -14;
}

pub fn refused_signature_length() -> [] int {
    return -15;
}

pub fn refused_signature_range() -> [] int {
    return -16;
}

pub fn refused_pkcs1_mismatch() -> [] int {
    return -17;
}

pub fn refused_pss_length() -> [] int {
    return -18;
}

pub fn refused_pss_trailer() -> [] int {
    return -19;
}

pub fn refused_pss_top_bits() -> [] int {
    return -20;
}

pub fn refused_pss_padding() -> [] int {
    return -21;
}

pub fn refused_pss_mismatch() -> [] int {
    return -22;
}

// The stable name of a refusal code, `std.bigmod`'s included.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == -10 {
        return "rsa-modulus-size";
    }
    if code == -11 {
        return "rsa-even-modulus";
    }
    if code == -12 {
        return "rsa-exponent";
    }
    if code == -13 {
        return "rsa-hash";
    }
    if code == -14 {
        return "rsa-digest-length";
    }
    if code == -15 {
        return "rsa-signature-length";
    }
    if code == -16 {
        return "rsa-signature-range";
    }
    if code == -17 {
        return "rsa-pkcs1-mismatch";
    }
    if code == -18 {
        return "rsa-pss-length";
    }
    if code == -19 {
        return "rsa-pss-trailer";
    }
    if code == -20 {
        return "rsa-pss-top-bits";
    }
    if code == -21 {
        return "rsa-pss-padding";
    }
    if code == -22 {
        return "rsa-pss-mismatch";
    }
    return bigmod.refusal_tag(code);
}

pub fn min_bits() -> [] int {
    return 2048;
}

// `std.bigmod`'s working space, which both verifiers need.
pub fn work_len() -> [] int {
    return bigmod.work_len();
}

fn hash_ok(hash_len: int) -> [] bool {
    return hash_len == 32 || hash_len == 48 || hash_len == 64;
}

fn hash[&m, &o](hash_len: int, msg: &m [byte], out: &!o [byte]) -> [] int {
    if hash_len == 32 {
        return crypto.sha256(msg, out);
    }
    if hash_len == 48 {
        return crypto.sha384(msg, out);
    }
    return crypto.sha512(msg, out);
}

// The checks on the key, the hash and the signature's length that both
// verifiers share (`docs/rsa.md` §3.1). 0, or the refusal.
fn check[&n, &e, &d, &s](hash_len: int, n: &n [byte], e: &e [byte], digest: &d [byte], sig: &s [byte]) -> [] int {
    let bits = bigmod.bit_length(n);
    if bits < min_bits() || bits > bigmod.max_bits() {
        return -10;
    }
    if int_of(n[len(n) - 1]) & 1 == 0 {
        return -11;
    }
    let ebits = bigmod.bit_length(e);
    // At most 64 bits (#317): verification costs a multiplication or two
    // a bit of `e`, and the server chooses both the key and the chain, so
    // an unbounded exponent lets it choose the client's CPU time (a
    // 4,095-bit one is about 250 times 65537's). OpenSSL's bound for
    // large moduli; the Web PKI uses 65537.
    if ebits < 2 || int_of(e[len(e) - 1]) & 1 == 0 || ebits >= bits || ebits > max_exponent_bits() {
        return -12;
    }
    if !hash_ok(hash_len) {
        return -13;
    }
    if len(digest) != hash_len {
        return -14;
    }
    if len(sig) != (bits + 7) / 8 {
        return -15;
    }
    return 0;
}

// m = sig^e mod n, as `len(n)` bytes into `m`.
fn open[&n, &e, &s, &m, &w](n: &n [byte], e: &e [byte], sig: &s [byte], m: &!m [byte], work: &!w [int]) -> [] int {
    let code = bigmod.pow_mod(n, e, sig, m, work);
    if code == bigmod.refused_not_reduced() {
        return -16;
    }
    return code;
}

// The DER DigestInfo prefix for SHA-2 (RFC 8017 §9.2 note 1) into
// `out[at..at + 19]`. The three differ only in the outer length, the
// OID's last arc and the digest's length:
//     30 (17 + h) 30 0d 06 09 60 86 48 01 65 03 04 02 (h - 16) / 16 05 00 04 h
fn digest_info[&o](hash_len: int, out: &!o [byte], at: int) -> [] int {
    out[at] = byte_of(0x30);
    out[at + 1] = byte_of(17 + hash_len);
    out[at + 2] = byte_of(0x30);
    out[at + 3] = byte_of(0x0d);
    out[at + 4] = byte_of(0x06);
    out[at + 5] = byte_of(0x09);
    out[at + 6] = byte_of(0x60);
    out[at + 7] = byte_of(0x86);
    out[at + 8] = byte_of(0x48);
    out[at + 9] = byte_of(0x01);
    out[at + 10] = byte_of(0x65);
    out[at + 11] = byte_of(0x03);
    out[at + 12] = byte_of(0x04);
    out[at + 13] = byte_of(0x02);
    out[at + 14] = byte_of((hash_len - 16) / 16);
    out[at + 15] = byte_of(0x05);
    out[at + 16] = byte_of(0x00);
    out[at + 17] = byte_of(0x04);
    out[at + 18] = byte_of(hash_len);
    return 0;
}

// RSASSA-PKCS1-v1_5 (`docs/rsa.md` §3.2): the expected block is built
// and compared with the opened one in full; nothing is parsed.
pub fn pkcs1_verify[&n, &e, &d, &s, &w](hash_len: int, n: &n [byte], e: &e [byte], digest: &d [byte], sig: &s [byte], work: &!w [int]) -> [] int {
    var code = check(hash_len, n, e, digest, sig);
    if code != 0 {
        return code;
    }
    let k = len(sig);
    region r {
        let m = alloc_slice[r](len(n), byte_of(0));
        let want = alloc_slice[r](len(n), byte_of(0));
        code = open(n, e, sig, m, work);
        if code == 0 {
            // 00 01 FF..FF 00 || DigestInfo || digest, right-aligned in
            // `len(n)` bytes (the bytes before the last `k` stay 0).
            let at = len(n) - k;
            let t = at + k - hash_len - 19;
            want[at + 1] = byte_of(1);
            var i = at + 2;
            while i < t - 1 {
                want[i] = byte_of(0xff);
                i = i + 1;
            }
            digest_info(hash_len, want, t);
            i = 0;
            while i < hash_len {
                want[t + 19 + i] = digest[i];
                i = i + 1;
            }
            i = 0;
            while i < len(n) && code == 0 {
                if m[i] != want[i] {
                    code = -17;
                }
                i = i + 1;
            }
        }
    }
    return code;
}

// MGF1 (RFC 8017 §B.2.1) over `seed` with the hash `hash_len`, XORed into
// `out`.
fn mgf1_xor[&s, &o](hash_len: int, seed: &s [byte], out: &!o [byte]) -> [] int {
    region r {
        let block = alloc_slice[r](len(seed) + 4, byte_of(0));
        let mask = alloc_slice[r](hash_len, byte_of(0));
        var i = 0;
        while i < len(seed) {
            block[i] = seed[i];
            i = i + 1;
        }
        var counter = 0;
        var done = 0;
        while done < len(out) {
            block[len(seed)] = byte_of(counter >> 24 & 255);
            block[len(seed) + 1] = byte_of(counter >> 16 & 255);
            block[len(seed) + 2] = byte_of(counter >> 8 & 255);
            block[len(seed) + 3] = byte_of(counter & 255);
            hash(hash_len, block, mask);
            var j = 0;
            while j < hash_len && done < len(out) {
                out[done] = byte_of(int_of(out[done]) ^ int_of(mask[j]));
                j = j + 1;
                done = done + 1;
            }
            counter = counter + 1;
        }
    }
    return 0;
}

// RSASSA-PSS (`docs/rsa.md` §3.3): EMSA-PSS-VERIFY with MGF1 over
// `mgf_hash_len` and a salt of exactly `salt_len` bytes. TLS 1.3's
// `rsa_pss_rsae_*` schemes are `pss_verify(h, h, h, ...)`.
pub fn pss_verify[&n, &e, &d, &s, &w](hash_len: int, mgf_hash_len: int, salt_len: int, n: &n [byte], e: &e [byte], digest: &d [byte], sig: &s [byte], work: &!w [int]) -> [] int {
    var code = check(hash_len, n, e, digest, sig);
    if code != 0 {
        return code;
    }
    if !hash_ok(mgf_hash_len) {
        return -13;
    }
    let em_bits = bigmod.bit_length(n) - 1;
    let em_len = (em_bits + 7) / 8;
    if salt_len < 0 || em_len < hash_len + salt_len + 2 {
        return -18;
    }
    region r {
        let m = alloc_slice[r](len(n), byte_of(0));
        code = open(n, e, sig, m, work);
        // EM is the last `em_len` bytes; anything before them must be 0
        // (m < 2^emBits, step 6 below covers the bits inside EM).
        let at = len(n) - em_len;
        var i = 0;
        while code == 0 && i < at {
            if int_of(m[i]) != 0 {
                code = -20;
            }
            i = i + 1;
        }
        if code == 0 && int_of(m[len(n) - 1]) != 0xbc {
            code = -19;
        }
        let db_len = em_len - hash_len - 1;
        let top = 8 * em_len - em_bits;
        if code == 0 && int_of(m[at]) >> 8 - top != 0 {
            code = -20;
        }
        if code == 0 {
            let db = alloc_slice[r](db_len, byte_of(0));
            let h = alloc_slice[r](hash_len, byte_of(0));
            i = 0;
            while i < db_len {
                db[i] = m[at + i];
                i = i + 1;
            }
            i = 0;
            while i < hash_len {
                h[i] = m[at + db_len + i];
                i = i + 1;
            }
            mgf1_xor(mgf_hash_len, h, db);
            db[0] = byte_of(int_of(db[0]) & 255 >> top);
            // DB = 00..00 01 || salt.
            let ps = db_len - salt_len - 1;
            i = 0;
            while i < ps && code == 0 {
                if int_of(db[i]) != 0 {
                    code = -21;
                }
                i = i + 1;
            }
            if code == 0 && int_of(db[ps]) != 1 {
                code = -21;
            }
            if code == 0 {
                // H' = Hash(00 x 8 || mHash || salt).
                let mp = alloc_slice[r](8 + hash_len + salt_len, byte_of(0));
                let hp = alloc_slice[r](hash_len, byte_of(0));
                i = 0;
                while i < hash_len {
                    mp[8 + i] = digest[i];
                    i = i + 1;
                }
                i = 0;
                while i < salt_len {
                    mp[8 + hash_len + i] = db[ps + 1 + i];
                    i = i + 1;
                }
                hash(hash_len, mp, hp);
                i = 0;
                while i < hash_len && code == 0 {
                    if hp[i] != h[i] {
                        code = -22;
                    }
                    i = i + 1;
                }
            }
        }
    }
    return code;
}
