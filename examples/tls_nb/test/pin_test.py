#!/usr/bin/env python3
"""`pin.is_public` against `lexsys-hooks`' `src/destination.ls`, the original it was copied from.

    python3 pin_test.py [path/to/lexsys-hooks]      # exits 0 if they agree on every address checked (see pin_check.ls); 77 if there is no checkout

A copy that drifts from its original is a second rule, and two rules for one thing is how an SSRF hole is made; this is what keeps the
copy honest until `pin.ls` is the package `lexsys-hooks` imports.
"""
import os, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

def main():
    hooks = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("LEXSYS_HOOKS", os.path.join(os.path.dirname(h.ROOT), "lexsys-hooks"))
    dest = os.path.join(hooks, "src", "destination.ls")
    if not os.path.exists(dest):
        print("skip: no lexsys-hooks checkout at %s" % hooks)
        return 77
    os.makedirs(h.WORK, exist_ok=True)
    exe = os.path.join(h.WORK, "pin_check")
    subprocess.run([h.LEXSYS, "build", os.path.join(h.EXAMPLE, "pin.ls"), dest, os.path.join(h.EXAMPLE, "pin_check.ls"), "--std", "-o", exe], check=True)
    p = subprocess.run([exe], capture_output=True, text=True)
    print(p.stdout.strip())
    return 0 if p.returncode == 0 else 1

sys.exit(main())
