#!/usr/bin/env python3
"""The import section of a wasm module, read straight from the binary.

`imports(bytes)` answers `module.name` for every function a module imports;
run as a program it prints them, one per line, for each file named. It is the
measurement behind `docs/wasm.md`'s authority check: what the runtime is asked to
grant a module is exactly this list, so it is read from the module rather than
inferred from how the module was built.

usage: wasm_imports.py MODULE.wasm...
"""
import sys


def _leb(data: bytes, at: int):
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return value, at


def _name(data: bytes, at: int):
    n, at = _leb(data, at)
    return data[at:at + n].decode(), at + n


def _limits(data: bytes, at: int):
    flags, at = _leb(data, at)
    _, at = _leb(data, at)
    if flags & 1:
        _, at = _leb(data, at)
    return at


def imports(wasm: bytes) -> list:
    """Every import as `module.name (kind)`, in the order the module lists them."""
    assert wasm[:4] == b"\0asm", "not a wasm module"
    at, out = 8, []
    while at < len(wasm):
        section, at = wasm[at], at + 1
        size, at = _leb(wasm, at)
        end = at + size
        if section == 2:
            count, p = _leb(wasm, at)
            for _ in range(count):
                module, p = _name(wasm, p)
                name, p = _name(wasm, p)
                kind, p = wasm[p], p + 1
                if kind == 0:
                    _, p = _leb(wasm, p)
                elif kind == 1:
                    p = _limits(wasm, p + 1)
                elif kind == 2:
                    p = _limits(wasm, p)
                elif kind == 3:
                    p += 2
                out.append(f"{module}.{name} ({['func', 'table', 'memory', 'global'][kind]})")
        at = end
    return out


def wasi_functions(wasm: bytes) -> set:
    """The WASI preview 1 functions a module imports, by bare name."""
    prefix = "wasi_snapshot_preview1."
    return {i[len(prefix):].split(" ")[0] for i in imports(wasm) if i.startswith(prefix)}


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print(path)
        for line in imports(open(path, "rb").read()):
            print("  ", line)
