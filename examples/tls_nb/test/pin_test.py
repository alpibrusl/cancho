#!/usr/bin/env python3
"""`pin.is_public` against `cancho-hooks`' `src/destination.cho`, the original it was copied from.

    python3 pin_test.py [path/to/cancho-hooks]      # exits 0 if they agree on every address checked (see pin_check.cho); 77 if there is no checkout

A copy that drifts from its original is a second rule, and two rules for one thing is how an SSRF hole is made; this is what keeps the
copy honest until `pin.cho` is the package `cancho-hooks` imports.
"""
import os, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

def main():
    hooks = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("CANCHO_HOOKS", os.path.join(os.path.dirname(h.ROOT), "cancho-hooks"))
    dest = os.path.join(hooks, "src", "destination.cho")
    if not os.path.exists(dest):
        print("skip: no cancho-hooks checkout at %s" % hooks)
        return 77
    os.makedirs(h.WORK, exist_ok=True)
    exe = os.path.join(h.WORK, "pin_check")
    subprocess.run([h.CANCHO, "build", os.path.join(h.EXAMPLE, "pin.cho"), dest, os.path.join(h.EXAMPLE, "pin_check.cho"), "--std", "-o", exe], check=True)
    p = subprocess.run([exe], capture_output=True, text=True)
    print(p.stdout.strip())
    return 0 if p.returncode == 0 else 1

sys.exit(main())
