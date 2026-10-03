#!/usr/bin/env python3
"""Differential test of the WebSocket example's SHA-1 and base64 against `hashlib` and `base64` (docs/websocket-spike.md, gate G1).

    python3 scripts/ws_codec_check.py <probe> [<server>]

`probe` is `examples/ocpp_ws/probe.ls` built with `lex-sys build --std`. 1,000 random inputs, every length from 0 to 130 at least
seven times; any difference is printed and the exit status is 1.
"""
import base64
import hashlib
import random
import subprocess
import sys

probe = sys.argv[1]
rng = random.Random(6455)
bad = 0
n = 0
for rep in range(8):
    for length in range(0, 131):
        data = bytes(rng.randrange(256) for _ in range(length))
        h = data.hex() or ""
        for op, want in (("sha1", hashlib.sha1(data).hexdigest()), ("b64", base64.b64encode(data).decode())):
            args = [probe, op, h] if h else [probe, op, ""]
            got = subprocess.run(args, capture_output=True, text=True).stdout.strip()
            n += 1
            if got != want:
                bad += 1
                print(f"DIFFERENT {op} length {length}: got {got!r} want {want!r}")
print(f"{n} checks, {bad} differences")
sys.exit(1 if bad else 0)
