module std.hkdf;
import std.hmac;

// `std.hkdf` — HKDF (RFC 5869) and the TLS 1.3 key-schedule functions
// built on it, HKDF-Expand-Label and Derive-Secret (RFC 8446 §7.1).
// `docs/hkdf.md` is the design; this is part of sub-issue 4 (#201) of
// the pure TLS 1.3 client (#197). Not independently reviewed (#209).
//
// The hash is named by its digest length, as in `std.hmac`: 32 is
// SHA-256, 48 is SHA-384. Every function answers 0 or a negative code
// whose name is `refusal_tag(code)`; `std.hmac`'s codes pass through
// unchanged. The output length is always the length of the slice the
// caller passes, so there is no second number to disagree with it.

pub fn refused_too_long() -> [] int {
    return -4;
}

pub fn refused_prk_length() -> [] int {
    return -5;
}

pub fn refused_label_length() -> [] int {
    return -6;
}

pub fn refused_context_length() -> [] int {
    return -7;
}

pub fn refused_transcript_length() -> [] int {
    return -8;
}

// The stable name of a refusal code, `std.hmac`'s included.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == -4 {
        return "hkdf-length-too-large";
    }
    if code == -5 {
        return "hkdf-prk-length";
    }
    if code == -6 {
        return "hkdf-label-length";
    }
    if code == -7 {
        return "hkdf-context-length";
    }
    if code == -8 {
        return "hkdf-transcript-hash-length";
    }
    return hmac.refusal_tag(code);
}

// HKDF-Extract (RFC 5869 §2.2): `prk = HMAC(salt, ikm)`, into `prk`,
// which must be exactly `hash_len` bytes. An empty `salt` is
// `hash_len` zero bytes, as the RFC says.
pub fn extract[&s, &i, &o](hash_len: int, salt: &s [byte], ikm: &i [byte], prk: &!o [byte]) -> [] int {
    let checked = hmac.check_hash(hash_len);
    if checked != 0 {
        return checked;
    }
    if len(prk) != hash_len {
        return hmac.refused_output_length();
    }
    if len(salt) == 0 {
        region r {
            let zeros = alloc_slice[r](hash_len, byte_of(0));
            hmac.mac(hash_len, zeros, ikm, prk);
        }
        return 0;
    }
    return hmac.mac(hash_len, salt, ikm, prk);
}

// HKDF-Expand (RFC 5869 §2.3): `len(okm)` bytes of output keying
// material from `prk` and `info`. `T(i) = HMAC(prk, T(i-1) || info ||
// i)`, fed to the HMAC in its three pieces rather than concatenated, so
// `info` may be any length. More than `255 * hash_len` bytes is refused,
// as the RFC requires; so is a `prk` shorter than the hash.
pub fn expand[&p, &i, &o](hash_len: int, prk: &p [byte], info: &i [byte], okm: &!o [byte]) -> [] int {
    let checked = hmac.check_hash(hash_len);
    if checked != 0 {
        return checked;
    }
    let total = len(okm);
    if total > 255 * hash_len {
        return refused_too_long();
    }
    if len(prk) < hash_len {
        return refused_prk_length();
    }
    region r {
        let st = alloc_slice[r](hmac.state_len(hash_len), 0);
        let t = alloc_slice[r](hash_len, byte_of(0));
        let counter = alloc_slice[r](1, byte_of(0));
        var done = 0;
        var i = 1;
        while done < total {
            hmac.init(hash_len, st, prk);
            if i > 1 {
                hmac.update(hash_len, st, t);
            }
            hmac.update(hash_len, st, info);
            counter[0] = byte_of(i);
            hmac.update(hash_len, st, counter);
            hmac.final(hash_len, st, t);
            var j = 0;
            while j < hash_len && done < total {
                okm[done] = t[j];
                done = done + 1;
                j = j + 1;
            }
            i = i + 1;
        }
        var k = 0;
        while k < hash_len {
            t[k] = byte_of(0);
            k = k + 1;
        }
    }
    return 0;
}

// HKDF-Expand-Label (RFC 8446 §7.1): HKDF-Expand with `info` the
// `HkdfLabel` structure — the output length as two bytes, then
// `"tls13 " + label` and `context`, each behind a one-byte length.
// `label` must be 1 to 249 bytes (the structure's `<7..255>` with the
// six-byte prefix), `context` at most 255, and the output at most
// 65,535 bytes as well as `255 * hash_len`; each limit is refused with
// its own tag rather than truncated into a different label.
pub fn expand_label[&s, &l, &c, &o](hash_len: int, secret: &s [byte], label: &l [byte], context: &c [byte], okm: &!o [byte]) -> [] int {
    let checked = hmac.check_hash(hash_len);
    if checked != 0 {
        return checked;
    }
    let ll = len(label);
    if ll < 1 || ll > 249 {
        return refused_label_length();
    }
    let cl = len(context);
    if cl > 255 {
        return refused_context_length();
    }
    let total = len(okm);
    if total > 65535 {
        return refused_too_long();
    }
    var code = 0;
    region r {
        let info = alloc_slice[r](2 + 1 + 6 + ll + 1 + cl, byte_of(0));
        info[0] = byte_of(total >> 8);
        info[1] = byte_of(total & 0xff);
        info[2] = byte_of(6 + ll);
        let prefix = "tls13 ";
        var i = 0;
        while i < 6 {
            info[3 + i] = prefix[i];
            i = i + 1;
        }
        i = 0;
        while i < ll {
            info[9 + i] = label[i];
            i = i + 1;
        }
        info[9 + ll] = byte_of(cl);
        i = 0;
        while i < cl {
            info[10 + ll + i] = context[i];
            i = i + 1;
        }
        code = expand(hash_len, secret, info, okm);
    }
    return code;
}

// Derive-Secret (RFC 8446 §7.1): `HKDF-Expand-Label(secret, label,
// Transcript-Hash(messages), hash_len)`. The caller hashes the
// transcript (it is kept running across the handshake, so the caller
// owns that state) and passes the hash, which must be `hash_len` bytes;
// `out` must be `hash_len` bytes too.
pub fn derive_secret[&s, &l, &h, &o](hash_len: int, secret: &s [byte], label: &l [byte], transcript_hash: &h [byte], out: &!o [byte]) -> [] int {
    let checked = hmac.check_hash(hash_len);
    if checked != 0 {
        return checked;
    }
    if len(transcript_hash) != hash_len {
        return refused_transcript_length();
    }
    if len(out) != hash_len {
        return hmac.refused_output_length();
    }
    return expand_label(hash_len, secret, label, transcript_hash, out);
}
