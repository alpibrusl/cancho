#!/usr/bin/env python3
"""Compare the checker port (`check.ls`) with the Rust checker's declarations half.

    check_diff.py ORACLE PORT file...

ORACLE is `check_declarations` (`crates/lex-sys-ir/examples/check_declarations.rs`), PORT is
`check.ls` built; both read a source file on standard input and print `OK` or `ERR rule start end`.
The port may also print `SKIP`, which means the declarations use something whose checks are not
ported yet: such a file is not compared, and counted by what the oracle said about it, so the
report says how much of the corpus the port has not reached rather than hiding it.
Exit status 1 if any compared file differs.
"""
import collections, concurrent.futures, subprocess, sys

def run(cmd, path):
    with open(path, "rb") as f:
        return subprocess.run([cmd], stdin=f, capture_output=True, timeout=120).stdout.decode()

def main():
    oracle, port, files = sys.argv[1], sys.argv[2], sys.argv[3:]
    def one(path):
        try:
            open(path, "rb").read().decode()
        except UnicodeDecodeError:
            return path, None, None
        return path, run(oracle, path).strip(), run(port, path).strip()
    same = collections.Counter()
    skipped = collections.Counter()
    bad = []
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for path, want, got in pool.map(one, files):
            if want is None:
                continue
            if got == "SKIP":
                skipped[want.split()[1] if want.startswith("ERR") else "OK"] += 1
            elif got == want:
                same[want.split()[1] if want.startswith("ERR") else "OK"] += 1
            else:
                bad.append((path, want, got))
    for path, want, got in bad[:40]:
        print(f"DIFFERENT {path}\n  oracle: {want}\n  port:   {got}")
    print(f"identical: {sum(same.values())}  skipped (not ported): {sum(skipped.values())}  different: {len(bad)}")
    print("identical by answer:", dict(sorted(same.items())))
    print("skipped by what the oracle said:", dict(sorted(skipped.items())))
    sys.exit(1 if bad else 0)

if __name__ == "__main__":
    main()
