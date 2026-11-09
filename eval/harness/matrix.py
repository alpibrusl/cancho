#!/usr/bin/env python3
"""Run the E2/E3 matrix for one model: 9 tasks x (cancho A, cancho B, rust, python, c) x seeds. Resumable: cells already in --out are skipped.

    matrix.py --model anthropic:claude-haiku-5-5 --out eval/data/e2e3_haiku.jsonl [--seeds 5] [--jobs 4]
"""
import argparse, json, os, subprocess, sys
from concurrent.futures import ThreadPoolExecutor
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here)
import tasks

CELLS = [("cancho", "A"), ("cancho", "B"), ("rust", "A"), ("python", "A"), ("c", "A")]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True); ap.add_argument("--out", required=True)
    ap.add_argument("--seeds", type=int, default=5); ap.add_argument("--jobs", type=int, default=4)
    a = ap.parse_args()
    done = set()
    if os.path.exists(a.out):
        for l in open(a.out):
            r = json.loads(l); done.add((r["task"], r["lang"], r["context"], r["seed"]))
    todo = [(t, l, c, s) for s in range(1, a.seeds + 1) for t in tasks.TASKS for (l, c) in CELLS if (t, l, c, s) not in done]
    print(f"{len(todo)} runs to do, {len(done)} done", flush=True)
    def go(x):
        t, l, c, s = x
        subprocess.run([sys.executable, os.path.join(here, "run.py"), "--task", t, "--lang", l, "--model", a.model,
                        "--context", c, "--seed", str(s), "--out", a.out], capture_output=True, text=True)
    with ThreadPoolExecutor(a.jobs) as ex: list(ex.map(go, todo))
    print("finished", flush=True)

if __name__ == "__main__": main()
