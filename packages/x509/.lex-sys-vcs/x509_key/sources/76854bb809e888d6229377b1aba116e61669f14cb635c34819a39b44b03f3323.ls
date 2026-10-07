edition 6;
module x509_key;
import std.bytes;
import std.ecdh;
import x509;

// `x509_key` -- P-256 private keys from PEM, for the TLS server
// (`docs/ecdsa-sign.md` §4; `docs/tls-server.md` §4). Step 1 of
// `docs/tls-server.md` §8. Not independently reviewed (#209).
//
// Two formats, both unencrypted and both P-256 only:
// - `PRIVATE KEY`: PKCS#8's PrivateKeyInfo (RFC 5208; RFC 5958's v2
//   with its public key too), algorithm id-ecPublicKey with the named
//   curve prime256v1 (RFC 5480), holding a SEC 1 ECPrivateKey;
// - `EC PRIVATE KEY`: SEC 1's ECPrivateKey (RFC 5915), with its named
//   curve, as RFC 5915 §3 requires.
// Anything else is refused with its own tag. The DER is read with
// `x509.tlv`, the certificate parser's strict reader.
//
// Unlike the rest of this package, this reads a secret. The base64 is
// decoded without a branch or an index on a key character's value
// (§4.2): what branches is whether a character is base64 at all, which
// for a key's body is the file's layout, not its content. The DER's
// structure (its tags and lengths) is the format's, not the key's. The
// key itself is only copied, and the decoded DER is zeroed before the
// answer. This runs once, when a program loads its key, and nothing an
// attacker sends reaches it.
//
// Every function answers 0 or a negative code whose name is
// `refusal_tag(code)`.

pub fn ok() -> [] int {
    return 0;
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -80 {
        return "key-pem";
    }
    if code == -81 {
        return "key-encrypted";
    }
    if code == -82 {
        return "key-algorithm";
    }
    if code == -83 {
        return "key-curve";
    }
    if code == -84 {
        return "key-der";
    }
    if code == -85 {
        return "key-version";
    }
    if code == -86 {
        return "key-length";
    }
    if code == -87 {
        return "key-range";
    }
    if code == -88 {
        return "key-public-mismatch";
    }
    if code == -89 {
        return "key-size";
    }
    if code == -90 {
        return "key-buffer-length";
    }
    if code == -91 {
        return "key-certificate";
    }
    if code == -92 {
        return "key-certificate-mismatch";
    }
    return "unknown";
}

// The words `work` must hold: `std.ecdh`'s, for the public point.
pub fn work_len() -> [] int {
    return ecdh.work_len();
}

// The largest DER block read. A P-256 key is about 140 bytes; this
// leaves room for an RSA key up to 8,192 bits, so that one is refused
// as `key-algorithm` rather than for its size.
pub fn max_der() -> [] int {
    return 8192;
}

pub fn format_pkcs8() -> [] int {
    return 8;
}

pub fn format_sec1() -> [] int {
    return 1;
}

// ---- PEM ----

// -1 when lo <= c <= hi, else 0: no branch on `c`, and through the
// barrier so the optimiser cannot make the masks below branches
// (`docs/value-barrier.md` §4).
fn within(c: int, lo: int, hi: int) -> [] int {
    return value_barrier((c - lo | hi - c) >> 63 ^ 0 - 1);
}

// The six-bit value of base64 character `c`, or 256 or more when `c` is
// not one (RFC 4648 §4), computed the same way for every `c`.
fn base64_value(c: int) -> [] int {
    let upper = within(c, 65, 90);
    let lower = within(c, 97, 122);
    let digit = within(c, 48, 57);
    let plus = within(c, 43, 43);
    let slash = within(c, 47, 47);
    let v = upper & c - 65 | lower & c - 71 | digit & c + 4 | plus & 62 | slash & 63;
    let valid = upper | lower | digit | plus | slash;
    return v | (valid ^ 0 - 1) & 256;
}

// `text[from..to]` base64-decoded into `out`: the length, -80 for a
// character outside base64 and space, or bad padding, -89 when it
// decodes to more than `out` holds.
fn base64_decode[&t, &o](text: &t [byte], from: int, to: int, out: &!o [byte]) -> [] int {
    var n = 0;
    var acc = 0;
    var bits = 0;
    var pad = 0;
    var p = from;
    while p < to {
        let c = int_of(text[p]);
        let v = base64_value(c);
        if v < 256 {
            if pad > 0 {
                return -80;
            }
            acc = acc << 6 & 0xffffff | v;
            bits = bits + 6;
            if bits >= 8 {
                bits = bits - 8;
                if n >= len(out) {
                    return -89;
                }
                out[n] = byte_of(acc >> bits & 0xff);
                n = n + 1;
            }
        } else if c == 61 {
            pad = pad + 1;
        } else if c != 10 && c != 13 && c != 32 && c != 9 {
            return -80;
        }
        p = p + 1;
    }
    // What is left must be the zero bits the padding accounts for.
    if pad > 2 || bits == 6 || acc & (1 << bits) - 1 != 0 || bits == 2 && pad != 1 || bits == 4 && pad != 2 || bits == 0 && pad != 0 {
        return -80;
    }
    return n;
}

// Where `needle` first occurs in `text` at or after `from`, or -1.
fn find_from[&t, &n](text: &t [byte], from: int, needle: &n [byte]) -> [] int {
    let at = bytes.find(text[from..len(text)], needle);
    if at < 0 {
        return at;
    }
    return from + at;
}

// What a block's label says: a format, 0 for a block that is not a
// private key (`EC PARAMETERS`, a certificate), or a refusal.
fn label_kind[&l](label: &l [byte]) -> [] int {
    if bytes.equal(label, "PRIVATE KEY") {
        return format_pkcs8();
    }
    if bytes.equal(label, "EC PRIVATE KEY") {
        return format_sec1();
    }
    if bytes.equal(label, "ENCRYPTED PRIVATE KEY") {
        return -81;
    }
    if bytes.ends_with(label, "PRIVATE KEY") {
        // RSA, DSA, OPENSSH, ...: a key, but not one this reads.
        return -82;
    }
    return 0;
}

// The first private key block of `pem`: `info[0]` its format, `info[1]`
// and `info[2]` where its body starts and ends. 0, or a refusal.
fn find_block[&p, &i](pem: &p [byte], info: &!i [int]) -> [] int {
    var at = 0;
    while at < len(pem) {
        let begin = find_from(pem, at, "-----BEGIN ");
        if begin < 0 {
            return -80;
        }
        let label = begin + 11;
        let close = find_from(pem, label, "-----");
        if close < 0 {
            return -80;
        }
        let kind = label_kind(pem[label..close]);
        let body = close + 5;
        // The END line names the same label.
        let end = find_from(pem, body, "-----END ");
        if end < 0 {
            return -80;
        }
        let end_label = end + 9;
        if end_label + (close - label) + 5 > len(pem) || !bytes.equal(pem[end_label..end_label + (close - label)], pem[label..close]) || !bytes.equal(pem[end_label + (close - label)..end_label + (close - label) + 5], "-----") {
            return -80;
        }
        if kind < 0 {
            return kind;
        }
        if kind > 0 {
            // RFC 1421's headers: a traditional encrypted key says so.
            if find_from(pem[0..end], body, "ENCRYPTED") >= 0 {
                return -81;
            }
            info[0] = kind;
            info[1] = body;
            info[2] = end;
            return 0;
        }
        at = end_label;
    }
    return -80;
}

// The private key in `pem` (the first `PRIVATE KEY` or `EC PRIVATE KEY`
// block; others, such as `EC PARAMETERS`, are skipped) into `key` (32
// bytes, big-endian) and its public point into `point` (65 bytes, `04
// || x || y`). `work` holds `work_len()` words.
pub fn parse_pem[&p, &k, &q, &w](pem: &p [byte], key: &!k [byte], point: &!q [byte], work: &!w [int]) -> [] int {
    if len(key) != 32 || len(point) != 65 {
        return -90;
    }
    var code = 0;
    region r {
        let info = alloc_slice[r](3, 0);
        code = find_block(pem, info);
        if code == 0 {
            let der = alloc_slice[r](max_der(), byte_of(0));
            let n = base64_decode(pem, info[1], info[2], der);
            if n < 0 {
                code = n;
            } else {
                code = parse_der(info[0], der[0..n], key, point, work);
            }
            bytes.zero(der);
        }
    }
    return code;
}

// ---- DER ----

// The TLV at `at` (`x509.tlv`) with tag `want`, ending by `end`; -84
// for anything else.
fn expect[&d, &t](der: &d [byte], at: int, end: int, want: int, t: &!t [int]) -> [] int {
    if x509.tlv(der, at, end, t) != 0 || t[0] != want {
        return -84;
    }
    return 0;
}

// The one-byte INTEGER at `at`, its value; -84 when it is not an
// INTEGER, -85 when it is not one byte (a version is 0 or 1).
fn version[&d, &t](der: &d [byte], at: int, end: int, t: &!t [int]) -> [] int {
    if expect(der, at, end, 0x02, t) != 0 {
        return -84;
    }
    if t[2] - t[1] != 1 || int_of(der[t[1]]) > 1 {
        return -85;
    }
    return int_of(der[t[1]]);
}

// A named curve OID at `at` filling `[at, end)`: 0 for P-256, -83 for
// anything else (another curve, or explicit parameters).
fn named_p256[&d, &t](der: &d [byte], at: int, end: int, t: &!t [int]) -> [] int {
    if x509.tlv(der, at, end, t) != 0 || t[0] != 0x06 || t[2] != end {
        return -83;
    }
    if x509.oid_code(der, t[1], t[2]) != x509.oid_p256() {
        return -83;
    }
    return 0;
}

// A BIT STRING public key in `der[s..e]` (its content): `00` and an
// uncompressed point. Its start in `der`, or -84.
fn bit_string_point[&d](der: &d [byte], s: int, e: int) -> [] int {
    if e - s != 66 || int_of(der[s]) != 0 || int_of(der[s + 1]) != 4 {
        return -84;
    }
    return s + 1;
}

// SEC 1's ECPrivateKey filling `der[at..end]` (RFC 5915 §3). `named`
// says the curve was named outside it (PKCS#8), so its own [0] may be
// left out. info[0] is where the private key's bytes start, info[1]
// where they end, and info[2] where its public key starts, or -1.
fn ec_private_key[&d, &i](der: &d [byte], at: int, end: int, named: bool, info: &!i [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        code = expect(der, at, end, 0x30, t);
        if code == 0 && t[2] != end {
            code = -84;
        }
        var p = t[1];
        if code == 0 {
            let v = version(der, p, end, t);
            if v < 0 {
                code = v;
            } else if v != 1 {
                code = -85;
            }
            p = t[2];
        }
        if code == 0 {
            code = expect(der, p, end, 0x04, t);
        }
        info[0] = t[1];
        info[1] = t[2];
        info[2] = -1;
        p = t[2];
        var curve = named;
        if code == 0 && p < end && int_of(der[p]) == 0xa0 {
            code = expect(der, p, end, 0xa0, t);
            if code == 0 {
                let stop = t[2];
                code = named_p256(der, t[1], stop, t);
                p = stop;
                curve = true;
            }
        }
        if code == 0 && !curve {
            code = -83;
        }
        // After the curve, so a P-384 key is refused for its curve.
        if code == 0 && (info[1] == info[0] || info[1] - info[0] > 32) {
            code = -86;
        }
        if code == 0 && p < end && int_of(der[p]) == 0xa1 {
            code = expect(der, p, end, 0xa1, t);
            if code == 0 {
                let stop = t[2];
                code = expect(der, t[1], stop, 0x03, t);
                if code == 0 && t[2] != stop {
                    code = -84;
                }
                if code == 0 {
                    let q = bit_string_point(der, t[1], t[2]);
                    if q < 0 {
                        code = q;
                    }
                    info[2] = q;
                }
                p = stop;
            }
        }
        if code == 0 && p != end {
            code = -84;
        }
    }
    return code;
}

// PKCS#8's PrivateKeyInfo filling `der` (RFC 5208 §5, RFC 5958 §2):
// info as `ec_private_key` answers it.
fn private_key_info[&d, &i](der: &d [byte], info: &!i [int]) -> [] int {
    let end = len(der);
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        code = expect(der, 0, end, 0x30, t);
        if code == 0 && t[2] != end {
            code = -84;
        }
        var p = t[1];
        if code == 0 {
            let v = version(der, p, end, t);
            if v < 0 {
                code = v;
            }
            p = t[2];
        }
        // AlgorithmIdentifier { id-ecPublicKey, namedCurve }.
        if code == 0 {
            code = expect(der, p, end, 0x30, t);
        }
        if code == 0 {
            let alg_end = t[2];
            p = alg_end;
            code = expect(der, t[1], alg_end, 0x06, t);
            if code == 0 && x509.oid_code(der, t[1], t[2]) != x509.oid_ec_public_key() {
                code = -82;
            }
            if code == 0 {
                code = named_p256(der, t[2], alg_end, t);
            }
        }
        if code == 0 {
            code = expect(der, p, end, 0x04, t);
        }
        if code == 0 {
            let inner_end = t[2];
            code = ec_private_key(der, t[1], inner_end, true, info);
            p = inner_end;
        }
        // [0] attributes, then v2's [1] publicKey, each optional.
        if code == 0 && p < end && int_of(der[p]) == 0xa0 {
            code = expect(der, p, end, 0xa0, t);
            p = t[2];
        }
        if code == 0 && p < end && int_of(der[p]) == 0x81 {
            code = expect(der, p, end, 0x81, t);
            if code == 0 {
                let q = bit_string_point(der, t[1], t[2]);
                if q < 0 {
                    code = q;
                } else if info[2] < 0 {
                    info[2] = q;
                } else if !bytes.equal(der[q..q + 65], der[info[2]..info[2] + 65]) {
                    code = -88;
                }
                p = t[2];
            }
        }
        if code == 0 && p != end {
            code = -84;
        }
    }
    return code;
}

// The private key in `der`, in `format` (`format_pkcs8()` or
// `format_sec1()`), into `key` and its public point into `point`, as
// `parse_pem` answers them. A public key the file carries must be the
// key's own (`key-public-mismatch`).
pub fn parse_der[&d, &k, &q, &w](format: int, der: &d [byte], key: &!k [byte], point: &!q [byte], work: &!w [int]) -> [] int {
    if len(key) != 32 || len(point) != 65 {
        return -90;
    }
    if format != format_pkcs8() && format != format_sec1() {
        return -82;
    }
    var code = 0;
    region r {
        let info = alloc_slice[r](3, 0);
        if format == format_pkcs8() {
            code = private_key_info(der, info);
        } else {
            code = ec_private_key(der, 0, len(der), false, info);
        }
        if code == 0 {
            // Left-padded to 32 bytes: OpenSSL before 1.1.0 wrote a key
            // with a leading zero byte one byte short.
            bytes.zero(key);
            let n = info[1] - info[0];
            var i = 0;
            while i < n {
                key[32 - n + i] = der[info[0] + i];
                i = i + 1;
            }
            code = public_point(key, point, work);
        }
        if code == 0 && info[2] >= 0 && !bytes.equal(der[info[2]..info[2] + 65], point) {
            code = -88;
        }
        if code != 0 {
            bytes.zero(key);
        }
    }
    return code;
}

// The public point of the private key `key` (32 bytes, big-endian):
// key·G by `std.ecdh`'s constant-time ladder, into `point` as `04 || x
// || y`. -87 when the key is 0 or not below n.
pub fn public_point[&k, &q, &w](key: &k [byte], point: &!q [byte], work: &!w [int]) -> [] int {
    if len(key) != 32 || len(point) != 65 {
        return -90;
    }
    if len(work) < work_len() {
        return -90;
    }
    let code = ecdh.public_key(256, key, point, work);
    if code == ecdh.refused_scalar_range() {
        return -87;
    }
    return code;
}

// Whether the certificate `cert` (DER) is for the key whose public point
// is `point` (`docs/tls-server.md` §4): 0 when its key is that P-256
// point; -91 when it does not parse or its key is not P-256; -92 when
// it is another P-256 key.
pub fn matches_certificate[&c, &q](cert: &c [byte], point: &q [byte]) -> [] int {
    var code = 0;
    region r {
        let view = alloc_slice[r](x509.view_len(), 0);
        if x509.parse(cert, view) != 0 {
            code = -91;
        } else if view[x509.key_algorithm()] != x509.oid_ec_public_key() || view[x509.key_curve()] != x509.oid_p256() {
            code = -91;
        } else if !bytes.equal(cert[view[x509.key_start()]..view[x509.key_end()]], point) {
            code = -92;
        }
    }
    return code;
}
