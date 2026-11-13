#!/usr/bin/env python3
"""Regenerate `examples/selfhost/runtime.cho` from `runtime.ll` and `runtime/*.ll` (cancho#487, first box), without Rust.

`runtime.cho` is those files as the strings the compiler in cancho prints. It used to be written by
`crates/cancho-ir/src/tests/selfhost_tables.rs`; this is the same algorithm, so the two agree byte for byte on today's inputs.

    scripts/embed_runtime.py            # rewrite runtime.cho
    scripts/embed_runtime.py --check    # exit 1 if runtime.cho is not what the .ll files say
"""
import os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SELF = os.path.join(ROOT, "examples", "selfhost")


def escape(text):
    return text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")


def read(name, required=True):
    path = os.path.join(SELF, name)
    if not os.path.exists(path):
        if required: raise SystemExit(f"missing {path}")
        return ""
    return escape(open(path).read())


def render():
    g = {k: read(f"runtime/{k}.ll") for k in (
        "net_linux", "net_darwin", "crypto_x86_64", "crypto_aarch64_linux", "crypto_aarch64_darwin", "os_linux", "os_darwin", "dirs_linux",
        "dirs_darwin", "signals_linux", "signals_darwin", "tty_linux", "tty_darwin", "camera_linux", "camera_darwin", "process")}
    region_linux, region_darwin = read("runtime/region_linux.ll", False), read("runtime/region_darwin.ll", False)
    runtime = read("runtime.ll")
    return (
        "module selfhost.runtime;\n\n"
        "// runtime.cho -- the support the code of a few builtins calls.\n"
        "//\n"
        "// GENERATED from `runtime.ll` by `scripts/embed_runtime.py`; do not edit. Its `--check` fails\n"
        "// when this file is not what `runtime.ll` says, and\n"
        "// running it with no argument rewrites it.\n\n"
        f'pub fn text() -> [] &static [byte] {{\n    return "{runtime}";\n}}\n\n'
        "// The sockets and the clock (`runtime/net.c`, built for each target by `runtime/make.sh`).\n"
        f'pub fn net_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["net_darwin"]}";\n    }}\n    return "{g["net_linux"]}";\n}}\n\n'
        "// The directory handles and listings (`runtime/dirs.c`).\n"
        f'pub fn dirs_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["dirs_darwin"]}";\n    }}\n    return "{g["dirs_linux"]}";\n}}\n\n'
        "// The signal claim (`runtime/signals.c`).\n"
        f'pub fn signals_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["signals_darwin"]}";\n    }}\n    return "{g["signals_linux"]}";\n}}\n\n'
        "// The serial-port verbs (`runtime/tty.c`).\n"
        f'pub fn tty_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["tty_darwin"]}";\n    }}\n    return "{g["tty_linux"]}";\n}}\n\n'
        "// The camera verbs (`runtime/camera.c`).\n"
        f'pub fn camera_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["camera_darwin"]}";\n    }}\n    return "{g["camera_linux"]}";\n}}\n\n'
        "// The thread that reaps a released child (`runtime/process.ll`).\n"
        f'pub fn process_text() -> [] &static [byte] {{\n    return "{g["process"]}";\n}}\n\n'
        "// The per-thread stack of region chunks (`cancho-codegen-llvm`'s `region_cache`).\n"
        f'pub fn region_text(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{region_darwin}";\n    }}\n    return "{region_linux}";\n}}\n\n'
        "// The hardware AES and GHASH functions, written by `cancho-codegen-llvm` for each target: `target` is 1 for\n"
        "// Darwin on aarch64, 2 for Linux on aarch64, 0 for Linux on x86-64.\n"
        f'pub fn crypto_text(target: int) -> [] &static [byte] {{\n    if target == 1 {{\n        return "{g["crypto_aarch64_darwin"]}";\n    }}\n    '
        f'if target == 2 {{\n        return "{g["crypto_aarch64_linux"]}";\n    }}\n    return "{g["crypto_x86_64"]}";\n}}\n\n'
        "// What differs between systems: the address of `errno`, how a spawn is set up, a child's word.\n"
        f'pub fn errno_location(darwin: bool) -> [] &static [byte] {{\n    if darwin {{\n        return "{g["os_darwin"]}";\n    }}\n    return "{g["os_linux"]}";\n}}\n'
    )


if __name__ == "__main__":
    path = os.path.join(SELF, "runtime.cho")
    text = render()
    if "--check" in sys.argv:
        if open(path).read() != text:
            print("FAILS runtime.cho is not what the .ll files say; run scripts/embed_runtime.py"); sys.exit(1)
        print("ok runtime.cho is what the .ll files say")
    else:
        open(path, "w").write(text); print("wrote", path)
