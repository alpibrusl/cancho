#!/usr/bin/env python3
"""Mutation check of the lowering of `mul_wide`, `add_carry` and `sub_borrow` (docs/wide-multiply.md §9).

    python3 scripts/wide_multiply_mutants.py

Each mutant is one deliberate bug in `crates/cancho-codegen-llvm/src/body/wide.rs`, in the Cranelift lowering
(`crates/cancho-codegen/src/body/expr.rs`) or in the builtins' edition (`crates/cancho-ir/src/builtin.rs`). The compiler is rebuilt and
`cargo test` runs the conformance tests of `wide_multiply` and the fixture walkers, and the LLVM backend's unit tests of the IR. A mutant is
killed when any of them fails or the build is refused. The unmutated tree is run first and must pass. The source files are restored, byte for byte, after
each mutant, even on an interrupt. A mutant of the wasm32 path is only killed by the test that runs wasm32, which needs `CLANG`, `WASM_LD`,
`WASI_SYSROOT` and `WASMTIME` in the environment (docs/wasm.md); without them those mutants are reported as NOT RUN and the script fails.
Exit status 1 if a mutant survives.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WIDE = "crates/cancho-codegen-llvm/src/body/wide.rs"
CLIF = "crates/cancho-codegen/src/body/expr.rs"
BUILTIN = "crates/cancho-ir/src/builtin.rs"

MUTANTS = [
    (WIDE, False, "the product is a sum", 'format!("mul i128 {wa}, {wb}")', 'format!("add i128 {wa}, {wb}")'),
    (WIDE, False, "the high word shifted by 63", 'format!("lshr i128 {p}, 64")', 'format!("lshr i128 {p}, 63")'),
    (WIDE, False, "the first operand sign-extended", 'format!("zext i64 {a} to i128")', 'format!("sext i64 {a} to i128")'),
    (WIDE, False, "high and low swapped", "return Ok(vec![LValue::Reg(hi), LValue::Reg(lo)]);\n        }\n        //", "return Ok(vec![LValue::Reg(lo), LValue::Reg(hi)]);\n        }\n        //"),
    (WIDE, False, "add_carry subtracts", 'let intrinsic = if sub { "usub" } else { "uadd" };', 'let intrinsic = if sub { "uadd" } else { "usub" };'),
    (WIDE, False, "a carry in of zero counts as one", 'format!("icmp ne i64 {c}, 0")', 'format!("icmp eq i64 {c}, 0")'),
    (WIDE, False, "the two carries are anded", 'format!("or i1 {o1}, {o2}")', 'format!("and i1 {o1}, {o2}")'),
    (WIDE, False, "the carry in never added", "(i64 {v1}, i64 {cin})", "(i64 {v1}, i64 0)"),
    (WIDE, False, "the first overflow bit used twice", 'format!("or i1 {o1}, {o2}")', 'format!("or i1 {o1}, {o1}")'),
    (WIDE, False, "aarch64 takes the narrow path", "X86_64 | Architecture::Aarch64(_)", "X86_64"),
    (WIDE, True, "wasm32: the middle carry shifted by 31", 'format!("lshr i64 {p00}, 32")', 'format!("lshr i64 {p00}, 31")'),
    (WIDE, True, "wasm32: the middle sum masked to 31 bits", 'format!("and i64 {mid1}, 4294967295")', 'format!("and i64 {mid1}, 2147483647")'),
    (WIDE, True, "wasm32: the cross term dropped", 'format!("mul i64 {a1}, {b0}")', 'format!("mul i64 {a1}, {b1}")'),
    (CLIF, False, "Cranelift: the signed high multiply", "self.builder.ins().umulhi(args[0], args[1])", "self.builder.ins().smulhi(args[0], args[1])"),
    (CLIF, False, "Cranelift: a signed carry test", "else { IntCC::UnsignedLessThan }", "else { IntCC::SignedLessThan }"),
    (CLIF, False, "Cranelift: the borrow test is >=", "if sub { IntCC::UnsignedGreaterThan }", "if sub { IntCC::UnsignedGreaterThanOrEqual }"),
    (CLIF, False, "Cranelift: the carries are anded", "let both = self.builder.ins().bor(o1, o2);", "let both = self.builder.ins().band(o1, o2);"),
    (CLIF, False, "Cranelift: a zero carry in counts", "icmp_imm(IntCC::NotEqual, args[2], 0)", "icmp_imm(IntCC::Equal, args[2], 0)"),
    (CLIF, False, "Cranelift: high and low swapped", "vec![hi, lo]", "vec![lo, hi]"),
    (BUILTIN, False, "the builtins visible from edition 6", "Builtin::MulWide | Builtin::AddCarry | Builtin::SubBorrow => 7,", "Builtin::MulWide | Builtin::AddCarry | Builtin::SubBorrow => 6,"),
]

TESTS = [
    ["cargo", "test", "-p", "cancho-codegen-llvm", "wide_tests"],
    ["cargo", "test", "-p", "cancho", "--test", "conformance", "--", "wide_multiply", "corpus"],
]


def passes():
    for cmd in TESTS:
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if r.returncode != 0:
            tail = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith(("error", "test ")) and ("FAILED" in l or l.startswith("error"))]
            return False, tail[:2]
    return True, []


def main():
    wasm = all(os.environ.get(v) for v in ("CLANG", "WASM_LD", "WASI_SYSROOT", "WASMTIME"))
    ok, why = passes()
    assert ok, f"the unmutated tree must pass: {why}"
    print(f"unmutated: passes ({'wasm32 run' if wasm else 'wasm32 not run'})")
    survived = 0
    for path, needs_wasm, name, old, new in MUTANTS:
        full = os.path.join(ROOT, path)
        original = open(full).read()
        assert original.count(old) == 1, f"{name}: occurs {original.count(old)} times in {path}"
        if needs_wasm and not wasm:
            print(f"NOT RUN       {name}")
            survived += 1
            continue
        try:
            open(full, "w").write(original.replace(old, new))
            ok, why = passes()
        finally:
            open(full, "w").write(original)
        print(f"{'SURVIVED' if ok else 'killed':13} {name}{'' if ok else ': ' + '; '.join(why)}", flush=True)
        survived += ok
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - survived} killed")
    subprocess.run(["cargo", "build", "--release", "-p", "cancho"], cwd=ROOT, capture_output=True)
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
