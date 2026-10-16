#!/usr/bin/env python3
"""Mutants of `docs/processes.md` §4.11: each breaks one thing the section
promises, runs the tests that watch it, and must see one fail.

    python3 scripts/process_release_mutants.py [name ...]

A mutant is a text replacement in one source file, applied, tested, and put
back. The tests are `process_release::` in the conformance suite, the accept
and reject fixtures, and the authority report. Run it from the repository
root; it needs a clean tree for the files it names, and restores them.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CL = "crates/cancho-codegen/src"
LL = "crates/cancho-codegen-llvm/src"
IR = "crates/cancho-ir/src"

# (name, file, old, new, test filter)
MUTANTS = [
    ("cl: the reaper does not wait, so a released child stays a zombie",
     f"{CL}/reaper.rs", "b.ins().call(f, &[pid, status, none]);", "let _ = (f, pid, status, none);",
     "process_release::released_children"),
    ("ll: the reaper does not wait, so a released child stays a zombie",
     f"{LL}/emit.rs", "%r = call i32 @waitpid(i32 %p, ptr %status, i32 0)", "%r = add i32 0, 0",
     "process_release::released_children"),
    ("ir: setsid dropped, so the detached child shares its parent's session",
     f"{IR}/process.rs", "if detached { base | if darwin { 0x400 } else { 0x80 } } else { base }", "base",
     "process_release::only_the_detached"),
    ("ll: setsid dropped on LLVM alone, so the two backends disagree",
     f"{LL}/body/process.rs", "spawn_flags(self.is_darwin(), detached)", "spawn_flags(self.is_darwin(), false)",
     "process_release::only_the_detached"),
    ("cl: a release kills the child, so it does not outlive the parent",
     f"{CL}/body/process.rs",
     "        let pid = self.builder.ins().ireduce(types::I32, word);\n        let pid = self.builder.ins().uextend(types::I64, pid);\n",
     "        let pid = self.builder.ins().ireduce(types::I32, word);\n        let nine = self.builder.ins().iconst(types::I32, 9);\n        self.libc_call(\"kill\", &[types::I32, types::I32], &[types::I32], &[pid, nine]);\n        let pid = self.builder.ins().uextend(types::I64, pid);\n",
     "process_release::a_detached_child_outlives"),
    ("ir: the detached start performs no `exec_detached`, and neither does a release",
     f"{IR}/lower/process.rs", "if detached {\n            self.performed.union(&Effects::plain([\"exec_detached\"]));\n        }", "",
     "process_release::the_authority_report refused_programs_are_refused accepted_programs_build_and_run"),
    ("ir: a release performs no `exec_detached`",
     f"{IR}/builtin.rs", "Builtin::ChildRelease => Effects::plain([\"exec_detached\"]),", "Builtin::ChildRelease => Effects::pure(),",
     "process_release::the_authority_report refused_programs_are_refused accepted_programs_build_and_run"),
    ("cl: the detached start skips the prefix check",
     f"{CL}/body/process.rs", "let program = self.checked_path(prefix, &path);",
     "let program = if detached { path[0] } else { self.checked_path(prefix, &path) };",
     "process_release::a_detached_start_stays"),
    ("cl: child_pid answers the pid plus one",
     f"{CL}/body/process.rs", "vec![self.builder.ins().uextend(types::I64, pid)]",
     "{ let wide = self.builder.ins().uextend(types::I64, pid); vec![self.builder.ins().iadd_imm(wide, 1)] }",
     "process_release::a_detached_child_outlives"),
    ("cl: the release leaves the pidfd open (Linux)",
     f"{CL}/body/process.rs",
     "        if !self.is_darwin() {\n            let high = self.builder.ins().ushr_imm(word, CHILD_PIDFD_SHIFT);\n            let pidfd = self.builder.ins().ireduce(types::I32, high);\n            self.libc_call(\"close\", &[types::I32], &[types::I32], &[pidfd]);\n        }\n        let pid = self.builder.ins().ireduce(types::I32, word);",
     "        let pid = self.builder.ins().ireduce(types::I32, word);",
     "process_release::releasing_a_child_gives_back"),
    ("cl: the reaper waits for any child, taking a held child's status",
     f"{CL}/reaper.rs", "let pid = b.ins().ireduce(types::I32, pid);", "let pid = b.ins().iconst(types::I32, -1); let _ = pid;",
     "process_release::released_children"),
]


def run(name, path, old, new, filt):
    full = os.path.join(ROOT, path)
    text = open(full).read()
    if old not in text:
        return "NOT APPLIED"
    open(full, "w").write(text.replace(old, new, 1))
    try:
        built = subprocess.run(["cargo", "test", "-p", "cancho", "--test", "conformance", "--"] + filt.split(),
                               cwd=ROOT, capture_output=True, text=True)
    finally:
        open(full, "w").write(text)
    out = built.stdout + built.stderr
    if "error[" in out or "could not compile" in out:
        return "DID NOT BUILD"
    return "killed" if built.returncode != 0 else "SURVIVED"


if __name__ == "__main__":
    wanted = sys.argv[1:]
    for mutant in MUTANTS:
        if wanted and not any(w in mutant[0] for w in wanted):
            continue
        print(f"{run(*mutant):12} {mutant[0]}", flush=True)
