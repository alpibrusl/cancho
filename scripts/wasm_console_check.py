#!/usr/bin/env python3
"""W2 (docs/wasm.md): the console on WASI imports exactly what the row says.

wasi-libc's stdio brought imports no program asked for -- `fd_fdstat_get`, `fd_seek`,
`fd_close`, and `clock_time_get` from a futex timeout in its locking -- so a program
with the row `[io_write]` imported a clock. The console is now written against
`fd_write` and `fd_read` directly. This builds one tiny program per effect for
`wasm32-wasip1` and requires its **exact** import set, then runs the
`checked_output.rs` probe (`flush_out`) against the module to check the buffer's
behaviour, not just its imports.

usage: wasm_console_check.py LEX_SYS

Needs what `lex-sys --help` says `--target` needs, and `wasmtime`. CI has neither.
"""
import os, re, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(__file__))
from wasm_imports import wasi_functions

STARTUP = {"args_get", "args_sizes_get", "proc_exit"}

HEAD = ("fn main(world: World) -> [] int {\n"
        "    let Split { io, ffi, fs, heap, args } = split(world);\n")
PROGRAMS = {
    # name: (source, the WASI functions it must import beyond the startup three)
    "pure": ("fn main(world: World) -> [] int { release(world); return 0; }\n", set()),
    "heap": (HEAD + "    release(io); release(ffi); release(fs); release(args);\n    var n = 0;\n"
             "    borrow mut heap as &!h in { let b = box(h, 41); n = unbox(h, b) - 41; }\n"
             "    release(heap);\n    return n;\n}\n", set()),
    "args": (HEAD + "    release(io); release(ffi); release(fs); release(heap);\n    var n = 0;\n"
             "    borrow args as &a in { n = arg_count(a) * 0; }\n    release(args);\n    return n;\n}\n", set()),
    "write": (HEAD + "    release(ffi); release(fs); release(heap); release(args);\n"
              "    borrow mut io as &!i in { putchar(i, 72); }\n    release(io);\n    return 0;\n}\n",
              {"fd_write"}),
    "stderr": (HEAD + "    release(ffi); release(fs); release(heap); release(args);\n"
               "    borrow mut io as &!i in { write_err(i, \"e\"); }\n    release(io);\n    return 0;\n}\n",
               {"fd_write"}),
    # `clock_ms` and `clock_unix_ms` (edition 6, which splits `clock` off the world).
    "clock": ("edition 6;\nfn main(world: World) -> [] int {\n"
              "    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n"
              "    release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n"
              "    release(signals);\n    var code = 0;\n"
              "    borrow clock as &c in { code = clock_ms(c) * 0 + clock_unix_ms(c) * 0; }\n"
              "    release(clock);\n    return code;\n}\n",
              {"clock_time_get"}),
    "read": (HEAD + "    release(ffi); release(fs); release(heap); release(args);\n    var c = 0;\n"
             "    borrow mut io as &!i in { c = getchar(i); }\n    release(io);\n    return c * 0;\n}\n",
             {"fd_read"}),
    "both": (HEAD + "    release(ffi); release(fs); release(heap); release(args);\n    var c = 0;\n"
             "    borrow mut io as &!i in { c = getchar(i); putchar(i, c); }\n    release(io);\n    return 0;\n}\n",
             {"fd_read", "fd_write"}),
}


def build(lex_sys, source, out):
    src = out + ".ls"
    open(src, "w").write(source)
    b = subprocess.run([lex_sys, "build", src, "--std", "--target", "wasm32-wasip1", "-o", out + ".wasm"],
                       capture_output=True, text=True)
    if b.returncode != 0:
        sys.exit(f"build failed for {out}:\n{b.stderr}")
    return out + ".wasm"


def checked_output_probe():
    """The program `checked_output.rs` runs on both native backends, so the wasm
    console is held to the same answers rather than to a copy of them."""
    root = os.path.join(os.path.dirname(__file__), "..")
    text = open(os.path.join(root, "crates/lex-sys/tests/conformance/checked_output.rs")).read()
    m = re.search(r'const PROBE: &str = "(.*?)";\n', text, re.S)
    if not m:
        sys.exit("cannot find PROBE in checked_output.rs")
    return m.group(1).replace('\\"', '"').replace("\\\\n", "\\n")


def run(module, args, reader_gone):
    p = subprocess.Popen(["wasmtime", "run", module, *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if reader_gone:
        p.stdout.close()
        out = b""
    else:
        out = p.stdout.read()
    p.wait(timeout=60)
    return p.returncode, out


def main():
    lex_sys = sys.argv[1]
    work = tempfile.mkdtemp(prefix="wasm-console-")
    failures = []

    print("imports, per effect (the startup three are args_get, args_sizes_get, proc_exit):")
    for name, (source, extra) in PROGRAMS.items():
        got = wasi_functions(open(build(lex_sys, source, os.path.join(work, name)), "rb").read())
        want = STARTUP | extra
        verdict = "ok" if got == want else "WRONG"
        extra_found = ", ".join(sorted(got - STARTUP)) or "-"
        print(f"  {name:6} {extra_found:26} {verdict}")
        if got != want:
            failures.append(f"{name}: imports {sorted(got)}, wanted {sorted(want)}")

    probe = build(lex_sys, checked_output_probe(), os.path.join(work, "probe"))
    print("flush_out (the checked_output.rs probe), against the module's own buffer:")
    cases = [
        ("small, reader present: status 100, the bytes arrive", [], False, 100, b"hello\n"),
        ("big (100,000 bytes), reader present: status 100, all arrive", ["big"], False, 100, b"x" * 100000),
        # wasmtime answers a write to a pipe whose reader left as "0 bytes written",
        # not EPIPE, which the console reports as EIO (29, which is 5 in the language's
        # numbering). A native Linux run would say EPIPE; that is the host's, not ours.
        ("small, reader gone: EIO", [], True, 105, b""),
        ("big, reader gone: an earlier failure, EIO, and it keeps being reported", ["big"], True, 105, b""),
    ]
    for label, args, gone, want_status, want_out in cases:
        status, out = run(probe, args, gone)
        ok = status == want_status and out == want_out
        print(f"  {label}: status={status} bytes={len(out)} {'ok' if ok else 'WRONG'}")
        if not ok:
            failures.append(f"{label}: status {status} (want {want_status}), {len(out)} bytes")

    if failures:
        sys.exit("\nFAILED:\n  " + "\n  ".join(failures))
    print("all as expected")


if __name__ == "__main__":
    main()
