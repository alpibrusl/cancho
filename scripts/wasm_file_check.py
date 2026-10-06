#!/usr/bin/env python3
"""The file builtins, native and on `wasm32-wasip1`, side by side (docs/wasm.md, W0.5).

One tiny program per builtin the accept fixtures do not reach -- the write-mode
openers (`open_write`, `open_append`, `open_new`, `open_rw`), `file_size`,
`file_sync`, `file_truncate`, `file_pread`, `file_pwrite`, `fs_read`, `fs_write`,
`fs_remove`, `fs_rename` -- each built for the host and for wasm32, run twice
(starting with `/tmp/lexsys-probe` absent, then present), and required to **exit
with the same status and leave the same files**.

It found that the write-mode openers failed on WASI with `EINVAL`: they went through
`fopen` and a `dup` of its descriptor, and WASI has no `dup`. No accept fixture
reached them, so the coverage map never saw it.

usage: wasm_file_check.py LEX_SYS [--imports]

`--imports` also prints each probe's WASI imports beyond the startup three, which is
how `docs/wasm.md`'s label-to-imports table was measured.

Needs what `lex-sys --help` says `--target` needs, and `wasmtime`. CI has neither.
"""
import glob, os, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(__file__))

STARTUP = {"args_get", "args_sizes_get", "proc_exit"}
PATH = "/tmp/lexsys-probe"
HEAD = '''edition 6;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(io); release(ffi); release(heap); release(args); release(net); release(clock); release(signals);
    let d = narrow(fs, "/tmp");
    var code = 0;
    borrow d as &f in {
%s
    }
    release(d);
    return code;
}
'''


def opener(op, inner=""):
    return f'''        match {op}(f, "{PATH}") {{
            Opened::Failed(e) => {{ code = 100 + e; }}
            Opened::Ok(opened) => {{
                var file = opened;
                borrow mut file as &!h in {{
{inner}
                }}
                file_close(file);
            }}
        }}
'''


PROBES = {
    "open_write": opener("open_write", '                    file_write(h, "x");'),
    "open_append": opener("open_append", '                    file_write(h, "x");'),
    "open_new": opener("open_new", '                    file_write(h, "x");'),
    "open_rw": opener("open_rw", '                    file_write(h, "x");'),
    "file_size": opener("open_read", "                    file_size(h);"),
    "file_sync": opener("open_write", "                    file_sync(h);"),
    "file_truncate": opener("open_write", "                    file_truncate(h, 0);"),
    "file_pwrite": opener("open_write", '                    file_pwrite(h, 0, "x");'),
    "file_pread": opener(
        "open_read",
        "                    region a { let buf = alloc_slice[a](8, byte_of(0)); file_pread(h, 0, buf); }"),
    "fs_remove": f'        fs_remove(f, "{PATH}");\n',
    "fs_rename": f'        fs_rename(f, "{PATH}", "{PATH}2");\n',
    "fs_read": f'        region a {{ let buf = alloc_slice[a](8, byte_of(0)); fs_read(f, "{PATH}", buf); }}\n',
    "fs_write": f'        fs_write(f, "{PATH}", "x");\n',
}


def reset(initial):
    for f in glob.glob(PATH + "*"):
        os.remove(f)
    if initial is not None:
        open(PATH, "w").write(initial)


def snapshot():
    return {os.path.basename(f): open(f, "rb").read() for f in sorted(glob.glob(PATH + "*"))}


def main():
    lex_sys, show_imports = sys.argv[1], "--imports" in sys.argv[2:]
    work = tempfile.mkdtemp(prefix="wasm-file-")
    bad = []
    for name, body in PROBES.items():
        src = os.path.join(work, name + ".ls")
        open(src, "w").write(HEAD % body)
        native, wasm = os.path.join(work, name), os.path.join(work, name + ".wasm")
        for out, extra in ((native, []), (wasm, ["--target", "wasm32-wasip1"])):
            b = subprocess.run([lex_sys, "build", src, "--std", *extra, "-o", out], capture_output=True, text=True)
            if b.returncode:
                sys.exit(f"{name}: build failed {extra or 'native'}:\n{b.stderr}")
        for initial in (None, "hello world"):
            reset(initial)
            n = subprocess.run([native], capture_output=True)
            ns = snapshot()
            reset(initial)
            w = subprocess.run(["wasmtime", "run", "--dir=/tmp", wasm], capture_output=True)
            ws = snapshot()
            same = n.returncode == w.returncode and ns == ws
            label = "absent" if initial is None else "present"
            print(f"  {name:14} file {label:7} native={n.returncode:<4} wasm={w.returncode:<4} "
                  f"{'same' if same else 'DIFFERENT'}")
            if not same:
                bad.append(f"{name} ({label}): native {n.returncode} {ns}, wasm {w.returncode} {ws}")
        if show_imports:
            from wasm_imports import wasi_functions  # the import-section parser

            extra = sorted(wasi_functions(open(wasm, "rb").read()) - STARTUP)
            print(f"  {'':14} imports beyond startup: {', '.join(extra) or '-'}")
    reset(None)
    if bad:
        sys.exit("\nFAILED:\n  " + "\n  ".join(bad))
    print("native and wasm agree on every probe")


if __name__ == "__main__":
    main()
