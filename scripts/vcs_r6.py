#!/usr/bin/env python3
"""R6 of docs/vcs-cli.md: how the non-.cho files of a real history are touched,
and how often three merge strategies would conflict on its historical merges.

Measurement tooling only (it runs git and reads a repository); the product
code never does. Usage: python3 -I scripts/vcs_r6.py <repo> [<repo>...]
"""
import collections, json, subprocess, sys, tomllib, os


def git(repo, *a, raw=False, check=True):
    p = subprocess.run(["git", "-C", repo, *a], capture_output=True)
    if check and p.returncode != 0:
        return None
    return p.stdout if raw else p.stdout.decode("utf-8", "replace")


def kind(path):
    base = os.path.basename(path)
    if base == "cancho.toml":
        return "cancho.toml"
    if path.startswith(".github/"):
        return "ci-yaml"
    ext = base.rsplit(".", 1)[-1] if "." in base else "(none)"
    return ext


def touches(repo):
    """File touches by type over every non-merge commit of HEAD."""
    commits = git(repo, "rev-list", "--no-merges", "HEAD").split()
    per_kind = collections.Counter()
    commits_with = collections.Counter()
    for c in commits:
        files = git(repo, "diff-tree", "--no-commit-id", "--name-only", "-r", "--root", c).splitlines()
        ks = set()
        for f in files:
            per_kind[kind(f)] += 1
            ks.add(kind(f))
        for k in ks:
            commits_with[k] += 1
    return len(commits), per_kind, commits_with


def blob(repo, rev, path):
    return git(repo, "show", f"{rev}:{path}", raw=True, check=True)


def hunks(a, b):
    """Histogram-diff hunks as (a_start, a_len, b_lines), 0-based, via git."""
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        fa, fb = os.path.join(d, "a"), os.path.join(d, "b")
        open(fa, "wb").write(a)
        open(fb, "wb").write(b)
        p = subprocess.run(["git", "diff", "--no-index", "--histogram", "-U0", "--no-color", fa, fb],
                           capture_output=True)
    out, cur = [], None
    for line in p.stdout.decode("utf-8", "replace").splitlines():
        if line.startswith("@@"):
            h = line.split()[1]  # -s,l
            s, _, l = h[1:].partition(",")
            s = int(s); l = int(l) if l != "" else 1
            if l == 0:
                s += 1  # insertion: after line s
            cur = [s - 1, l, []]
            out.append(cur)
        elif cur is not None and line.startswith("+") and not line.startswith("+++"):
            cur[2].append(line[1:])
    return out


def anchored_ops(base, other):
    """Edit ops (old_text, new_text) with the fewest context lines that make old_text occur once in base."""
    bl = base.decode("utf-8", "replace").split("\n")
    ops = []
    for s, l, new in hunks(base, other):
        lo, hi = s, s + l  # replaced range [lo,hi); insertion has lo==hi
        k = 0
        while True:
            a, b = max(0, lo - k), min(len(bl), hi + k)
            old = "\n".join(bl[a:b])
            if (b > a) and ("\n".join(bl)).count(old) == 1:
                break
            if a == 0 and b == len(bl):
                break
            k += 1
        a, b = max(0, lo - k), min(len(bl), hi + k)
        newtext = "\n".join(bl[a:lo] + new + bl[hi:b])
        ops.append(("\n".join(bl[a:b]), newtext))
    return ops


def apply_ops(text, ops):
    """Apply ops whose old text occurs exactly once; return (text, tag or None)."""
    for old, new in ops:
        n = text.count(old)
        if n == 0:
            return None, "anchor-not-found"
        if n > 1:
            return None, "anchor-ambiguous"
        text = text.replace(old, new, 1)
    return text, None


def line_merge(repo, base, ours, theirs):
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        paths = []
        for n, data in (("o", ours), ("b", base), ("t", theirs)):
            p = os.path.join(d, n); open(p, "wb").write(data); paths.append(p)
        p = subprocess.run(["git", "merge-file", "-p", *paths], capture_output=True)
    return p.stdout, p.returncode != 0


def flat(v, prefix=()):
    if isinstance(v, dict):
        out = {}
        for k, x in v.items():
            out.update(flat(x, prefix + (k,)))
        return out or {prefix: {}}
    return {prefix: v}


def keypath_conflict(fmt, base, ours, theirs):
    load = (lambda b: json.loads(b)) if fmt == "json" else (lambda b: tomllib.loads(b.decode()))
    try:
        fb, fo, ft = (flat(load(x)) for x in (base, ours, theirs))
    except Exception:
        return None  # unparseable on some side
    for k in set(fb) | set(fo) | set(ft):
        b, o, t = fb.get(k, ...), fo.get(k, ...), ft.get(k, ...)
        if o != b and t != b and o != t:
            return True
    return False


def merges(repo):
    rows = []
    for m in git(repo, "rev-list", "--merges", "HEAD").split():
        ps = git(repo, "rev-list", "--parents", "-n1", m).split()[1:]
        if len(ps) != 2:
            continue
        base = (git(repo, "merge-base", ps[0], ps[1]) or "").strip()
        if not base:
            continue
        ch = lambda p: set(git(repo, "diff", "--name-only", base, p).splitlines())
        both = sorted(ch(ps[0]) & ch(ps[1]))
        for f in both:
            if f.endswith(".cho"):
                continue
            b, o, t = (blob(repo, r, f) for r in (base, ps[0], ps[1]))
            if b is None or o is None or t is None or b"\0" in b + o + t:
                rows.append((m, f, kind(f), "skipped", None, None)); continue
            if o == t:
                rows.append((m, f, kind(f), "same-edit", False, False)); continue
            _, lc = line_merge(repo, b, o, t)
            # anchored: theirs' ops applied to ours, and ours' ops applied to theirs
            ot = apply_ops(o.decode("utf-8", "replace"), anchored_ops(b, t))[1]
            to = apply_ops(t.decode("utf-8", "replace"), anchored_ops(b, o))[1]
            ac = ot is not None or to is not None
            kp = None
            if f.endswith(".json"):
                kp = keypath_conflict("json", b, o, t)
            elif f.endswith(".toml"):
                kp = keypath_conflict("toml", b, o, t)
            rows.append((m, f, kind(f), "both-edited", lc, ac, kp))
    return rows, len(git(repo, "rev-list", "--merges", "HEAD").split())


def main():
    for repo in sys.argv[1:]:
        n, per, cw = touches(repo)
        print(f"## {repo}: {n} non-merge commits")
        total = sum(per.values())
        print(f"file touches: {total}; .cho {per['cho']}")
        for k, v in per.most_common(12):
            print(f"  {k:12} touches {v:5}  commits {cw[k]:4}")
        rows, nm = merges(repo)
        both = [r for r in rows if r[3] in ("both-edited", "same-edit")]
        edited = [r for r in rows if r[3] == "both-edited"]
        print(f"merges {nm}; non-.cho files changed on both sides: {len(rows)} "
              f"(identical edit {len([r for r in rows if r[3]=='same-edit'])}, "
              f"skipped binary/missing {len([r for r in rows if r[3]=='skipped'])}, differing {len(edited)})")
        print(f"  line 3-way conflicts: {sum(1 for r in edited if r[4])}")
        print(f"  anchored-op conflicts: {sum(1 for r in edited if r[5])}")
        print(f"  anchored not-found/ambiguous only counted above; merges with >=1 conflicting file: "
              f"line {len({r[0] for r in edited if r[4]})}, anchored {len({r[0] for r in edited if r[5]})}")
        kinds = collections.Counter(r[2] for r in edited)
        for k in kinds:
            sub = [r for r in edited if r[2] == k]
            print(f"  {k:12} n={len(sub):3} line={sum(1 for r in sub if r[4])} anchored={sum(1 for r in sub if r[5])}")
        kp = [r for r in edited if len(r) > 6 and r[6] is not None]
        print(f"  json/toml differing files {len(kp)}: key-path conflicts {sum(1 for r in kp if r[6])}, "
              f"line conflicts on same files {sum(1 for r in kp if r[4])}, anchored {sum(1 for r in kp if r[5])}")


main()
