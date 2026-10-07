#!/usr/bin/env python3
"""Compare the body checker port (`bodies.cho`) with the Rust checker, function by function.

    bodies_diff.py [--std] ORACLE PORT file...

ORACLE is `check_bodies` (`crates/cancho-ir/examples/check_bodies.rs`), PORT is `bodies.cho` built
(`bodies_files.cho` with `--std`). Both write the first refusal of the parser or the declarations
as one line, or one line for each function, `fn <start> <end>` and `OK` or `ERR rule start end`;
the port may write `SKIP` for a function whose body uses something it does not check yet. A function
the port skips is not compared and is counted, by what the oracle said, so the report shows how much
of the corpus the port has reached and how much of what it reached is refusals.

With `--std` each program is parsed with the standard library, as `cancho check --std` does, and
the library's own functions are in the report.
Exit status 1 if any compared function differs.
"""
import collections, concurrent.futures, glob, subprocess, sys

STD = None

def stream(path):
    global STD
    if STD is None:
        STD = [open(f, "rb").read() for f in sorted(glob.glob("std/*.cho"))]
    out = b""
    for data in [open(path, "rb").read()] + STD:
        out += b"FILE %d\n" % len(data) + data
    return out

def run(cmd, path, with_std, oracle):
    if with_std:
        args = [cmd] + (["--files"] if oracle else [])
        return subprocess.run(args, input=stream(path), capture_output=True, timeout=300).stdout.decode()
    with open(path, "rb") as f:
        return subprocess.run([cmd], stdin=f, capture_output=True, timeout=300).stdout.decode()

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
        return path, run(oracle, path, with_std, True).splitlines(), run(port, path, with_std, False).splitlines()

    same = collections.Counter()
    skipped = collections.Counter()
    bad = []
    programs = 0
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for path, want, got in pool.map(one, files):
            if want is None:
                continue
            programs += 1
            if len(want) != len(got):
                bad.append((path, f"{len(want)} lines", f"{len(got)} lines"))
                continue
            for w, g in zip(want, got):
                answer = w.split(" ", 3)
                key = (answer[3].split()[1] if len(answer) > 3 and answer[3].startswith("ERR") else "OK") if w.startswith("fn ") else (w.split()[1] if w.startswith("ERR") else "?")
                if g.endswith(" SKIP"):
                    skipped[key] += 1
                elif g == w:
                    same[key] += 1
                else:
                    bad.append((path, w, g))
    for path, want, got in bad[:40]:
        print(f"DIFFERENT {path}\n  oracle: {want}\n  port:   {got}")
    print(f"programs: {programs}  functions (and refusals before them) identical: {sum(same.values())}  "
          f"skipped (not ported): {sum(skipped.values())}  different: {len(bad)}")
    print("identical by answer:", dict(sorted(same.items())))
    print("skipped by what the oracle said:", dict(sorted(skipped.items())))
    sys.exit(1 if bad else 0)

if __name__ == "__main__":
    main()
