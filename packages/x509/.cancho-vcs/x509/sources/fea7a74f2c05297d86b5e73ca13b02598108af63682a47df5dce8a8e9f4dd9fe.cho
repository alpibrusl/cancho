module x509;

// `x509` -- a strict DER reader and an X.509 v3 certificate parser
// (`docs/x509.md`). Sub-issue 5 (#202) of the self-contained TLS 1.3
// client (#197): `docs/tls-pure.md` §1 puts certificate code in a
// package, versioned apart from the compiler. Not independently
// reviewed (#209).
//
// Everything here reads public data (a certificate is sent in the
// clear), so there are no constant-time rules (`docs/tls-pure.md` §4.2).
// The rules are about input instead: no input reaches a trap, every
// refusal has its own code and tag, and nothing is allocated from a size
// the input names. A parsed certificate is a fixed `[int]` of offsets
// into its own DER (`view_len()` words, laid out in `docs/x509.md` §2),
// never a copy.
//
// No `import std`: nothing here needs it, and a package that imports
// `std` must be published with `--std` (`docs/package-system.md` §4.8).

// ---- Refusals (`docs/x509.md` §4) ----

pub fn ok() -> [] int {
    return 0;
}

// The stable name of a refusal code.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "der-truncated";
    }
    if code == -2 {
        return "der-trailing-bytes";
    }
    if code == -3 {
        return "der-indefinite-length";
    }
    if code == -4 {
        return "der-non-minimal-length";
    }
    if code == -5 {
        return "der-tag";
    }
    if code == -6 {
        return "der-too-deep";
    }
    if code == -7 {
        return "der-integer";
    }
    if code == -8 {
        return "der-boolean";
    }
    if code == -9 {
        return "der-bit-string";
    }
    if code == -10 {
        return "x509-structure";
    }
    if code == -11 {
        return "x509-version";
    }
    if code == -12 {
        return "x509-time";
    }
    if code == -13 {
        return "x509-signature-algorithm-mismatch";
    }
    if code == -14 {
        return "x509-duplicate-extension";
    }
    if code == -15 {
        return "x509-critical-extension";
    }
    if code == -16 {
        return "x509-too-large";
    }
    if code == -17 {
        return "x509-name";
    }
    if code == -18 {
        return "pem";
    }
    if code == -19 {
        return "x509-key";
    }
    return "unknown";
}

// ---- Limits (`docs/x509.md` §3) ----

// A certificate larger than this is refused before anything else is read
// (`docs/tls-pure.md` §5.2).
pub fn max_certificate() -> [] int {
    return 16384;
}

fn max_depth() -> [] int {
    return 24;
}

fn max_extensions() -> [] int {
    return 32;
}

fn max_san_entries() -> [] int {
    return 1024;
}

// ---- The view: where each part of a parsed certificate is ----
//
// Ranges are `[start, end)` byte offsets into the certificate's DER.
// `docs/x509.md` §2 is the table; these are its names.

pub fn view_len() -> [] int {
    return 40;
}

// The whole TBSCertificate TLV: the bytes the issuer signed.
pub fn tbs_start() -> [] int {
    return 0;
}

pub fn tbs_end() -> [] int {
    return 1;
}

// The certificate's signature algorithm (`oid_*` codes) and its
// parameters' range, and the signature's bits (after the unused-bits
// byte, which must be 0).
pub fn signature_algorithm() -> [] int {
    return 2;
}

pub fn signature_params_start() -> [] int {
    return 3;
}

pub fn signature_params_end() -> [] int {
    return 4;
}

pub fn signature_start() -> [] int {
    return 5;
}

pub fn signature_end() -> [] int {
    return 6;
}

// The serial number's content (a DER INTEGER, possibly non-positive in
// old roots: `docs/x509.md` §3.2).
pub fn serial_start() -> [] int {
    return 7;
}

pub fn serial_end() -> [] int {
    return 8;
}

// The issuer and subject Names, whole TLVs: compared byte for byte when
// a chain is built (#206).
pub fn issuer_start() -> [] int {
    return 9;
}

pub fn issuer_end() -> [] int {
    return 10;
}

pub fn subject_start() -> [] int {
    return 11;
}

pub fn subject_end() -> [] int {
    return 12;
}

// The validity period, in seconds since 1970 (UTC).
pub fn not_before() -> [] int {
    return 13;
}

pub fn not_after() -> [] int {
    return 14;
}

// The whole SubjectPublicKeyInfo TLV, the key's algorithm and curve
// (`oid_*` codes, 0 when not one of ours), and the key's bits.
pub fn spki_start() -> [] int {
    return 15;
}

pub fn spki_end() -> [] int {
    return 16;
}

pub fn key_algorithm() -> [] int {
    return 17;
}

pub fn key_curve() -> [] int {
    return 18;
}

pub fn key_start() -> [] int {
    return 19;
}

pub fn key_end() -> [] int {
    return 20;
}

// 1 or 3 (a v2 certificate is read as v3 without extensions).
pub fn version() -> [] int {
    return 21;
}

// basicConstraints: -1 absent, else 0 or 1; the path length, -1 for none.
pub fn is_ca() -> [] int {
    return 22;
}

pub fn path_len() -> [] int {
    return 23;
}

// keyUsage: -1 absent, else bit `i` is RFC 5280's named bit `i`
// (0 digitalSignature, 5 keyCertSign, ...).
pub fn key_usage() -> [] int {
    return 24;
}

// extendedKeyUsage: -1 absent, else `eku_*` flags.
pub fn ext_key_usage() -> [] int {
    return 25;
}

// subjectAltName's GeneralNames content, nameConstraints' content, the
// subject key identifier and the authority's key identifier: 0, 0 when
// absent.
pub fn san_start() -> [] int {
    return 26;
}

pub fn san_end() -> [] int {
    return 27;
}

pub fn name_constraints_start() -> [] int {
    return 28;
}

pub fn name_constraints_end() -> [] int {
    return 29;
}

pub fn ski_start() -> [] int {
    return 30;
}

pub fn ski_end() -> [] int {
    return 31;
}

pub fn aki_start() -> [] int {
    return 32;
}

pub fn aki_end() -> [] int {
    return 33;
}

pub fn extension_count() -> [] int {
    return 34;
}

// What was accepted that strict RFC 5280 would not (`docs/x509.md` §3.2):
// `lenient_*` flags.
pub fn leniency() -> [] int {
    return 35;
}

// For an RSA key, its modulus and exponent (INTEGER contents).
pub fn rsa_modulus_start() -> [] int {
    return 36;
}

pub fn rsa_modulus_end() -> [] int {
    return 37;
}

pub fn rsa_exponent_start() -> [] int {
    return 38;
}

pub fn rsa_exponent_end() -> [] int {
    return 39;
}

pub fn eku_server_auth() -> [] int {
    return 1;
}

pub fn eku_client_auth() -> [] int {
    return 2;
}

pub fn eku_any() -> [] int {
    return 4;
}

pub fn eku_other() -> [] int {
    return 8;
}

// A GeneralizedTime for a year before 2050 (RFC 5280 §4.1.2.5 says
// UTCTime); one of the 128 roots in this machine's bundle has one
// (`docs/x509.md` §5.1).
pub fn lenient_generalized_time() -> [] int {
    return 1;
}

// A BOOLEAN DEFAULT FALSE written out as FALSE (X.690 §11.5 says omit
// it). None of the 128 roots measured does, but other issuers have, and
// refusing it gains nothing (`docs/x509.md` §3.2).
pub fn lenient_explicit_default() -> [] int {
    return 2;
}

// ---- The DER reader ----
//
// One TLV at `at`, which must end at or before `end`: `out[0]` the tag,
// `out[1]` where the content starts, `out[2]` where it ends (where the
// next TLV starts). Strict DER (X.690 §10): a tag in one byte (the
// high-tag-number form is refused), a definite length in the fewest
// bytes (long form only from 128, no leading zero byte, at most four
// bytes), the content entirely inside `end`.
pub fn tlv[&d, &o](der: &d [byte], at: int, end: int, out: &!o [int]) -> [] int {
    if at < 0 || end > len(der) || at + 2 > end {
        return -1;
    }
    let tag = int_of(der[at]);
    if tag & 0x1f == 0x1f {
        return -5;
    }
    let first = int_of(der[at + 1]);
    var length = 0;
    var start = at + 2;
    if first < 0x80 {
        length = first;
    } else if first == 0x80 {
        return -3;
    } else {
        let n = first & 0x7f;
        if n > 4 {
            return -4;
        }
        if at + 2 + n > end {
            return -1;
        }
        if int_of(der[at + 2]) == 0 {
            return -4;
        }
        var i = 0;
        while i < n {
            length = length << 8 | int_of(der[at + 2 + i]);
            i = i + 1;
        }
        if length < 0x80 {
            return -4;
        }
        start = at + 2 + n;
    }
    if length > end - start {
        return -1;
    }
    out[0] = tag;
    out[1] = start;
    out[2] = start + length;
    return 0;
}

// `tlv`, and the tag must be `want`.
fn expect[&d, &o](der: &d [byte], at: int, end: int, want: int, out: &!o [int]) -> [] int {
    let code = tlv(der, at, end, out);
    if code != 0 {
        return code;
    }
    if out[0] != want {
        return -5;
    }
    return 0;
}

// Every TLV in `der[at..end]` is well-formed DER, recursively into
// constructed ones, at most `max_depth()` deep, and together they fill
// the range exactly. Primitive contents are checked by their readers.
fn well_formed[&d](der: &d [byte], at: int, end: int, depth: int) -> [] int {
    if depth > max_depth() {
        return -6;
    }
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        var p = at;
        while p < end && code == 0 {
            code = tlv(der, p, end, t);
            if code == 0 {
                if t[0] & 0x20 != 0 {
                    code = well_formed(der, t[1], t[2], depth + 1);
                }
                p = t[2];
            }
        }
    }
    return code;
}

// A DER INTEGER's content: at least one byte, and minimal (no leading
// 0x00 before a byte below 0x80, no leading 0xff before one above).
fn integer_ok[&d](der: &d [byte], s: int, e: int) -> [] int {
    if e <= s {
        return -7;
    }
    if e - s > 1 {
        let a = int_of(der[s]);
        let b = int_of(der[s + 1]);
        if a == 0 && b < 0x80 || a == 0xff && b >= 0x80 {
            return -7;
        }
    }
    return 0;
}

// A small non-negative INTEGER's value, or -1 when it is negative or
// does not fit in 31 bits.
fn small_integer[&d](der: &d [byte], s: int, e: int) -> [] int {
    if e - s > 4 || e <= s || int_of(der[s]) >= 0x80 {
        return -1;
    }
    var v = 0;
    var i = s;
    while i < e {
        v = v << 8 | int_of(der[i]);
        i = i + 1;
    }
    return v;
}

// A DER BOOLEAN's value: 1 for 0xff, 0 for 0x00, -8 for anything else.
fn boolean[&d](der: &d [byte], s: int, e: int) -> [] int {
    if e - s != 1 {
        return -8;
    }
    let v = int_of(der[s]);
    if v == 0xff {
        return 1;
    }
    if v == 0 {
        return 0;
    }
    return -8;
}

// ---- Object identifiers ----
//
// The OIDs this package recognises, as their DER content bytes: each
// entry is `code, length, bytes...`, and a 0 code ends the table.
// Generated from the dotted forms by a script and each checked against
// `openssl asn1parse -genstr` (`docs/x509.md` §2.3); not typed by hand.
static oid_table: [int] {
    let t = alloc_slice[static](275, 0);
    // 1: rsaEncryption 1.2.840.113549.1.1.1
    t[0] = 0x01;
    t[1] = 0x09;
    t[2] = 0x2a;
    t[3] = 0x86;
    t[4] = 0x48;
    t[5] = 0x86;
    t[6] = 0xf7;
    t[7] = 0x0d;
    t[8] = 0x01;
    t[9] = 0x01;
    t[10] = 0x01;
    // 2: sha1WithRSAEncryption 1.2.840.113549.1.1.5
    t[11] = 0x02;
    t[12] = 0x09;
    t[13] = 0x2a;
    t[14] = 0x86;
    t[15] = 0x48;
    t[16] = 0x86;
    t[17] = 0xf7;
    t[18] = 0x0d;
    t[19] = 0x01;
    t[20] = 0x01;
    t[21] = 0x05;
    // 3: sha256WithRSAEncryption 1.2.840.113549.1.1.11
    t[22] = 0x03;
    t[23] = 0x09;
    t[24] = 0x2a;
    t[25] = 0x86;
    t[26] = 0x48;
    t[27] = 0x86;
    t[28] = 0xf7;
    t[29] = 0x0d;
    t[30] = 0x01;
    t[31] = 0x01;
    t[32] = 0x0b;
    // 4: sha384WithRSAEncryption 1.2.840.113549.1.1.12
    t[33] = 0x04;
    t[34] = 0x09;
    t[35] = 0x2a;
    t[36] = 0x86;
    t[37] = 0x48;
    t[38] = 0x86;
    t[39] = 0xf7;
    t[40] = 0x0d;
    t[41] = 0x01;
    t[42] = 0x01;
    t[43] = 0x0c;
    // 5: sha512WithRSAEncryption 1.2.840.113549.1.1.13
    t[44] = 0x05;
    t[45] = 0x09;
    t[46] = 0x2a;
    t[47] = 0x86;
    t[48] = 0x48;
    t[49] = 0x86;
    t[50] = 0xf7;
    t[51] = 0x0d;
    t[52] = 0x01;
    t[53] = 0x01;
    t[54] = 0x0d;
    // 6: rsassa-pss 1.2.840.113549.1.1.10
    t[55] = 0x06;
    t[56] = 0x09;
    t[57] = 0x2a;
    t[58] = 0x86;
    t[59] = 0x48;
    t[60] = 0x86;
    t[61] = 0xf7;
    t[62] = 0x0d;
    t[63] = 0x01;
    t[64] = 0x01;
    t[65] = 0x0a;
    // 7: ecPublicKey 1.2.840.10045.2.1
    t[66] = 0x07;
    t[67] = 0x07;
    t[68] = 0x2a;
    t[69] = 0x86;
    t[70] = 0x48;
    t[71] = 0xce;
    t[72] = 0x3d;
    t[73] = 0x02;
    t[74] = 0x01;
    // 8: prime256v1 1.2.840.10045.3.1.7
    t[75] = 0x08;
    t[76] = 0x08;
    t[77] = 0x2a;
    t[78] = 0x86;
    t[79] = 0x48;
    t[80] = 0xce;
    t[81] = 0x3d;
    t[82] = 0x03;
    t[83] = 0x01;
    t[84] = 0x07;
    // 9: secp384r1 1.3.132.0.34
    t[85] = 0x09;
    t[86] = 0x05;
    t[87] = 0x2b;
    t[88] = 0x81;
    t[89] = 0x04;
    t[90] = 0x00;
    t[91] = 0x22;
    // 10: secp521r1 1.3.132.0.35
    t[92] = 0x0a;
    t[93] = 0x05;
    t[94] = 0x2b;
    t[95] = 0x81;
    t[96] = 0x04;
    t[97] = 0x00;
    t[98] = 0x23;
    // 11: ecdsa-with-SHA256 1.2.840.10045.4.3.2
    t[99] = 0x0b;
    t[100] = 0x08;
    t[101] = 0x2a;
    t[102] = 0x86;
    t[103] = 0x48;
    t[104] = 0xce;
    t[105] = 0x3d;
    t[106] = 0x04;
    t[107] = 0x03;
    t[108] = 0x02;
    // 12: ecdsa-with-SHA384 1.2.840.10045.4.3.3
    t[109] = 0x0c;
    t[110] = 0x08;
    t[111] = 0x2a;
    t[112] = 0x86;
    t[113] = 0x48;
    t[114] = 0xce;
    t[115] = 0x3d;
    t[116] = 0x04;
    t[117] = 0x03;
    t[118] = 0x03;
    // 13: ecdsa-with-SHA512 1.2.840.10045.4.3.4
    t[119] = 0x0d;
    t[120] = 0x08;
    t[121] = 0x2a;
    t[122] = 0x86;
    t[123] = 0x48;
    t[124] = 0xce;
    t[125] = 0x3d;
    t[126] = 0x04;
    t[127] = 0x03;
    t[128] = 0x04;
    // 14: Ed25519 1.3.101.112
    t[129] = 0x0e;
    t[130] = 0x03;
    t[131] = 0x2b;
    t[132] = 0x65;
    t[133] = 0x70;
    // 15: sha256 2.16.840.1.101.3.4.2.1
    t[134] = 0x0f;
    t[135] = 0x09;
    t[136] = 0x60;
    t[137] = 0x86;
    t[138] = 0x48;
    t[139] = 0x01;
    t[140] = 0x65;
    t[141] = 0x03;
    t[142] = 0x04;
    t[143] = 0x02;
    t[144] = 0x01;
    // 16: sha384 2.16.840.1.101.3.4.2.2
    t[145] = 0x10;
    t[146] = 0x09;
    t[147] = 0x60;
    t[148] = 0x86;
    t[149] = 0x48;
    t[150] = 0x01;
    t[151] = 0x65;
    t[152] = 0x03;
    t[153] = 0x04;
    t[154] = 0x02;
    t[155] = 0x02;
    // 17: sha512 2.16.840.1.101.3.4.2.3
    t[156] = 0x11;
    t[157] = 0x09;
    t[158] = 0x60;
    t[159] = 0x86;
    t[160] = 0x48;
    t[161] = 0x01;
    t[162] = 0x65;
    t[163] = 0x03;
    t[164] = 0x04;
    t[165] = 0x02;
    t[166] = 0x03;
    // 18: mgf1 1.2.840.113549.1.1.8
    t[167] = 0x12;
    t[168] = 0x09;
    t[169] = 0x2a;
    t[170] = 0x86;
    t[171] = 0x48;
    t[172] = 0x86;
    t[173] = 0xf7;
    t[174] = 0x0d;
    t[175] = 0x01;
    t[176] = 0x01;
    t[177] = 0x08;
    // 20: subjectKeyIdentifier 2.5.29.14
    t[178] = 0x14;
    t[179] = 0x03;
    t[180] = 0x55;
    t[181] = 0x1d;
    t[182] = 0x0e;
    // 21: keyUsage 2.5.29.15
    t[183] = 0x15;
    t[184] = 0x03;
    t[185] = 0x55;
    t[186] = 0x1d;
    t[187] = 0x0f;
    // 22: subjectAltName 2.5.29.17
    t[188] = 0x16;
    t[189] = 0x03;
    t[190] = 0x55;
    t[191] = 0x1d;
    t[192] = 0x11;
    // 23: basicConstraints 2.5.29.19
    t[193] = 0x17;
    t[194] = 0x03;
    t[195] = 0x55;
    t[196] = 0x1d;
    t[197] = 0x13;
    // 24: nameConstraints 2.5.29.30
    t[198] = 0x18;
    t[199] = 0x03;
    t[200] = 0x55;
    t[201] = 0x1d;
    t[202] = 0x1e;
    // 25: cRLDistributionPoints 2.5.29.31
    t[203] = 0x19;
    t[204] = 0x03;
    t[205] = 0x55;
    t[206] = 0x1d;
    t[207] = 0x1f;
    // 26: certificatePolicies 2.5.29.32
    t[208] = 0x1a;
    t[209] = 0x03;
    t[210] = 0x55;
    t[211] = 0x1d;
    t[212] = 0x20;
    // 27: authorityKeyIdentifier 2.5.29.35
    t[213] = 0x1b;
    t[214] = 0x03;
    t[215] = 0x55;
    t[216] = 0x1d;
    t[217] = 0x23;
    // 28: extKeyUsage 2.5.29.37
    t[218] = 0x1c;
    t[219] = 0x03;
    t[220] = 0x55;
    t[221] = 0x1d;
    t[222] = 0x25;
    // 29: policyConstraints 2.5.29.36
    t[223] = 0x1d;
    t[224] = 0x03;
    t[225] = 0x55;
    t[226] = 0x1d;
    t[227] = 0x24;
    // 30: inhibitAnyPolicy 2.5.29.54
    t[228] = 0x1e;
    t[229] = 0x03;
    t[230] = 0x55;
    t[231] = 0x1d;
    t[232] = 0x36;
    // 31: policyMappings 2.5.29.33
    t[233] = 0x1f;
    t[234] = 0x03;
    t[235] = 0x55;
    t[236] = 0x1d;
    t[237] = 0x21;
    // 32: authorityInfoAccess 1.3.6.1.5.5.7.1.1
    t[238] = 0x20;
    t[239] = 0x08;
    t[240] = 0x2b;
    t[241] = 0x06;
    t[242] = 0x01;
    t[243] = 0x05;
    t[244] = 0x05;
    t[245] = 0x07;
    t[246] = 0x01;
    t[247] = 0x01;
    // 40: serverAuth 1.3.6.1.5.5.7.3.1
    t[248] = 0x28;
    t[249] = 0x08;
    t[250] = 0x2b;
    t[251] = 0x06;
    t[252] = 0x01;
    t[253] = 0x05;
    t[254] = 0x05;
    t[255] = 0x07;
    t[256] = 0x03;
    t[257] = 0x01;
    // 41: clientAuth 1.3.6.1.5.5.7.3.2
    t[258] = 0x29;
    t[259] = 0x08;
    t[260] = 0x2b;
    t[261] = 0x06;
    t[262] = 0x01;
    t[263] = 0x05;
    t[264] = 0x05;
    t[265] = 0x07;
    t[266] = 0x03;
    t[267] = 0x02;
    // 42: anyExtendedKeyUsage 2.5.29.37.0
    t[268] = 0x2a;
    t[269] = 0x04;
    t[270] = 0x55;
    t[271] = 0x1d;
    t[272] = 0x25;
    t[273] = 0x00;
    t[274] = 0;
    return t;
}

// Whether a host name is to be read as an IP address: only digits and
// dots, or any colon. Shared by `x509_names` and `packages/tls`, which
// sends no SNI for an IP address (RFC 6066 §3).
pub fn is_ip_literal[&h](host: &h [byte]) -> [] bool {
    var digits_and_dots = true;
    var i = 0;
    while i < len(host) {
        let c = int_of(host[i]);
        if c == 58 {
            return true;
        }
        if c != 46 && (c < 48 || c > 57) {
            digits_and_dots = false;
        }
        i = i + 1;
    }
    return digits_and_dots;
}

// The code of the OID whose content is `der[s..e]`, or 0.
pub fn oid_code[&d](der: &d [byte], s: int, e: int) -> [] int {
    var i = 0;
    while oid_table[i] != 0 {
        let n = oid_table[i + 1];
        if n == e - s {
            var j = 0;
            while j < n && int_of(der[s + j]) == oid_table[i + 2 + j] {
                j = j + 1;
            }
            if j == n {
                return oid_table[i];
            }
        }
        i = i + 2 + n;
    }
    return 0;
}

pub fn oid_rsa_encryption() -> [] int {
    return 1;
}

pub fn oid_ec_public_key() -> [] int {
    return 7;
}

pub fn oid_p256() -> [] int {
    return 8;
}

pub fn oid_p384() -> [] int {
    return 9;
}

pub fn oid_p521() -> [] int {
    return 10;
}

pub fn oid_ed25519() -> [] int {
    return 14;
}

// ---- Times ----

fn digits[&d](der: &d [byte], at: int, n: int) -> [] int {
    var v = 0;
    var i = 0;
    while i < n {
        let c = int_of(der[at + i]);
        if c < 48 || c > 57 {
            return -1;
        }
        v = v * 10 + c - 48;
        i = i + 1;
    }
    return v;
}

fn days_in_month(y: int, m: int) -> [] int {
    if m == 2 {
        if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            return 29;
        }
        return 28;
    }
    if m == 4 || m == 6 || m == 9 || m == 11 {
        return 30;
    }
    return 31;
}

// Days from 1970-01-01 to `y-m-d` in the proleptic Gregorian calendar,
// for `y` from 1950 to 9999 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y0: int, m: int, d: int) -> [] int {
    var y = y0;
    if m <= 2 {
        y = y - 1;
    }
    let era = y / 400;
    let yoe = y - era * 400;
    var mp = m + 9;
    if m > 2 {
        mp = m - 3;
    }
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}

// A UTCTime (`YYMMDDHHMMSSZ`, RFC 5280 §4.1.2.5.1: YY below 50 is 20YY)
// or GeneralizedTime (`YYYYMMDDHHMMSSZ`, no fractions), as seconds since
// 1970, into `out[0]`; `out[1]` gets 1 for a GeneralizedTime before
// 2050. Every field is range-checked.
fn time_of[&d, &o](der: &d [byte], tag: int, s: int, e: int, out: &!o [int]) -> [] int {
    var y = 0;
    var at = s;
    out[1] = 0;
    if tag == 0x17 {
        if e - s != 13 {
            return -12;
        }
        y = digits(der, s, 2);
        if y < 0 {
            return -12;
        }
        if y < 50 {
            y = y + 2000;
        } else {
            y = y + 1900;
        }
        at = s + 2;
    } else if tag == 0x18 {
        if e - s != 15 {
            return -12;
        }
        y = digits(der, s, 4);
        if y < 0 {
            return -12;
        }
        if y < 2050 {
            out[1] = 1;
        }
        at = s + 4;
    } else {
        return -12;
    }
    let mo = digits(der, at, 2);
    let d = digits(der, at + 2, 2);
    let h = digits(der, at + 4, 2);
    let mi = digits(der, at + 6, 2);
    let se = digits(der, at + 8, 2);
    if int_of(der[at + 10]) != 90 || mo < 1 || mo > 12 || d < 1 || h < 0 || h > 23 || mi < 0 || mi > 59 || se < 0 || se > 59 {
        return -12;
    }
    if d > days_in_month(y, mo) {
        return -12;
    }
    out[0] = days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se;
    return 0;
}

// ---- Parts of a certificate ----

// An AlgorithmIdentifier SEQUENCE at `at`: `out[3]` its OID's code,
// `out[4]`/`out[5]` the parameters' range (empty when absent), `out[2]`
// where the SEQUENCE ends.
fn algorithm[&d, &o](der: &d [byte], at: int, end: int, out: &!o [int]) -> [] int {
    var code = expect(der, at, end, 0x30, out);
    if code != 0 {
        return code;
    }
    let seq_end = out[2];
    let oid_at = out[1];
    code = expect(der, oid_at, seq_end, 0x06, out);
    if code != 0 {
        return code;
    }
    let alg = oid_code(der, out[1], out[2]);
    let params = out[2];
    out[2] = seq_end;
    out[3] = alg;
    out[4] = params;
    out[5] = seq_end;
    return 0;
}

// A Name: a SEQUENCE of non-empty SETs of SEQUENCE { OID, value }.
fn name_ok[&d](der: &d [byte], s: int, e: int) -> [] int {
    var code = 0;
    region r {
        let set = alloc_slice[r](3, 0);
        let atv = alloc_slice[r](3, 0);
        let part = alloc_slice[r](3, 0);
        var p = s;
        while p < e && code == 0 {
            code = expect(der, p, e, 0x31, set);
            if code == 0 && set[1] == set[2] {
                code = -17;
            }
            var q = set[1];
            while code == 0 && q < set[2] {
                code = expect(der, q, set[2], 0x30, atv);
                if code == 0 {
                    code = expect(der, atv[1], atv[2], 0x06, part);
                }
                if code == 0 {
                    code = tlv(der, part[2], atv[2], part);
                }
                if code == 0 && part[2] != atv[2] {
                    code = -17;
                }
                q = atv[2];
            }
            p = set[2];
        }
    }
    if code == -5 || code == -1 {
        return -17;
    }
    return code;
}

// The SubjectPublicKeyInfo at `at` into the view.
fn public_key[&d, &v](der: &d [byte], at: int, end: int, view: &!v [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](6, 0);
        code = expect(der, at, end, 0x30, t);
        if code == 0 {
            view[spki_start()] = at;
            view[spki_end()] = t[2];
            let inner_end = t[2];
            code = algorithm(der, t[1], inner_end, t);
            let alg = t[3];
            let ps = t[4];
            let pe = t[5];
            if code == 0 {
                code = expect(der, t[2], inner_end, 0x03, t);
            }
            if code == 0 && (t[2] != inner_end || t[2] == t[1] || int_of(der[t[1]]) != 0) {
                code = -9;
            }
            if code == 0 {
                let ks = t[1] + 1;
                let ke = t[2];
                view[key_algorithm()] = alg;
                view[key_curve()] = 0;
                view[key_start()] = ks;
                view[key_end()] = ke;
                code = key_ok(der, alg, ps, pe, ks, ke, view, t);
            }
        }
    }
    return code;
}

// The parameters and key bits that each supported algorithm requires
// (RFC 3279, RFC 5480, RFC 8410). An algorithm this package does not
// know is parsed and left to the verifier to refuse.
fn key_ok[&d, &v, &t](der: &d [byte], alg: int, ps: int, pe: int, ks: int, ke: int, view: &!v [int], t: &!t [int]) -> [] int {
    if alg == oid_rsa_encryption() {
        // Parameters NULL; the key an RSAPublicKey { n, e }.
        if pe - ps != 2 || int_of(der[ps]) != 5 || int_of(der[ps + 1]) != 0 {
            return -19;
        }
        var code = expect(der, ks, ke, 0x30, t);
        if code != 0 || t[2] != ke {
            return -19;
        }
        let seq_end = t[2];
        code = expect(der, t[1], seq_end, 0x02, t);
        if code != 0 || integer_ok(der, t[1], t[2]) != 0 || int_of(der[t[1]]) >= 0x80 {
            return -19;
        }
        view[rsa_modulus_start()] = t[1];
        view[rsa_modulus_end()] = t[2];
        code = expect(der, t[2], seq_end, 0x02, t);
        if code != 0 || t[2] != seq_end || integer_ok(der, t[1], t[2]) != 0 || int_of(der[t[1]]) >= 0x80 {
            return -19;
        }
        view[rsa_exponent_start()] = t[1];
        view[rsa_exponent_end()] = t[2];
        return 0;
    }
    if alg == oid_ec_public_key() {
        // Parameters a named curve; the key an uncompressed point.
        let code = expect(der, ps, pe, 0x06, t);
        if code != 0 || t[2] != pe {
            return -19;
        }
        let curve = oid_code(der, t[1], t[2]);
        view[key_curve()] = curve;
        var want = 0;
        if curve == oid_p256() {
            want = 65;
        } else if curve == oid_p384() {
            want = 97;
        } else if curve == oid_p521() {
            want = 133;
        }
        if want != 0 && (ke - ks != want || int_of(der[ks]) != 4) {
            return -19;
        }
        return 0;
    }
    if alg == oid_ed25519() {
        if pe != ps || ke - ks != 32 {
            return -19;
        }
        return 0;
    }
    return 0;
}

// ---- Extensions ----

// keyUsage's BIT STRING as a mask: bit `i` is named bit `i` (the most
// significant bit of the first byte is bit 0). The unused bits must be
// zero (X.690 §11.2.1).
fn key_usage_bits[&d](der: &d [byte], s: int, e: int) -> [] int {
    if e - s < 2 || e - s > 3 {
        return -9;
    }
    let unused = int_of(der[s]);
    if unused > 7 {
        return -9;
    }
    let last = int_of(der[e - 1]);
    if last & (1 << unused) - 1 != 0 {
        return -9;
    }
    var mask = 0;
    var i = s + 1;
    var base = 0;
    while i < e {
        let b = int_of(der[i]);
        var k = 0;
        while k < 8 {
            if b >> 7 - k & 1 == 1 {
                mask = mask | 1 << base + k;
            }
            k = k + 1;
        }
        base = base + 8;
        i = i + 1;
    }
    return mask;
}

// GeneralNames (RFC 5280 §4.2.1.6): at least one entry, at most
// `max_san_entries()`; a dNSName is ASCII, an iPAddress 4 or 16 bytes.
fn general_names_ok[&d](der: &d [byte], s: int, e: int) -> [] int {
    var code = 0;
    var n = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        var p = s;
        while p < e && code == 0 {
            code = tlv(der, p, e, t);
            if code == 0 {
                n = n + 1;
                let tag = t[0];
                if tag == 0x82 || tag == 0x81 || tag == 0x86 {
                    var i = t[1];
                    while i < t[2] && code == 0 {
                        if int_of(der[i]) >= 0x80 {
                            code = -17;
                        }
                        i = i + 1;
                    }
                } else if tag == 0x87 {
                    if t[2] - t[1] != 4 && t[2] - t[1] != 16 {
                        code = -17;
                    }
                } else if tag != 0xa0 && tag != 0xa3 && tag != 0xa4 && tag != 0xa5 && tag != 0x88 {
                    code = -5;
                }
                p = t[2];
            }
        }
    }
    if code == 0 && n == 0 {
        return -10;
    }
    if code == 0 && n > max_san_entries() {
        return -16;
    }
    return code;
}

// One extension's value, `der[s..e]` (the OCTET STRING's content), for
// the extension coded `which`.
fn extension_value[&d, &v](der: &d [byte], which: int, s: int, e: int, view: &!v [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        if which == 23 {
            // basicConstraints { cA BOOLEAN DEFAULT FALSE, pathLen INTEGER OPTIONAL }
            code = expect(der, s, e, 0x30, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            var p = t[1];
            let seq_end = t[2];
            view[is_ca()] = 0;
            view[path_len()] = -1;
            if code == 0 && p < seq_end && int_of(der[p]) == 0x01 {
                code = tlv(der, p, seq_end, t);
                if code == 0 {
                    let b = boolean(der, t[1], t[2]);
                    if b < 0 {
                        code = b;
                    } else {
                        view[is_ca()] = b;
                        if b == 0 {
                            view[leniency()] = view[leniency()] | lenient_explicit_default();
                        }
                    }
                    p = t[2];
                }
            }
            if code == 0 && p < seq_end {
                code = expect(der, p, seq_end, 0x02, t);
                if code == 0 && (t[2] != seq_end || integer_ok(der, t[1], t[2]) != 0) {
                    code = -7;
                }
                if code == 0 {
                    view[path_len()] = small_integer(der, t[1], t[2]);
                    if view[path_len()] < 0 {
                        code = -7;
                    }
                }
            }
        } else if which == 21 {
            code = expect(der, s, e, 0x03, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            if code == 0 {
                let mask = key_usage_bits(der, t[1], t[2]);
                if mask < 0 {
                    code = mask;
                } else {
                    view[key_usage()] = mask;
                }
            }
        } else if which == 28 {
            code = expect(der, s, e, 0x30, t);
            if code == 0 && (t[2] != e || t[1] == t[2]) {
                code = -10;
            }
            var flags = 0;
            var p = t[1];
            let seq_end = t[2];
            while code == 0 && p < seq_end {
                code = expect(der, p, seq_end, 0x06, t);
                if code == 0 {
                    let o = oid_code(der, t[1], t[2]);
                    if o == 40 {
                        flags = flags | eku_server_auth();
                    } else if o == 41 {
                        flags = flags | eku_client_auth();
                    } else if o == 42 {
                        flags = flags | eku_any();
                    } else {
                        flags = flags | eku_other();
                    }
                    p = t[2];
                }
            }
            view[ext_key_usage()] = flags;
        } else if which == 22 {
            code = expect(der, s, e, 0x30, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            if code == 0 {
                code = general_names_ok(der, t[1], t[2]);
            }
            view[san_start()] = t[1];
            view[san_end()] = t[2];
        } else if which == 24 {
            code = expect(der, s, e, 0x30, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            view[name_constraints_start()] = t[1];
            view[name_constraints_end()] = t[2];
        } else if which == 20 {
            code = expect(der, s, e, 0x04, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            view[ski_start()] = t[1];
            view[ski_end()] = t[2];
        } else if which == 27 {
            code = expect(der, s, e, 0x30, t);
            if code == 0 && t[2] != e {
                code = -10;
            }
            if code == 0 && t[1] < t[2] && int_of(der[t[1]]) == 0x80 {
                code = tlv(der, t[1], t[2], t);
                view[aki_start()] = t[1];
                view[aki_end()] = t[2];
            }
        } else {
            // Not read here; its DER was checked by `well_formed`.
            code = 0;
        }
    }
    return code;
}

// Whether a critical extension coded `which` is one this package
// understands: keyUsage, subjectAltName, basicConstraints,
// nameConstraints and extendedKeyUsage. Anything else that is critical
// is refused (RFC 5280 §4.2).
fn understood(which: int) -> [] bool {
    return which == 21 || which == 22 || which == 23 || which == 24 || which == 28;
}

fn extensions[&d, &v](der: &d [byte], s: int, e: int, view: &!v [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        let seen_s = alloc_slice[r](32, 0);
        let seen_e = alloc_slice[r](32, 0);
        var n = 0;
        code = expect(der, s, e, 0x30, t);
        if code == 0 && (t[2] != e || t[1] == t[2]) {
            code = -10;
        }
        var p = t[1];
        let list_end = t[2];
        while code == 0 && p < list_end {
            code = expect(der, p, list_end, 0x30, t);
            let ext_end = t[2];
            let next = t[2];
            if code == 0 {
                code = expect(der, t[1], ext_end, 0x06, t);
            }
            let os = t[1];
            let oe = t[2];
            var critical = 0;
            if code == 0 && n >= max_extensions() {
                code = -16;
            }
            // A second extension with the same OID (RFC 5280 §4.2).
            var k = 0;
            while code == 0 && k < n {
                if seen_e[k] - seen_s[k] == oe - os {
                    var j = 0;
                    while j < oe - os && der[seen_s[k] + j] == der[os + j] {
                        j = j + 1;
                    }
                    if j == oe - os {
                        code = -14;
                    }
                }
                k = k + 1;
            }
            if code == 0 {
                seen_s[n] = os;
                seen_e[n] = oe;
                n = n + 1;
                code = tlv(der, oe, ext_end, t);
            }
            if code == 0 && t[0] == 0x01 {
                critical = boolean(der, t[1], t[2]);
                if critical < 0 {
                    code = critical;
                } else if critical == 0 {
                    view[leniency()] = view[leniency()] | lenient_explicit_default();
                }
                if code == 0 {
                    code = tlv(der, t[2], ext_end, t);
                }
            }
            if code == 0 && (t[0] != 0x04 || t[2] != ext_end) {
                code = -10;
            }
            if code == 0 {
                let which = oid_code(der, os, oe);
                if critical == 1 && !understood(which) {
                    code = -15;
                } else {
                    code = extension_value(der, which, t[1], t[2], view);
                }
            }
            p = next;
        }
        view[extension_count()] = n;
    }
    return code;
}

// ---- The certificate ----

// Parses the DER certificate `der` (exactly one, nothing after it) into
// `view`, which must hold `view_len()` words. Answers 0 or a refusal
// code; on a refusal the view's contents are not meaningful.
pub fn parse[&d, &v](der: &d [byte], view: &!v [int]) -> [] int {
    if len(view) < view_len() {
        return -10;
    }
    var i = 0;
    while i < view_len() {
        view[i] = 0;
        i = i + 1;
    }
    view[is_ca()] = -1;
    view[path_len()] = -1;
    view[key_usage()] = -1;
    view[ext_key_usage()] = -1;
    if len(der) > max_certificate() {
        return -16;
    }
    var code = 0;
    region r {
        let t = alloc_slice[r](6, 0);
        let tm = alloc_slice[r](2, 0);
        code = expect(der, 0, len(der), 0x30, t);
        if code == 0 && t[2] != len(der) {
            code = -2;
        }
        if code == 0 {
            code = well_formed(der, t[1], t[2], 1);
        }
        let cert_end = t[2];
        // TBSCertificate, then the outer AlgorithmIdentifier, then the signature.
        var p = t[1];
        if code == 0 {
            code = expect(der, p, cert_end, 0x30, t);
        }
        let tbs_s = p;
        let tbs_e = t[2];
        let tbs_content = t[1];
        view[tbs_start()] = tbs_s;
        view[tbs_end()] = tbs_e;
        var outer_s = tbs_e;
        var outer_e = tbs_e;
        if code == 0 {
            code = algorithm(der, tbs_e, cert_end, t);
            outer_e = t[2];
            view[signature_algorithm()] = t[3];
            view[signature_params_start()] = t[4];
            view[signature_params_end()] = t[5];
        }
        if code == 0 {
            code = expect(der, outer_e, cert_end, 0x03, t);
            if code == 0 && (t[2] != cert_end || t[2] == t[1] || int_of(der[t[1]]) != 0) {
                code = -9;
            }
            view[signature_start()] = t[1] + 1;
            view[signature_end()] = t[2];
        }
        // The TBSCertificate's fields, in order (RFC 5280 §4.1).
        p = tbs_content;
        view[version()] = 1;
        if code == 0 && p < tbs_e && int_of(der[p]) == 0xa0 {
            code = tlv(der, p, tbs_e, t);
            let after = t[2];
            if code == 0 {
                code = expect(der, t[1], t[2], 0x02, t);
            }
            if code == 0 && t[2] != after {
                code = -11;
            }
            if code == 0 {
                let v = small_integer(der, t[1], t[2]);
                if t[2] - t[1] != 1 || v < 1 || v > 2 {
                    code = -11;
                }
                view[version()] = v + 1;
            }
            p = after;
        }
        if code == 0 {
            code = expect(der, p, tbs_e, 0x02, t);
            if code == 0 {
                code = integer_ok(der, t[1], t[2]);
            }
            if code == 0 && t[2] - t[1] > 21 {
                code = -7;
            }
            view[serial_start()] = t[1];
            view[serial_end()] = t[2];
            p = t[2];
        }
        if code == 0 {
            // The signature algorithm inside TBS must be the outer one,
            // byte for byte (RFC 5280 §4.1.1.2).
            code = expect(der, p, tbs_e, 0x30, t);
            if code == 0 {
                outer_s = tbs_e;
                if t[2] - p != outer_e - outer_s {
                    code = -13;
                } else {
                    var j = 0;
                    while j < t[2] - p && code == 0 {
                        if der[p + j] != der[outer_s + j] {
                            code = -13;
                        }
                        j = j + 1;
                    }
                }
                p = t[2];
            }
        }
        if code == 0 {
            code = expect(der, p, tbs_e, 0x30, t);
            if code == 0 {
                code = name_ok(der, t[1], t[2]);
            }
            view[issuer_start()] = p;
            view[issuer_end()] = t[2];
            p = t[2];
        }
        if code == 0 {
            code = expect(der, p, tbs_e, 0x30, t);
            let validity_end = t[2];
            let first = t[1];
            if code == 0 {
                code = tlv(der, first, validity_end, t);
            }
            if code == 0 {
                code = time_of(der, t[0], t[1], t[2], tm);
                view[not_before()] = tm[0];
                if tm[1] == 1 {
                    view[leniency()] = view[leniency()] | lenient_generalized_time();
                }
            }
            if code == 0 {
                code = tlv(der, t[2], validity_end, t);
            }
            if code == 0 && t[2] != validity_end {
                code = -10;
            }
            if code == 0 {
                code = time_of(der, t[0], t[1], t[2], tm);
                view[not_after()] = tm[0];
                if tm[1] == 1 {
                    view[leniency()] = view[leniency()] | lenient_generalized_time();
                }
            }
            p = validity_end;
        }
        if code == 0 {
            code = expect(der, p, tbs_e, 0x30, t);
            if code == 0 {
                code = name_ok(der, t[1], t[2]);
            }
            view[subject_start()] = p;
            view[subject_end()] = t[2];
            p = t[2];
        }
        if code == 0 {
            code = tlv(der, p, tbs_e, t);
            let spki_e = t[2];
            if code == 0 {
                code = public_key(der, p, tbs_e, view);
            }
            p = spki_e;
        }
        // issuerUniqueID [1], subjectUniqueID [2]: v2 and v3 only.
        while code == 0 && p < tbs_e && (int_of(der[p]) == 0x81 || int_of(der[p]) == 0xa1 || int_of(der[p]) == 0x82 || int_of(der[p]) == 0xa2) {
            if view[version()] == 1 {
                code = -11;
            } else {
                code = tlv(der, p, tbs_e, t);
                p = t[2];
            }
        }
        if code == 0 && p < tbs_e {
            code = expect(der, p, tbs_e, 0xa3, t);
            if code == 0 && view[version()] != 3 {
                code = -11;
            }
            if code == 0 && t[2] != tbs_e {
                code = -10;
            }
            if code == 0 {
                code = extensions(der, t[1], t[2], view);
            }
            p = t[2];
        }
        if code == 0 && p != tbs_e {
            code = -10;
        }
    }
    return code;
}

// ---- PEM ----
//
// The next `-----BEGIN CERTIFICATE-----` block in `text` at or after
// `at`, base64-decoded into `out`. `info[0]` is the DER's length and
// `info[1]` where to look for the next block. Answers 0, 1 when there is
// no further block, -16 for a block that decodes to more bytes than
// `out` holds, or -18 for a block that is not well-formed (no END line,
// a character outside base64, a padding error). Line breaks and spaces
// inside the block are skipped. Once the END line is found, `info[1]` is
// past it even on a refusal, so a caller can go on to the next block;
// with no END line, `info[1]` is `len(text)`.
pub fn pem_next[&t, &o, &i](text: &t [byte], at: int, out: &!o [byte], info: &!i [int]) -> [] int {
    let begin = find(text, at, "-----BEGIN CERTIFICATE-----");
    if begin < 0 {
        return 1;
    }
    let body = begin + 27;
    let stop = find(text, body, "-----END CERTIFICATE-----");
    info[0] = 0;
    if stop < 0 {
        info[1] = len(text);
        return -18;
    }
    info[1] = stop + 25;
    var n = 0;
    var acc = 0;
    var bits = 0;
    var pad = 0;
    var p = body;
    while p < stop {
        let c = int_of(text[p]);
        var v = -1;
        if c >= 65 && c <= 90 {
            v = c - 65;
        } else if c >= 97 && c <= 122 {
            v = c - 71;
        } else if c >= 48 && c <= 57 {
            v = c + 4;
        } else if c == 43 {
            v = 62;
        } else if c == 47 {
            v = 63;
        } else if c == 61 {
            pad = pad + 1;
        } else if c != 10 && c != 13 && c != 32 && c != 9 {
            return -18;
        }
        if v >= 0 {
            if pad > 0 {
                return -18;
            }
            acc = acc << 6 & 0xffffff | v;
            bits = bits + 6;
            if bits >= 8 {
                bits = bits - 8;
                if n >= len(out) {
                    return -16;
                }
                out[n] = byte_of(acc >> bits & 0xff);
                n = n + 1;
            }
        }
        p = p + 1;
    }
    // What is left must be the zero bits padding explains.
    if pad > 2 || bits == 6 || acc & (1 << bits) - 1 != 0 || bits == 2 && pad != 1 || bits == 4 && pad != 2 || bits == 0 && pad != 0 {
        return -18;
    }
    info[0] = n;
    return 0;
}

fn find[&t, &n](text: &t [byte], from: int, needle: &n [byte]) -> [] int {
    var i = from;
    while i + len(needle) <= len(text) {
        var j = 0;
        while j < len(needle) && text[i + j] == needle[j] {
            j = j + 1;
        }
        if j == len(needle) {
            return i;
        }
        i = i + 1;
    }
    return -1;
}
