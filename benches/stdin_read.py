#!/usr/bin/env python3
"""What reading standard input costs (docs/standard-input.md §7): `getchar`, `read_bytes`, and /dev/stdin.

    stdin_read.py CANCHO FILE [REPS]

Builds benches/stdin_read.cho, checks that every mode counts the same bytes and newlines as Python does, and
prints the best of REPS (5) wall times per mode, with the file redirected (`< FILE`) and through a pipe
(`cat FILE |`). For a shared machine: nice -n 19 ionice -c3 taskset -c 0-5 python3 stdin_read.py ...
"""
import os, subprocess, sys, tempfile, time

cancho, path = sys.argv[1], sys.argv[2]
reps = int(sys.argv[3]) if len(sys.argv) > 3 else 5
here = os.path.dirname(os.path.abspath(__file__))
work = tempfile.mkdtemp(prefix="stdin-read-")
exe = os.path.join(work, "stdin_read")
subprocess.run([cancho, "build", "--std", os.path.join(here, "stdin_read.cho"), "-o", exe], check=True, capture_output=True)
data = open(path, "rb").read()
newlines = data.count(b"\n")
want = f"{len(data)} {newlines}"
size = len(data)
for mode in ("getchar", "bulk", "dev"):
    for how in ("redirect", "pipe"):
        best = 9e9
        for _ in range(reps):
            t = time.time()
            if how == "redirect":
                r = subprocess.run([exe, mode], stdin=open(path, "rb"), capture_output=True)
            else:
                cat = subprocess.Popen(["cat", path], stdout=subprocess.PIPE)
                r = subprocess.run([exe, mode], stdin=cat.stdout, capture_output=True)
                cat.wait()
            best = min(best, time.time() - t)
            if r.stdout.decode().strip() != want:
                sys.exit(f"{mode}/{how}: answered {r.stdout.decode().strip()!r}, Python counts {want!r}")
        print(f"{mode:8s} {how:8s} {best:7.3f} s  {size / 1e6 / best:8.0f} MB/s")
