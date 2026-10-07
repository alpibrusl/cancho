#!/usr/bin/env python3
"""Mutation check of processes slices 2 and 3 (docs/processes.md §4.8, §7.1, §8).

    python3 scripts/process_mutants.py [--capture] [name-substring ...]

Without `--capture`, the poller (slice 2) and the spawn's working directory
(§4.10) against the process tests; with it, `std/process.cho` (slice 3, and
`capture_both`, §7.2) against the capture tests.

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

CL_PROCESS = "crates/cancho-codegen/src/body/process.rs"
LL_PROCESS = "crates/cancho-codegen-llvm/src/body/process.rs"
CL_POLLER = "crates/cancho-codegen/src/body/poller.rs"
LL_POLLER = "crates/cancho-codegen-llvm/src/body/poller.rs"
CL_EXPR = "crates/cancho-codegen/src/body/expr.rs"
LL_EXPR = "crates/cancho-codegen-llvm/src/body/expr.rs"

# (name, file, the text replaced, its replacement). Each `old` occurs exactly
# once in its file. The two already run (cl `pidfd` not closed, cl wrong pid,
# both killed) are kept so the list is the whole set of thirteen, then §4.10's.
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

# §4.10: the spawn's working directory, each choice undone. `cl` and `ll` are
# the two backends; the swap is ordering the file actions, which only a
# `closefrom` makes observable, so it is on the Linux path.
CL_CHDIR = """        if let Some(dir) = &dir {
            let fd = self.dir_fd(dir[0]);
            let refused = self.libc_call(
                "posix_spawn_file_actions_addfchdir_np",
                &[pointer, types::I32],
                &[types::I32],
                &[actions, fd],
            );
            self.builder.ins().trapnz(refused, TrapCode::HEAP_OUT_OF_BOUNDS);
        }

"""
CLOSEFROM_WHY = """        // §4.5: and nothing else. Close-on-exec covers what this program
        // opened; this covers what it inherited without the flag, so the child
        // holds exactly the three streams. Darwin does the same with
        // `POSIX_SPAWN_CLOEXEC_DEFAULT` (`spawn_flags`); glibc has
        // `addclosefrom_np` from 2.34.
"""
CL_CLOSEFROM = """        if !self.is_darwin() {
            let lowest = self.builder.ins().iconst(types::I32, 3);
            self.libc_call(
                "posix_spawn_file_actions_addclosefrom_np",
                &[pointer, types::I32],
                &[types::I32],
                &[actions, lowest],
            );
        }
"""
LL_CHDIR = """        if let Some(dir) = &dir {
            let fd = self.dir_fd(&operand(&dir[0]));
            let refused = self.fresh();
            self.out.push_str(&format!(
                "  {refused} = call i32 @posix_spawn_file_actions_addfchdir_np(ptr {actions}, i32 {fd})\\n"
            ));
            let failed = self.fresh();
            self.out.push_str(&format!("  {failed} = icmp ne i32 {refused}, 0\\n"));
            self.trap_if(&failed)?;
        }

"""
LL_CLOSEFROM = """        if !self.is_darwin() {
            self.out.push_str(&format!(
                "  call i32 @posix_spawn_file_actions_addclosefrom_np(ptr {actions}, i32 3)\\n"
            ));
        }
"""

CWD = [
    ("cl: no chdir action", CL_PROCESS,
     "        if let Some(dir) = &dir {\n            let fd = self.dir_fd(dir[0]);",
     "        if let (Some(dir), true) = (&dir, false) {\n            let fd = self.dir_fd(dir[0]);"),
    ("ll: no chdir action", LL_PROCESS,
     "        if let Some(dir) = &dir {\n            let fd = self.dir_fd(&operand(&dir[0]));",
     "        if let (Some(dir), true) = (&dir, false) {\n            let fd = self.dir_fd(&operand(&dir[0]));"),
    ("cl: the chdir after the closefrom", CL_PROCESS,
     CL_CHDIR + CLOSEFROM_WHY + CL_CLOSEFROM, CLOSEFROM_WHY + CL_CLOSEFROM + CL_CHDIR),
    ("ll: the chdir after the closefrom", LL_PROCESS,
     LL_CHDIR + CLOSEFROM_WHY + LL_CLOSEFROM, CLOSEFROM_WHY + LL_CLOSEFROM + LL_CHDIR),
    ("cl: the chdir to descriptor 0", CL_PROCESS,
     "            let fd = self.dir_fd(dir[0]);\n            let refused",
     "            let fd = self.builder.ins().iconst(types::I32, 0);\n            let refused"),
    ("ll: the chdir to descriptor 0", LL_PROCESS,
     "            let fd = self.dir_fd(&operand(&dir[0]));\n            let refused",
     "            let fd = \"0\".to_owned();\n            let refused"),
    ("cl: the Dir closed after the spawn", CL_PROCESS,
     "        self.free(argv);\n        self.free(envp);\n",
     "        self.free(argv);\n        self.free(envp);\n        if let Some(dir) = &dir {\n            let fd = self.dir_fd(dir[0]);\n            self.libc_call(\"close\", &[types::I32], &[types::I32], &[fd]);\n        }\n"),
    ("ll: the Dir closed after the spawn", LL_PROCESS,
     '        self.out.push_str(&format!("  call void @free(ptr {envp})\\n"));\n',
     '        self.out.push_str(&format!("  call void @free(ptr {envp})\\n"));\n        if let Some(dir) = &dir {\n            let fd = self.dir_fd(&operand(&dir[0]));\n            self.out.push_str(&format!("  call i32 @close(i32 {fd})\\n"));\n        }\n'),
    ("cl: exec_spawn_in drops the Dir argument's place", CL_PROCESS,
     "let at = 1 + usize::from(in_dir);\n        let dir = in_dir.then(|| self.expr(&args[1]));",
     "let at = 1 + usize::from(in_dir);\n        let dir = in_dir.then(|| self.expr(&args[2]));"),
]

MUTANTS = MUTANTS + CWD

PROCESS = "std/process.cho"

# Slice 3: each choice §7.1 makes, undone.
CAPTURE = [
    ("the input written blocking", PROCESS,
     "                                pipe_nonblocking(wp);\n",
     ""),
    ("no drain after the exit", PROCESS,
     "                        if gone && state == running() {\n                            // Everything",
     "                        if gone && state == 99 {\n                            // Everything"),
    ("the deadline taken per wait", PROCESS,
     "let left = deadline - clock_ms(clock);",
     "let left = timeout + 0 * clock_ms(clock);"),
    ("the bound off by one", PROCESS,
     "if n > room {",
     "if n >= room {"),
    ("no kill on the deadline", PROCESS,
     "    if state != exited() {\n",
     "    if state != exited() && state != timed_out() {\n"),
    ("no kill past the bound", PROCESS,
     "    if state != exited() {\n",
     "    if state != exited() && state != too_much() {\n"),
    ("no kill when it cannot be watched", PROCESS,
     "    if state != exited() {\n",
     "    if state != exited() && state != failed() {\n"),
    ("the input end never closed", PROCESS,
     "                        if input_done {\n                            shut(writer);",
     "                        if input_done && false {\n                            shut(writer);"),
    ("the end of the output not acted on", PROCESS,
     "                                                if found == read_end() {\n                                                    output_done = true;",
     "                                                if found == read_end() {\n                                                    output_done = false;"),
    ("a failed write not ending the input", PROCESS,
     "                                                Sent::Failed(e) => {\n                                                    input_done = true;",
     "                                                Sent::Failed(e) => {\n                                                    input_done = false;"),
    ("a NUL let through", PROCESS,
     "        if one[i] == byte_of(0) {",
     "        if one[i] == byte_of(0) && false {"),
]

# §7.2: `capture_both`, each choice undone. Not here, and why: a mutant that
# skips the errors' drain after the exit survives (measured: 4 s, tests pass).
# The drain is redundant by construction, on the output's as on the errors':
# the poller is level-triggered and a child's last write precedes its exit, so
# a channel still holding bytes is in the very batch that reports the exit, and
# the event loop has read it by then (§7.2). The "no drain after the exit"
# mutant above undoes the flow, not the drain.
CAPTURE = CAPTURE + [
    ("the errors never read", PROCESS,
     "                            if token == errors_token() {",
     "                            if token == errors_token() && false {"),
    ("the errors registered under the output's token", PROCESS,
     "                            let added = poller_add_pipe(ph, rp, errors_token(), 1);",
     "                            let added = poller_add_pipe(ph, rp, output_token(), 1);"),
    ("the errors bounded by the output's bound", PROCESS,
     "                                                let (kept, found) = drain(heap, rp, contents(sb), err, most_errors);\n                                                err = kept;\n                                                if found == read_too_much() {\n                                                    state = too_much();\n                                                }\n                                                if found == read_end() {",
     "                                                let (kept, found) = drain(heap, rp, contents(sb), err, most);\n                                                err = kept;\n                                                if found == read_too_much() {\n                                                    state = too_much();\n                                                }\n                                                if found == read_end() {"),
    ("the errors overrun not ending the child", PROCESS,
     "                                                    state = too_much();\n                                                }\n                                                if found == read_end() {\n                                                    errors_done = true;",
     "                                                    state = running();\n                                                }\n                                                if found == read_end() {\n                                                    errors_done = true;"),
    ("the end of the errors not acted on", PROCESS,
     "                                                    errors_done = true;",
     "                                                    errors_done = false;"),
    ("the errors end left open at the finish", PROCESS,
     "    shut(reader);\n    shut(errors);\n    if state != exited() {",
     "    shut(reader);\n    if state != exited() {"),
]

FILTER = "processes::"
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
        ["cargo", "test", "--test", "conformance", "--", FILTER],
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
    global FILTER
    wanted = sys.argv[1:]
    mutants = MUTANTS
    if "--capture" in wanted:
        wanted.remove("--capture")
        mutants, FILTER = CAPTURE, "capture::"
    chosen = [m for m in mutants if not wanted or any(w in m[0] for w in wanted)]

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
