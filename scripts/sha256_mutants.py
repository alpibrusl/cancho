#!/usr/bin/env python3
"""Mutation check of the compiler's SHA-256 builtins (docs/sha256-builtin.md §7).

    python3 scripts/sha256_mutants.py <scratch dir> [--target-dir <cargo target dir>] [--only <substring>] [--arch-only]

`scripts/kdf_mutants.py` mutates the cancho `std.crypto`, `std.hmac` and `std.hkdf` are written in. This mutates the Rust
that writes the LLVM IR for `hw_sha256`, `sha256_blocks` and `sha256_block_words` (`crates/cancho-codegen-llvm/src/
crypto/sha256.rs` and `body/crypto.rs`), the way `scripts/gcm_wide_mutants.py` does for AES-GCM: the repository is copied
to `<scratch dir>` (never edited in place), and for each mutant one text is replaced in one file, the compiler rebuilt,
and the evidence run:

- `cargo test -p cancho-codegen-llvm crypto`: the generated text (what is stored, what is declared);
- `cargo test -p cancho --test conformance sha256_builtins`: FIPS 180-4's examples through both builtins, 800 random
  states and blocks against the compression written from FIPS 180-4 in the test, a 16 KiB run, `hw_sha256()` against
  what the operating system says the CPU has, and every wrong length a trap;
- `cargo test -p cancho --test conformance kdf`: the CAVP files, RFC 4231 and 5869, the key schedules and Wycheproof,
  through `std.crypto` on the builtins.

A mutant is killed when any fails, or the build does not complete. The unmutated tree is run first and must pass. A
mutant for the other instruction set is skipped on this machine: run the script on x86-64 and on aarch64 and the union is
the tally (`--arch-only` runs just the mutants of this machine's instruction set, for the second machine). Exit status 1
if one survives.
"""
import os
import platform
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHA = "crates/cancho-codegen-llvm/src/crypto/sha256.rs"
CALLS = "crates/cancho-codegen-llvm/src/body/crypto.rs"

# (file, name, the text replaced, its replacement, the instruction set it applies to: None for both). Each `old` must
# occur exactly once in its file.
MUTANTS = [
    # The rounds and the schedule.
    (SHA, "a round constant wrong", "0x428a2f98, 0x71374491", "0x428a2f99, 0x71374491", None),
    (SHA, "the last group's constants wrong", "0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2", "0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f3", None),
    (SHA, "60 rounds, not 64", "for group in 0..16 {", "for group in 0..15 {", None),
    (SHA, "no feed-forward of the first half", "let r0 = g.op(&format!(\"add {W} {s0}, %s0\"));", "let r0 = s0.clone();", None),
    (SHA, "no feed-forward of the second half", "let r1 = g.op(&format!(\"add {W} {s1}, %s1\"));", "let r1 = s1.clone();", None),
    (SHA, "the schedule's constants added to the wrong group", "w[group], k_group(group)", "w[group], k_group(group % 15)", None),
    (SHA, "the schedule from the wrong group", "(&w[group - 4], &w[group - 3], &w[group - 2], &w[group - 1])", "(&w[group - 4], &w[group - 3], &w[group - 1], &w[group - 2])", None),
    (SHA, "x86: msg2 with the wrong group", "@llvm.x86.sha256msg2({W} {sum}, {W} {d})", "@llvm.x86.sha256msg2({W} {sum}, {W} {c})", "x86"),
    (SHA, "x86: the middle words one lane off", "<4 x i32> <i32 1, i32 2, i32 3, i32 4>", "<4 x i32> <i32 0, i32 1, i32 2, i32 3>", "x86"),
    (SHA, "x86: msg1's sum not taken", "let sum = g.op(&format!(\"add {W} {m1}, {mid}\"));", "let sum = g.op(&format!(\"add {W} {mid}, {mid}\"));", "x86"),
    (SHA, "x86: the second pair of rounds on the first pair's words", "<4 x i32> <i32 2, i32 3, i32 0, i32 0>", "<4 x i32> <i32 0, i32 1, i32 0, i32 0>", "x86"),
    (SHA, "x86: the rounds' state operands swapped", "@llvm.x86.sha256rnds2({W} {s1}, {W} {s0}, {W} {wk})", "@llvm.x86.sha256rnds2({W} {s0}, {W} {s1}, {W} {wk})", "x86"),
    (SHA, "x86: ABEF built with two words swapped", "\"<i32 5, i32 4, i32 1, i32 0>\"", "\"<i32 4, i32 5, i32 1, i32 0>\"", "x86"),
    (SHA, "x86: the state written back with two words swapped", "\"<i32 3, i32 2, i32 7, i32 6, i32 1, i32 0, i32 5, i32 4>\"", "\"<i32 3, i32 2, i32 6, i32 7, i32 1, i32 0, i32 5, i32 4>\"", "x86"),
    (SHA, "arm: h2 given the new ABCD", "@llvm.aarch64.crypto.sha256h2({W} {s1}, {W} {s0}, {W} {wk})", "@llvm.aarch64.crypto.sha256h2({W} {s1}, {W} {abcd}, {W} {wk})", "arm"),
    (SHA, "arm: su1 with its words swapped", "@llvm.aarch64.crypto.sha256su1({W} {u0}, {W} {c}, {W} {d})", "@llvm.aarch64.crypto.sha256su1({W} {u0}, {W} {d}, {W} {c})", "arm"),
    (SHA, "arm: su0 not applied", "let u0 =\n                    g.op(&format!(\"call {W} @llvm.aarch64.crypto.sha256su0({W} {a}, {W} {b})\"));", "let u0 = a.clone();", "arm"),
    (SHA, "arm: EFGH loaded from the wrong words", "\"<i32 4, i32 5, i32 6, i32 7>\",", "\"<i32 4, i32 5, i32 7, i32 6>\",", "arm"),
    # Loads and stores.
    (SHA, "a block read at a stride of 32", "%off = shl i64 %i, 6", "%off = shl i64 %i, 5", None),
    (SHA, "the message not swapped to big-endian", "%m{k} = call {W} @llvm.bswap.v4i32({W} %b{k})", "%m{k} = add {W} %b{k}, zeroinitializer", None),
    (SHA, "the result sign-extended into its int", "%o64 = zext <8 x i32> %o32 to <8 x i64>", "%o64 = sext <8 x i32> %o32 to <8 x i64>", None),
    (SHA, "the words' block read a group off", "format!(\"i32 {}\", 4 * k + j)", "format!(\"i32 {}\", (4 * k + j + 4) % 16)", None),
    # The probe.
    (SHA, "x86: AVX512CD asked for, not SHA", "%sha = and i32 %ebx, 536870912", "%sha = and i32 %ebx, 268435456", "x86"),
    (SHA, "x86: SSE4.1 not asked for", "%want1 = and i32 %ecx, 524800\n  %has1 = icmp eq i32 %want1, 524800", "%want1 = and i32 %ecx, 512\n  %has1 = icmp eq i32 %want1, 512", "x86"),
    (SHA, "x86: leaf 7 asked whatever the highest leaf", "%has7b = and i1 %has7, %leaf7", "%has7b = and i1 %has7, true", "x86"),
    (SHA, "arm Linux: HWCAP_SVE asked for, not SHA2", "%want = and i64 %hwcap, 64", "%want = and i64 %hwcap, 4194304", "arm-linux"),
    (SHA, "arm Darwin: a feature the system does not name", "hw.optional.arm.FEAT_SHA256", "hw.optional.arm.FEAT_SHA257", "arm-darwin"),
    (SHA, "the answer never cached as yes", "%fresh = select i1 %has, i8 2, i8 1\\n\\\n  store atomic i8 %fresh, ptr @cancho_hw_sha_cache", "%fresh = select i1 %has, i8 3, i8 1\\n\\\n  store atomic i8 %fresh, ptr @cancho_hw_sha_cache", None),
    # The call sites.
    (CALLS, "the state's length unchecked in sha256_blocks", "self.check_len(&state_len, 8)?;\n        let rest", "let rest", None),
    (CALLS, "a ragged run of blocks accepted", "{rest} = and i64 {data_len}, 63", "{rest} = and i64 {data_len}, 31", None),
    (CALLS, "only the first block compressed", "call void @cancho_sha256_blocks(ptr {state}, ptr {data}, i64 {blocks})", "call void @cancho_sha256_blocks(ptr {state}, ptr {data}, i64 1)", None),
    (CALLS, "the blocks counted as 128 bytes", "{blocks} = lshr i64 {data_len}, 6", "{blocks} = lshr i64 {data_len}, 7", None),
    (CALLS, "the block's length unchecked in sha256_block_words", "self.check_len(&block_len, 16)?;\n        self.out.push_str(&format!(\n            \"  call void @cancho_sha256_block_words", "self.out.push_str(&format!(\n            \"  call void @cancho_sha256_block_words", None),
    (CALLS, "the state's length unchecked in sha256_block_words", "self.check_len(&state_len, 8)?;\n        self.check_len(&block_len, 16)?;", "self.check_len(&block_len, 16)?;", None),
]

# Mutants that cannot change an answer, with why. They are run all the same: one that is killed is reported.
EQUIVALENT = {
    "x86: SSE4.1 not asked for": "every CPU with SHA has SSE4.1 (it came with Goldmont and Zen, both after SSE4.1), so "
    "the answer is the same on every machine this can run on; asking is what makes the claim not depend on that",
    "x86: leaf 7 asked whatever the highest leaf": "every CPU with SHA has leaf 7 (Intel's and AMD's manuals), and on "
    "every machine this runs on the highest leaf is far above 7, so the guard against a CPU without it never decides",
}


def run(cmd, cwd, env=None, timeout=3600):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env, timeout=timeout)


def evidence(work, env):
    """None when everything passes, else a line saying what failed."""
    for cmd in (["cargo", "test", "-p", "cancho-codegen-llvm", "crypto"],
                ["cargo", "test", "-p", "cancho", "--test", "conformance", "--", "sha256_builtins", "kdf"]):
        r = run(cmd, work, env)
        if r.returncode != 0:
            failed = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("test ") and "FAILED" in l]
            built = "could not compile" in r.stderr
            return "did not build" if built else "; ".join(failed[:2]) or "failed"
    return None


def main():
    args = sys.argv[1:]
    scratch = os.path.abspath(args[0])
    target = os.path.abspath(args[args.index("--target-dir") + 1]) if "--target-dir" in args else os.path.join(scratch, "target")
    only = args[args.index("--only") + 1] if "--only" in args else ""
    arch = "arm" if platform.machine() in ("arm64", "aarch64") else "x86"
    # The aarch64 probe asks the operating system, so its mutants are the operating system's.
    here = {None, arch, f"{arch}-{platform.system().lower()}"}
    env = dict(os.environ, CARGO_TARGET_DIR=target)
    work = os.path.join(scratch, "tree")
    os.makedirs(work, exist_ok=True)
    if shutil.which("rsync"):
        subprocess.run(["rsync", "-a", "--delete", "--exclude", "target", "--exclude", ".git", ROOT + "/", work + "/"], check=True)
    else:
        shutil.copytree(ROOT, work, dirs_exist_ok=True, ignore=shutil.ignore_patterns("target", ".git"))
    # Every source newer than any earlier build, so cargo never takes a stale artifact for this tree's.
    for dirpath, _, names in os.walk(work):
        for name in names:
            if name.endswith((".rs", ".cho", ".toml")):
                os.utime(os.path.join(dirpath, name), None)
    failed = skipped = killed = 0
    bad = evidence(work, env)
    assert bad is None, f"the unmutated tree must pass: {bad}"
    print("unmutated: passes", flush=True)
    for path, name, old, new, isa in MUTANTS:
        if only not in name:
            continue
        if isa not in here or ("--arch-only" in args and isa is None):
            skipped += 1
            print(f"skipped       {name} ({isa or 'both'})")
            continue
        full = os.path.join(work, path)
        text = open(full).read()
        assert text.count(old) == 1, f"{name}: `{old}` occurs {text.count(old)} times in {path}"
        open(full, "w").write(text.replace(old, new))
        try:
            why = evidence(work, env)
        finally:
            open(full, "w").write(text)
        if why is None and name in EQUIVALENT:
            print(f"EQUIVALENT    {name}: {EQUIVALENT[name]}", flush=True)
            killed += 1
        elif why is None:
            failed += 1
            print(f"SURVIVED      {name}", flush=True)
        else:
            killed += 1
            print(f"{'killed':13} {name}: {why}", flush=True)
    run(["touch", os.path.join(work, CALLS)], work)
    print(f"{killed + failed} mutants run on {arch}, {killed} killed, {failed} survived, {skipped} for the other instruction set")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
