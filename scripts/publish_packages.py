#!/usr/bin/env python3
"""Publish `packages/x509`, `packages/tls` and `packages/http-server` as `.cancho-vcs` stores (docs/tls-hooks.md §3).

    python3 scripts/publish_packages.py            # rebuild the committed stores
    python3 scripts/publish_packages.py --check    # fail if they are not what a fresh publish gives

Each module is published into its own store, `packages/<package>/.cancho-vcs/<module>`, requiring the stores of
the modules it imports. `vcs publish --dir` cannot be used: it refuses `--requires`, and `packages/tls` imports
`packages/x509`. So the order and the requirements are read from each file's `module` and `import` lines, and the
modules are published one file at a time, as `docs/package-system.md` §7.4 describes. A store records its
requirements as paths relative to itself, so the layout above is part of the result.

`--check` publishes a copy of the sources into a scratch tree with the same layout and compares every file of every
store with the committed one. Publishing is deterministic (`docs/package-system.md` §7.4), so any difference is a
source that changed without its store.

`packages/http-server` is one module in one file and its store is `packages/http-server/.cancho-vcs` itself (the
layout `examples/api/server.lock` and the locks of `tests/programs` pin), so it is published, and checked, on its
own (`docs/http-server.md` §11.7).

The compiler is `target/release/cancho`, or `CANCHO`.
"""
import filecmp
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PACKAGES = ["x509", "tls"]
FLAT = ["http-server"]
STORE = ".cancho-vcs"


def compiler():
    exe = os.environ.get("CANCHO") or os.path.join(ROOT, "target/release/cancho")
    if not os.path.exists(exe):
        raise SystemExit("build the compiler first: cargo build --release -p cancho")
    return exe


def modules(tree):
    """{module: (package, file, [imported modules that are not std])} for every `.cho` of the packages in `tree`."""
    found = {}
    for pkg in PACKAGES:
        d = os.path.join(tree, "packages", pkg)
        for name in sorted(os.listdir(d)):
            if not name.endswith(".cho"):
                continue
            text = open(os.path.join(d, name)).read()
            mod = re.search(r"^module\s+(\w+)\s*;", text, re.M)
            if not mod:
                raise SystemExit(f"{pkg}/{name} declares no module")
            imports = [i for i in re.findall(r"^import\s+([\w.]+)\s*;", text, re.M) if not i.startswith("std.")]
            found[mod.group(1)] = (pkg, os.path.join(d, name), imports)
    return found


def order(found):
    """The modules, each after everything it imports; refuses a cycle or an import it cannot find."""
    done, out = set(), []

    def visit(m, path):
        if m in done:
            return
        if m in path:
            raise SystemExit("import cycle: " + " -> ".join(path + [m]))
        if m not in found:
            raise SystemExit(f"`{path[-1]}` imports `{m}`, which no file of {PACKAGES} declares")
        for dep in found[m][2]:
            visit(dep, path + [m])
        done.add(m)
        out.append(m)

    for m in sorted(found):
        visit(m, [])
    return out


def store_of(tree, found, m):
    return os.path.join(tree, "packages", found[m][0], STORE, m)


def publish(tree):
    exe = compiler()
    found = modules(tree)
    for pkg in PACKAGES:
        shutil.rmtree(os.path.join(tree, "packages", pkg, STORE), ignore_errors=True)
    with tempfile.TemporaryDirectory() as locks:
        for m in order(found):
            pkg, path, deps = found[m]
            args = [exe, "vcs", "publish", "--store", store_of(tree, found, m), "--std"]
            for dep in deps:
                lock = os.path.join(locks, f"{dep}.lock")
                if not os.path.exists(lock):
                    subprocess.run([exe, "vcs", "lock", "--store", store_of(tree, found, dep), "-o", lock, "--all"],
                                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                args += ["--requires", f"{lock}:{store_of(tree, found, dep)}"]
            r = subprocess.run(args + [path], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            if r.returncode != 0:
                raise SystemExit(f"publishing {pkg}/{m} failed:\n{r.stdout}")
            print(f"{pkg}/{m}: {r.stdout.count('published ')} declarations")


def publish_flat(tree):
    """Each of `FLAT`: its one `.cho` published, with `--std`, into the package's own store."""
    exe = compiler()
    for pkg in FLAT:
        d = os.path.join(tree, "packages", pkg)
        shutil.rmtree(os.path.join(d, STORE), ignore_errors=True)
        sources = [os.path.join(d, n) for n in sorted(os.listdir(d)) if n.endswith(".cho")]
        r = subprocess.run([exe, "vcs", "publish", "--store", os.path.join(d, STORE), "--std"] + sources,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        if r.returncode != 0:
            raise SystemExit(f"publishing {pkg} failed:\n{r.stdout}")
        print(f"{pkg}: {r.stdout.count('published ')} declarations")


def differences(a, b):
    """Paths (relative) that differ between two trees, or are in one only."""
    out = []
    cmp = filecmp.dircmp(a, b)
    out += [os.path.join(a, n) for n in cmp.left_only] + [os.path.join(b, n) for n in cmp.right_only]
    out += [os.path.join(a, n) for n in cmp.diff_files + cmp.funny_files]
    for sub in cmp.common_dirs:
        out += differences(os.path.join(a, sub), os.path.join(b, sub))
    return out


def main():
    if "--check" not in sys.argv[1:]:
        publish(ROOT)
        publish_flat(ROOT)
        return
    with tempfile.TemporaryDirectory() as scratch:
        for pkg in PACKAGES:
            shutil.copytree(os.path.join(ROOT, "packages", pkg), os.path.join(scratch, "packages", pkg),
                            ignore=shutil.ignore_patterns(STORE))
        for pkg in FLAT:
            shutil.copytree(os.path.join(ROOT, "packages", pkg), os.path.join(scratch, "packages", pkg),
                            ignore=shutil.ignore_patterns(STORE))
        publish(scratch)
        publish_flat(scratch)
        bad = []
        for pkg in PACKAGES + FLAT:
            committed = os.path.join(ROOT, "packages", pkg, STORE)
            fresh = os.path.join(scratch, "packages", pkg, STORE)
            if not os.path.isdir(committed):
                bad.append(committed + " (missing)")
                continue
            bad += [p.replace(scratch, "<fresh>").replace(ROOT, "<committed>") for p in differences(committed, fresh)]
        if bad:
            print("the committed stores are not what a fresh publish gives; run scripts/publish_packages.py:")
            print("\n".join("  " + p for p in bad[:20]))
            sys.exit(1)
        print("the committed stores match a fresh publish")


if __name__ == "__main__":
    main()
