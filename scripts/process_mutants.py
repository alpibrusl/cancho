#!/usr/bin/env python3
"""Mutation check of processes slice 2, the poller (docs/processes.md §4.8, §8).

    python3 scripts/process_mutants.py [name-substring ...]

Each mutant is one backend source file with one deliberate bug, at one site.
It is run against `cargo test --test conformance -- processes::` with a limit
of five minutes. A mutant is killed when a test fails, or when the run hangs
past the limit: a `pidfd` or a channel end left open, or a wrong one watched,
leaves a waiter waiting, and the whole process group is then killed. The
unmutated tree is run first and must pass.

The file is restored after every mutant from the copy read before it was
changed -- also on an exception or a signal -- and checked to be byte for byte
what it was. Run it on Linux: every mutant here but the dispatch is on the
Linux path, which a Darwin build never takes. Exit status 1 if a mutant
survives or a file could not be restored.
"""
import os
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIMIT = 300

CL_PROCESS = "crates/lex-sys-codegen/src/body/process.rs"
LL_PROCESS = "crates/lex-sys-codegen-llvm/src/body/process.rs"
CL_POLLER = "crates/lex-sys-codegen/src/body/poller.rs"
LL_POLLER = "crates/lex-sys-codegen-llvm/src/body/poller.rs"
CL_EXPR = "crates/lex-sys-codegen/src/body/expr.rs"
LL_EXPR = "crates/lex-sys-codegen-llvm/src/body/expr.rs"

# (name, file, the text replaced, its replacement). Each `old` occurs exactly
# once in its file. The two already run (cl `pidfd` not closed, cl wrong pid,
# both killed) are kept so the list is the whole set of thirteen.
MUTANTS = [
    ("cl: the pidfd not closed", CL_PROCESS,
     'self.libc_call("close", &[types::I32], &[types::I32], &[pidfd]);\n        }\n        vec![tag, code, bit, reason]',
     '\n        }\n        vec![tag, code, bit, reason]'),
    ("ll: the pidfd not closed", LL_PROCESS,
     'self.out.push_str(&format!("  call i32 @close(i32 {pidfd})\\n"));\n        }\n        Ok(vec![LValue::Reg(tag)',
     '\n        }\n        Ok(vec![LValue::Reg(tag)'),
    ("cl: pidfd_open asked for pid 1", CL_PROCESS,
     "let pid64 = self.builder.ins().sextend(types::I64, pid);\n        let flags",
     "let pid64 = self.builder.ins().iconst(types::I64, 1);\n        let flags"),
    ("ll: pidfd_open asked for pid 1", LL_PROCESS,
     '@syscall(i64 {SYS_PIDFD_OPEN}, i64 {pid64}, i64 0)',
     '@syscall(i64 {SYS_PIDFD_OPEN}, i64 1, i64 0)'),
    ("cl: the pidfd read from the low half", CL_POLLER,
     "let high = self.builder.ins().ushr_imm(word, CHILD_PIDFD_SHIFT);",
     "let high = self.builder.ins().ushr_imm(word, 0);"),
    ("cl: the refusal's errno not negated", CL_PROCESS,
     "let opened = self.builder.ins().select(refused, negated, opened);",
     "let opened = self.builder.ins().select(refused, reason, opened);"),
    ("ll: the refusal's errno not negated", LL_PROCESS,
     '"  {chosen} = select i1 {refused}, i64 {negated}, i64 {opened}\\n"',
     '"  {chosen} = select i1 {refused}, i64 {}, i64 {opened}\\n", operand(&reason)'),
    ("cl: the pidfd watched for EPOLLOUT", CL_POLLER,
     "let mask = self.builder.ins().iconst(types::I32, EPOLLIN);\n        self.builder.ins().store(unaligned(), mask, ev, 0);\n        self.builder.ins().store(unaligned(), token, ev, data_at as i32);\n        let op",
     "let mask = self.builder.ins().iconst(types::I32, EPOLLOUT);\n        self.builder.ins().store(unaligned(), mask, ev, 0);\n        self.builder.ins().store(unaligned(), token, ev, data_at as i32);\n        let op"),
    ("ll: the pidfd watched for EPOLLOUT", LL_POLLER,
     'self.store_unaligned(&ev, 0, "i32", &EPOLLIN.to_string());\n        self.store_unaligned(&ev, data_at, "i64", &token);\n        let result',
     'self.store_unaligned(&ev, 0, "i32", &EPOLLOUT.to_string());\n        self.store_unaligned(&ev, data_at, "i64", &token);\n        let result'),
    ("cl: a missing pidfd not checked", CL_POLLER,
     "vec![self.builder.ins().select(given, answer, why)]",
     "let _ = (given, why);\n        vec![answer]"),
    ("ll: a missing pidfd not checked", LL_POLLER,
     '"  {chosen} = select i1 {given}, i64 {answer}, i64 {why}\\n"',
     '"  {chosen} = select i1 true, i64 {answer}, i64 {why}\\n"'),
    ("cl: poller_add_pipe takes the modify path", CL_EXPR,
     "Callee::Builtin(Builtin::PollerAddPipe) => self.poller_ctl(&args, false, false),",
     "Callee::Builtin(Builtin::PollerAddPipe) => self.poller_ctl(&args, false, true),"),
    ("ll: poller_add_pipe takes the modify path", LL_EXPR,
     "self.poller_ctl(&args, false, false)\n            }\n            Callee::Builtin(Builtin::PollerAddChild)",
     "self.poller_ctl(&args, false, true)\n            }\n            Callee::Builtin(Builtin::PollerAddChild)"),
]

ORIGINALS = {}


def restore_all():
    """Write back every file this run has changed, and check each one."""
    ok = True
    for path, text in ORIGINALS.items():
        with open(path, "wb") as f:
            f.write(text)
        with open(path, "rb") as f:
            if f.read() != text:
                print(f"!! {path} could not be restored", flush=True)
                ok = False
    return ok


def on_signal(number, _frame):
    restore_all()
    sys.exit(128 + number)


def run_tests():
    """`(passed, seconds, tail)`; a run past the limit has its group killed."""
    start = time.time()
    proc = subprocess.Popen(
        ["cargo", "test", "--test", "conformance", "--", "processes::"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=LIMIT)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.communicate()
        return False, time.time() - start, "hung past the limit"
    text = out.decode(errors="replace")
    failed = [l for l in text.splitlines() if l.endswith("FAILED") or "error[" in l or "error:" in l]
    return proc.returncode == 0, time.time() - start, "; ".join(failed[:4])


def main():
    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    wanted = sys.argv[1:]
    chosen = [m for m in MUTANTS if not wanted or any(w in m[0] for w in wanted)]

    passed, seconds, tail = run_tests()
    print(f"unmutated: {'pass' if passed else 'FAIL'} ({seconds:.0f}s) {tail}", flush=True)
    if not passed:
        return 1

    survivors = []
    for name, rel, old, new in chosen:
        path = os.path.join(ROOT, rel)
        with open(path, "rb") as f:
            original = f.read()
        text = original.decode()
        if text.count(old) != 1:
            print(f"!! {name}: the site occurs {text.count(old)} times in {rel}", flush=True)
            return 1
        ORIGINALS[path] = original
        try:
            with open(path, "w") as f:
                f.write(text.replace(old, new, 1))
            passed, seconds, tail = run_tests()
        finally:
            restored = restore_all()
            del ORIGINALS[path]
        if not restored:
            return 1
        verdict = "SURVIVED" if passed else "killed"
        print(f"{verdict:8} {name} ({seconds:.0f}s) {tail}", flush=True)
        if passed:
            survivors.append(name)
    print(f"{len(chosen) - len(survivors)} of {len(chosen)} killed", flush=True)
    return 1 if survivors else 0


if __name__ == "__main__":
    sys.exit(main())
