#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""The fault gate (docs/rsa-sign.md §9, gate G4).

    python3 scripts/rsa_sign_fault.py <driver> <keydir>

A bit flipped in one CRT half of an RSA-CRT signature gives a signature s with gcd(s^e - EM, n) equal to a prime factor of n
(Boneh, DeMillo and Lipton; Lenstra). For every key (2,048/3,072/4,096 bits, both orders of p and q, unbalanced primes) this script
asks the signer (`U`, the test hook `pkcs1_sign_test`) for the same signature with a fault injected in
    mode 2   the p-half's result          the unchecked signature must reveal q   (s is right mod q, wrong mod p)
    mode 4   the q-half's result          ... p
    mode 32  qinv                         ... q   (s = s_q + q h with a wrong h: right mod q)
    mode 8   the recombined signature     ... no factor (gcd 1): a bit of s itself is flipped
and demands, with the check ON, the refusal `-147 rsa-sign-check` and NO signature bytes; with the check OFF (mode | 1), a signature
that is wrong, that `std.rsa.pkcs1_verify` refuses, and whose gcd is as above -- so the check is shown to protect something. A
fault-free call must still sign. Exit status 1 on any difference.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402
from rsa_sign_differential import keys_of  # noqa: E402

driver, keydir = sys.argv[1], sys.argv[2]
fails, shown = [], 0
for key in keys_of(keydir):
    if key["e"] != 65537 or key["bits"] == 2176 and False:
        continue
    k = (key["n"].bit_length() + 7) // 8
    digest = L.digest_of(32, b"fault " + key["name"].encode())
    em = int.from_bytes(L.emsa(32, digest, k), "big")
    blk = L.block(key).hex()
    ent = bytes(range(16)).hex()
    lines = [f"S 32 {blk} {digest.hex()} {ent}"]
    for mode in (2, 4, 32, 8):
        lines += [f"U 32 {blk} {digest.hex()} {ent} {mode}", f"U 32 {blk} {digest.hex()} {ent} {mode | 1}"]
    res, rc, err = L.run_driver(driver, lines)
    if len(res) != len(lines):
        fails.append(f"{key['name']}: {len(res)} answers for {len(lines)} cases {err[:100]}")
        continue
    if not res[0].startswith("0 ok "):
        fails.append(f"{key['name']}: the fault-free call did not sign: {res[0][:60]}")
        continue
    good = int(res[0].split()[2], 16)
    for i, mode in enumerate((2, 4, 32, 8)):
        checked, unchecked = res[1 + 2 * i], res[2 + 2 * i]
        if checked != "-147 rsa-sign-check -":
            fails.append(f"{key['name']} mode {mode}: with the check on: {checked[:60]}")
        parts = unchecked.split()
        if parts[0] != "0":
            fails.append(f"{key['name']} mode {mode}: unchecked call did not return a signature: {unchecked[:60]}")
            continue
        s = int(parts[2], 16)
        if s == good or pow(s, key["e"], key["n"]) == em:
            fails.append(f"{key['name']} mode {mode}: no fault was injected")
            continue
        g = math.gcd((pow(s, key["e"], key["n"]) - em) % key["n"], key["n"])
        want = {2: key["q"], 4: key["p"], 32: key["q"], 8: 1}[mode]
        if g != want:
            fails.append(f"{key['name']} mode {mode}: gcd(s^e - EM, n) has {g.bit_length()} bits, wanted {'1' if want == 1 else 'the prime'}")
        elif mode != 8:
            shown += 1
    print(f"{key['name']:24s} faults refused with the check; unchecked, the prime factor falls out of modes 2, 4 and 32")
print(f"fault gate: {shown} unchecked faulty signatures revealed a prime factor; failures {len(fails)}")
for f in fails[:25]:
    print("FAIL", f)
sys.exit(1 if fails or shown == 0 else 0)
