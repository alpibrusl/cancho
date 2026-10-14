#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Mutation check of `std.rsa_sign`, `x509_rsa_key` and `std.bigmod`'s secret-modulus functions (docs/rsa-sign.md §11, gate G5).

    python3 scripts/rsa_sign_mutants.py <cancho binary> <keydir>

Each mutant is one of `std/bigmod.cho`, `std/rsa_sign.cho` or `packages/x509/rsa_key.cho` with ONE plausible bug. The three are built
as local modules (`bigmod`, `rsa_sign`, `x509_rsa_key`) beside a copy of `tests/programs/rsa_sign_driver.cho`, `x509.cho` and `key.cho`
and run against the evidence of `conformance/rsa_sign.rs`, as exact answers:
  NIST SigGen15 (60, through the PEM parser), our own known answers (24), the malformed-key corpus (every file's parse rule and every
  signature), every refusal of the signer, EMSA, the fault gate (modes 2, 4, 8, 32 refused, unchecked faults differ and `std.rsa` refuses
  them), the blinded exponents of both halves for three entropies against dp + kappa (p - 1), and `std.bigmod`'s Z, R and Y against Python
  integers (moduli of 8, 16, 128 and 256 bytes; the refusals of `setup_secret`);
plus `scripts/rsa_sign_differential.py sign` against OpenSSL on <keydir>'s 2,048-bit keys, 3 messages a key.
A mutant is killed when any answer differs. The unmutated files must pass first. A survivor is printed, not hidden: it is either a gap in
the evidence (and a case is added) or equivalent (and why is written next to it). Needs `openssl`. Exit status 1 on a survivor or a mutant
that does not build.
"""
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import rsa_sign_lib as L  # noqa: E402

# (module, name, the text replaced (exactly once), its replacement)
MUTANTS = [
    # ---- std.bigmod ----
    ("bigmod", "setup_secret accepts an even modulus", "    if int_of(n[len(n) - 1]) & 1 == 0 {\n        return -1;\n    }\n    let k = len(n) / 8;", "    let k = len(n) / 8;"),
    ("bigmod", "setup_secret accepts a clear top bit", "    if int_of(n[0]) < 128 {\n        return -2;\n    }\n    if int_of(n[len(n) - 1]) & 1 == 0 {\n        return -1;", "    if int_of(n[0]) < 0 {\n        return -2;\n    }\n    if int_of(n[len(n) - 1]) & 1 == 0 {\n        return -1;"),
    ("bigmod", "setup_secret accepts a length that is not a multiple of 8", "len(n) > max_bits() / 8 || len(n) % 8 != 0 {", "len(n) > max_bits() / 8 {"),
    ("bigmod", "r_squared's doubling is not reduced", "        ct_reduce(w, two, k);\n        d = d - 1;", "        d = d - 1;"),
    ("bigmod", "r_squared doubles once too few", "var d = 64 * k + 1 - (bits - 1);", "var d = 64 * k - (bits - 1);"),
    ("bigmod", "reduce_once does nothing", "    return ct_reduce(w, a, w[0]);\n}\n\n// `dst` (a register's offset)", "    return 0;\n}\n\n// `dst` (a register's offset)"),
    ("bigmod", "select's index off by one", "let m = value_barrier((i ^ idx) - 1 >> 63);", "let m = value_barrier((i ^ (idx + 1)) - 1 >> 63);"),
    ("bigmod", "select never reads the last entry", "    var i = 0;\n    while i < count {", "    var i = 0;\n    while i < count - 1 {"),
    ("bigmod", "pow_secret's table stops at 15", "    var i = 2;\n    while i < 16 {", "    var i = 2;\n    while i < 15 {"),
    ("bigmod", "pow_secret squares 3 times a window", "        mont_mul(w, dst, dst, dst);\n        mont_mul(w, dst, dst, dst);\n        mont_mul(w, dst, dst, dst);\n        mont_mul(w, dst, dst, dst);\n        let at", "        mont_mul(w, dst, dst, dst);\n        mont_mul(w, dst, dst, dst);\n        mont_mul(w, dst, dst, dst);\n        let at"),
    ("bigmod", "pow_secret drops the top window", "var wi = nwin - 1;", "var wi = nwin - 2;"),
    ("bigmod", "pow_secret reads a window's bits in the wrong order", "bit(w, exp, at + 1) << 1 | bit(w, exp, at + 2) << 2", "bit(w, exp, at + 1) << 2 | bit(w, exp, at + 2) << 1"),
    ("bigmod", "blind_exponent uses n, not n - 1", "            nm = wrapping_sub(nm, 1);", "            nm = nm;"),
    ("bigmod", "blind_exponent drops the carry limb", "    out[k] = carry;", "    out[k] = 0;"),
    # ---- std.rsa_sign ----
    ("rsa_sign", "EMSA: 01 for 00 before the DigestInfo", "    out[t - 1] = byte_of(0);", "    out[t - 1] = byte_of(1);"),
    ("rsa_sign", "EMSA: block type 02", "    out[1] = byte_of(1);", "    out[1] = byte_of(2);"),
    ("rsa_sign", "SHA-1's length accepted", "    return hash_len == 32 || hash_len == 48 || hash_len == 64;", "    return hash_len == 20 || hash_len == 32 || hash_len == 48 || hash_len == 64;"),
    ("rsa_sign", "EMSA does not check the digest's length", "    if len(digest) != hash_len {\n        return -141;\n    }\n    if len(out)", "    if len(out)"),
    ("rsa_sign", "17 bytes of entropy accepted", "if blinded && len(entropy) != 16 {", "if blinded && len(entropy) < 16 {"),
    ("rsa_sign", "the exponent is not blinded", "    if blind {\n        bigmod.blind_exponent", "    if blind && false {\n        bigmod.blind_exponent"),
    ("rsa_sign", "kappa's top bit not forced", "    return v | 1 << 63;", "    return v;"),
    ("rsa_sign", "the q-half blinded with the p-half's entropy", "kq = kappa_at(entropy, 8);", "kq = kappa_at(entropy, 0);"),
    ("rsa_sign", "the blinded exponent loses its top window", "        nwin = 16 * (k + 1);", "        nwin = 16 * (k + 1) - 1;"),
    ("rsa_sign", "verify-after-sign never runs", "        if code == 0 && mode & 1 == 0 {", "        if code == 0 && mode & 1 == 0 && false {"),
    ("rsa_sign", "the verdict of the check is ignored", "digest, sig, wq[0..rsa.work_len()]) != 0 {", "digest, sig, wq[0..rsa.work_len()]) != 0 && false {"),
    ("rsa_sign", "the check is made under e shifted right by a byte", "rsa.pkcs1_verify(hash_len, n, key[l..l + 8], digest", "rsa.pkcs1_verify(hash_len, n, key[l..l + 7], digest"),
    ("rsa_sign", "Garner: sq not reduced mod p", "    bigmod.reduce_once(w, r_sq());\n    bigmod.sub(w, r_sp(), r_sq(), r_diff());", "    bigmod.sub(w, r_sp(), r_sq(), r_diff());"),
    ("rsa_sign", "Garner: sq - sp, not sp - sq", "bigmod.sub(w, r_sp(), r_sq(), r_diff());", "bigmod.sub(w, r_sq(), r_sp(), r_diff());"),
    ("rsa_sign", "Garner: qinv not in Montgomery form", "    bigmod.to_mont(w, r_qi(), r_qi());\n", ""),
    ("rsa_sign", "recombination drops sq", "                add = d[i];", "                add = 0;"),
    ("rsa_sign", "the hi half of EM loses its last byte", "bigmod.load_reduced(em[0..len(em) - l], w, bigmod.reg(3)) != 0", "bigmod.load_reduced(em[0..len(em) - l - 1], w, bigmod.reg(3)) != 0"),
    ("rsa_sign", "the lo half of EM never added", "    bigmod.add(w, bigmod.reg(3), bigmod.reg(4), bigmod.reg(3));\n", ""),
    ("rsa_sign", "the q-half uses dp", "code = crt_half(q, em, dq, blinded, kq, sq, wq);", "code = crt_half(q, em, dp, blinded, kq, sq, wq);"),
    ("rsa_sign", "the modulus's top bit not checked", "    if int_of(key[0]) < 128 {\n        return -146;", "    if int_of(key[0]) < 0 {\n        return -146;"),
    ("rsa_sign", "any h from 64 accepted", "if h < 128 || h > 256 || h % 8 != 0 {", "if h < 64 || h > 256 || h % 8 != 0 {"),
    ("rsa_sign", "h need not be a multiple of 8", "if h < 128 || h > 256 || h % 8 != 0 {", "if h < 128 || h > 256 {"),
    ("rsa_sign", "check_key: the product is not checked", "        if cmp_limbs(prod, n, 2 * k8) != 0 {", "        if false {"),
    ("rsa_sign", "check_key: e dp = 1 mod p - 1 not checked", "&& times_is_one(e, x, pm1, k);", "&& true;"),
    ("rsa_sign", "check_key: dp's range not checked", "ok = cmp_limbs(x, pm1, k) < 0 && times_is_one", "ok = times_is_one"),
    ("rsa_sign", "check_key: dq not checked against d", " || !d_matches(dl, 2 * k8, dql, ql, k8) {", " {"),
    ("rsa_sign", "check_key: dp not checked against d", "|| !d_matches(dl, 2 * k8, dpl, pl, k8)", ""),
    ("rsa_sign", "check_key: d not below n", "if cmp_limbs(dl, n, 2 * k8) >= 0 || !d_matches", "if !d_matches"),
    ("rsa_sign", "check_key: d may be longer than n", "            if len(d) > l {", "            if false {"),
    ("rsa_sign", "check_key: q qinv = 1 not checked", "    if !bigmod.equal(w, bigmod.reg(2), bigmod.reg(3)) {", "    if false {"),
    ("rsa_sign", "check_key: qinv's range not checked", "cmp_limbs(qil, pl, k8) >= 0 {", "false {"),
    ("rsa_sign", "check_key: p may equal q", " || bytes.equal(p, q) {", " {"),
    ("rsa_sign", "check_key: e = 1 allowed", "bigmod.bit_length(key[l..l + 8]) < 2 {", "bigmod.bit_length(key[l..l + 8]) < 1 {"),
    ("rsa_sign", "check_key: an even e allowed", "    if int_of(key[l + 7]) & 1 == 0 || bigmod.bit_length", "    if bigmod.bit_length"),
    ("rsa_sign", "check_key: an even n allowed", "    if int_of(key[l - 1]) & 1 == 0 {\n        return -151;\n    }\n", ""),
    ("rsa_sign", "mod_limbs stops one step early", "var steps = 64 * (nx - nd) + 1;", "var steps = 64 * (nx - nd);"),
    # ---- x509_rsa_key ----
    ("rsa_key", "a negative INTEGER read as positive", "    if first >= 128 {\n        return negative;\n    }\n", ""),
    ("rsa_key", "a non-minimal INTEGER accepted", "if first == 0 && second < 128 || first == 255 && second >= 128 {", "if false {"),
    ("rsa_key", "00 00 accepted (FF FF still refused)", "first == 0 && second < 128 ||", "first == 0 && second < 0 ||"),
    ("rsa_key", "FF FF accepted (00 00 still refused)", "first == 0 && second < 128 || first == 255 && second >= 128 {", "first == 0 && second < 128 {"),
    ("rsa_key", "trailing bytes after the PKCS#1 key accepted", "        code = expect(der, at, end, 0x30, t);\n        if code == 0 && t[2] != end {\n            code = -173;\n        }\n        var p = t[1];\n        let stop = t[2];", "        code = expect(der, at, end, 0x30, t);\n        var p = t[1];\n        let stop = t[2];"),
    ("rsa_key", "an element after the last integer accepted", "        if code == 0 && p != stop {\n            code = -173;\n        }", ""),
    ("rsa_key", "another PRIVATE KEY label skipped", "    if bytes.ends_with(label, \"PRIVATE KEY\") {", "    if bytes.ends_with(label, \"PRIVATE KEY\") && false {"),
    ("rsa_key", "a traditional encrypted key not recognised", "x509_key.find_from(pem[0..end], body, \"ENCRYPTED\") >= 0 {", "x509_key.find_from(pem[0..end], body, \"ENCRYPTED\") >= 1000000 {"),
    ("rsa_key", "the END label not compared", " || !bytes.equal(pem[end_label..end_label + width], pem[label..close])", ""),
    ("rsa_key", "version 1 is a bad version, not multi-prime", "            } else if int_of(der[t[1]]) == 1 {\n                code = -175;", "            } else if int_of(der[t[1]]) == 1 {\n                code = -174;"),
    ("rsa_key", "PKCS#1 version 2 accepted", "            } else if int_of(der[t[1]]) != 0 {\n                code = -174;\n            }", "            }"),
    ("rsa_key", "PKCS#8 version not checked", "if code == 0 && (t[2] - t[1] != 1 || int_of(der[t[1]]) != 0) {", "if code == 0 && (t[2] - t[1] != 1) {"),
    ("rsa_key", "PKCS#8's NULL may be followed by another element", "if t[2] + 2 != alg_end || int_of(der[t[2]]) != 0x05 || int_of(der[t[2] + 1]) != 0 {", "if int_of(der[t[2]]) != 0x05 || int_of(der[t[2] + 1]) != 0 {"),
    ("rsa_key", "PKCS#8's parameters need not be a NULL tag", "if t[2] + 2 != alg_end || int_of(der[t[2]]) != 0x05 || int_of(der[t[2] + 1]) != 0 {", "if t[2] + 2 != alg_end || int_of(der[t[2] + 1]) != 0 {"),
    ("rsa_key", "PKCS#8's NULL may have a length", "if t[2] + 2 != alg_end || int_of(der[t[2]]) != 0x05 || int_of(der[t[2] + 1]) != 0 {", "if t[2] + 2 != alg_end || int_of(der[t[2]]) != 0x05 {"),
    ("rsa_key", "PKCS#8's algorithm not checked", "if code == 0 && x509.oid_code(der, t[1], t[2]) != x509.oid_rsa_encryption() {", "if code == 0 && false {"),
    ("rsa_key", "an element after PKCS#8's key accepted", "            if inner_end != end {\n                code = -173;\n            } else {", "            if false {\n                code = -173;\n            } else {"),
    ("rsa_key", "an over-long block is another rule", "            } else if n == -89 {\n                code = -178;", "            } else if n == -89 {\n                code = -170;"),
    ("rsa_key", "bad base64 is not rsa-key-pem", "            if n == -80 {\n                code = -170;", "            if n == -80 {\n                code = -173;"),
    ("rsa_key", "n of 264 bytes accepted", "nlen < 256 || nlen > 512 || nlen % 16 != 0 {", "nlen < 256 || nlen > 512 {"),
    ("rsa_key", "n of 128 bytes accepted", "nlen < 256 || nlen > 512 || nlen % 16 != 0 {", "nlen < 128 || nlen > 512 || nlen % 16 != 0 {"),
    ("rsa_key", "e of 9 bytes accepted", "put(der, info[2], info[3], key[l..l + 8]) != 0 {", "put(der, info[2], info[3], key[l..l + 8]) != 0 && false {"),
    ("rsa_key", "a negative p is another rule", "            } else if f == 3 || f == 4 {\n                negative = -153;", "            } else if f == 3 || f == 4 {\n                negative = -154;"),
    ("rsa_key", "a negative dp is another rule", "            } else if f == 5 || f == 6 {\n                negative = -155;", "            } else if f == 5 || f == 6 {\n                negative = -156;"),
    ("rsa_key", "a negative d is another rule", "            } else if f == 2 {\n                negative = -157;", "            } else if f == 2 {\n                negative = -155;"),
    ("rsa_key", "a negative qinv is another rule", "    var negative = -156;", "    var negative = -155;"),
    ("rsa_key", "a format other than 8 or 2 read as PKCS#1", "    } else {\n        code = -172;\n    }\n    if code < 0 {", "    } else {\n        code = rsa_private_key(der, 0, len(der), key, work);\n    }\n    if code < 0 {"),
]

EVIDENCE_SEED = 20261014


def evidence(keydir):
    ent = "000102030405060708090a0b0c0d0e0f"
    pairs = []

    def add(case, want):
        pairs.append((case, want))

    pem = None
    for line in open(os.path.join(ROOT, "tests/vectors/rsa_sign/nist_siggen15.txt")):
        f = line.split()
        if f and f[0] == "K":
            pem = f[2]
        elif f and f[0] == "M":
            add(f"G {pem} {f[1]} {f[2]} {ent}", f"0 ok {f[3]}")
    blocks = {}
    for line in open(os.path.join(ROOT, "tests/vectors/rsa_sign/valid.txt")):
        f = line.split()
        if f and f[0] == "K":
            blocks[f[1]] = f[4]
        elif f and f[0] == "M":
            add(f"M {f[2]} {blocks[f[1]]} {f[3]} {ent}", f"0 ok {f[4]}")
    msg = b"rsa-sign corpus".hex()
    for line in open(os.path.join(ROOT, "tests/vectors/rsa_sign/malformed.txt")):
        if line.startswith("#"):
            continue
        parse, sign, name, sig, data = line.split()
        if parse == "ok":
            add(f"P {data}", lambda a: a.split()[1] == "ok" and int(a.split()[0]) > 0)
            add(f"G {data} 32 {msg} {ent}", f"0 ok {sig}" if sign == "ok" else (lambda t: lambda a: a.split()[1] == t)(sign))
        else:
            add(f"P {data}", (lambda t: lambda a: a.split()[1] == t and int(a.split()[0]) < 0)(parse))
    # the refusals, from the first 2,048-bit block
    blk = bytes.fromhex(blocks["k2048_e65537_0"])
    d = bytes([0x5a] * 32).hex()
    key = blk.hex()

    def edit(at, mask=None, xor=None):
        b = bytearray(blk)
        if mask is not None:
            b[at] &= mask
        if xor is not None:
            b[at] ^= xor
        return b.hex()

    for case, want in [
        (f"S 33 {key} {d} {ent}", "-140 rsa-sign-hash -"), (f"S 32 {key} {bytes(31).hex()} {ent}", "-141 rsa-sign-digest-length -"),
        (f"S 32 {blk[:-1].hex()} {d} {ent}", "-142 rsa-sign-key-layout -"), (f"S 32 {(blk + b'0').hex()} {d} {ent}", "-142 rsa-sign-key-layout -"),
        (f"S 32 {(b'0' * (7 * 120 + 8)).hex()} {d} {ent}", "-142 rsa-sign-key-layout -"), (f"S 32 {(b'0' * (7 * 64 + 8)).hex()} {d} {ent}", "-142 rsa-sign-key-layout -"), (f"S 32 {(b'0' * (7 * 132 + 8)).hex()} {d} {ent}", "-142 rsa-sign-key-layout -"),
        (f"S 32 {key} {d} {bytes(15).hex()}", "-143 rsa-sign-entropy -"), (f"S 32 {key} {d} {bytes(17).hex()}", "-143 rsa-sign-entropy -"), (f"S 32 {key} {d} -", "-143 rsa-sign-entropy -"),
        (f"O 32 {key} {d} {ent} 255", "-144 rsa-sign-output-length -"), (f"O 32 {key} {d} {ent} 257", "-144 rsa-sign-output-length -"),
        (f"W 32 {key} {d} {ent}", "-145 rsa-sign-work-length -"),
        (f"S 32 {edit(264, mask=0x7f)} {d} {ent}", "-146 rsa-sign-key-shape -"), (f"S 32 {edit(392, mask=0x7f)} {d} {ent}", "-146 rsa-sign-key-shape -"),
        (f"S 32 {edit(391, mask=0xfe)} {d} {ent}", "-146 rsa-sign-key-shape -"), (f"S 32 {edit(519, mask=0xfe)} {d} {ent}", "-146 rsa-sign-key-shape -"),
        (f"S 32 {edit(0, mask=0x7f)} {d} {ent}", "-146 rsa-sign-key-shape -"),
        (f"S 32 {edit(255, mask=0xfe)} {d} {ent}", "-147 rsa-sign-check -"), (f"S 32 {edit(900, xor=1)} {d} {ent}", "-147 rsa-sign-check -"),
        (f"S 32 {edit(100, xor=1)} {d} {ent}", "-147 rsa-sign-check -"), (f"S 32 {edit(560, xor=1)} {d} {ent}", "-147 rsa-sign-check -"),
        (f"E 20 {bytes(20).hex()} 256", "-140 rsa-sign-hash -"), (f"E 32 {bytes(31).hex()} 256", "-141 rsa-sign-digest-length -"),
        (f"E 32 {bytes(32).hex()} 61", "-144 rsa-sign-output-length -"), (f"B 7 3000", "-172 rsa-key-algorithm -"),
    ]:
        add(case, want)
    prefix = {32: "3031300d060960864801650304020105000420", 48: "3041300d060960864801650304020205000430", 64: "3051300d060960864801650304020305000440"}
    for h in (32, 48, 64):
        dg = bytes(range(h))
        em = b"\x00\x01" + b"\xff" * (256 - 3 - 19 - h) + b"\x00" + bytes.fromhex(prefix[h]) + dg
        add(f"E {h} {dg.hex()} 256", f"0 ok {em.hex()}")
    # the fault gate and blinding
    good = f"0 ok {L.py_sign(L.load_json(os.path.join(keydir, 'k2048_e65537_0.json')), 32, bytes([0x5a]) * 32).hex()}"
    add(f"S 32 {key} {d} {ent}", good)
    for mode in (2, 4, 8, 32):
        add(f"U 32 {key} {d} {ent} {mode}", "-147 rsa-sign-check -")
        add(f"U 32 {key} {d} {ent} {mode | 1}", lambda a, g=good: a.startswith("0 ok ") and a != g)
    add(f"U 32 {key} {d} {ent} 1", good)
    add(f"U 32 {key} {d} - 16", good)
    add(f"U 32 {key} {d} - 17", good)
    rng = random.Random(EVIDENCE_SEED)
    for name in ("k2048_e65537_0", "k4096_e65537_0", "k2048_e65537_upq"):
        k = L.load_json(os.path.join(keydir, name + ".json"))
        blk2 = L.block(k).hex()
        h = (k["p"].bit_length() + 7) // 8
        for _ in range(3):
            e = rng.randbytes(16)
            kp, kq = int.from_bytes(e[:8], "big") | 1 << 63, int.from_bytes(e[8:], "big") | 1 << 63
            for mode, v in ((64, k["dp"] + kp * (k["p"] - 1)), (128, k["dq"] + kq * (k["q"] - 1))):
                add(f"U 32 {blk2} {d} {e.hex()} {mode}", f"0 ok {v:0{2 * (h + 16)}x}" + "00" * (h - 16))
        add(f"U 32 {blk2} {d} - {64 | 16}", f"0 ok {k['dp']:0{2 * (h + 16)}x}" + "00" * (h - 16))
    # std.bigmod
    def modulus(nb):
        return rng.getrandbits(8 * nb) | (1 << (8 * nb - 1)) | 1

    for nb in (8, 16, 128, 256):
        for _ in range(8):
            n = modulus(nb)
            a, e = rng.randrange(n), rng.getrandbits(8 * rng.choice([1, nb]))
            add(f"Z {n:0{2 * nb}x} {e:0{2 * nb if e.bit_length() > 8 else 2}x} {a:0{2 * nb}x}", f"0 ok {pow(a, e, n):0{2 * nb}x}")
            x = rng.randrange(2 * n) if (2 * n).bit_length() <= 8 * nb else rng.randrange(n)
            add(f"R {n:0{2 * nb}x} {x:0{2 * nb}x}", f"0 ok {x % n:0{2 * nb}x}")
            dd, kappa = rng.randrange(n), rng.getrandbits(64) | 1 << 63
            add(f"Y {n:0{2 * nb}x} {dd:0{2 * nb}x} {kappa:016x}", f"0 ok {dd + kappa * (n - 1):0{2 * (nb + 16)}x}")
    add("Z fffffffffffffffe 03 02", "-1 bigmod-even-modulus -")
    add("Z 7fffffffffffffff 03 02", "-2 bigmod-modulus-size -")
    add("Z ffffffffffffffffff 03 02", "-2 bigmod-modulus-size -")
    return pairs


def agrees(answer, want):
    return want(answer) if callable(want) else answer == want


def judge(compiler, sources, work, pairs, keydir, sign_check=True):
    def local(text):
        return text.replace("import std.bigmod;", "import bigmod;").replace("import std.rsa_sign;", "import rsa_sign;")

    files = {
        "bigmod.cho": sources["bigmod"].replace("module std.bigmod;", "module bigmod;", 1),
        "rsa_sign.cho": local(sources["rsa_sign"].replace("module std.rsa_sign;", "module rsa_sign;", 1)),
        "rsa_key.cho": local(sources["rsa_key"]),
        "key.cho": open(os.path.join(ROOT, "packages/x509/key.cho")).read(),
        "x509.cho": open(os.path.join(ROOT, "packages/x509/x509.cho")).read(),
        "driver.cho": local(open(os.path.join(ROOT, "tests/programs/rsa_sign_driver.cho")).read()),
    }
    for name, text in files.items():
        open(os.path.join(work, name), "w").write(text)
    exe = os.path.join(work, "driver")
    build = subprocess.run([compiler, "build", "--std", "--backend", "llvm"] + [os.path.join(work, n) for n in files] + ["-o", exe], capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    run = subprocess.run([exe], input="\n".join(c for c, _ in pairs) + "\n", capture_output=True, text=True, timeout=1800)
    answers = [a.rstrip() for a in run.stdout.splitlines()]
    if run.returncode != 0 or len(answers) != len(pairs):
        return True, ["the driver failed or stopped early"]
    for (case, want), answer in zip(pairs, answers):
        if not agrees(answer, want):
            return True, [f"{case[:30]}...: {answer[:60]}"]
    if sign_check:
        diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/rsa_sign_differential.py"), exe, "sign", keydir, "3"], capture_output=True, text=True, env=dict(os.environ, RSA_SIGN_ONLY_2048="1"))
        if diff.returncode != 0:
            return True, ["the OpenSSL differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    keydir = os.path.abspath(sys.argv[2])
    only = sys.argv[3] if len(sys.argv) > 3 else None
    sources = {
        "bigmod": open(os.path.join(ROOT, "std/bigmod.cho")).read(),
        "rsa_sign": open(os.path.join(ROOT, "std/rsa_sign.cho")).read(),
        "rsa_key": open(os.path.join(ROOT, "packages/x509/rsa_key.cho")).read(),
    }
    pairs = evidence(keydir)
    failed, ran = 0, 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = judge(compiler, sources, work, pairs, keydir)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(pairs)} exact cases and the OpenSSL differential", flush=True)
        for module, name, old, new in MUTANTS:
            if only and only not in name:
                continue
            assert sources[module].count(old) == 1, f"{module}: {name}: `{old[:60]}` occurs {sources[module].count(old)} times"
            mutated = dict(sources)
            mutated[module] = sources[module].replace(old, new)
            dead, why = judge(compiler, mutated, work, pairs, keydir)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {module}: {name}: {'; '.join(why)}", flush=True)
            failed += dead is not True
            ran += 1
    print(f"{ran} mutants, {ran - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
