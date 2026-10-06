#!/usr/bin/env python3
"""The file builtins, native and on `wasm32-wasip1`, side by side (docs/wasm.md, W0.5).

Part one: one tiny program per builtin the accept fixtures do not reach -- the write-mode
openers (`open_write`, `open_append`, `open_new`, `open_rw`), `file_size`,
`file_sync`, `file_truncate`, `file_pread`, `file_pwrite`, `fs_read`, `fs_write`,
`fs_remove`, `fs_rename` -- each built for the host and for wasm32, run twice
(starting with `/tmp/lexsys-probe` absent, then present), and required to **exit
with the same status and leave the same files**.

It found that the write-mode openers failed on WASI with `EINVAL`: they went through
`fopen` and a `dup` of its descriptor, and WASI has no `dup`. No accept fixture
reached them, so the coverage map never saw it.

Part two runs the directory driver `directory_writes.rs` uses on both native backends
(`dir_open_new`, `dir_open_append`, `dir_rename`, `dir_remove`, `dir_sync`) over a small
tree, and requires the same output, status and tree on both builds.

Part three runs `directory_modes.rs`'s probe (`dir_mode`, `dir_own_mode`) over a tree
with the same names and permission bits, and compares what it prints.

usage: wasm_file_check.py LEX_SYS [--imports]

`--imports` also prints each probe's WASI imports beyond the startup three, which is
how `docs/wasm.md`'s label-to-imports table was measured.

Needs what `lex-sys --help` says `--target` needs, and `wasmtime`. CI has neither.
"""
import glob, os, re, shutil, subprocess, sys, tempfile

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


TREE = "/tmp/lexsys-dir"
# (operation, names...). The driver prints `ok <n>` or `err <errno>`.
# Differences that are the platform's, not the port's. Printed every run, as "known".
KNOWN_DIR_DIFFERENCES = {
    ("sync",): "`dir_sync` is `fsync` on a directory descriptor: natively `ok 0`, on WASI `err 9` "
               "(EBADF) under wasmtime 49, which does not let a directory descriptor be synced. A "
               "durability call that fails with its errno is the honest answer; nothing to fix here",
}
DIR_CASES = [
    ("new", "b"), ("new", "a"), ("new", "sub"), ("new", ".."), ("new", "../x"), ("new", "a/b"),
    ("append", "a"), ("append", "zz"), ("append", "sub"),
    ("rename", "a", "c"), ("rename", "nope", "d"), ("rename", "a", "sub"), ("rename", "a", "../a"),
    ("remove", "a"), ("remove", "nope"), ("remove", "sub"), ("remove", ".."),
    ("sync",),
]


def directory_driver():
    """The probe program `directory_writes.rs` runs on both native backends."""
    root = os.path.join(os.path.dirname(__file__), "..")
    text = open(os.path.join(root, "crates/lex-sys/tests/conformance/directory_writes.rs")).read()
    m = re.search(r'const PROBE: &str = r#"(.*?)"#;', text, re.S)
    if not m:
        sys.exit("cannot find PROBE in directory_writes.rs")
    return m.group(1)


def tree_state():
    out = {}
    for dirpath, dirs, files in os.walk(TREE):
        for d in dirs:
            out[os.path.relpath(os.path.join(dirpath, d), TREE) + "/"] = None
        for f in files:
            full = os.path.join(dirpath, f)
            out[os.path.relpath(full, TREE)] = open(full, "rb").read()
    return out


def fresh_tree():
    shutil.rmtree(TREE, ignore_errors=True)
    os.makedirs(os.path.join(TREE, "sub"))
    open(os.path.join(TREE, "a"), "w").write("old")


def directory_part(lex_sys, work, show_imports):
    src = os.path.join(work, "dirdriver.ls")
    open(src, "w").write(directory_driver())
    native, wasm = os.path.join(work, "dirdriver"), os.path.join(work, "dirdriver.wasm")
    for out, extra in ((native, []), (wasm, ["--target", "wasm32-wasip1"])):
        b = subprocess.run([lex_sys, "build", src, "--std", *extra, "-o", out], capture_output=True, text=True)
        if b.returncode:
            sys.exit(f"directory driver: build failed {extra or 'native'}:\n{b.stderr}")
    bad = []
    for case in DIR_CASES:
        fresh_tree()
        n = subprocess.run([native, TREE, *case], capture_output=True)
        ns = tree_state()
        fresh_tree()
        w = subprocess.run(["wasmtime", "run", "--dir=/tmp", wasm, TREE, *case], capture_output=True)
        ws = tree_state()
        same = (n.returncode, n.stdout, ns) == (w.returncode, w.stdout, ws)
        said = n.stdout.decode().strip()
        known = case in KNOWN_DIR_DIFFERENCES and not same
        verdict = "same" if same else ("known difference" if known else "DIFFERENT")
        print(f"  dir {' '.join(case):22} native: {said:8} wasm: {w.stdout.decode().strip():8} {verdict}")
        if known:
            print(f"      {KNOWN_DIR_DIFFERENCES[case]}")
        elif not same:
            bad.append(f"dir {case}: native {n.returncode} {n.stdout!r} {sorted(ns)}, "
                       f"wasm {w.returncode} {w.stdout!r} {sorted(ws)}")
    if show_imports:
        from wasm_imports import wasi_functions

        extra = sorted(wasi_functions(open(wasm, "rb").read()) - STARTUP)
        print(f"  dir driver imports beyond startup: {', '.join(extra)}")
    shutil.rmtree(TREE, ignore_errors=True)
    return bad


MODES = "/tmp/lexsys-modes"


def modes_driver():
    root = os.path.join(os.path.dirname(__file__), "..")
    text = open(os.path.join(root, "crates/lex-sys/tests/conformance/directory_modes.rs")).read()
    m = re.search(r'const PROBE: &str = r#"(.*?)"#;', text, re.S)
    if not m:
        sys.exit("cannot find PROBE in directory_modes.rs")
    return m.group(1)


def build_modes_tree():
    shutil.rmtree(MODES, ignore_errors=True)
    os.makedirs(MODES)
    for name, bits in (("f600", 0o600), ("f644", 0o644), ("f400", 0o400), ("f4755", 0o4755), ("f000", 0o000)):
        path = os.path.join(MODES, name)
        open(path, "w").write("x")
        os.chmod(path, bits)
    for name, bits in (("d0750", 0o750), ("d1777", 0o1777)):
        path = os.path.join(MODES, name)
        os.makedirs(path)
        os.chmod(path, bits)
    os.symlink("f644", os.path.join(MODES, "link"))
    os.chmod(MODES, 0o755)


def modes_part(lex_sys, work, show_imports):
    src = os.path.join(work, "modes.ls")
    open(src, "w").write(modes_driver())
    native, wasm = os.path.join(work, "modes"), os.path.join(work, "modes.wasm")
    b = subprocess.run([lex_sys, "build", src, "--std", "-o", native], capture_output=True, text=True)
    if b.returncode:
        sys.exit(f"modes driver: native build failed:\n{b.stderr}")
    w = subprocess.run([lex_sys, "build", src, "--std", "--target", "wasm32-wasip1", "-o", wasm],
                       capture_output=True, text=True)
    if w.returncode and "permission bits do not exist" in w.stderr:
        # W0.6: WASI's stat has no permission bits, so `dir_mode` would answer 0 for every
        # file. Before the refusal this part printed `f600 0` against `600`.
        print("  modes: refused on WASI (W0.6): permission bits do not exist there")
        return []
    if w.returncode:
        sys.exit(f"modes driver: wasm build failed:\n{w.stderr}")
    build_modes_tree()
    n = subprocess.run([native, MODES], capture_output=True)
    w = subprocess.run(["wasmtime", "run", "--dir=/tmp", wasm, MODES], capture_output=True)
    nl, wl = n.stdout.decode().splitlines(), w.stdout.decode().splitlines()
    bad = []
    for i in range(max(len(nl), len(wl))):
        a, b = (nl[i] if i < len(nl) else "<none>"), (wl[i] if i < len(wl) else "<none>")
        same = a == b
        print(f"  modes native: {a:14} wasm: {b:14} {'same' if same else 'DIFFERENT'}")
        if not same:
            bad.append(f"modes line {i}: native {a!r}, wasm {b!r}")
    if show_imports:
        from wasm_imports import wasi_functions

        extra = sorted(wasi_functions(open(wasm, "rb").read()) - STARTUP)
        print(f"  modes driver imports beyond startup: {', '.join(extra)}")
    shutil.rmtree(MODES, ignore_errors=True)
    return bad


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
    bad += directory_part(lex_sys, work, show_imports)
    bad += modes_part(lex_sys, work, show_imports)
    if bad:
        sys.exit("\nFAILED:\n  " + "\n  ".join(bad))
    print("native and wasm agree on every probe")


if __name__ == "__main__":
    main()
