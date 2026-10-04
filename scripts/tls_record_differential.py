#!/usr/bin/env python3
"""`packages/tls`'s record layer against pyca/cryptography (docs/tls-core.md §9).

    python3 scripts/tls_record_differential.py <driver> [<count>]

`driver` is `tests/programs/tls_driver.ls` built with the package's files. For
each of `count` rounds (default 2,000): a record sealed by `tls_record.seal`
must equal the RFC 8446 §5.2 record built here with pyca's ChaCha20Poly1305
(OpenSSL underneath), and a record built here, with random padding after the
content type, must open to the same content. One in three of those has one bit
flipped and must be refused as `tls-bad-record-mac`. Sequence numbers include 0,
255, 256 and 2^62 - 1; contents include 0, 1, 15 to 17 bytes and 2^14. Exit
status 1 on any difference.
"""
import random
import subprocess
import sys

from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
    rng = random.Random(205)
    cases, want = [], []
    for _ in range(count):
        key, iv = rng.randbytes(32), rng.randbytes(12)
        seq = rng.choice([0, 1, 2, 255, 256, rng.getrandbits(40), (1 << 62) - 1])
        kind = rng.choice([21, 22, 23])
        text = rng.randbytes(rng.choice([0, 1, 15, 16, 17, rng.randint(0, 2000), 16384]))
        nonce = bytes(a ^ b for a, b in zip(iv, seq.to_bytes(12, "big")))
        header = bytes([23, 3, 3]) + (len(text) + 17).to_bytes(2, "big")
        record = header + ChaCha20Poly1305(key).encrypt(nonce, text + bytes([kind]), header)
        cases.append(f"S {key.hex()} {iv.hex()} {seq} {kind} {text.hex() or '-'}")
        want.append(f"0 ok {record.hex()}")
        pad = rng.choice([0, 0, 1, 100])
        header = bytes([23, 3, 3]) + (len(text) + 1 + pad + 16).to_bytes(2, "big")
        record = header + ChaCha20Poly1305(key).encrypt(nonce, text + bytes([kind]) + bytes(pad), header)
        if rng.random() < 1 / 3:
            flipped = bytearray(record)
            at = rng.randrange(5, len(flipped))
            flipped[at] ^= 1 << rng.randrange(8)
            record = bytes(flipped)
            want.append("-10 tls-bad-record-mac -")
        else:
            want.append(f"0 ok {kind} {text.hex() or '-'}")
        cases.append(f"O {key.hex()} {iv.hex()} {seq} {record.hex()}")
    out = subprocess.run([driver], input="\n".join(cases) + "\n", capture_output=True, text=True, check=True).stdout.splitlines()
    bad = sum(1 for g, w in zip(out, want) if g != w) + abs(len(out) - len(want))
    print(f"records: {count} sealed and {count} opened, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
