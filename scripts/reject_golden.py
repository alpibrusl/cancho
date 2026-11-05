#!/usr/bin/env python3
"""Record or check the reject goldens (docs/removing-rust.md phase 1b, #488, #491).

    scripts/reject_golden.py record CANCHO   # rewrite tests/reject.golden from CANCHO's `check --output json`
    scripts/reject_golden.py check CANCHO    # the rules CANCHO refuses each program by (as a multiset) against tests/reject.golden; exit 1 on a difference
    scripts/reject_golden.py check CANCHO --exact   # also line:column and message (the Rust front end's wording; the compiler in cancho differs by design)

tests/reject.golden has one line per refusal: file, rule, line:column, message (tab separated; `\\n`, `\\t`, `\\\\` escaped).
It was recorded once from the Rust front end, so that the answer no longer depends on that compiler existing. `check`
with the `cancho` written in cancho is the test that it still gives the same rule, position and sentence. Re-record only
when a refusal is changed on purpose, in the PR that changes it, saying why.

`check` compares rules only, by decision (#491, 2026-11-05): the compiler in cancho says the rule's sentence (#479) and reports some
positions at another column than the Rust front end. Programs in tests/reject.known are skipped (one per line, `name  # reason`):
they cannot be refused yet by a compiler with no WebAssembly target (#486). `check` also prints how many programs differ in position, as a count.
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


def load(rows):
    table = {}
    for row in rows:
        f = row.split("\t")
        table.setdefault(f[0], []).append((f[1], f[2], row))
    return table


def main():
    args = sys.argv[1:]
    exact = "--exact" in args
    args = [a for a in args if a != "--exact"]
    if len(args) != 2 or args[0] not in ("record", "check"):
        sys.exit(__doc__)
    got = lines(args[1])
    if args[0] == "record":
        open("tests/reject.golden", "w").write("\n".join(HEADER + got) + "\n")
        print(f"recorded {len(got)} refusals")
        return
    want = [l for l in open("tests/reject.golden").read().splitlines() if not l.startswith("#")]
    known = {l.split("#")[0].strip() for l in open("tests/reject.known").read().splitlines() if l.split("#")[0].strip()}
    if exact:
        bad = [f"- {w}" for w in want if w not in got] + [f"+ {g}" for g in got if g not in want]
        print("\n".join(bad) if bad else f"ok: {len(got)} refusals agree exactly")
        sys.exit(1 if bad else 0)
    W, G = load(want), load(got)
    bad, moved = [], 0
    for name in sorted(W):
        if name in known:
            continue
        rw = sorted(r for r, _, _ in W[name])
        rg = sorted(r for r, _, _ in G.get(name, []))
        if rw != rg:
            bad.append(f"{name}: golden {rw}, got {rg}")
        elif sorted(p for _, p, _ in W[name]) != sorted(p for _, p, _ in G[name]):
            moved += 1
    print("\n".join(bad) if bad else f"ok: {len(W) - len(known)} programs refused by the same rules ({len(known)} known, {moved} at another position)")
    sys.exit(1 if bad else 0)


main()
