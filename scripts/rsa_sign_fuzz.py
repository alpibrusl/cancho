#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""A deterministic mutational fuzz of key files through `x509_rsa_key` (docs/rsa-sign.md §11, gate G11).

    python3 scripts/rsa_sign_fuzz.py <driver> <malformed.txt> [count]

"Input never reaches a panic" (CONTRIBUTING.md): every mutation of every file of the corpus (bit flips, byte replacements, byte
insertions and deletions, truncation, a run of one byte, splicing two files, DER length fields set to extremes) is parsed by
`parse_pem` and, for the raw DER inside, by `parse_der` in both formats; whatever comes back must be a CODE (the driver answers
`<code> <tag> ...` and exits 0). A trap (SIGILL), a hang or an exit status other than 0 is a failure, and the mutation that did it
is printed. Every answer's tag must be one of the tags of docs/rsa-sign.md §6 and a code that parses must be one that signs or
refuses with a tag of §6. cancho's AFL++ setup (docs/fuzzing.md) was not available on this machine; this is not coverage-guided.
"""
import base64
import os
import random
import re
import subprocess
import sys

TAGS = {"ok", "rsa-key-pem", "rsa-key-encrypted", "rsa-key-algorithm", "rsa-key-parameters", "rsa-key-der", "rsa-key-version", "rsa-key-multiprime",
        "rsa-key-buffer-length", "rsa-key-block-size", "rsa-key-size", "rsa-key-modulus", "rsa-key-exponent", "rsa-key-prime", "rsa-key-product",
        "rsa-key-crt-exponent", "rsa-key-crt-coefficient", "rsa-key-private-exponent", "rsa-sign-hash", "rsa-sign-digest-length",
        "rsa-sign-key-layout", "rsa-sign-entropy", "rsa-sign-output-length", "rsa-sign-work-length", "rsa-sign-key-shape", "rsa-sign-check"}


def mutate(rng, data, pool):
    d = bytearray(data)
    kind = rng.randrange(9)
    if not d:
        return bytes(rng.randbytes(rng.randrange(1, 40)))
    if kind == 0:
        d[rng.randrange(len(d))] ^= 1 << rng.randrange(8)
    elif kind == 1:
        d[rng.randrange(len(d))] = rng.randrange(256)
    elif kind == 2:
        d.insert(rng.randrange(len(d) + 1), rng.randrange(256))
    elif kind == 3:
        del d[rng.randrange(len(d))]
    elif kind == 4:
        d = d[:rng.randrange(len(d))]
    elif kind == 5:
        at = rng.randrange(len(d))
        d[at:at + rng.randrange(1, 64)] = bytes([rng.randrange(256)]) * rng.randrange(1, 64)
    elif kind == 6:
        other = rng.choice([x for x in pool if x])
        d = d[:rng.randrange(len(d))] + other[rng.randrange(len(other)):]
    elif kind == 7:
        at = rng.randrange(len(d))
        d[at] = rng.choice([0x00, 0x7f, 0x80, 0x81, 0x82, 0x83, 0x84, 0xff])
    else:
        for _ in range(rng.randrange(2, 8)):
            d[rng.randrange(len(d))] ^= 1 << rng.randrange(8)
    return bytes(d)


def der_of(pem_bytes):
    m = re.search(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----(.*?)-----END", pem_bytes, re.S)
    if not m:
        return None
    try:
        return base64.b64decode(re.sub(rb"[^A-Za-z0-9+/=]", b"", m.group(1)))
    except Exception:
        return None


def main():
    driver, corpus = sys.argv[1], sys.argv[2]
    count = int(sys.argv[3]) if len(sys.argv) > 3 else 20000
    files = [bytes.fromhex(l.split(" ")[4]) if l.split(" ")[4] != "-" else b"" for l in open(corpus).read().splitlines() if not l.startswith("#")]
    rng = random.Random(20261014)
    fails, seen, total = [], {}, 0
    batch = 500
    for start in range(0, count, batch):
        muts = [mutate(rng, rng.choice(files), files) for _ in range(min(batch, count - start))]
        lines = []
        for m in muts:
            lines.append(f"G {m.hex() or '-'} 32 - 000102030405060708090a0b0c0d0e0f")
            der = der_of(m)
            lines.append(f"B 8 {der.hex() if der else '-'}")
            lines.append(f"B 2 {der.hex() if der else '-'}")
        out = subprocess.run([driver], input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=600)
        res = out.stdout.splitlines()
        if out.returncode != 0 or len(res) != len(lines):
            # find the offender one by one
            for m in muts:
                one = [f"G {m.hex() or '-'} 32 - 000102030405060708090a0b0c0d0e0f"]
                r = subprocess.run([driver], input="\n".join(one) + "\n", capture_output=True, text=True, timeout=60)
                if r.returncode != 0 or len(r.stdout.splitlines()) != 1:
                    fails.append(f"exit {r.returncode}: {m.hex()[:120]}")
                    break
            else:
                fails.append(f"batch {start}: exit {out.returncode}, {len(res)} answers for {len(lines)}: {out.stderr[:100]}")
            continue
        for l in res:
            total += 1
            f = l.split(" ")
            seen[f[1]] = seen.get(f[1], 0) + 1
            if f[1] not in TAGS:
                fails.append(f"unknown tag in {l[:60]}")
    print(f"fuzz: {total} answers to {count} mutations x (G, B 8, B 2); tags seen: " + ", ".join(f"{k} {v}" for k, v in sorted(seen.items())))
    print(f"failures {len(fails)}")
    for f in fails[:20]:
        print("FAIL", f)
    sys.exit(1 if fails else 0)


main()
