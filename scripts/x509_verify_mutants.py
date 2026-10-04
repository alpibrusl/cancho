#!/usr/bin/env python3
"""Mutation check of `packages/x509`'s verifier (docs/x509-verify.md §6.4).

    python3 scripts/x509_verify_mutants.py <lex-sys binary>

Each mutant is `verify.ls` or `names.ls` with one deliberate bug. The package
is copied to a scratch directory, the mutant applied there, and
`tests/programs/x509_verify_driver.ls` built against it. It replays what
`conformance/x509_verify.rs` replays: the OpenSSL matrix, the 14 saved chains
against the system roots, and the x509-limbo subset, each answer compared with
the one recorded. A mutant is killed when any answer differs, the driver
traps, or a file takes over 120 seconds. The unmutated package is run first
and must pass. Exit status 1 if a mutant survives or fails to build.
"""
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VEC = os.path.join(ROOT, "tests/vectors/x509/verify")

# (name, file, the text replaced, its replacement). Each `old` occurs exactly once in its file.
MUTANTS = [
    ("an intermediate's signature not checked", "verify.ls",
     "var c = signature_ok(der, view, jder, jview);", "var c = 0;"),
    ("a root's signature not checked", "verify.ls",
     "var c = signature_ok(der, view, root, rview);", "var c = 0;"),
    ("a wildcard crossing a dot", "names.ls",
     "return same(rest, host[first + 1..len(host)]);",
     "return len(host) > len(rest) && same(rest, host[len(host) - len(rest)..len(host)]);"),
    ("notAfter compared off by one", "verify.ls",
     "if now > view[x509.not_after()] {", "if now >= view[x509.not_after()] {"),
    ("notBefore compared off by one", "verify.ls",
     "if now < view[x509.not_before()] {", "if now <= view[x509.not_before()] {"),
    ("a missed cA", "verify.ls",
     "if view[x509.is_ca()] != 1 && !(root && view[x509.version()] == 1) {", "if false {"),
    ("pathLen counting self-issued intermediates", "verify.ls",
     "        if !self_issued(certs[ranges[2 * i]..ranges[2 * i + 1]], views[w..w + x509.view_len()]) {\n            n = n + 1;\n        }",
     "        n = n + 1;"),
    ("pathLen not checked", "verify.ls", "if pl >= 0 && below > pl {", "if false {"),
    ("a wildcard on a one-label suffix", "names.ls", "        if dots < 1 {", "        if dots < 0 {"),
    ("excluded subtrees ignored", "names.ls", "if code == 0 && t[0] == 0xa1 && found[1] == 1 {", "if false {"),
    ("permitted subtrees ignored", "names.ls",
     "if code == 0 && t[0] == 0xa0 && found[0] == 1 && found[1] == 0 {", "if false {"),
    ("a dNSName subtree matched as a plain suffix", "names.ls",
     "return n > k && int_of(name[n - k - 1]) == 46 && same(sub, name[n - k..n]);",
     "return n > k && same(sub, name[n - k..n]);"),
    ("an iPAddress mask ignored", "names.ls",
     "if int_of(ip[k]) & m != int_of(sub[k]) & m {", "if int_of(ip[k]) & 0 != int_of(sub[k]) & 0 {"),
    ("the leaf's EKU not checked", "verify.ls",
     "if code == 0 && eku >= 0 && eku & x509.eku_server_auth() == 0 {", "if false {"),
    ("the leaf's keyUsage not checked", "verify.ls", "if code == 0 && ku >= 0 && ku & 1 == 0 {", "if false {"),
    ("the budget off", "verify.ls", "pub fn max_signatures() -> [] int {\n    return 64;", "pub fn max_signatures() -> [] int {\n    return 1 << 40;"),
    ("a root's dates not checked", "verify.ls",
     "                    if c == 0 {\n                        c = time_ok(rview, now);\n                    }\n", ""),
    ("the host not matched", "verify.ls", "if !x509_names.san_matches(", "if false && !x509_names.san_matches("),
    ("an RSA key of 1024 bits allowed", "verify.ls", "if bits < 2048 || bits > 4096 {", "if bits < 1024 || bits > 4096 {"),
    ("the AKI not matched against the SKI", "verify.ls",
     "    return bytes.equal(der[a..ae], issuer[k..ke]);\n}", "    return true;\n}"),
    ("an intermediate's dates not checked", "verify.ls",
     "                if c == 0 {\n                    c = time_ok(jview, now);\n                }\n", ""),
    ("keyCertSign not required of an issuer", "verify.ls", "if ku >= 0 && ku >> 5 & 1 == 0 {", "if false {"),
]

FILES = ["matrix.txt", "online.txt", "limbo_subset.txt"]


def recorded(name):
    asked, answered = [], []
    for line in open(os.path.join(VEC, name)):
        line = line.rstrip("\n")
        if line.startswith("= "):
            answered.append(line[2:])
        elif line == "S @roots.pem":
            asked.append("S " + open(os.path.join(VEC, "roots.pem"), "rb").read().hex())
        elif not line.startswith("#"):
            asked.append(line)
    return asked, answered


def evidence(lexsys, pkg, work):
    exe = os.path.join(work, "driver")
    r = subprocess.run([lexsys, "build", "--std", os.path.join(ROOT, "tests/programs/x509_verify_driver.ls"),
                        *[os.path.join(pkg, f) for f in ["verify.ls", "names.ls", "x509.ls"]], "-o", exe],
                       capture_output=True, text=True)
    if r.returncode != 0:
        return "BUILD " + r.stderr.strip().splitlines()[0]
    for name in FILES:
        asked, answered = recorded(name)
        try:
            out = subprocess.run([exe], input=("\n".join(asked) + "\n").encode(), capture_output=True, timeout=120)
        except subprocess.TimeoutExpired:
            return f"{name}: timed out"
        got = out.stdout.decode().splitlines()
        for k, (g, w) in enumerate(zip(got, answered)):
            if g != w:
                return f"{name}: answer {k}, {w} became {g}"
        if len(got) != len(answered):
            return f"{name}: {len(got)} answers of {len(answered)} (exit {out.returncode})"
    return None


def main():
    lexsys = sys.argv[1]
    work = tempfile.mkdtemp(prefix="x509-mutants-")
    pkg = os.path.join(work, "x509")
    src = os.path.join(ROOT, "packages/x509")
    shutil.copytree(src, pkg)
    base = evidence(lexsys, pkg, work)
    if base:
        print(f"the unmutated package fails: {base}")
        sys.exit(1)
    print("unmutated: passes")
    survived = 0
    for name, file, old, new in MUTANTS:
        text = open(os.path.join(src, file)).read()
        assert text.count(old) == 1, f"{name}: the text occurs {text.count(old)} times"
        open(os.path.join(pkg, file), "w").write(text.replace(old, new))
        found = evidence(lexsys, pkg, work)
        shutil.copy(os.path.join(src, file), os.path.join(pkg, file))
        if found is None or found.startswith("BUILD"):
            survived += 1
            print(f"SURVIVED {name}" + (f": {found}" if found else ""))
        else:
            print(f"killed   {name}: {found[:110]}")
    shutil.rmtree(work, ignore_errors=True)
    print(f"{len(MUTANTS) - survived} of {len(MUTANTS)} mutants killed")
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
