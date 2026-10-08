#!/usr/bin/env python3
"""Mutation check of the compiler's hardware AES and GHASH code (docs/gcm-wide.md §7).

    python3 scripts/gcm_wide_mutants.py <scratch dir> [--target-dir <cargo target dir>] [--only <substring>] [--arch-only]

`scripts/gcm_mutants.py` mutates the cancho the standard library is written in. This mutates
the Rust that writes the LLVM IR for `aes_ctr32`, `ghash_powers`, `gcm_tag` and `gcm_tag_diff`
(`crates/cancho-codegen-llvm/src/crypto/` and `body/crypto.rs`), because a mistake there is
a mistake in every record. The repository is copied to `<scratch dir>` (never edited in
place), built once, and then for each mutant: one text replaced in one file, the compiler
rebuilt, and the evidence run:

- `cargo test -p cancho --test conformance crypto_builtins`: FIPS 197, NIST test case 2,
  `aes_ctr32` on every length 0 to 300 and counters about to wrap, the eight powers,
  `gcm_tag` on every text length 0 to 300 and up to 16 KiB, `gcm_tag_diff` equal and
  changed in each byte, and every wrong length a trap, against references written from
  FIPS 197 and SP 800-38D in the test;
- `cargo test -p cancho-codegen-llvm crypto`: the generated text itself (a wipe that no
  answer shows).

A mutant is killed when either fails (or the build does not complete: a mutant that is not
a program is not a surviving one, and is reported as such). The unmutated tree is run first
and must pass. A mutant for the other instruction set is skipped on this machine: run the
script on x86-64 and on aarch64 and the union is the tally (`--arch-only` runs just the
mutants of this machine's instruction set, for the second machine). Exit status 1 if one survives.
"""
import os
import platform
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRYPTO = "crates/cancho-codegen-llvm/src/crypto"
CALLS = "crates/cancho-codegen-llvm/src/body/crypto.rs"

# (file, name, the text replaced, its replacement, the instruction set it applies to: None for
# both). Each `old` must occur exactly once in its file.
MUTANTS = [
    # Counter mode.
    (f"{CRYPTO}/aes.rs", "block k uses the counter plus k + 1", "<i32 0, i32 0, i32 0, i32 {k}>\"));", "<i32 0, i32 0, i32 0, i32 {}>\", k + 1));", None),
    (f"{CRYPTO}/aes.rs", "the counter not swapped to big-endian", "call <4 x i32> @llvm.bswap.v4i32(<4 x i32> {sum})", "add <4 x i32> {sum}, zeroinitializer", None),
    (f"{CRYPTO}/aes.rs", "the counter put in the wrong lane", "<i32 0, i32 1, i32 2, i32 7>", "<i32 0, i32 1, i32 7, i32 3>", None),
    (f"{CRYPTO}/aes.rs", "the counter advanced by seven a group", "<i32 0, i32 0, i32 0, i32 8>\\n  %i1", "<i32 0, i32 0, i32 0, i32 7>\\n  %i1", None),
    (f"{CRYPTO}/aes.rs", "groups of 64 bytes counted for groups of 128", "%big = lshr i64 %len, 7", "%big = lshr i64 %len, 6", None),
    (f"{CRYPTO}/aes.rs", "a tail of up to 63 bytes", "%rem = and i64 %len, 127", "%rem = and i64 %len, 63", None),
    (f"{CRYPTO}/aes.rs", "the tail's counter restarts", "counter_blocks(tail, 8, \"%base\", \"%cur\")", "counter_blocks(tail, 8, \"%base\", \"%cv\")", None),
    (f"{CRYPTO}/aes.rs", "the tail's whole blocks use block 0's keystream", "%kp = getelementptr i8, ptr %buf, i64 %o\\n", "%kp = getelementptr i8, ptr %buf, i64 0\\n", None),
    (f"{CRYPTO}/aes.rs", "the tail's bytes start in the middle of a block", "%bstart = shl i64 %chunks, 4", "%bstart = shl i64 %chunks, 3", None),
    (f"{CRYPTO}/aes.rs", "the tail's bytes use the wrong keystream byte", "%bkp = getelementptr i8, ptr %buf, i64 %q\\n", "%bkp = getelementptr i8, ptr %buf, i64 %bpos\\n", None),
    (f"{CRYPTO}/aes.rs", "five whole blocks of tail for every eight", "%chunks = lshr i64 %rem, 4", "%chunks = lshr i64 %rem, 5", None),
    (f"{CRYPTO}/aes.rs", "the keystream left on the stack", "store volatile", "store", None),
    # The cipher.
    (f"{CRYPTO}/aes.rs", "rounds stepped by two", "%i1 = add i64 %i, 1\\n  br label %loop", "%i1 = add i64 %i, 2\\n  br label %loop", None),
    (f"{CRYPTO}/aes.rs", "the first round key not added", "xor {V} %b{k}, %k0", "xor {V} %b{k}, %b{k}", "x86"),
    (f"{CRYPTO}/aes.rs", "the last round with MixColumns", "@llvm.x86.aesni.aesenclast({V} %s{k}, {V} %kl)", "@llvm.x86.aesni.aesenc({V} %s{k}, {V} %kl)", "x86"),
    (f"{CRYPTO}/aes.rs", "the last round key not added", "xor {V} %eb{k}, %kf", "xor {V} %eb{k}, %kl", "arm"),
    (f"{CRYPTO}/aes.rs", "a round without MixColumns", "%n{k} = bitcast <16 x i8> %m{k} to {V}", "%n{k} = bitcast <16 x i8> %e{k} to {V}", "arm"),
    # The reduction and the twisted powers.
    (f"{CRYPTO}/ghash.rs", "the first phase's shift by 57 wrong", "%t3 = shl {V} %a4, <i64 57, i64 57>", "%t3 = shl {V} %a4, <i64 56, i64 56>", None),
    (f"{CRYPTO}/ghash.rs", "the first phase's shift by 5 wrong", "%a3 = shl {V} %a2, <i64 5, i64 5>", "%a3 = shl {V} %a2, <i64 4, i64 4>", None),
    (f"{CRYPTO}/ghash.rs", "the first phase's shift by 1 wrong", "%a1 = shl {V} %data, <i64 1, i64 1>", "%a1 = shl {V} %data, <i64 2, i64 2>", None),
    (f"{CRYPTO}/ghash.rs", "the low lane's fold not carried into the high lane", "%t2 = shufflevector {V} %t3, {V} zeroinitializer, <2 x i32> <i32 2, i32 0>", "%t2 = shufflevector {V} %t3, {V} zeroinitializer, <2 x i32> <i32 2, i32 2>", None),
    (f"{CRYPTO}/ghash.rs", "the high lane's fold not carried into the top half", "%top2 = xor {V} %top, %t3h", "%top2 = xor {V} %top, %top", None),
    (f"{CRYPTO}/ghash.rs", "the second phase's shift by 5 wrong", "%b1 = lshr {V} %data2, <i64 5, i64 5>", "%b1 = lshr {V} %data2, <i64 6, i64 6>", None),
    (f"{CRYPTO}/ghash.rs", "the second phase's last shift wrong", "%b5 = lshr {V} %b4, <i64 1, i64 1>", "%b5 = lshr {V} %b4, <i64 2, i64 2>", None),
    (f"{CRYPTO}/ghash.rs", "the folded low half dropped from the result", "%r = xor {V} %r0, %data2", "%r = xor {V} %r0, %data", None),
    (f"{CRYPTO}/ghash.rs", "the twist's constant wrong", "257870231182273679343338569694386847745", "257870231182273679343338569694386847744", None),
    (f"{CRYPTO}/ghash.rs", "the first power not twisted", "let first = twist(&mut g, &h);", "let first = h.clone();", None),
    (f"{CRYPTO}/ghash.rs", "the powers made with the untwisted H", "accumulate(isa, &mut g, None, &power, &first)", "accumulate(isa, &mut g, None, &power, &h)", None),
    # The products.
    (f"{CRYPTO}/ghash.rs", "the cross terms' low part put high", "%midl = shufflevector {V} %mid, {V} zeroinitializer, <2 x i32> <i32 2, i32 0>", "%midl = shufflevector {V} %mid, {V} zeroinitializer, <2 x i32> <i32 0, i32 2>", None),
    (f"{CRYPTO}/ghash.rs", "the cross terms' high part dropped", "%top = xor {V} %hi, %midh", "%top = xor {V} %hi, zeroinitializer", None),
    (f"{CRYPTO}/ghash.rs", "one cross product missing", "let mid = g.op(&format!(\"xor {V} {m1}, {m2}\"));", "let mid = m1.clone();", None),
    (f"{CRYPTO}/ghash.rs", "a cross product of the wrong halves", "let m2 = clmul(isa, g, x, true, h, false);", "let m2 = clmul(isa, g, x, true, h, true);", None),
    (f"{CRYPTO}/ghash.rs", "the x86 half selector without the second operand's", "u8::from(a_hi) | u8::from(b_hi) << 4", "u8::from(a_hi) | u8::from(b_hi)", "x86"),
    (f"{CRYPTO}/ghash.rs", "the aarch64 second operand always the low half", "extractelement {V} {b}, i32 {}\", u8::from(b_hi)", "extractelement {V} {b}, i32 {}\", 0", "arm"),
    # Aggregation.
    (f"{CRYPTO}/ghash.rs", "the powers in the wrong order", "16 * (n - 1 - j)", "16 * j", None),
    (f"{CRYPTO}/ghash.rs", "the state not added to the first block", "x = g.op(&format!(\"xor {V} {x}, %y\"));", "x = g.op(&format!(\"xor {V} {x}, zeroinitializer\"));", None),
    (f"{CRYPTO}/ghash.rs", "a block not byte-reversed", "reverse(g, &raw)\n}", "raw\n}", None),
    (f"{CRYPTO}/ghash.rs", "the byte reversal swaps two bytes wrong", "<i32 15, i32 14, i32 13,", "<i32 14, i32 15, i32 13,", None),
    (f"{CRYPTO}/ghash.rs", "groups of 64 bytes counted for groups of 128", "%nb = lshr i64 %len, 7\\n", "%nb = lshr i64 %len, 6\\n", None),
    (f"{CRYPTO}/ghash.rs", "a group of eight read at a stride of 64", "%off = shl i64 %i, 7", "%off = shl i64 %i, 6", None),
    (f"{CRYPTO}/ghash.rs", "the ragged block not counted", "%nbl = add i64 %full, %rg", "%nbl = add i64 %full, 0", None),
    (f"{CRYPTO}/ghash.rs", "the ragged tail copied short", "ptr %src0, i64 %rem, i1 false", "ptr %src0, i64 %part, i1 false", None),
    (f"{CRYPTO}/ghash.rs", "the padding buffer not zeroed", "(ptr %buf, i8 0, i64 128, i1 false)", "(ptr %buf, i8 1, i64 128, i1 false)", None),
    (f"{CRYPTO}/ghash.rs", "the groups of the tail chosen by the wrong bit", "%t{n} = and i64 %nbl, {n}", "%t{n} = and i64 %nbl, 1", None),
    # The tag.
    (f"{CRYPTO}/ghash.rs", "the two lengths swapped", "%l0 = insertelement {V} undef, i64 %tbits, i32 0", "%l0 = insertelement {V} undef, i64 %abits, i32 0", None),
    (f"{CRYPTO}/ghash.rs", "the length in bytes, not bits", "%tbits = shl i64 %textlen, 3", "%tbits = shl i64 %textlen, 0", None),
    (f"{CRYPTO}/ghash.rs", "the mask of counter 2, not J0", "i32 16777216", "i32 33554432", None),
    (f"{CRYPTO}/ghash.rs", "the mask not XORed into the tag", "g.line(&format!(\"%tag = xor {V} {raw}, %mask\"));", "g.line(&format!(\"%tag = xor {V} {raw}, zeroinitializer\"));", None),
    (f"{CRYPTO}/ghash.rs", "the text never hashed", "{V} %y1, ptr %text, i64 %textlen", "{V} %y1, ptr %aad, i64 %aadlen", None),
    (f"{CRYPTO}/ghash.rs", "the associated data never hashed", "{V} zeroinitializer, ptr %aad, i64 %aadlen", "{V} zeroinitializer, ptr %text, i64 %textlen", None),
    (f"{CRYPTO}/ghash.rs", "only the low half of the tag compared", "%o = or i64 %lo, %hi", "%o = or i64 %lo, %lo", None),
    (f"{CRYPTO}/ghash.rs", "the last power missing", "for k in 1..8 {", "for k in 1..7 {", None),
    (f"{CRYPTO}/ghash.rs", "the powers stored at a stride of 8", "g.op(&format!(\"getelementptr i8, ptr %table, i64 {}\", 16 * k))", "g.op(&format!(\"getelementptr i8, ptr %table, i64 {}\", 8 * k))", None),
    (f"{CRYPTO}/ghash.rs", "the powers squared, not multiplied by H", "accumulate(isa, &mut g, None, &power, &first)", "accumulate(isa, &mut g, None, &power, &power)", None),
    # The call sites.
    (CALLS, "the nonce's length unchecked", "self.check_len(&nonce_len, 12)?;\n        // `counter`", "// `counter`", None),
    (CALLS, "a counter of 2^32 accepted", "icmp ugt i64 {counter}, 4294967295", "icmp ugt i64 {counter}, 4294967296", None),
    (CALLS, "an output of another length accepted", "icmp ne i64 {input_len}, {out_len}", "icmp ult i64 {input_len}, {out_len}", None),
    (CALLS, "AES-192 refused", "icmp eq i64 {rounds}, 12", "icmp eq i64 {rounds}, 13", None),
    (CALLS, "the key's length unchecked", "self.trap_if(&bad_keys)\n", "Ok(())\n", None),
    (CALLS, "the table's length unchecked", "self.check_len(&table_len, 128)?;\n        self.check_len(&nonce_len, 12)?;", "self.check_len(&nonce_len, 12)?;", None),
    (CALLS, "the tag's length unchecked", "self.check_len(&tag_len, 16)?;\n", "", None),
]


def run(cmd, cwd, env=None, timeout=3600):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, env=env, timeout=timeout)


def evidence(work, env):
    """None when everything passes, else a line saying what failed."""
    for cmd in (["cargo", "test", "-p", "cancho-codegen-llvm", "crypto"],
                ["cargo", "test", "-p", "cancho", "--test", "conformance", "crypto_builtins"]):
        r = run(cmd, work, env)
        if r.returncode != 0:
            failed = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("test ") and "FAILED" in l]
            built = "error" in r.stderr and "could not compile" in r.stderr
            return ("did not build" if built else "; ".join(failed[:2]) or "failed")
    return None


def main():
    args = sys.argv[1:]
    scratch = os.path.abspath(args[0])
    target = os.path.abspath(args[args.index("--target-dir") + 1]) if "--target-dir" in args else os.path.join(scratch, "target")
    only = args[args.index("--only") + 1] if "--only" in args else ""
    arch = "arm" if platform.machine() in ("arm64", "aarch64") else "x86"
    env = dict(os.environ, CARGO_TARGET_DIR=target)
    work = os.path.join(scratch, "tree")
    os.makedirs(work, exist_ok=True)
    subprocess.run(["rsync", "-a", "--delete", "--exclude", "target", "--exclude", ".git", ROOT + "/", work + "/"], check=True)
    failed = skipped = killed = 0
    bad = evidence(work, env)
    assert bad is None, f"the unmutated tree must pass: {bad}"
    print("unmutated: passes", flush=True)
    for path, name, old, new, isa in MUTANTS:
        if only not in name:
            continue
        if isa not in (None, arch) or ("--arch-only" in args and isa is None):
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
        if why is None:
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
