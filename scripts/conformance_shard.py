#!/usr/bin/env python3
"""The tests of tests/conformance that one CI shard runs (docs/ci-speed.md section 5).

    conformance_shard.py <shards> <index> [--skip-file FILE]      (index counts from 1)

Prints the full name of each test of the shard, one to a line, for `cargo test -p cancho --test conformance -- --exact ...`.
The tests are listed from the test binary itself, so a new test is in a shard the day it is written; whole modules go to the
shard with the least weight so far, heaviest first, by `.github/conformance-weights.txt` (a module not in it weighs 3).
`--skip-file` names a file of test names, `#` comments allowed (`.github/selfhost-known-differences.txt`), left out of every shard.
Every test is in exactly one shard: `--check <shards>` says so, or fails.
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UNKNOWN = 3.0


def tests():
    out = subprocess.run(
        ["cargo", "test", "-p", "cancho", "--test", "conformance", "--", "--list", "--format", "terse"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return [line[: -len(": test")] for line in out.splitlines() if line.endswith(": test")]


def weights():
    found = {}
    for line in (ROOT / ".github" / "conformance-weights.txt").read_text().splitlines():
        line = line.split("#")[0].split()
        if len(line) == 2:
            found[line[0]] = float(line[1])
    return found


def shards(names, count):
    by_module = {}
    for name in names:
        by_module.setdefault(name.split("::")[0], []).append(name)
    weight = weights()
    order = sorted(by_module, key=lambda m: (-weight.get(m, UNKNOWN), m))
    load = [0.0] * count
    out = [[] for _ in range(count)]
    for module in order:
        k = min(range(count), key=lambda i: (load[i], i))
        load[k] += weight.get(module, UNKNOWN)
        out[k].extend(by_module[module])
    return out, load


def main(argv):
    skip = set()
    if "--skip-file" in argv:
        at = argv.index("--skip-file")
        for line in Path(argv[at + 1]).read_text().splitlines():
            line = line.split("#")[0].split()
            if line:
                skip.add(line[0])
        del argv[at : at + 2]
    names = [n for n in tests() if n not in skip]
    if argv[0] == "--check":
        count = int(argv[1])
        parts, load = shards(names, count)
        flat = [n for part in parts for n in part]
        assert sorted(flat) == sorted(set(flat)) == sorted(names), "a test is in no shard or in two"
        print("%d tests in %d shards, weights %s" % (len(names), count, [round(x) for x in load]))
        return 0
    count, index = int(argv[0]), int(argv[1])
    parts, _ = shards(names, count)
    print("\n".join(parts[index - 1]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
