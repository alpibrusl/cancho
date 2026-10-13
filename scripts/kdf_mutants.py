#!/usr/bin/env python3
"""Mutation check of SHA-2 (`std/crypto.cho`), `std/hmac.cho` and `std/hkdf.cho` (docs/hkdf.md §5).

    python3 scripts/kdf_mutants.py <cancho binary>

Each mutant is one of the three files with one deliberate bug. All three are
built as local modules (`crypto`, `hmac`, `hkdf`) beside a copy of
`tests/programs/kdf_driver.cho`, and run against the evidence
`conformance/kdf.rs` uses: the vector table (and its HMAC and label rows again
under a prepared key), the CAVP short-message and Monte Carlo files, every
Wycheproof HMAC and HKDF case, the refusal rows, and rounds of
`scripts/kdf_differential.py` (1,000 on LLVM, 200 on Cranelift), on both
backends: the LLVM build hashes with the CPU's SHA-256 instructions where it
has them and the Cranelift build in software, so a mutant of either path is
caught by the backend that runs it. A mutant is killed when any of them
disagrees, or the driver traps. The unmutated files are run first and must
pass. Exit status 1 if a mutant survives or fails to build.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    ("SHA-384 started from SHA-512's words", "crypto", "return init512(state, sha384_h0);", "return init512(state, sha512_h0);"),
    ("a wrong SHA-384 initial word", "crypto", "h[7] = 0x47b5481d << 32 | 0xbefa4fa4;", "h[7] = 0x47b5481d << 32 | 0xbefa4fa5;"),
    ("SHA-256 length in bytes, not bits", "crypto", "let bit_len = state[24] * 8;", "let bit_len = state[24];"),
    ("SHA-512 length field one byte early", "crypto", "state[130 + k] = bit_len", "state[129 + k] = bit_len"),
    ("SHA-256 padding room off by one", "crypto", "    if have >= 56 {", "    if have >= 57 {"),
    ("SHA-256 round adds temp1 twice", "crypto", "wa = mask32(wrapping_add(temp1, temp2));", "wa = mask32(wrapping_add(temp1, temp1));"),
    ("a buffered SHA-512 byte dropped", "crypto", "        while have < 128 && at < n {", "        while have < 127 && at < n {"),
    # docs/sha256-builtin.md §5: the partial block as sixteen words, and the two paths.
    ("a buffered SHA-256 byte in the wrong place of its word", "crypto", "b << 24 - ((have & 3) << 3)", "b << 24 - ((have & 3) << 2)"),
    ("the partial block not cleared after a compression", "crypto", "    return clear_block(state);\n}", "    return 0;\n}"),
    ("the partial block's words not copied to the schedule", "crypto", "state[74 + t] = state[8 + t];", "state[74 + t] = 0;"),
    ("the whole blocks of an update not compressed", "crypto", "        compress_bytes(state, data[at..at + whole]);\n", ""),
    ("a SHA-256 state resumed with no bytes counted", "crypto", "    state[24] = bytes;", "    state[24] = 0;"),
    ("a SHA-512 state resumed with no bytes counted", "crypto", "    state[8] = bytes;", "    state[8] = 0;"),
    ("the wrong inner pad", "hmac", "pad[i] = byte_of(int_of(k0[i]) ^ 0x36);", "pad[i] = byte_of(int_of(k0[i]) ^ 0x35);"),
    ("the wrong outer pad", "hmac", "pad[i] = byte_of(int_of(k0[i]) ^ 0x5c);", "pad[i] = byte_of(int_of(k0[i]) ^ 0x5d);"),
    ("a block-length key hashed first", "hmac", "if len(key) > bl {", "if len(key) >= bl {"),
    ("SHA-384's block taken as 64 bytes", "hmac", "        return 64;\n    }\n    return 128;", "        return 64;\n    }\n    return 64;"),
    ("the inner digest left out of the outer hash", "hmac", "    hash_update_words(hash_len, state[0..hs], state[hs + 8..hs + 16]);\n", ""),
    ("SHA-384's inner digest fed as eight words", "hmac", "crypto.sha512_update_words(st, words[0..6]);", "crypto.sha512_update_words(st, words[0..8]);"),
    ("a fed word one place on in the partial block", "crypto", "state[8 + (have >> 2)] = words[i];", "state[9 + (have >> 2)] = words[i];"),
    ("a fed 64-bit word's bytes in the wrong order", "crypto", "words[i] >> (7 - k) * 8 & 0xff", "words[i] >> k * 8 & 0xff"),
    # docs/hkdf.md §9: prepared keys.
    ("the outer hash not resumed from the outer words", "hmac", "hash_resume(hash_len, state[0..hs], state[hs..hs + 8]);", "hash_init(hash_len, state[0..hs]);"),
    ("a prepared key started from its outer words", "hmac", "hash_resume(hash_len, state[0..hs], prepared[0..8]);", "hash_resume(hash_len, state[0..hs], prepared[8..16]);"),
    ("a prepared key's outer words kept from the inner hash", "hmac", "prepared[8 + i] = state[hs + i];", "prepared[8 + i] = state[i];"),
    ("a prepared key's outer words not copied in", "hmac", "        state[hs + i] = prepared[8 + i];", "        state[hs + i] = prepared[i];"),
    ("a prepared secret shorter than the hash accepted", "hkdf", "if len(secret) < hash_len {\n        return refused_prk_length();\n    }\n    return hmac.prepare", "if len(secret) < 0 {\n        return refused_prk_length();\n    }\n    return hmac.prepare"),
    ("the expand counter off by one", "hkdf", "counter[0] = byte_of(i);", "counter[0] = byte_of(i + 1);"),
    ("T(i-1) not chained", "hkdf", "        if i > 1 {", "        if i > 2 {"),
    ("255 * HashLen allowed past", "hkdf", "if len(okm) > 255 * hash_len {", "if len(okm) > 256 * hash_len {"),
    ("the label prefix misspelt", "hkdf", 'let prefix = "tls13 ";', 'let prefix = "tls12 ";'),
    ("the output length's high byte dropped", "hkdf", "info[0] = byte_of(total >> 8);", "info[0] = byte_of(0);"),
    ("a 250-byte label accepted", "hkdf", "if ll < 1 || ll > 249 {", "if ll < 1 || ll > 250 {"),
    ("a transcript hash of any length accepted", "hkdf", "if len(transcript_hash) != hash_len {\n        return refused_transcript_length();\n    }\n    if len(out) != hash_len {\n        return hmac.refused_output_length();\n    }\n    return expand_label(",
     "if len(transcript_hash) < 0 {\n        return refused_transcript_length();\n    }\n    if len(out) != hash_len {\n        return hmac.refused_output_length();\n    }\n    return expand_label("),
    ("a prepared Derive-Secret's transcript hash of any length accepted", "hkdf", "if len(transcript_hash) != hash_len {\n        return refused_transcript_length();\n    }\n    if len(out) != hash_len {\n        return hmac.refused_output_length();\n    }\n    return expand_label_prepared(",
     "if len(transcript_hash) < 0 {\n        return refused_transcript_length();\n    }\n    if len(out) != hash_len {\n        return hmac.refused_output_length();\n    }\n    return expand_label_prepared("),
]


def or_dash(x):
    return x or "-"


def evidence():
    cases, checks = [], []

    def add(case, check):
        cases.append(case)
        checks.append(check)

    for n, line in enumerate(l for l in open(os.path.join(ROOT, "tests/vectors/kdf.txt")) if l.strip() and not l.startswith("#")):
        _, case, want = line.rstrip("\n").split(" | ")
        add(case, lambda a, w=want: a == f"0 ok {w}")
        # The same under a prepared key (docs/hkdf.md §9), as `conformance/kdf.rs` runs them.
        f = case.split(" ")
        if f[0] == "M":
            add(f"P {f[1]} {f[2]} {1 + n % 70} {f[3]} {f[4]}", lambda a, w=want: a == f"0 ok {w}")
        elif f[0] in "LD":
            add(("Q " if f[0] == "L" else "R ") + " ".join(f[1:]), lambda a, w=want: a == f"0 ok {w}")
    for alg, f in ((256, "SHA256ShortMsg"), (384, "SHA384ShortMsg"), (512, "SHA512ShortMsg")):
        length, msg = 0, ""
        for line in open(os.path.join(ROOT, f"tests/vectors/cavp/{f}.rsp")):
            if line.startswith("Len = "):
                length = int(line.split("=")[1])
            elif line.startswith("Msg = "):
                msg = line.split("=")[1].strip() if length else ""
            elif line.startswith("MD = "):
                add(f"H {alg} {or_dash(msg)}", lambda a, w=line.split("=")[1].strip(): a == f"0 ok {w}")
    for alg, f in ((256, "SHA256Monte"), (384, "SHA384Monte"), (512, "SHA512Monte")):
        text = open(os.path.join(ROOT, f"tests/vectors/cavp/{f}.rsp")).read().splitlines()
        seed = [l for l in text if l.startswith("Seed = ")][0].split("=")[1].strip()
        mds = [l.split("=")[1].strip() for l in text if l.startswith("MD = ")]
        add(f"C {alg} {seed}", lambda a, w=",".join(mds): a == f"0 ok {w}")
    for hl, f in ((32, "hmac_sha256_test"), (48, "hmac_sha384_test")):
        for g in json.load(open(os.path.join(ROOT, f"tests/vectors/wycheproof/{f}.json")))["testGroups"]:
            for t in g["tests"]:
                valid = t["result"] == "valid"
                add(f"M {hl} {hl} {or_dash(t['key'])} {or_dash(t['msg'])}",
                    lambda a, w=t["tag"], v=valid: a.startswith("0 ok ") and a[5:].startswith(w) == v)
    for hl, f in ((32, "hkdf_sha256_test"), (48, "hkdf_sha384_test")):
        for g in json.load(open(os.path.join(ROOT, f"tests/vectors/wycheproof/{f}.json")))["testGroups"]:
            for t in g["tests"]:
                case = f"K {hl} {or_dash(t['salt'])} {or_dash(t['ikm'])} {or_dash(t['info'])} {t['size']}"
                if t["result"] == "valid":
                    add(case, lambda a, w=t["okm"]: a == f"0 ok {w}")
                else:
                    add(case, lambda a: a.startswith("-4 hkdf-length-too-large"))
    # `conformance/kdf.rs`'s streamed-against-one-shot cases: every length
    # to 300 in pieces that straddle the block boundaries. The fixed vectors
    # are all hashed in one call and never fill a partly full buffer.
    import hashlib
    for alg, digest in ((256, hashlib.sha256), (384, hashlib.sha384), (512, hashlib.sha512)):
        for n in range(301):
            msg = bytes((i * 131 + n) % 256 for i in range(n))
            for piece in (1, 7, 63, 64, 65, 127, 128, 129):
                add(f"U {alg} {piece} {or_dash(msg.hex())}", lambda a, w=digest(msg).hexdigest(): a == f"0 ok {w}")
    k32 = "07" * 32
    add(f"L 32 {k32} {'61' * 250} - 32", lambda a: a.startswith("-6 hkdf-label-length"))
    add(f"L 32 {k32} {'61' * 249} - 32", lambda a: a.startswith("0 ok "))
    add(f"D 32 32 {k32} {'61' * 7} {'00' * 31}", lambda a: a.startswith("-8 hkdf-transcript-hash-length"))
    add(f"X 32 {k32} - {255 * 32 + 1}", lambda a: a.startswith("-4 hkdf-length-too-large"))
    add(f"Q 32 {'07' * 31} {'61' * 5} - 32", lambda a: a.startswith("-5 hkdf-prk-length"))
    add(f"R 32 32 {k32} {'61' * 7} {'00' * 31}", lambda a: a.startswith("-8 hkdf-transcript-hash-length"))
    return cases, checks


def run(compiler, sources, driver_src, work, cases, checks):
    """On both backends: LLVM's SHA-256 is the CPU's instructions where it has them, Cranelift's the software path, and
    a mutant of either path is killed only by the backend that runs it (docs/sha256-builtin.md §7)."""
    for backend, rounds in (("llvm", "1000"), ("cranelift", "200")):
        dead, why = run_one(compiler, sources, driver_src, work, cases, checks, backend, rounds)
        if dead is not False:
            return dead, [f"{backend}: {w}" for w in why]
    return False, []


def run_one(compiler, sources, driver_src, work, cases, checks, backend, rounds):
    paths = []
    for name, text in sources.items():
        text = text.replace(f"module std.{name};", f"module {name};", 1)
        text = text.replace("import std.crypto;", "import crypto;").replace("import std.hmac;", "import hmac;")
        path = os.path.join(work, f"{name}.cho")
        open(path, "w").write(text)
        paths.append(path)
    drv = os.path.join(work, "driver.cho")
    exe = os.path.join(work, "driver")
    text = driver_src
    for name in sources:
        text = text.replace(f"import std.{name};", f"import {name};", 1)
    open(drv, "w").write(text)
    build = subprocess.run([compiler, "build", "--std", "--backend", backend, drv, *paths, "-o", exe], capture_output=True,
                           text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    out = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=600)
    answers = [l.rstrip() for l in out.stdout.splitlines()]
    if out.returncode != 0 or len(answers) != len(cases):
        return True, [f"the driver stopped (status {out.returncode}) after {len(answers)} of {len(cases)} cases"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:50]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/kdf_differential.py"), exe, rounds],
                          capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {n: open(os.path.join(ROOT, f"std/{n}.cho")).read() for n in ("crypto", "hmac", "hkdf")}
    driver_src = open(os.path.join(ROOT, "tests/programs/kdf_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = run(compiler, sources, driver_src, work, cases, checks)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for name, file, old, new in MUTANTS:
            n = sources[file].count(old)
            assert n == 1, f"{name}: `{old[:40]}` occurs {n} times in {file}"
            mutated = dict(sources)
            mutated[file] = sources[file].replace(old, new)
            dead, why = run(compiler, mutated, driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {file}: {name}: {'; '.join(why)}")
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
