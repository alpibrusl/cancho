#!/usr/bin/env python3
"""Mutation check of the word-at-a-time primitives (docs/word-scan.md section 7), the shape of `scripts/conn_peer_mutants.py`.

    python3 scripts/word_scan_mutants.py [--only <text in a mutant's name>] [--timeout SECONDS]

Each mutant is one deliberate bug at one site in the two backends' `words.rs`, or in `Builtin::since`. The file is changed
in place, `cargo test -p cancho --test conformance -- word_scan refused_programs_are_refused_with_the_stated_reason` runs (it builds real programs with both
backends), and the file is put back, whatever happens. A mutant is killed when that run fails; one that does not compile
proves nothing and is an error, not a kill. The unmutated tree is run first and must pass. A run that outlives the timeout is
killed and counts as a kill by a hang (a count made poison loops for ever). Exit status 1 if a mutant survives, an `old` text
does not occur exactly once, or a mutant fails to build.

Run it from a clean checkout of the change. The conformance tests keep their scratch directories under the system temporary
directory, by test name, so another checkout running the same tests at the same time can fail them: a failing baseline is
a reason to rerun, not a result.
"""
import argparse, os, signal, subprocess, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
LLVM = "crates/cancho-codegen-llvm/src/body/words.rs"
CRANE = "crates/cancho-codegen/src/body/words.rs"
IR = "crates/cancho-ir/src/builtin.rs"
CMD = ["cargo", "test", "-p", "cancho", "--test", "conformance", "--", "word_scan", "refused_programs_are_refused_with_the_stated_reason"]

MUTANTS = [
    ("llvm: no `length < 8` test", LLVM, '{failed} = or i1 {past}, {short}', '{failed} = or i1 {past}, {past}'),
    ("llvm: bound off by one (uge)", LLVM, 'icmp ugt i64 {at}, {room}', 'icmp uge i64 {at}, {room}'),
    ("llvm: signed compare (a negative at gets through)", LLVM, 'icmp ugt i64 {at}, {room}', 'icmp sgt i64 {at}, {room}'),
    ("llvm: mask lanes shifted by 8q", LLVM, 'shl i64 {wide}, {}\\n", 16 * q', 'shl i64 {wide}, {}\\n", 8 * q'),
    ("llvm: mask compares ne", LLVM, 'icmp eq <16 x i8> {v}, {splat}', 'icmp ne <16 x i8> {v}, {splat}'),
    ("llvm: load_le64 reads one byte on", LLVM,
     'getelementptr i8, ptr {base}, i64 {at}\\n"));\n        let word',
     'getelementptr i8, ptr {base}, i64 {at}\\n"));\n        let place = self.fresh();\n'
     '        self.out.push_str(&format!("  {place} = getelementptr i8, ptr {base}, i64 1\\n"));\n        let word'),
    ("llvm: cttz is ctlz", LLVM, 'Builtin::TrailingZeros => format!("call i64 @llvm.cttz.i64(i64 {x}, i1 false)")',
     'Builtin::TrailingZeros => format!("call i64 @llvm.ctlz.i64(i64 {x}, i1 false)")'),
    ("llvm: cttz with zero as poison (hangs)", LLVM, '@llvm.cttz.i64(i64 {x}, i1 false)', '@llvm.cttz.i64(i64 {x}, i1 true)'),
    ("llvm: popcount is ctlz", LLVM, '_ => format!("call i64 @llvm.ctpop.i64(i64 {x})")',
     '_ => format!("call i64 @llvm.ctlz.i64(i64 {x}, i1 false)")'),
    ("llvm: load_le64 width 7", LLVM, 'self.trap_unless_room(&length, &at, 8)?', 'self.trap_unless_room(&length, &at, 7)?'),
    ("llvm: byte_mask64 width 56", LLVM, 'self.trap_unless_room(&length, &at, 64)?', 'self.trap_unless_room(&length, &at, 56)?'),
    ("cranelift: no `length < width` test", CRANE, 'let failed = self.builder.ins().bor(past, short);',
     'let failed = self.builder.ins().bor(past, past);'),
    ("cranelift: signed compare", CRANE, 'IntCC::UnsignedGreaterThan', 'IntCC::SignedGreaterThan'),
    ("cranelift: bound off by one", CRANE, 'IntCC::UnsignedGreaterThan', 'IntCC::UnsignedGreaterThanOrEqual'),
    ("cranelift: big-endian load", CRANE, 'Endianness::Little', 'Endianness::Big'),
    ("cranelift: mask lanes shifted by 8q", CRANE, 'ishl_imm(wide, 16 * i64::from(q))', 'ishl_imm(wide, 8 * i64::from(q))'),
    ("cranelift: mask compares ne", CRANE, 'icmp(IntCC::Equal, lanes, splat)', 'icmp(IntCC::NotEqual, lanes, splat)'),
    ("cranelift: ctz is clz", CRANE, 'Builtin::TrailingZeros => self.builder.ins().ctz(x),',
     'Builtin::TrailingZeros => self.builder.ins().clz(x),'),
    ("cranelift: popcnt is ctz", CRANE, '_ => self.builder.ins().popcnt(x),', '_ => self.builder.ins().ctz(x),'),
    ("cranelift: load_le64 width 7", CRANE, 'self.trap_unless_room(length, at, 8);', 'self.trap_unless_room(length, at, 7);'),
    ("ir: the five names visible from edition 7", IR, '| Builtin::Popcount => 8,', '| Builtin::Popcount => 7,'),
]


def run(timeout):
    p = subprocess.Popen(CMD, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        out, _ = p.communicate(timeout=timeout)
        return p.returncode, out, False
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        out, _ = p.communicate()
        return 1, out, True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    ap.add_argument("--timeout", type=int, default=1200)
    a = ap.parse_args()
    status, out, _ = run(a.timeout)
    if status != 0:
        sys.exit("the unmutated tree does not pass:\n" + out[-2000:])
    bad = 0
    for name, rel, old, new in MUTANTS:
        if a.only and a.only not in name:
            continue
        path = os.path.join(ROOT, rel)
        source = open(path).read()
        if source.count(old) != 1:
            print(f"ERROR  {name}: `old` occurs {source.count(old)} times in {rel}")
            bad += 1
            continue
        open(path, "w").write(source.replace(old, new))
        try:
            status, out, hung = run(a.timeout)
        finally:
            open(path, "w").write(source)
        if "could not compile" in out or "error[E" in out:
            print(f"ERROR  {name}: the mutant does not build")
            bad += 1
        elif status == 0:
            print(f"SURVIVED  {name}")
            bad += 1
        else:
            print(f"killed{' (hang)' if hung else ''}  {name}", flush=True)
    sys.exit(1 if bad else 0)


main()
