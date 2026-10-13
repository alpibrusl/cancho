#!/usr/bin/env python3
"""The KDF evidence (scripts/kdf_mutants.py's: the vector table and its prepared rows, the CAVP short-message and Monte
Carlo files, every Wycheproof HMAC and HKDF case, the streamed cases, the refusal rows) plus the CAVP long-message files,
through a kdf_driver run under Valgrind, which hides SHA-NI (docs/sha256-builtin.md §2): std's software path on LLVM.

    python3 scripts/kdf_valgrind.py <repo> <driver>

`scripts/kdf_differential.py` runs on the same path through a wrapper that is the driver under Valgrind:
`printf '#!/bin/sh\nexec valgrind -q /abs/kdf_driver "$@"\n' > vg_driver; chmod +x vg_driver`.
"""
import os
import subprocess
import sys

repo, exe = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.join(repo, "scripts"))
os.chdir(repo)
import kdf_mutants  # noqa: E402

cases, checks = kdf_mutants.evidence()
for alg, f in ((256, "SHA256LongMsg"), (384, "SHA384LongMsg")):
    length, msg = 0, ""
    for line in open(os.path.join(repo, f"tests/vectors/cavp/{f}.rsp")):
        if line.startswith("Len = "):
            length = int(line.split("=")[1])
        elif line.startswith("Msg = "):
            msg = line.split("=")[1].strip() if length else ""
        elif line.startswith("MD = "):
            w = line.split("=")[1].strip()
            cases.append(f"H {alg} {msg or '-'}")
            checks.append(lambda a, w=w: a == f"0 ok {w}")
out = subprocess.run(["valgrind", "-q", exe], input="\n".join(cases) + "\n", capture_output=True, text=True)
answers = [l.rstrip() for l in out.stdout.splitlines()]
assert out.returncode == 0 and len(answers) == len(cases), (out.returncode, len(answers), len(cases), out.stderr[-500:])
bad = [i for i, (c, a) in enumerate(zip(checks, answers)) if not c(a)]
print(f"under valgrind: {len(cases)} cases, {len(bad)} wrong" + (f": first {cases[bad[0]][:60]}" if bad else ""))
sys.exit(1 if bad else 0)
