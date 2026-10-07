module std.hmac;
import std.crypto;

// `std.hmac` — HMAC (RFC 2104) over SHA-256 and SHA-384. `docs/hkdf.md`
// is the design; this is part of sub-issue 4 (#201) of the pure TLS 1.3
// client (#197), in a file of its own so it can move to a package
// without a rewrite. Not independently reviewed (#209).
//
// The hash is named by its digest length, `hash_len`: 32 is SHA-256, 48
// is SHA-384, and anything else is refused. That is the number the
// callers (HKDF, the TLS key schedule) already carry, so there is no
// second name for the same choice.
//
// The interface streams: `init` with the key, `update` with as many
// pieces as the message comes in, `final` for the tag. Nothing the size
// of the message is allocated, so a 64 KiB payload is no different from
// a 64-byte one (`docs/hkdf.md` §2).

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_hash() -> [] int {
    return -1;
}

pub fn refused_output_length() -> [] int {
    return -2;
}

pub fn refused_state_length() -> [] int {
    return -3;
}

// The stable name of a refusal code.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "hash-unsupported";
    }
    if code == -2 {
        return "hash-output-length";
    }
    if code == -3 {
        return "hmac-state-length";
    }
    return "unknown";
}

fn block_len(hash_len: int) -> [] int {
    if hash_len == 32 {
        return 64;
    }
    return 128;
}

fn hash_state_len(hash_len: int) -> [] int {
    if hash_len == 32 {
        return crypto.sha256_state_len();
    }
    return crypto.sha512_state_len();
}

// 0 for SHA-256 and SHA-384, the refusal otherwise.
pub fn check_hash(hash_len: int) -> [] int {
    if hash_len == 32 || hash_len == 48 {
        return ok();
    }
    return refused_hash();
}

// The length of an HMAC state for `hash_len`: the hash's own state,
// then the key block XOR `opad`, kept for `final`. 0 for a hash that is
// not supported.
pub fn state_len(hash_len: int) -> [] int {
    if check_hash(hash_len) != 0 {
        return 0;
    }
    return hash_state_len(hash_len) + block_len(hash_len);
}

fn hash_init[&st](hash_len: int, st: &!st [int]) -> [] int {
    if hash_len == 32 {
        return crypto.sha256_init(st);
    }
    return crypto.sha384_init(st);
}

fn hash_update[&st, &d](hash_len: int, st: &!st [int], data: &d [byte]) -> [] int {
    if hash_len == 32 {
        return crypto.sha256_update(st, data);
    }
    return crypto.sha384_update(st, data);
}

fn hash_final[&st, &o](hash_len: int, st: &!st [int], out: &!o [byte]) -> [] int {
    if hash_len == 32 {
        return crypto.sha256_final(st, out);
    }
    return crypto.sha384_final(st, out);
}

// Starts an HMAC of `key` into `state` (exactly `state_len(hash_len)`
// words). A key longer than the hash's block is hashed first (RFC 2104
// §2); a shorter one is padded with zeros.
pub fn init[&st, &k](hash_len: int, state: &!st [int], key: &k [byte]) -> [] int {
    if check_hash(hash_len) != 0 {
        return refused_hash();
    }
    if len(state) != state_len(hash_len) {
        return refused_state_length();
    }
    let hs = hash_state_len(hash_len);
    let bl = block_len(hash_len);
    region r {
        let k0 = alloc_slice[r](bl, byte_of(0));
        if len(key) > bl {
            hash_init(hash_len, state[0..hs]);
            hash_update(hash_len, state[0..hs], key);
            hash_final(hash_len, state[0..hs], k0);
        } else {
            var i = 0;
            while i < len(key) {
                k0[i] = key[i];
                i = i + 1;
            }
        }
        let pad = alloc_slice[r](bl, byte_of(0));
        var i = 0;
        while i < bl {
            let b = int_of(k0[i]);
            state[hs + i] = b ^ 0x5c;
            pad[i] = byte_of(b ^ 0x36);
            k0[i] = byte_of(0);
            i = i + 1;
        }
        hash_init(hash_len, state[0..hs]);
        hash_update(hash_len, state[0..hs], pad);
        // Best effort (`docs/hkdf.md` §3): the key's pads are secret.
        i = 0;
        while i < bl {
            pad[i] = byte_of(0);
            i = i + 1;
        }
    }
    return ok();
}

// Feeds the next piece of the message.
pub fn update[&st, &d](hash_len: int, state: &!st [int], data: &d [byte]) -> [] int {
    if check_hash(hash_len) != 0 {
        return refused_hash();
    }
    if len(state) != state_len(hash_len) {
        return refused_state_length();
    }
    hash_update(hash_len, state[0..hash_state_len(hash_len)], data);
    return ok();
}

// The tag, into `out[0..hash_len]`. The state is spent.
pub fn final[&st, &o](hash_len: int, state: &!st [int], out: &!o [byte]) -> [] int {
    if check_hash(hash_len) != 0 {
        return refused_hash();
    }
    if len(state) != state_len(hash_len) {
        return refused_state_length();
    }
    if len(out) < hash_len {
        return refused_output_length();
    }
    let hs = hash_state_len(hash_len);
    let bl = block_len(hash_len);
    region r {
        let inner = alloc_slice[r](hash_len, byte_of(0));
        hash_final(hash_len, state[0..hs], inner);
        let pad = alloc_slice[r](bl, byte_of(0));
        var i = 0;
        while i < bl {
            pad[i] = byte_of(state[hs + i]);
            i = i + 1;
        }
        hash_init(hash_len, state[0..hs]);
        hash_update(hash_len, state[0..hs], pad);
        hash_update(hash_len, state[0..hs], inner);
        hash_final(hash_len, state[0..hs], out);
        i = 0;
        while i < bl {
            pad[i] = byte_of(0);
            state[hs + i] = 0;
            i = i + 1;
        }
    }
    return ok();
}

// HMAC of `msg` under `key` in one call, into `out[0..hash_len]`.
pub fn mac[&k, &m, &o](hash_len: int, key: &k [byte], msg: &m [byte], out: &!o [byte]) -> [] int {
    if check_hash(hash_len) != 0 {
        return refused_hash();
    }
    if len(out) < hash_len {
        return refused_output_length();
    }
    region r {
        let st = alloc_slice[r](state_len(hash_len), 0);
        init(hash_len, st, key);
        update(hash_len, st, msg);
        final(hash_len, st, out);
    }
    return ok();
}

// HMAC-SHA256, into `out[0..32]`.
pub fn sha256[&k, &m, &o](key: &k [byte], msg: &m [byte], out: &!o [byte]) -> [] int {
    return mac(32, key, msg, out);
}

// HMAC-SHA384, into `out[0..48]`.
pub fn sha384[&k, &m, &o](key: &k [byte], msg: &m [byte], out: &!o [byte]) -> [] int {
    return mac(48, key, msg, out);
}
