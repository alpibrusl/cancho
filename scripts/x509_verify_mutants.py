#!/usr/bin/env python3
"""Mutation check of `packages/x509`'s verifier (docs/x509-verify.md §6.4).

    python3 scripts/x509_verify_mutants.py <cancho binary>

Each mutant is `verify.cho` or `names.cho` with one deliberate bug. The package
is copied to a scratch directory, the mutant applied there, and
`tests/programs/x509_verify_driver.cho` built against it. It replays what
`conformance/x509_verify.rs` replays: the OpenSSL matrix, the 14 saved chains
against the system roots, and the x509-limbo subset, each answer compared with
the one recorded, and the no-name matrix (`chain_matrix.txt`, §10.4). A mutant is killed when any answer differs, the driver
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
    ("a wildcard escaping a leading-dot excluded subtree (#318)", "names.cho",
     " || wild_excluded && dotted && dns_within(value, sub[1..len(sub)]) {", " {"),
    ("an intermediate's signature not checked", "verify.cho",
     "var c = signature_ok(der, view, jder, jview);", "var c = 0;"),
    ("a root's signature not checked", "verify.cho",
     "var c = signature_ok(der, view, root, rview);", "var c = 0;"),
    ("a wildcard crossing a dot", "names.cho",
     "return same(rest, host[first + 1..len(host)]);",
     "return len(host) > len(rest) && same(rest, host[len(host) - len(rest)..len(host)]);"),
    ("notAfter compared off by one", "verify.cho",
     "if now > view[x509.not_after()] {", "if now >= view[x509.not_after()] {"),
    ("notBefore compared off by one", "verify.cho",
     "if now < view[x509.not_before()] {", "if now <= view[x509.not_before()] {"),
    ("a missed cA", "verify.cho",
     "if view[x509.is_ca()] != 1 && !(root && view[x509.version()] == 1) {", "if false {"),
    ("pathLen counting self-issued intermediates", "verify.cho",
     "        if !self_issued(certs[ranges[2 * i]..ranges[2 * i + 1]], views[w..w + x509.view_len()]) {\n            n = n + 1;\n        }",
     "        n = n + 1;"),
    ("pathLen not checked", "verify.cho", "if pl >= 0 && below > pl {", "if false {"),
    ("a wildcard on a one-label suffix", "names.cho", "        if dots < 1 {", "        if dots < 0 {"),
    ("excluded subtrees ignored", "names.cho", "if code == 0 && t[0] == 0xa1 && found[1] == 1 {", "if false {"),
    ("permitted subtrees ignored", "names.cho",
     "if code == 0 && t[0] == 0xa0 && found[0] == 1 && found[1] == 0 {", "if false {"),
    ("a dNSName subtree matched as a plain suffix", "names.cho",
     "return n > k && int_of(name[n - k - 1]) == 46 && same(sub, name[n - k..n]);",
     "return n > k && same(sub, name[n - k..n]);"),
    ("an iPAddress mask ignored", "names.cho",
     "if int_of(ip[k]) & m != int_of(sub[k]) & m {", "if int_of(ip[k]) & 0 != int_of(sub[k]) & 0 {"),
    ("the leaf's EKU not checked", "verify.cho",
     "if code == 0 && eku >= 0 && eku & want == 0 {", "if false {"),
    ("the leaf's keyUsage not checked", "verify.cho", "if code == 0 && ku >= 0 && ku & 1 == 0 {", "if false {"),
    ("the budget off", "verify.cho", "pub fn max_signatures() -> [] int {\n    return 64;", "pub fn max_signatures() -> [] int {\n    return 1 << 40;"),
    ("a root's dates not checked", "verify.cho",
     "                    if c == 0 {\n                        c = time_ok(rview, now);\n                    }\n", ""),
    ("the host not matched", "verify.cho", "if code == 0 && !x509_names.san_matches(", "if false && !x509_names.san_matches("),
    ("an RSA key of 1024 bits allowed", "verify.cho", "if bits < 2048 || bits > 4096 {", "if bits < 1024 || bits > 4096 {"),
    ("the AKI not matched against the SKI", "verify.cho",
     "    return bytes.equal(der[a..ae], issuer[k..ke]);\n}", "    return true;\n}"),
    ("an intermediate's dates not checked", "verify.cho",
     "                if c == 0 {\n                    c = time_ok(jview, now);\n                }\n", ""),
    ("keyCertSign not required of an issuer", "verify.cho", "if ku >= 0 && ku >> 5 & 1 == 0 {", "if false {"),
    # A chain without a name (docs/x509-verify.md §10.4).
    ("the purpose ignored at the leaf (serverAuth always)", "verify.cho",
     "if code == 0 && eku >= 0 && eku & want == 0 {", "if code == 0 && eku >= 0 && eku & x509.eku_server_auth() == 0 {"),
    ("the purpose ignored at an intermediate (serverAuth always)", "verify.cho",
     "if !root && eku >= 0 && eku & want == 0 {", "if !root && eku >= 0 && eku & x509.eku_server_auth() == 0 {"),
    ("the client purpose is the server's", "verify.cho",
     "pub fn purpose_client_auth() -> [] int {\n    return x509.eku_client_auth();",
     "pub fn purpose_client_auth() -> [] int {\n    return x509.eku_server_auth();"),
    ("a purpose that is neither accepted", "verify.cho",
     "if purpose != purpose_server_auth() && purpose != purpose_client_auth() {", "if false {"),
    ("a root's EKU not read for client certificates", "verify.cho",
     "if root && want == purpose_client_auth() && eku >= 0 && eku & want == 0 {", "if false {"),
    ("the leaf's view not cleared on a refusal", "verify.cho", "                leaf[k] = 0;", "                leaf[k] = views[k];"),
    ("san_next skipping the first entry", "verify.cho",
     "    if p == 0 {\n        p = s;\n    }",
     "    if p == 0 {\n        if x509.tlv(leaf_der, s, e, entry) != 0 {\n            return 0;\n        }\n        p = entry[2];\n    }"),
    ("verify_name matching nothing", "verify.cho",
     "if code == 0 && !x509_names.san_matches(leaf_der, s, e, name[0..hinfo[0]], hinfo[1]) {", "if code == 0 {"),
    ("verify built on the client purpose", "verify.cho",
     "code = chain(store, certs, ranges, now, max_intermediates, purpose_server_auth(), leaf);",
     "code = chain(store, certs, ranges, now, max_intermediates, purpose_client_auth(), leaf);"),
    ("verify not checking the host is readable first", "verify.cho",
     "            code = x509_names.host_parse(host, name, hinfo);\n        }\n        let leaf",
     "            code = 0;\n        }\n        let leaf"),
]

FILES = ["matrix.txt", "chain_matrix.txt", "online.txt", "limbo_subset.txt"]


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


def evidence(cancho, pkg, work):
    exe = os.path.join(work, "driver")
    r = subprocess.run([cancho, "build", "--std", os.path.join(ROOT, "tests/programs/x509_verify_driver.cho"),
                        *[os.path.join(pkg, f) for f in ["verify.cho", "names.cho", "x509.cho"]], "-o", exe],
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
    cancho = sys.argv[1]
    work = tempfile.mkdtemp(prefix="x509-mutants-")
    pkg = os.path.join(work, "x509")
    src = os.path.join(ROOT, "packages/x509")
    shutil.copytree(src, pkg)
    base = evidence(cancho, pkg, work)
    if base:
        print(f"the unmutated package fails: {base}")
        sys.exit(1)
    print("unmutated: passes")
    survived = 0
    for name, file, old, new in MUTANTS:
        text = open(os.path.join(src, file)).read()
        assert text.count(old) == 1, f"{name}: the text occurs {text.count(old)} times"
        open(os.path.join(pkg, file), "w").write(text.replace(old, new))
        found = evidence(cancho, pkg, work)
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
