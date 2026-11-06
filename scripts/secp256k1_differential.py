#!/usr/bin/env python3
"""packages/secp256k1 against libsecp256k1 (coincurve) and python-ecdsa, byte for byte (docs/secp256k1.md section 3).

    python3 -I scripts/secp256k1_differential.py [--mutants] [--cases N]        # needs: pip install coincurve ecdsa
    CANCHO=the-compiler  for --mutants

Builds tests/programs/secp256k1_driver.cho with the compiler (CANCHO, default `cancho`), then checks:
  public keys (libsecp256k1 and python-ecdsa), signatures r||s||recid byte for byte (RFC 6979 and low s make them deterministic),
  recovery, verification of valid and bit-flipped signatures, and the refusals (key 0, n, n+1; r or s 0 or n; recid 4; a point off the curve).
--mutants rebuilds with one deliberate fault at a time; every mutant must be killed by the same checks.
"""
import os, random, shutil, subprocess, sys, tempfile
import coincurve
from ecdsa import SECP256k1, SigningKey

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "packages/secp256k1/secp256k1.cho")
DRIVER = os.path.join(ROOT, "tests/programs/secp256k1_driver.cho")
N = SECP256k1.order
P = SECP256k1.curve.p()


def build(cancho, src, out):
    r = subprocess.run([cancho, "build", "--std", DRIVER, src, "-o", out], capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit("build failed:\n" + r.stderr)


def call(exe, line):
    out = subprocess.run([exe], input=line.encode(), capture_output=True, timeout=60).stdout.decode().strip()
    parts = out.split()
    return parts[0], (parts[1] if len(parts) > 1 else "")


def b32(n):
    return n.to_bytes(32, "big")


def keys(rng, count):
    yield from (1, 2, 3, N - 1, N - 2, N // 2, N // 2 + 1, 2**255, 2**128, 0xFFFF)
    for _ in range(count):
        yield rng.randrange(1, N)


def digests(rng, count):
    yield from (b32(0), b32(1), b32(N - 1), b32(N), b32(N + 1), b32(2**256 - 1), bytes.fromhex("4b688df40bcedbe641ddb16ff0a1842d9c67ea1c3bf63f3e0471baa664531d1a"))
    for _ in range(count):
        yield bytes(rng.randrange(256) for _ in range(32))


def run(exe, cases):
    rng = random.Random(20261106)
    bad = total = 0

    def check(name, got, want):
        nonlocal bad, total
        total += 1
        if got != want:
            bad += 1
            if bad <= 5:
                print(f"  MISMATCH {name}: got {got} want {want}")

    for k in keys(rng, cases):
        kb = b32(k)
        pub = coincurve.PrivateKey(kb).public_key.format(compressed=False).hex()
        rc, got = call(exe, f"P {kb.hex()}")
        check(f"pubkey {k:x}", (rc, got), ("0", pub))
        if k < 2**200:  # python-ecdsa as a second, independent public key
            ecd = SigningKey.from_secret_exponent(k, curve=SECP256k1).get_verifying_key().to_string("uncompressed").hex()
            check("pubkey ecdsa", got, ecd)
    for k in list(keys(rng, cases // 4)):
        kb = b32(k)
        ck = coincurve.PrivateKey(kb)
        pub = ck.public_key.format(compressed=False)
        for d in list(digests(rng, 2)):
            want = ck.sign_recoverable(d, hasher=None)
            rc, sig = call(exe, f"S {d.hex()} {kb.hex()}")
            check(f"sign key={k:x} d={d.hex()[:8]}", (rc, sig), ("0", want.hex()))
            if rc != "0":
                continue
            rc, rec = call(exe, f"R {d.hex()} {sig}")
            check("recover", (rc, rec), ("0", pub.hex()))
            ref = coincurve.PublicKey.from_signature_and_message(want, d, hasher=None).format(compressed=False).hex()
            check("recover vs libsecp", rec, ref)
            rc, _ = call(exe, f"V {d.hex()} {pub.hex()} {sig}")
            check("verify valid", rc, "0")
            # flip one bit of r, s, the digest and the key: each must be refused (E122), or a different answer from recover
            sb = bytearray(bytes.fromhex(sig))
            for pos in (0, 31, 32, 63):
                f = bytearray(sb); f[pos] ^= 0x01
                rc, _ = call(exe, f"V {d.hex()} {pub.hex()} {bytes(f).hex()}")
                check(f"verify flipped sig byte {pos}", rc in ("E122", "E117"), True)
            fd = bytearray(d); fd[7] ^= 0x10
            rc, _ = call(exe, f"V {bytes(fd).hex()} {pub.hex()} {sig}")
            check("verify flipped digest", rc, "E122")
            other = coincurve.PrivateKey(b32(k % (N - 2) + 1 if k % (N - 2) + 1 != k else 5)).public_key.format(compressed=False)
            # (a digest of 0 mod n cannot tell Q from -Q: u1 = 0, so only x(u2 Q) is looked at)
            if int.from_bytes(d, "big") % N != 0:
                rc, _ = call(exe, f"V {d.hex()} {other.hex()} {sig}")
                check("verify wrong key", rc, "E122")
            # a high-s signature is a valid ECDSA signature
            r_, s_ = int.from_bytes(sb[:32], "big"), int.from_bytes(sb[32:64], "big")
            hi = b32(r_) + b32(N - s_) + bytes([sb[64] ^ 1])
            rc, _ = call(exe, f"V {d.hex()} {pub.hex()} {hi.hex()}")
            check("verify high s", rc, "0")
            rc, rec = call(exe, f"R {d.hex()} {hi.hex()}")
            check("recover high s", (rc, rec), ("0", pub.hex()))
    # RFC 6979 section 3.6 added randomness: python-ecdsa's extra_entropy, with low s applied to its answer
    import hashlib
    from ecdsa.util import sigencode_strings
    for _ in range(max(6, cases // 6)):
        k = rng.randrange(1, N)
        d = bytes(rng.randrange(256) for _ in range(32))
        extra = bytes(rng.randrange(256) for _ in range(32))
        rs, ss = SigningKey.from_secret_exponent(k, curve=SECP256k1).sign_digest_deterministic(d, hashfunc=hashlib.sha256, sigencode=sigencode_strings, extra_entropy=extra)
        s_int = int.from_bytes(ss, "big")
        if s_int > N // 2:
            s_int = N - s_int
        rc, got = call(exe, f"S {d.hex()} {b32(k).hex()} {extra.hex()}")
        check("sign with extra entropy (r, s)", (rc, got[:128]), ("0", rs.hex() + b32(s_int).hex()))
    # recover on synthetic signatures, every recovery id, against libsecp256k1: the same keys, or both refuse
    for i in range(max(40, cases * 2)):
        r_ = rng.randrange(1, 2**128) if i % 2 == 0 else rng.randrange(1, N)  # a small r makes recid 2 and 3 reachable
        s_ = rng.randrange(1, N)
        d = bytes(rng.randrange(256) for _ in range(32))
        for recid in range(4):
            sig = b32(r_) + b32(s_) + bytes([recid])
            try:
                want = coincurve.PublicKey.from_signature_and_message(sig, d, hasher=None).format(compressed=False).hex()
            except Exception:
                want = None
            rc, got = call(exe, f"R {d.hex()} {sig.hex()}")
            check(f"recover synthetic recid={recid}", (got if rc == "0" else None), want)
    # the refusals
    kb = b32(1)
    d = b32(7).hex()
    for name, line, want in (
        ("key 0", f"S {d} {b32(0).hex()}", "E112"),
        ("key n", f"S {d} {b32(N).hex()}", "E112"),
        ("key n+1", f"S {d} {b32(N + 1).hex()}", "E112"),
        ("key 2^256-1", f"S {d} {b32(2**256 - 1).hex()}", "E112"),
        ("pubkey 0", f"P {b32(0).hex()}", "E112"),
        ("pubkey n", f"P {b32(N).hex()}", "E112"),
        ("digest 31 bytes", f"S {d[:-2]} {kb.hex()}", "E110"),
        ("key 31 bytes", f"S {d} {kb.hex()[:-2]}", "E111"),
        ("extra 5 bytes", f"S {d} {kb.hex()} 0102030405", "E113"),
    ):
        rc, _ = call(exe, line)
        check(f"refuse {name}", rc, want)
    good = coincurve.PrivateKey(kb).sign_recoverable(bytes.fromhex(d), hasher=None)
    pub = coincurve.PrivateKey(kb).public_key.format(compressed=False)
    gr, gs = good[:32], good[32:64]
    for name, sig in (
        ("r 0", b32(0) + gs + b"\x00"), ("s 0", gr + b32(0) + b"\x00"),
        ("r n", b32(N) + gs + b"\x00"), ("s n", gr + b32(N) + b"\x00"),
        ("recid 4", gr + gs + b"\x04"),
    ):
        rc, _ = call(exe, f"R {d} {sig.hex()}")
        check(f"recover refuses {name}", rc, "E117")
        if name != "recid 4":  # verify does not use the recovery id
            rc, _ = call(exe, f"V {d} {pub.hex()} {sig.hex()}")
            check(f"verify refuses {name}", rc, "E117")
    off = b"\x04" + b32(1) + b32(1)
    rc, _ = call(exe, f"V {d} {off.hex()} {good.hex()}")
    check("verify refuses a point off the curve", rc, "E120")
    big = b"\x04" + b32(P) + b32(1)
    rc, _ = call(exe, f"V {d} {big.hex()} {good.hex()}")
    check("verify refuses x = p", rc, "E119")
    rc, _ = call(exe, f"V {d} {(b'05' + pub[1:]).hex()} {good.hex()}")
    check("verify refuses a bad prefix", rc, "E118")
    # recid with an x that is not on the curve: r = 5 has no point (5^3 + 7 = 132 is a non-residue mod p), recover refuses
    for r_ in range(1, 40):
        rhs = (pow(r_, 3, P) + 7) % P
        if pow(rhs, (P - 1) // 2, P) != 1:
            rc, _ = call(exe, f"R {d} {(b32(r_) + gs + b'\x00').hex()}")
            check(f"recover refuses r={r_} (no point)", rc, "E121")
            break
    return bad, total


# (name, old, new): one fault each; every one must be killed
MUTANTS = [
    ("b3 = 22", "bigmod.set_small(w, reg_b3(), 21);", "bigmod.set_small(w, reg_b3(), 22);"),
    ("curve constant 7 -> 8 (on_curve)", "bigmod.set_small(w, t(2), 7);\n    bigmod.to_mont(w, t(2), t(2));\n    bigmod.add(w, t(1), t(2), t(1));\n    return bigmod.equal", "bigmod.set_small(w, t(2), 8);\n    bigmod.to_mont(w, t(2), t(2));\n    bigmod.add(w, t(1), t(2), t(1));\n    return bigmod.equal"),
    ("addition step 6 uses t3 twice", "bigmod.mul(w, t(3), t(4), t(3));", "bigmod.mul(w, t(3), t(3), t(3));"),
    ("addition step 26", "bigmod.mul(w, t(3), t(1), t(2));", "bigmod.mul(w, t(3), t(0), t(2));"),
    ("addition final step", "bigmod.add(w, z3, t(0), z3);", "bigmod.sub(w, z3, t(0), z3);"),
    ("doubling step 8", "bigmod.mul(w, t(2), z3, x3);", "bigmod.mul(w, t(0), z3, x3);"),
    ("doubling step 17", "bigmod.mul(w, t(0), t(1), x3);", "bigmod.mul(w, t(0), t(0), x3);"),
    ("doubling last step", 'bigmod.add(w, x3, x3, x3);\n    return ecdsa.copy_point(w, pt_out(), d);\n}\n\n// -1', 'bigmod.add(w, x3, x3, t(0));\n    return ecdsa.copy_point(w, pt_out(), d);\n}\n\n// -1'),
    ("table select reads entry 0 only", "let m = value_barrier(eq_mask(i, idx));", "let m = value_barrier(eq_mask(i, 0));"),
    ("window shift", 'window(w, b >> 4);\n        window(w, b & 15);', 'window(w, b & 15);\n        window(w, b >> 4);'),
    ("three doublings per window", 'while k < 4 {\n        double_point', 'while k < 3 {\n        double_point'),
    ("p constant", "fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f", "fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2d"),
    ("n constant", "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141", "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364143"),
    ("half n constant", "7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0", "7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a1"),
    ("sqrt exponent", "3fffffffffffffffffffffffffffffffffffffffffffffffffffffffbfffff0c", "3fffffffffffffffffffffffffffffffffffffffffffffffffffffffbfffff0d"),
    ("generator y", "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8", "483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b9"),
    ("key range: zero accepted", "any = any | int_of(s[i]);", "any = any | 1;"),
    ("key range: n accepted", "under = d >> 63 & 1;", "under = d >> 63 & 0;"),
    ("nonce: separator 1 -> 2", "sep[0] = byte_of(1);", "sep[0] = byte_of(2);"),
    ("nonce: extra data ignored", '        hmac.update(32, st, h);\n        hmac.update(32, st, extra);', '        hmac.update(32, st, h);'),
    ("nonce: digest not hashed in", '        hmac.update(32, st, key);\n        hmac.update(32, st, h);', '        hmac.update(32, st, key);'),
    ("s: k not inverted", "bigmod.inverse(w, r_k(), r_k());\n    // e is the digest", "// e is the digest"),
    ("s: r not multiplied by d", "bigmod.mul(w, r_r(), r_d(), r_s());", "bigmod.mul(w, r_r(), r_r(), r_s());"),
    ("s: e dropped", "bigmod.add(w, r_e(), r_s(), r_s());", "bigmod.add(w, r_s(), r_s(), r_s());"),
    ("no low s", "if cmp32(sig[32..64], half_n) > 0 {", "if cmp32(sig[32..64], half_n) > 1 {"),
    ("low s without flipping the parity", "recid = recid ^ 1;", "recid = recid ^ 0;"),
    ("recid ignores R.y parity", "var recid = int_of(y[31]) & 1;", "var recid = 0;"),
    ("recid ignores x >= n", "recid = recid | 2;", "recid = recid | 0;"),
    ("recover: wrong root parity", "if int_of(y[31]) & 1 != recid & 1 {", "if int_of(y[31]) & 1 == recid & 1 {"),
    ("recover: u1 not negated", "bigmod.sub(w, r_zero(), r_d(), r_d());\n        bigmod.mul(w, r_s(), r_k(), r_k());", "bigmod.mul(w, r_s(), r_k(), r_k());"),
    ("recover: x = r + n ignored", "if recid >= 2 {", "if recid >= 4 {"),
    ("recover: y^2 = rhs not checked", "if cmp32(ny, rhs) != 0 {", "if cmp32(ny, rhs) != 0 && false {"),
    ("recover: recid 4 accepted", "if recid > 3 || !signature_ok(sig) {", "if recid > 4 || !signature_ok(sig) {"),
    ("verify: x mod n not compared", "if cmp32(y, sig[0..32]) == 0 {", "if cmp32(y, sig[0..32]) != 2 {"),
    ("verify: on-curve check dropped", "if !on_curve(work) {\n        wipe(work);\n        return -120;", "if false {\n        wipe(work);\n        return -120;"),
    ("verify: s zero accepted", "if is_zero32(sig[0..32]) || is_zero32(sig[32..64]) {", "if is_zero32(sig[0..32]) {"),
    ("verify: s = n accepted", "return cmp32(sig[0..32], order_n) < 0 && cmp32(sig[32..64], order_n) < 0;", "return cmp32(sig[0..32], order_n) < 0 && cmp32(sig[32..64], order_n) <= 0;"),
    ("verify: coordinates not below p", "if bigmod.load_reg(x, w, x_of(p)) != 0 || bigmod.load_reg(y, w, y_of(p)) != 0 {", "if false {"),
]


# Mutants no input can kill, with the reason. They are reported, not failed: a survivor outside this list fails the run.
EXPECTED_SURVIVORS = {
    "half n constant": "its last digit only matters for an s of exactly n / 2, which no signing run reaches (probability 2^-256)",
    "recid ignores x >= n": "bit 1 of the recovery id is set only when R.x >= n, probability about 2^-128; recover's side of it is killed (x = r + n ignored)",
}


def mutants(cancho, cases):
    text = open(SRC).read()
    survived = 0
    for name, old, new in MUTANTS:
        if text.count(old) != 1:
            raise SystemExit(f"mutant pattern not unique ({text.count(old)}): {name}")
        d = tempfile.mkdtemp()
        try:
            src = os.path.join(d, "secp256k1.cho")
            open(src, "w").write(text.replace(old, new))
            exe = os.path.join(d, "drv")
            r = subprocess.run([cancho, "build", "--std", DRIVER, src, "-o", exe], capture_output=True, text=True)
            if r.returncode != 0:
                print(f"mutant '{name}': did not build (counts as killed only if the refusal is intended)\n{r.stderr[:300]}")
                survived += 1
                continue
            bad, total = run(exe, cases)
            if not bad and name in EXPECTED_SURVIVORS:
                print(f"mutant '{name}': SURVIVED, expected: {EXPECTED_SURVIVORS[name]}")
                continue
            print(f"mutant '{name}': {'KILLED' if bad else 'SURVIVED'} ({bad}/{total})")
            survived += 0 if bad else 1
        finally:
            shutil.rmtree(d)
    return survived


def main():
    cancho = os.environ.get("CANCHO", "cancho")
    cases = 60
    if "--cases" in sys.argv:
        cases = int(sys.argv[sys.argv.index("--cases") + 1])
    d = tempfile.mkdtemp()
    try:
        exe = os.path.join(d, "drv")
        build(cancho, SRC, exe)
        bad, total = run(exe, cases)
        print(f"secp256k1: {total - bad}/{total} checks agree with libsecp256k1 and python-ecdsa")
        if bad:
            sys.exit(1)
        if "--mutants" in sys.argv:
            s = mutants(cancho, max(4, cases // 6))
            print(f"mutants: {len(MUTANTS) - s - len(EXPECTED_SURVIVORS)}/{len(MUTANTS)} killed, {len(EXPECTED_SURVIVORS)} unreachable by any input (above), {s} unexpected survivors")
            if s:
                sys.exit(1)
    finally:
        shutil.rmtree(d)


main()
