#!/usr/bin/env python3
"""R3 / V2 of docs/vcs-cli.md: replay every historical two-parent merge of a git
repository through `cancho vcs merge --dry-run` and compare it with what git
does (`git merge-tree --write-tree`) and with what the human merge recorded.

Measurement tooling only: it runs git to read the original and `cancho` to
merge; the product never runs git. The cancho store must be an `import-git`
of the same repository (its `gitmap` file maps commit ids to change ids).

Usage: python3 -I scripts/vcs_r3.py <cancho binary> <git repo> <imported dir> [--gate] [--pairs N]
With --gate the gate runs on each clean merge (CANCHO_PACKAGES is used if set).
With --pairs N it also merges N seeded random pairs of divergent commits of
`git rev-list --all` (no human result exists for those: only whether both
tools call the merge clean or conflicted, and on which paths, is compared).
"""
import collections, json, os, subprocess, sys, time


def git(repo, *a):
    p = subprocess.run(["git", "-C", repo, *a], capture_output=True)
    return p.returncode, p.stdout.decode("utf-8", "replace")


def pairs(binary, repo, work, gm, n, gate=False):
    import random
    random.seed(20261105)
    commits = [c for c in git(repo, "rev-list", "--all")[1].split() if c in gm]
    c = collections.Counter()
    bad = []
    tries = 0
    while c["pairs"] < n and tries < n * 20:
        tries += 1
        a, b = random.sample(commits, 2)
        if git(repo, "merge-base", "--is-ancestor", a, b)[0] == 0 or git(repo, "merge-base", "--is-ancestor", b, a)[0] == 0:
            continue
        if git(repo, "merge-base", a, b)[0] != 0:
            continue
        rc, out = git(repo, "merge-tree", "--write-tree", "--name-only", a, b)
        g_conf = []
        parts = out.split("\n\n")
        if rc != 0 and len(parts) > 1:
            g_conf = sorted(set(parts[0].splitlines()[1:]))
        p = subprocess.run([binary, "vcs", "merge", gm[b], "--onto", gm[a], "--dry-run", "--time", "1",
                            "--intent-prompt", "r3", "--output", "json"] + ([] if gate else ["--no-gate"]),
                           capture_output=True, cwd=work)
        try:
            doc = json.loads(p.stdout)
        except Exception:
            c["cancho no json"] += 1
            continue
        c["pairs"] += 1
        if not doc.get("ok"):
            c["cancho refused " + doc["refused"]["tag"]] += 1
            if gate and doc["refused"]["tag"] == "merge-gate-failed":
                c["gate failures of merges that git calls clean"] += rc == 0
                bad.append((a[:10], b[:10], doc["refused"]["message"][:140]))
            continue
        cc = [(x["path"], x["tag"]) for x in doc.get("conflicts", [])]
        cclean = not cc
        c["git clean"] += rc == 0
        c["cancho clean"] += cclean
        c["agree on clean/conflict"] += (rc == 0) == cclean
        if rc != 0 and not cclean:
            cp = {x[0] for x in cc}
            gp = set(g_conf)
            key = "same paths" if cp == gp else "cancho a subset (finer)" if cp < gp else "git a subset" if gp < cp else "other"
            c["both conflict: " + key] += 1
        if (rc == 0) != cclean:
            bad.append((a[:10], b[:10], "git " + ("clean" if rc == 0 else "conflict"), cc[:3], g_conf[:3]))
        for _, tag in cc:
            c["conflict tag: " + tag] += 1
        for x in doc.get("merged_paths", []):
            c["paths merged by layer: " + x["layer"]] += 1
    print(f"random divergent pairs ({tries} tried):")
    for k in sorted(c):
        print(f"  {k}: {c[k]}")
    for b in bad[:25]:
        print("   disagreement:", b)


def main():
    binary, repo, work = sys.argv[1:4]
    gate = "--gate" in sys.argv
    gm = {}
    for line in open(os.path.join(work, ".cancho/vcs/gitmap")):
        a, b = line.split()
        gm[a] = b
    if "--pairs" in sys.argv:
        pairs(binary, repo, work, gm, int(sys.argv[sys.argv.index("--pairs") + 1]), gate)
    merges = git(repo, "rev-list", "--merges", "HEAD")[1].split()
    rows = []
    env = dict(os.environ)
    t0 = time.time()
    for m in merges:
        parents = git(repo, "rev-list", "--parents", "-n1", m)[1].split()[1:]
        if len(parents) != 2:
            continue
        p1, p2 = parents
        human = git(repo, "rev-parse", m + "^{tree}")[1].strip()
        rc, out = git(repo, "merge-tree", "--write-tree", "--name-only", p1, p2)
        lines = out.splitlines()
        g_clean = rc == 0
        g_tree = lines[0] if lines else ""
        g_conf = [] if g_clean else sorted({l for l in lines[1:] if l and not l.startswith("CONFLICT") and "/" not in l[:0]})[:0]
        args = [binary, "vcs", "merge", gm[p2], "--onto", gm[p1], "--dry-run", "--time", "1",
                "--intent-prompt", "r3", "--output", "json"]
        if not gate:
            args.append("--no-gate")
        p = subprocess.run(args, capture_output=True, cwd=work, env=env)
        try:
            doc = json.loads(p.stdout)
        except Exception:
            doc = {"ok": False, "refused": {"tag": "no-json", "message": p.stderr.decode()[:200]}}
        shown = subprocess.run([binary, "vcs", "show", gm[m], "--output", "json"],
                               capture_output=True, cwd=work)
        human_tree = json.loads(shown.stdout).get("tree")
        if doc.get("ok"):
            if doc.get("open_conflicts"):
                status = "conflict"
            elif doc.get("up_to_date") or doc.get("fast_forward"):
                status = "trivial"
            else:
                status = "clean"
        else:
            status = "refused:" + doc["refused"]["tag"]
        rows.append({
            "merge": m, "git_clean": g_clean, "git_eq_human": g_clean and g_tree == human,
            "cancho": status, "cancho_eq_human": doc.get("tree") == human_tree,
            "conflicts": [(c["path"], c["tag"]) for c in doc.get("conflicts", [])],
            "layers": [x["layer"] for x in doc.get("merged_paths", [])],
            "git_conflict_paths": [l for l in lines[1:] if l and "\t" not in l and not l.startswith("Auto-merging") and not l.startswith("CONFLICT")] if not g_clean else [],
        })
    n = len(rows)
    c = collections.Counter()
    for r in rows:
        c["merges"] += 1
        c["git clean"] += r["git_clean"]
        c["git conflicts"] += not r["git_clean"]
        c["git clean and equals the human merge"] += r["git_eq_human"]
        c["cancho " + r["cancho"].split(":")[0]] += 1
        if r["cancho"].startswith("refused"):
            c["cancho " + r["cancho"]] += 1
        c["cancho clean and equals the human merge"] += r["cancho"] in ("clean", "trivial") and r["cancho_eq_human"]
        both = r["git_clean"] and r["cancho"] == "clean"
        c["both clean"] += both
        c["both clean, cancho tree = human, git tree = human"] += both and r["cancho_eq_human"] and r["git_eq_human"]
        c["git clean, cancho conflict"] += r["git_clean"] and r["cancho"] == "conflict"
        c["git conflict, cancho clean"] += (not r["git_clean"]) and r["cancho"] == "clean"
        c["both conflict"] += (not r["git_clean"]) and r["cancho"] == "conflict"
        for layer in r["layers"]:
            c["paths merged by layer: " + layer] += 1
        for _, tag in r["conflicts"]:
            c["conflict tag: " + tag] += 1
    print(f"{repo}: {n} merges in {time.time()-t0:.0f}s")
    for k in sorted(c):
        print(f"  {k}: {c[k]}")
    print("  disagreements (git clean, cancho conflict | git conflict, cancho clean):")
    for r in rows:
        if (r["git_clean"] and r["cancho"] == "conflict") or ((not r["git_clean"]) and r["cancho"] == "clean"):
            print("   ", r["merge"][:10], "git", "clean" if r["git_clean"] else "conflict", "cancho", r["cancho"], r["conflicts"][:3])
    json.dump(rows, open(os.path.join(work, "r3-rows.json"), "w"), indent=1)


main()
