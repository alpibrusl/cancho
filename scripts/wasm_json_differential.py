#!/usr/bin/env python3
"""W1 (docs/wasm.md): the same JSON filter, native and as a wasm module.

Builds `tests/programs/json_roundtrip.ls` -- parse standard input with `std.json`
and write it back, or `E <code> <position>` if the document is refused -- twice,
for the host and for `wasm32-wasip1`, feeds both the same documents, and requires
**byte-identical stdout and the same exit code** for every one. It exercises the
heap, arenas, boxed slices and bytes, which is exactly where a pointer-width bug
would show, and `std.json`'s float printer and parser, which is where a numeric
one would.

It also prints the module's **import list**, read straight from the binary's
import section, because that list is what W2 turns into a check.

usage: wasm_json_differential.py LEX_SYS [--count N] [--seed S] [--keep DIR]

Needs what `lex-sys --help` says `--target` needs (CLANG, WASI_SYSROOT, wasm-ld)
and `wasmtime` on the path. CI has none of it, so this is not in CI.
"""
import argparse, concurrent.futures, decimal, math, os, random, struct, subprocess, sys, tempfile
from fractions import Fraction

PROGRAM = "tests/programs/json_roundtrip.ls"

# The seeds `crates/lex-sys/tests/conformance/json.rs` uses: valid documents that
# exercise every production.
SEEDS = [
    r'{"name":"Ada","age":36,"pi":3.14159,"ok":true,"none":null,"tags":["a","b"],"nest":{"x":[1,2,{"y":-1e3}]}}',
    r'[1,-2,3.5,-0.25,1e10,1E-5,0,-0,true,false,null,"s","",{},[]]',
    r'{"esc":"quote\" back\\ slash\/ nl\n cr\r tab\t bs\b ff\f","uni":"é日😀","raw":"é日😀"}',
    r'  { "spaced" : [ 1 , 2 , 3 ] , "k" : "v" }  ',
    r'{"a":{"b":{"c":{"d":{"e":[[[[[1]]]]]}}}}}',
    r'[0.1,0.2,0.3,123456789.123456789,2.5e-8,1.5e22,100,200,300]',
    '"just a string"',
    "-12.5e-3",
    "true",
    r'{"":"empty key","a b":1,"\u0000":2}',
]

POOL = b'{}[]",:\\-+.eE0123456789tfnul \t\n\r\x00\x1f\x7f\xc3\xa9\xff\xed\xa0\x80uabx/\''


def mutate(seed: bytes, rng: random.Random) -> bytes:
    doc = bytearray(seed)
    for _ in range(1 + rng.randrange(2)):
        if not doc:
            break
        at = rng.randrange(len(doc))
        kind = rng.randrange(5)
        if kind == 0:
            doc[at] = POOL[rng.randrange(len(POOL))]
        elif kind == 1:
            del doc[at]
        elif kind == 2:
            doc.insert(at, POOL[rng.randrange(len(POOL))])
        elif kind == 3:
            del doc[at:]
        else:
            to = rng.randrange(len(doc))
            doc[at], doc[to] = doc[to], doc[at]
    return bytes(doc)


def numbers(rng: random.Random, n: int) -> list:
    """Numbers as text: every exponent, long decimals, and the halfway cases
    between two adjacent doubles that a floating-point shortcut gets wrong."""
    out = []
    for _ in range(n):
        x = struct.unpack("<d", struct.pack("<Q", rng.getrandbits(63)))[0]
        if math.isfinite(x):
            out.append(f"{x:e}")
            out.append(f"-{x!r}")
    for _ in range(n):
        x = rng.random()
        out.append(f"{x:.16e}")
        out.append(f"{x:.20f}")
        out.append(f"{x * 1000:.3f}")
    decimal.getcontext().prec = 900
    for _ in range(n // 2):
        x = 1.0 + rng.random() * 1e3
        y = math.nextafter(x, math.inf)
        mid = (decimal.Decimal(x) + decimal.Decimal(y)) / 2
        for d in (mid, mid * (1 + decimal.Decimal("1e-30")), mid * (1 - decimal.Decimal("1e-30"))):
            out.append(format(d, "f"))
    return out


def corpus(count: int, seed: int) -> list:
    rng = random.Random(seed)
    docs = [s.encode() for s in SEEDS]
    docs += [mutate(SEEDS[rng.randrange(len(SEEDS))].encode(), rng) for _ in range(count)]
    docs += [f"[{n}]".encode() for n in numbers(rng, max(count // 4, 10))]
    return docs


def leb(data: bytes, at: int):
    value = shift = 0
    while True:
        b = data[at]
        at += 1
        value |= (b & 0x7F) << shift
        shift += 7
        if not b & 0x80:
            return value, at


def imports(wasm: bytes) -> list:
    """`module.name (kind)` for every entry of a wasm module's import section."""
    assert wasm[:4] == b"\0asm", "not a wasm module"
    at, out = 8, []
    while at < len(wasm):
        sec, at = wasm[at], at + 1
        size, at = leb(wasm, at)
        end = at + size
        if sec == 2:
            count, p = leb(wasm, at)
            for _ in range(count):
                n, p = leb(wasm, p); mod, p = wasm[p:p + n].decode(), p + n
                n, p = leb(wasm, p); name, p = wasm[p:p + n].decode(), p + n
                kind, p = wasm[p], p + 1
                if kind == 0:
                    _, p = leb(wasm, p)
                elif kind == 1:
                    p += 1
                    flags, p = leb(wasm, p); _, p = leb(wasm, p)
                    if flags & 1:
                        _, p = leb(wasm, p)
                elif kind == 2:
                    flags, p = leb(wasm, p); _, p = leb(wasm, p)
                    if flags & 1:
                        _, p = leb(wasm, p)
                elif kind == 3:
                    p += 2
                out.append(f"{mod}.{name} ({['func', 'table', 'memory', 'global'][kind]})")
        at = end
    return out


def run(cmd, doc: bytes):
    p = subprocess.run(cmd, input=doc, capture_output=True, timeout=120)
    return p.returncode, p.stdout, p.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lex_sys")
    ap.add_argument("--count", type=int, default=600, help="mutated documents (numbers add count/4 * ~8)")
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--keep", help="keep the built binaries in this directory")
    args = ap.parse_args()

    work = args.keep or tempfile.mkdtemp(prefix="wasm-json-")
    os.makedirs(work, exist_ok=True)
    native, wasm = os.path.join(work, "roundtrip"), os.path.join(work, "roundtrip.wasm")
    for out, extra in ((native, []), (wasm, ["--target", "wasm32-wasip1"])):
        b = subprocess.run([args.lex_sys, "build", PROGRAM, "--std", *extra, "-o", out],
                           capture_output=True, text=True)
        if b.returncode != 0:
            sys.exit(f"build failed ({extra or 'native'}):\n{b.stderr}")

    docs = corpus(args.count, args.seed)
    wasmtime = os.environ.get("WASMTIME", "wasmtime")

    def one(doc):
        a = run([native], doc)
        b = run([wasmtime, "run", wasm], doc)
        return doc, a, b

    accepted = refused = 0
    bad = []
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for doc, (rc_a, out_a, _), (rc_b, out_b, err_b) in pool.map(one, docs):
            if (rc_a, out_a) != (rc_b, out_b):
                bad.append((doc, rc_a, out_a, rc_b, out_b, err_b))
            elif out_a.startswith(b"E "):
                refused += 1
            else:
                accepted += 1

    print(f"documents: {len(docs)}  accepted (identical): {accepted}  refused (identical): {refused}  "
          f"different: {len(bad)}")
    print("imports of the wasm module:")
    for line in imports(open(wasm, "rb").read()):
        print("  ", line)
    for doc, rc_a, out_a, rc_b, out_b, err_b in bad[:8]:
        print(f"\nDIFFERENT on {doc[:100]!r}\n  native rc={rc_a} {out_a[:100]!r}\n"
              f"  wasm   rc={rc_b} {out_b[:100]!r} {err_b[:160]!r}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
