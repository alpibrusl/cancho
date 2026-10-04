#!/usr/bin/env python3
"""The TLS 1.2 PRF and extended master secret of `packages/tls` against OpenSSL
(docs/tls-parity.md §3.4).

    python3 scripts/tls12_prf_differential.py <driver> [<count>] [--write <rows.txt>]

`driver` is `tests/programs/tls_driver.ls` built with the package's files; its
`P` line is `tls_record.prf`. For each of `count` cases (default 1,000): a hash
(SHA-256 or SHA-384), a secret, a label and a seed at random, among them the
extended master secret's (RFC 7627 §4: label "extended master secret", a
session hash as the seed, 48 bytes), the key block's and Finished's. The
answer must equal both OpenSSL's TLS1-PRF (`openssl kdf`, a separate process
per case) and RFC 5246 §5's P_hash written here on Python's `hmac`. With
`--write`, the first 40 cases are written as `tests/vectors/tls/prf12.txt`.
Exit status 1 on any difference.
"""
import hashlib
import hmac
import random
import subprocess
import sys

LABELS = [b"extended master secret", b"key expansion", b"client finished", b"server finished", b"master secret"]


def p_hash(h, secret, seed, n):
    out, a = b"", hmac.new(secret, seed, h).digest()
    while len(out) < n:
        out += hmac.new(secret, a + seed, h).digest()
        a = hmac.new(secret, a, h).digest()
    return out[:n]


def openssl(name, secret, seed, n):
    out = subprocess.run(["openssl", "kdf", "-keylen", str(n), "-kdfopt", f"digest:{name}", "-kdfopt",
                          f"hexsecret:{secret.hex()}", "-kdfopt", f"hexseed:{seed.hex()}", "TLS1-PRF"],
                         capture_output=True, text=True, check=True).stdout
    return bytes.fromhex(out.strip().replace(":", ""))


def main():
    args = [a for a in sys.argv[1:] if a != "--write"]
    write = sys.argv[sys.argv.index("--write") + 1] if "--write" in sys.argv else None
    if write:
        args.remove(write)
    driver = args[0]
    count = int(args[1]) if len(args) > 1 else 1000
    rng = random.Random(5246)
    cases, wants = [], []
    for i in range(count):
        h, name = rng.choice([(hashlib.sha256, "SHA256"), (hashlib.sha384, "SHA384")])
        label = LABELS[i % len(LABELS)]
        if label == b"extended master secret":
            secret, seed, n = rng.randbytes(rng.choice([32, 48])), rng.randbytes(h().digest_size), 48
        elif label == b"key expansion":
            secret, seed, n = rng.randbytes(48), rng.randbytes(64), rng.choice([40, 72, 88])
        elif label.endswith(b"finished"):
            secret, seed, n = rng.randbytes(48), rng.randbytes(h().digest_size), 12
        else:
            secret, seed, n = rng.randbytes(rng.randrange(1, 200)), rng.randbytes(rng.randrange(0, 200)), rng.randrange(1, 300)
        want = p_hash(h, secret, label + seed, n)
        theirs = openssl(name, secret, label + seed, n)
        assert want == theirs, f"Python and OpenSSL differ on case {i}"
        cases.append(f"P {h().digest_size} {secret.hex()} {label.hex()} {seed.hex() or '-'} {n}")
        wants.append(want.hex())
    out = subprocess.run([driver], input="\n".join(cases) + "\n", capture_output=True, text=True, check=True).stdout
    lines = out.splitlines()
    bad = sum(1 for g, w in zip(lines, wants) if g != f"0 ok {w}") + abs(len(lines) - len(wants))
    print(f"TLS 1.2 PRF: {count} cases against OpenSSL's TLS1-PRF and Python, {bad} differences")
    if write:
        head = ["# The TLS 1.2 PRF (RFC 5246 §5) and extended master secret (RFC 7627 §4): `tls_driver.ls`'s",
                "# `P <hash length> <secret> <label> <seed> <n>` | the output. Printed by",
                "# `scripts/tls12_prf_differential.py --write` from OpenSSL's TLS1-PRF (`openssl kdf`), each row",
                "# also checked against P_hash on Python's hmac. Read by `conformance/tls.rs`."]
        open(write, "w").write("\n".join(head + [f"{c} | {w}" for c, w in zip(cases[:40], wants[:40])]) + "\n")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
