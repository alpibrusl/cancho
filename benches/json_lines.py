#!/usr/bin/env python3
"""Per-document cost of `std.json.parse` and `parse_with` on a JSON-lines file (docs/json.md §3.1).

    json_lines.py gen DIR [ROWS]                      writes DIR/bench.jsonl (default 1,000,000 rows)
    json_lines.py time CANCHO DIR [--threads N ...]   builds benches/json_lines.cho and times each mode
    json_lines.py time CANCHO DIR --without-with      the same for a compiler that has no `parse_with`
                                                      (the lines between WITH-BEGIN and WITH-END are cut)

A mode's cost per line is the difference between ROUNDS (11) rounds and 1 round, over (ROUNDS-1) rounds
and the line count: loading the file through `getchar` is the same in both and drops out. Best of REPS (3)
runs of each. The answer each mode prints ("count sum refused") is compared with the one the generator
computed from the rows, so a mode that is fast because it is wrong is refused.

For a quiet shared machine, wrap it: nice -n 19 ionice -c3 taskset -c 0-5 python3 json_lines.py time ...
"""
import json, os, random, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROUNDS = int(os.environ.get("ROUNDS", "11"))
REPS = int(os.environ.get("REPS", "3"))


def gen(d, rows):
    os.makedirs(d, exist_ok=True)
    rnd = random.Random(1)
    count = total = 0
    with open(f"{d}/bench.jsonl", "w") as f:
        for i in range(rows):
            status = rnd.choice([200, 200, 200, 301, 404, 500])
            size = rnd.randint(0, 99999)
            o = {"id": i, "status": status, "bytes": size, "path": f"/p/{rnd.randint(0, 999)}", "note": f"a,b {i % 7}"}
            f.write(json.dumps(o, separators=(",", ":")) + "\n")
            count += status == 404
            total += size
    with open(f"{d}/truth", "w") as f:
        f.write(f"{count} {total} 0\n")


def build(cancho, work, strip):
    src = open(os.path.join(HERE, "json_lines.cho")).read()
    if strip:
        out, skipping = [], False
        for line in src.split("\n"):
            if line.strip() == "// WITH-BEGIN":
                skipping = True
            elif line.strip() == "// WITH-END":
                skipping = False
            elif not skipping:
                out.append(line)
        src = "\n".join(out)
    path = os.path.join(work, "json_lines.cho")
    open(path, "w").write(src)
    exe = os.path.join(work, "json_lines")
    subprocess.run([cancho, "build", "--std", path, "-o", exe], check=True, capture_output=True)
    return exe


def run(exe, mode, rounds, d, threads):
    best, answer = 9e9, None
    for _ in range(REPS):
        t = time.time()
        r = subprocess.run([exe, mode, str(rounds)] + ([str(threads)] if threads > 1 else []),
                           stdin=open(f"{d}/bench.jsonl"), capture_output=True)
        best = min(best, time.time() - t)
        answer = r.stdout.decode().strip()
    return best, answer


def timing(cancho, d, threads_list, strip):
    work = tempfile.mkdtemp(prefix="json-lines-")
    exe = build(cancho, work, strip)
    truth = open(f"{d}/truth").read().strip()
    lines = sum(1 for _ in open(f"{d}/bench.jsonl", "rb"))
    modes = ["parse", "region"] + ([] if strip else ["with"])
    print(f"{lines} lines; per-line cost = (best of {REPS} with {ROUNDS} rounds - best with 1 round) / {ROUNDS - 1} rounds / lines")
    for threads in threads_list:
        for mode in modes:
            one, a1 = run(exe, mode, 1, d, threads)
            many, a2 = run(exe, mode, ROUNDS, d, threads)
            if a1 != truth or a2 != truth:
                sys.exit(f"{mode} x{threads}: answered {a1!r} / {a2!r}, the rows say {truth!r}")
            per = (many - one) / (ROUNDS - 1)
            print(f"{mode:7s} threads {threads:2d}: {per / lines * 1e9:7.1f} ns/line ({per * 1000:7.1f} ms per round)")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a and a[0] == "gen":
        gen(a[1], int(a[2]) if len(a) > 2 else 1000000)
    elif a and a[0] == "time":
        threads = [1]
        if "--threads" in a:
            i = a.index("--threads")
            threads = [int(x) for x in a[i + 1:] if x.isdigit()]
        timing(a[1], a[2], threads, "--without-with" in a)
    else:
        sys.exit(__doc__)
