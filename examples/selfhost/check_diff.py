#!/usr/bin/env python3
"""Compare the checker port (`check.ls`) with the Rust checker's declarations half.

    check_diff.py [--std] ORACLE PORT file...

ORACLE is `check_declarations` (`crates/lex-sys-ir/examples/check_declarations.rs`), PORT is
`check.ls` built; both read a source file on standard input and print `OK` or `ERR rule start end`.
Both print `OK` or `ERR rule start end`, and must print the same.
With `--std` each program is parsed with the standard library, as `lex-sys check --std` does: the
oracle and the port are given a stream of files (see `driver.ls`), the program's and then every
file of `std/`, and the port is `check_files`.
Exit status 1 if any compared file differs.
"""
import collections, concurrent.futures, glob, subprocess, sys

STD = None

def stream(path):
    """The program's file and then the library's, as `FILE <length>` records."""
    global STD
    if STD is None:
        STD = [open(f, "rb").read() for f in sorted(glob.glob("std/*.ls"))]
    out = b""
    for data in [open(path, "rb").read()] + STD:
        out += b"FILE %d\n" % len(data) + data
    return out

def run(cmd, path, with_std=False):
    if with_std:
        args = [cmd] + (["--files"] if "check_declarations" in cmd or "dump_ast" in cmd else [])
        return subprocess.run(args, input=stream(path), capture_output=True, timeout=120).stdout.decode()
    with open(path, "rb") as f:
        return subprocess.run([cmd], stdin=f, capture_output=True, timeout=120).stdout.decode()

def main():
    args = sys.argv[1:]
    with_std = args[0] == "--std"
    if with_std:
        args = args[1:]
    oracle, port, files = args[0], args[1], args[2:]
    def one(path):
        try:
            open(path, "rb").read().decode()
        except UnicodeDecodeError:
            return path, None, None
        return path, run(oracle, path, with_std).strip(), run(port, path, with_std).strip()
    same = collections.Counter()
    bad = []
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for path, want, got in pool.map(one, files):
            if want is None:
                continue
            if got == want:
                same[want.split()[1] if want.startswith("ERR") else "OK"] += 1
            else:
                bad.append((path, want, got))
    for path, want, got in bad[:40]:
        print(f"DIFFERENT {path}\n  oracle: {want}\n  port:   {got}")
    print(f"identical: {sum(same.values())}  different: {len(bad)}")
    print("identical by answer:", dict(sorted(same.items())))
    sys.exit(1 if bad else 0)

if __name__ == "__main__":
    main()
