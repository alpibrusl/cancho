#!/usr/bin/env python3
"""Record or check the reject goldens (docs/removing-rust.md phase 1b, #488, #491).

    scripts/reject_golden.py record CANCHO   # rewrite tests/reject.golden from CANCHO's `check --output json`
    scripts/reject_golden.py check CANCHO    # compare CANCHO against tests/reject.golden; exit 1 on any difference

tests/reject.golden has one line per refusal: file, rule, line:column, message (tab separated; `\\n`, `\\t`, `\\\\` escaped).
It was recorded once from the Rust front end, so that the answer no longer depends on that compiler existing. `check`
with the `cancho` written in cancho is the test that it still gives the same rule, position and sentence. Re-record only
when a refusal is changed on purpose, in the PR that changes it, saying why.
"""
import glob, json, os, subprocess, sys

HEADER = [
    "# tests/reject.golden: what `cancho check --output json` says about each program of tests/reject.",
    "# file<TAB>rule<TAB>line:column<TAB>message. Recorded from the Rust front end (2026-11-05, main 4fafbef2).",
    "# scripts/reject_golden.py check CANCHO compares; do not regenerate from the compiler under test.",
]


def lines(cancho):
    out = []
    for path in sorted(glob.glob("tests/reject/*.cho")):
        text = open(path).read().splitlines()
        target = [l[len("//~ TARGET "):].strip() for l in text if l.startswith("//~ TARGET ")]
        cmd = [cancho, "check", path, "--output", "json"] + (["--target", target[0]] if target else [])
        ran = subprocess.run(cmd, capture_output=True, text=True)
        name = os.path.basename(path)
        try:
            refused = json.loads(ran.stdout)["refused"]
        except Exception:
            out.append(f"{name}\tNOJSON\texit{ran.returncode}")
            continue
        for r in refused:
            p = r["position"]
            at = f"{p['line']}:{p['column']}" if p else "-"
            msg = r["message"].replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")
            out.append("\t".join([name, r["rule"], at, msg]))
    return out


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("record", "check"):
        sys.exit(__doc__)
    got = lines(sys.argv[2])
    if sys.argv[1] == "record":
        open("tests/reject.golden", "w").write("\n".join(HEADER + got) + "\n")
        print(f"recorded {len(got)} refusals")
        return
    want = [l for l in open("tests/reject.golden").read().splitlines() if not l.startswith("#")]
    bad = [f"- {w}" for w in want if w not in got] + [f"+ {g}" for g in got if g not in want]
    print("\n".join(bad) if bad else f"ok: {len(got)} refusals agree")
    sys.exit(1 if bad else 0)


main()
