#!/usr/bin/env python3
"""Check `vcsdiff.cho` against the Rust `cancho vcs` (docs/vcs-native.md section 6, slice b4d).

The Rust command records a text file again and again; the edits it derives for each change (`show`) are the oracle for `derive_edits` of `vcsdiff.cho`, which
the filter `vcsdifftool` runs on the same pair of texts. Both are the histogram diff of `diff_text.rs`, widened to anchors that occur once (`anchor.rs`): the
edits must be equal, one for one, old and new.

usage: vcsdiff_check.py CANCHO [SEEDS] [PAIRS]
"""
import json, os, random, subprocess, sys, tempfile

cancho = os.path.abspath(sys.argv[1])
seeds = int(sys.argv[2]) if len(sys.argv) > 2 else 3
pairs = int(sys.argv[3]) if len(sys.argv) > 3 else 120
here = os.path.dirname(os.path.abspath(__file__))
work = tempfile.mkdtemp()
tool = os.path.join(work, "dt")
files = ["vcsdifftool", "vcsdiff", "jsonwrite", "lexcore"]
subprocess.run([cancho, "build", *[os.path.join(here, f + ".cho") for f in files], "--std", "-o", tool], check=True, capture_output=True)
env = dict(os.environ, CANCHO_AUTHOR="Ada <a@x>")


def run(args, cwd):
    return subprocess.run([cancho, "vcs", *args, "--output", "json"], cwd=cwd, env=env, capture_output=True, text=True)


def make(rng, size):
    """A text of `size` lines drawn from a small alphabet (so lines repeat), sometimes without a final newline, sometimes with blank lines."""
    alphabet = rng.choice([3, 6, 12, 40, 400])
    lines = []
    for _ in range(size):
        k = rng.randrange(alphabet)
        lines.append("" if rng.random() < 0.05 else f"line {k}")
    text = "\n".join(lines)
    if lines and rng.random() < 0.9:
        text += "\n"
    return text


def mutate(rng, text):
    lines = text.split("\n")
    for _ in range(rng.choice([1, 1, 2, 3, 6, 15])):
        op = rng.choice(["del", "ins", "rep", "dup", "swap", "block"])
        if not lines:
            lines = [""]
        i = rng.randrange(len(lines))
        if op == "del":
            del lines[i]
        elif op == "ins":
            lines.insert(i, f"line {rng.randrange(8)}")
        elif op == "rep":
            lines[i] = f"line {rng.randrange(8)}" if rng.random() < 0.7 else "changed"
        elif op == "dup":
            lines.insert(i, lines[i])
        elif op == "swap" and len(lines) > 1:
            j = rng.randrange(len(lines)); lines[i], lines[j] = lines[j], lines[i]
        elif op == "block":
            j = min(len(lines), i + rng.randrange(1, 6)); lines[i:j] = [f"new {rng.randrange(5)}" for _ in range(rng.randrange(0, 5))]
    out = "\n".join(lines)
    return out


def boundary_texts(n):
    """Texts in which one line occurs n times (the histogram gives up on a region where every line occurs 65 times or more)."""
    a = "a\n" * n
    return [a + "b\nc\n", a + "X\nc\n", "z\n" + a + "X\nc\n", "z\n" + a + "X\nc\nz\n", a + "q\n" + a, a + "q\n" + a + "q\n" + a,
            "m\n" + a + "m\n" + a + "m\n", "m\n" + a[:-2] + "m\n" + a + "m\n"]


checked = differ = fallback = nonempty = 0
smallest = 10**9
if os.path.exists('/tmp/vd_small'):
    os.remove('/tmp/vd_small')
for seed in range(seeds + 8):
    rng = random.Random(seed)
    repo = tempfile.mkdtemp(dir=work)
    run(["init"], repo)
    scripted = boundary_texts(60 + seed) if seed >= seeds else None
    text = scripted[0] if scripted else make(rng, rng.choice([1, 2, 5, 20, 80, 300]))
    path = os.path.join(repo, "f.txt")
    texts = []
    for n in range(len(scripted) if scripted else pairs):
        open(path, "w", newline="").write(text)
        r = run(["record", "--all", "--no-gate", "--time", str(1000 + n), "--intent-prompt", f"p{n}"], repo)
        doc = json.loads(r.stdout)
        if doc.get("unchanged") or not doc.get("ok"):
            text = scripted[(n + 1) % len(scripted)] if scripted else (mutate(rng, text) if rng.random() < 0.9 else make(rng, rng.choice([0, 1, 7, 60, 200])))
            continue
        texts.append((doc["change"], text))
        text = scripted[(n + 1) % len(scripted)] if scripted else (mutate(rng, text) if rng.random() < 0.85 else make(rng, rng.choice([0, 1, 7, 60, 200])))
    for k in range(1, len(texts)):
        change, new = texts[k]
        base = texts[k - 1][1]
        shown = json.loads(run(["show", change], repo).stdout)
        want = [[e["old"], e["new"]] for e in shown["edits"]]
        stdin = (str(len(base.encode())) + "\n").encode() + base.encode() + new.encode()
        got = json.loads(subprocess.run([tool], input=stdin, capture_output=True, check=True).stdout.decode())
        checked += 1
        nonempty += bool(want)
        if len(want) == 1 and want[0][0] == base and base:
            fallback += 1
        if want != got:
            differ += 1
            if os.environ.get("VCSDIFF_DUMP") and (not os.path.exists("/tmp/vd_small") or len(base) + len(new) < smallest):
                smallest = len(base) + len(new)
                open("/tmp/vd_small", "w").write(json.dumps({"base": base, "new": new, "rust": want, "cancho": got}))
            if differ <= 3:
                print(f"DIFFERENT seed {seed} step {k}:\n  rust    {want!r:.300}\n  cancho  {got!r:.300}")
print(f"{checked} pairs ({nonempty} with edits, {fallback} whole-file fallbacks): {differ} differ")
sys.exit(1 if differ else 0)
