#!/usr/bin/env python3
"""The builtin tables of the compiler in cancho, owned by `examples/selfhost` (cancho#487, third box).

Until this script, `builtins.cho`, `signatures.cho` and `tables.cho` were generated from the Rust crates by
`crates/cancho-ir/src/tests/selfhost_tables.rs`. They are now checked-in data that this script reads, checks and extends. It does not need
Rust. The format is the one that test wrote: `static NAME: [int] { let t = alloc_slice[static](LEN, 0); t[i] = v; ... return t; }` with only
the non-zero entries listed, and `fn NAME() -> [] &static [byte] { return "..."; }` for the name blobs.

    scripts/builtin_tables.py check                      # the invariants below; exit 1 and say which one if any fails
    scripts/builtin_tables.py show NAME                  # one builtin, decoded
    scripts/builtin_tables.py append NAME --since N --regions R --effects "dir_read" --sig "<ints>"   # add a builtin at the end
    scripts/builtin_tables.py append NAME --like OTHER [--since N]   # the same, copying the signature, regions and effects of OTHER (and its edition unless --since)

Builtins are appended, never inserted: every per-builtin array is indexed by the builtin's position, so inserting would renumber all that follow.
The order is also what `tests/builtins.golden` lists, and `check` requires the two to agree.

A signature is the run of integers `builtin_sigs` holds for the builtin: 1 if the lowering checks it by hand (its table signature is `() -> ()`),
else 0; the number of parameters; each parameter's type and then the result's, each as a prefix code: 1 to 6 are `int`, `byte`, `float`, `f32`,
`bool` and `()`; 10 and 11 a shared and a unique reference, then the region (a parameter's position, or 99 for `static`) and the referent; 12 a
slice, then its element; 13 a prelude type, then its index, how many arguments, and them; 14 a tuple, its size and parts; 15 a type parameter,
then its position; 16 a literal type; 17 `c_ptr`. `show` decodes one so a person can read what they are about to add.
"""
import argparse, os, re, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SELF = os.path.join(ROOT, "examples", "selfhost")
FILES = {k: os.path.join(SELF, k + ".cho") for k in ("tables", "builtins", "signatures")}
GOLDEN = os.path.join(ROOT, "tests", "builtins.golden")

TABLE = re.compile(r"static (\w+): \[int\] \{\n    let t = alloc_slice\[static\]\((\d+), 0\);\n((?:    t\[\d+\] = \d+;\n)*)    return t;\n\}\n")
BLOB = re.compile(r'fn (\w+)\(\) -> \[\] &static \[byte\] \{\n    return "([^"\n]*)";\n\}\n')
COUNT = re.compile(r"pub fn (\w+)\(\) -> \[\] int \{\n    return (\d+);\n\}\n")


class File:
    def __init__(self, key):
        self.path, self.key = FILES[key], key
        self.text = open(self.path).read()

    def table(self, name):
        for m in TABLE.finditer(self.text):
            if m.group(1) == name:
                n = int(m.group(2)); vals = [0] * n
                for i, v in re.findall(r"t\[(\d+)\] = (\d+);", m.group(3)): vals[int(i)] = int(v)
                return vals
        raise SystemExit(f"{self.path}: no table {name}")

    def set_table(self, name, values):
        """Rewrite one table in the renderer's format (length at least 1, non-zero entries only)."""
        body = "".join(f"    t[{i}] = {v};\n" for i, v in enumerate(values) if v)
        new = f"static {name}: [int] {{\n    let t = alloc_slice[static]({max(len(values), 1)}, 0);\n{body}    return t;\n}}\n"
        for m in TABLE.finditer(self.text):
            if m.group(1) == name:
                self.text = self.text[:m.start()] + new + self.text[m.end():]
                return
        raise SystemExit(f"{self.path}: no table {name}")

    def blob(self, name):
        for m in BLOB.finditer(self.text):
            if m.group(1) == name: return m.group(2)
        raise SystemExit(f"{self.path}: no blob {name}")

    def set_blob(self, name, value):
        for m in BLOB.finditer(self.text):
            if m.group(1) == name:
                self.text = self.text[:m.start(2)] + value + self.text[m.end(2):]
                return
        raise SystemExit(f"{self.path}: no blob {name}")

    def count(self, name):
        for m in COUNT.finditer(self.text):
            if m.group(1) == name: return int(m.group(2))
        raise SystemExit(f"{self.path}: no count {name}")

    def set_count(self, name, value):
        for m in COUNT.finditer(self.text):
            if m.group(1) == name:
                self.text = self.text[:m.start(2)] + str(value) + self.text[m.end(2):]
                return
        raise SystemExit(f"{self.path}: no count {name}")

    def save(self):
        open(self.path, "w").write(self.text)


def load():
    return {k: File(k) for k in FILES}


def names_of(t):
    chars, starts, lens = t["tables"].blob("builtin_chars"), t["tables"].table("builtin_starts"), t["tables"].table("builtin_lens")
    return [chars[s:s + n] for s, n in zip(starts, lens)]


def skip_type(s, i):
    """The index just past the type that starts at s[i]."""
    c = s[i]
    if c in (1, 2, 3, 4, 5, 6, 16, 17): return i + 1
    if c in (10, 11): return skip_type(s, i + 2)
    if c == 12: return skip_type(s, i + 1)
    if c == 13:
        j, k = i + 3, s[i + 2]
        for _ in range(k): j = skip_type(s, j)
        return j
    if c == 14:
        j = i + 2
        for _ in range(s[i + 1]): j = skip_type(s, j)
        return j
    if c == 15: return i + 2
    raise SystemExit(f"unknown type code {c} at {i}")


def sig_end(s, start):
    """The end of the signature that starts at `start`: the flag, the count, that many parameters and the result."""
    n, i = s[start + 1], start + 2
    for _ in range(n + 1): i = skip_type(s, i)
    return i


PRIM = {1: "int", 2: "byte", 3: "float", 4: "f32", 5: "bool", 6: "()", 17: "c_ptr", 16: "lit"}


def decode_type(s, i, prelude):
    c = s[i]
    if c in PRIM: return PRIM[c], i + 1
    if c in (10, 11):
        r = "static" if s[i + 1] == 99 else f"r{s[i + 1]}"
        inner, j = decode_type(s, i + 2, prelude)
        return ("&" if c == 10 else "&!") + r + " " + inner, j
    if c == 12:
        inner, j = decode_type(s, i + 1, prelude)
        return f"[{inner}]", j
    if c == 13:
        name = prelude[s[i + 1]] if prelude and s[i + 1] < len(prelude) else f"type#{s[i + 1]}"
        j, args = i + 3, []
        for _ in range(s[i + 2]):
            a, j = decode_type(s, j, prelude); args.append(a)
        return name + (f"[{', '.join(args)}]" if args else ""), j
    if c == 14:
        j, parts = i + 2, []
        for _ in range(s[i + 1]):
            p, j = decode_type(s, j, prelude); parts.append(p)
        return "(" + ", ".join(parts) + ")", j
    if c == 15: return f"T{s[i + 1]}", i + 2
    raise SystemExit(f"unknown type code {c}")


def prelude_names(t):
    chars, starts, lens = t["tables"].blob("prelude_chars"), t["tables"].table("prelude_starts"), t["tables"].table("prelude_lens")
    return [chars[s:s + n] for s, n in zip(starts, lens)]


def effect_names(t):
    chars = t["builtins"].blob("effect_chars")
    starts, lens = t["builtins"].table("effect_name_starts"), t["builtins"].table("effect_name_lens")
    return [chars[s:s + n] for s, n in zip(starts, lens)]


def builtin_record(t, b):
    sigs, starts = t["signatures"].table("builtin_sigs"), t["signatures"].table("builtin_sig_starts")
    s = starts[b]; e = sig_end(sigs, s)
    effs = effect_names(t)
    es, ec, stream = t["builtins"].table("builtin_effect_starts")[b], t["builtins"].table("builtin_effect_counts")[b], t["builtins"].table("builtin_effect_stream")
    labels = [(effs[v // 2], bool(v % 2)) for v in stream[es:es + ec]]
    return dict(name=names_of(t)[b], since=t["tables"].table("builtin_sinces")[b], regions=t["builtins"].table("builtin_region_counts")[b],
                sig=sigs[s:e], labels=labels, raw_effects=stream[es:es + ec])


def show_sig(sig, prelude):
    n, i, params = sig[1], 2, []
    for _ in range(n):
        p, i = decode_type(sig, i, prelude); params.append(p)
    ret, _ = decode_type(sig, i, prelude)
    return f"({', '.join(params)}) -> {ret}" + ("   [checked by hand in the lowering]" if sig[0] else "")


def check(t):
    problems = []
    n = t["tables"].count("builtin_count")
    names = names_of(t)
    if len(names) != n: problems.append(f"builtin_count is {n} but builtin_starts lists {len(names)}")
    for key, name in (("tables", "builtin_starts"), ("tables", "builtin_lens"), ("tables", "builtin_sinces"), ("builtins", "builtin_region_counts"),
                      ("builtins", "builtin_effect_starts"), ("builtins", "builtin_effect_counts"), ("signatures", "builtin_sig_starts")):
        if len(t[key].table(name)) != max(n, 1): problems.append(f"{name} has {len(t[key].table(name))} entries, not {n}")
    chars, lens, starts = t["tables"].blob("builtin_chars"), t["tables"].table("builtin_lens"), t["tables"].table("builtin_starts")
    if sum(lens) != len(chars): problems.append(f"builtin_chars has {len(chars)} characters, builtin_lens sum to {sum(lens)}")
    pos = 0
    for i, (s, l) in enumerate(zip(starts, lens)):
        if s != pos: problems.append(f"builtin {i} ({names[i]}) starts at {s} in builtin_chars, expected {pos}"); break
        pos += l
    if len(set(names)) != len(names): problems.append("two builtins share a name")
    golden = [l.strip() for l in open(GOLDEN) if l.strip()] if os.path.exists(GOLDEN) else []
    if golden != names:
        first = next((i for i, (a, b) in enumerate(zip(golden, names)) if a != b), min(len(golden), len(names)))
        problems.append(f"tests/builtins.golden ({len(golden)}) and the tables ({len(names)}) differ from entry {first}")
    sigs, sstarts = t["signatures"].table("builtin_sigs"), t["signatures"].table("builtin_sig_starts")
    pos = 0
    for i in range(n):
        if sstarts[i] != pos: problems.append(f"signature of builtin {i} ({names[i]}) starts at {sstarts[i]}, expected {pos}"); break
        try: pos = sig_end(sigs, sstarts[i])
        except (IndexError, SystemExit): problems.append(f"signature of builtin {i} ({names[i]}) does not decode"); break
    else:
        if pos != len([1 for _ in sigs]) and any(sigs[pos:]): problems.append(f"builtin_sigs has non-zero data after the last signature (at {pos})")
    es, ec, stream = (t["builtins"].table(x) for x in ("builtin_effect_starts", "builtin_effect_counts", "builtin_effect_stream"))
    pos = 0
    for i in range(n):
        if es[i] != pos: problems.append(f"effects of builtin {i} ({names[i]}) start at {es[i]}, expected {pos}"); break
        pos += ec[i]
    ne = len(effect_names(t))
    if ne != t["builtins"].count("effect_count"): problems.append("effect_count disagrees with effect_name_starts")
    if any(v // 2 >= ne for v in stream): problems.append("builtin_effect_stream names an effect that does not exist")
    return problems


def append(t, name, since, regions, effects, sig, like):
    n = t["tables"].count("builtin_count")
    names = names_of(t)
    if name in names: raise SystemExit(f"{name} is already builtin {names.index(name)}")
    if like:
        if like not in names: raise SystemExit(f"no builtin called {like}")
        rec = builtin_record(t, names.index(like))
        since = since if since is not None else rec["since"]
        regions, sig, raw = rec["regions"], rec["sig"], rec["raw_effects"]
    else:
        effs = effect_names(t); raw = []
        for e in effects:
            label, has_arg = (e[:-1], True) if e.endswith("*") else (e, False)
            if label not in effs: raise SystemExit(f"effect `{label}` is not in the table; add it with the effect name tables first")
            raw.append(effs.index(label) * 2 + int(has_arg))
    tb, bu, si = t["tables"], t["builtins"], t["signatures"]
    chars = tb.blob("builtin_chars")
    tb.set_blob("builtin_chars", chars + name)
    tb.set_table("builtin_starts", tb.table("builtin_starts")[:n] + [len(chars)])
    tb.set_table("builtin_lens", tb.table("builtin_lens")[:n] + [len(name)])
    tb.set_table("builtin_sinces", tb.table("builtin_sinces")[:n] + [since])
    tb.set_count("builtin_count", n + 1)
    bu.set_table("builtin_region_counts", bu.table("builtin_region_counts")[:n] + [regions])
    stream = bu.table("builtin_effect_stream")
    used = bu.table("builtin_effect_starts")[n - 1] + bu.table("builtin_effect_counts")[n - 1]
    bu.set_table("builtin_effect_stream", stream[:used] + raw)
    bu.set_table("builtin_effect_starts", bu.table("builtin_effect_starts")[:n] + [used])
    bu.set_table("builtin_effect_counts", bu.table("builtin_effect_counts")[:n] + [len(raw)])
    sigs = si.table("builtin_sigs"); end = sig_end(sigs, si.table("builtin_sig_starts")[n - 1])
    si.set_table("builtin_sigs", sigs[:end] + list(sig))
    si.set_table("builtin_sig_starts", si.table("builtin_sig_starts")[:n] + [end])
    for f in t.values(): f.save()
    with open(GOLDEN, "a") as g: g.write(name + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check")
    sp = sub.add_parser("show"); sp.add_argument("name")
    sa = sub.add_parser("append"); sa.add_argument("name"); sa.add_argument("--since", type=int); sa.add_argument("--regions", type=int)
    sa.add_argument("--effects", default=""); sa.add_argument("--sig", default=""); sa.add_argument("--like")
    a = ap.parse_args(); t = load()
    if a.cmd == "check":
        problems = check(t)
        for p in problems: print("FAILS", p)
        if problems: sys.exit(1)
        print(f"ok {t['tables'].count('builtin_count')} builtins, {len(effect_names(t))} effect names, tables and tests/builtins.golden agree")
    elif a.cmd == "show":
        names = names_of(t)
        if a.name not in names: raise SystemExit(f"no builtin called {a.name}")
        r = builtin_record(t, names.index(a.name))
        print(f"{r['name']}  (builtin {names.index(a.name)}, edition {r['since']}, {r['regions']} region parameter(s))")
        print("  signature:", show_sig(r["sig"], prelude_names(t)))
        print("  effects:  ", " ".join(n + ("(..)" if arg else "") for n, arg in r["labels"]) or "none")
        print("  sig ints: ", " ".join(map(str, r["sig"])))
    else:
        sig = [int(x) for x in a.sig.split()] if a.sig else []
        if not a.like and (a.since is None or a.regions is None or not sig): raise SystemExit("give --like, or --since, --regions and --sig")
        append(t, a.name, a.since, a.regions, [e for e in a.effects.split() if e], sig, a.like)
        problems = check(load())
        for p in problems: print("FAILS", p)
        if problems: sys.exit(1)
        print(f"appended {a.name} as builtin {t['tables'].count('builtin_count') - 1}; the tables check")


if __name__ == "__main__":
    main()
