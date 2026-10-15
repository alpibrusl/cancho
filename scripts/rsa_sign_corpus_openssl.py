#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""What OpenSSL does with each file of the malformed-key corpus (docs/rsa-sign.md §6 and §11, gate G3).

    python3 scripts/rsa_sign_corpus_openssl.py tests/vectors/rsa_sign/malformed.txt

Not a gate: the point of the corpus is that its expected rules are its own. This prints how many of the files the corpus says must
be refused `openssl pkey` and `openssl rsa -check` nevertheless accept, which is why OpenSSL cannot be the oracle for refusals
(B2 measured 16 of 28 for `pkey`).
"""
import os
import subprocess
import sys
import tempfile

rows = [l.rstrip("\n").split(" ") for l in open(sys.argv[1]) if not l.startswith("#")]
tmp = tempfile.mkdtemp(prefix="rsasign_ossl_")
bad_pkey = bad_check = malformed = 0
print(f"{'case':44s} {'expected':26s} pkey      rsa -check")
for parse, sign, name, sig, hexdata in rows:
    f = os.path.join(tmp, name + ".pem")
    open(f, "wb").write(b"" if hexdata == "-" else bytes.fromhex(hexdata))
    r1 = subprocess.run(["openssl", "pkey", "-in", f, "-noout", "-passin", "pass:x"], capture_output=True, text=True)
    r2 = subprocess.run(["openssl", "rsa", "-in", f, "-check", "-noout", "-passin", "pass:x"], capture_output=True, text=True)
    a1 = r1.returncode == 0
    a2 = r2.returncode == 0 and "RSA key ok" in r2.stdout
    print(f"{name:44s} {parse:26s} {'accepts' if a1 else 'refuses':9s} {'accepts' if a2 else 'refuses'}")
    if parse != "ok":
        malformed += 1
        bad_pkey += a1
        bad_check += a2
print(f"\n{len(rows)} files; {malformed} that the corpus says must be refused; `openssl pkey` accepts {bad_pkey} of them, `openssl rsa -check` accepts {bad_check}")
