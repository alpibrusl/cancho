#!/usr/bin/env python3
"""The vectors of the native backend's data layout (docs/native-backend.md section 4): for each type below, the size and the alignment clang gives it, and
for each struct the offset of every field, written into tests/native/layout_test.cho. Run from the root of the repository:

    python3 tests/native/layout_vectors.py            # rewrite the two tables of tests/native/layout_test.cho
    python3 tests/native/layout_vectors.py --check    # exit 1 if they would change

clang is asked exactly what the compiler asks it (`ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)`), for aarch64-none-elf,
aarch64-unknown-linux-gnu and x86_64-unknown-linux-gnu, and the three must agree: the layout module has one answer.
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TEST = os.path.join(ROOT, "tests", "native", "layout_test.cho")
CLANG = os.environ.get("CLANG", "clang")
TARGETS = ["aarch64-none-elf", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"]

SCALARS = ["i1", "i8", "i16", "i32", "i64", "i128", "float", "double", "ptr"]
TYPES = SCALARS + [
    "{}", "{ i8, i64 }", "{ i64, i1 }", "{ ptr, i64 }", "{ i32, i8 }", "[3 x { i32, i8 }]", "{ i8, { i8, i32 }, i8 }", "{ i8, i16, i8 }",
    "{ i1, double }", "{ i8, i128 }", "[5 x i8]", "[0 x i64]", "{ { ptr, i64 }, i64 }", "{ i64, { { ptr, i64 }, i64 }, i8 }",
    "{ i8, [3 x i16], i8, float, i8, double }", "[2 x [3 x { i8, i32 }]]", "{ {}, i64, {} }", "{ i32, i32, i32, i32, i32 }",
    "{ i64, i64, i64, i64, i64 }", "{ float, i8, float }", "{ i1, i1, i1, ptr, i1 }", "[7 x { i16, i8 }]", "{ [3 x i8], i32, [5 x i8], i64 }",
    "{ i8, { i8, { i8, { i8, i64 } } } }", "{ ptr, { i64, i1 }, [2 x ptr] }", "<16 x i8>", "<2 x i64>", "<4 x i32>", "<16 x i1>", "{ i8, <2 x i64> }",
]


def fields(ty):
    """The top-level field types of a struct type's text."""
    if not (ty.startswith("{ ") and ty.endswith(" }")):
        return []
    out, depth, start = [], 0, 2
    for i, c in enumerate(ty[2:-2], 2):
        if c in "{[<":
            depth += 1
        elif c in "}]>":
            depth -= 1
        elif c == "," and depth == 0:
            out.append(ty[start:i].strip())
            start = i + 1
    out.append(ty[start:-2].strip())
    return out


def ask(target):
    """size, alignment and field offsets of each type, from the constants clang folds."""
    lines = []
    for n, ty in enumerate(TYPES):
        lines.append("@s%d = global i64 ptrtoint (ptr getelementptr (%s, ptr null, i32 1) to i64)" % (n, ty))
        lines.append("@a%d = global i64 ptrtoint (ptr getelementptr ({ i8, %s }, ptr null, i32 0, i32 1) to i64)" % (n, ty))
        for k in range(len(fields(ty))):
            lines.append("@o%d_%d = global i64 ptrtoint (ptr getelementptr (%s, ptr null, i32 0, i32 %d) to i64)" % (n, k, ty, k))
    with tempfile.TemporaryDirectory() as d:
        src = os.path.join(d, "l.ll")
        open(src, "w").write("\n".join(lines) + "\n")
        out = subprocess.run([CLANG, "-O1", "-Wno-override-module", "--target=" + target, "-S", "-emit-llvm", src, "-o", "-"], capture_output=True, text=True, check=True).stdout
    values = dict(re.findall(r"^@(\w+) = .*global i64 (\d+)", out, re.M))
    rows = []
    for n, ty in enumerate(TYPES):
        rows.append([int(values["s%d" % n]), int(values["a%d" % n])] + [int(values["o%d_%d" % (n, k)]) for k in range(len(fields(ty)))])
    return rows


def main():
    answers = [ask(t) for t in TARGETS]
    for t, a in zip(TARGETS[1:], answers[1:]):
        assert a == answers[0], "%s and %s disagree" % (TARGETS[0], t)
    module = "".join("@g%d = external global %s\\n" % (n, ty) for n, ty in enumerate(TYPES))
    expected = "".join(" " + " ".join(str(v) for v in row) + " ;" for row in answers[0])
    text = open(TEST).read()
    new = re.sub(r'(fn globals\(\) -> \[\] &static \[byte\] \{\n    return ")[^"]*(";)', lambda m: m.group(1) + module + m.group(2), text)
    new = re.sub(r'(fn expected\(\) -> \[\] &static \[byte\] \{\n    return ")[^"]*(";)', lambda m: m.group(1) + expected + m.group(2), new)
    if "--check" in sys.argv:
        print("layout vectors: %d types, %s" % (len(TYPES), "unchanged" if new == text else "WOULD CHANGE"))
        sys.exit(0 if new == text else 1)
    open(TEST, "w").write(new)
    print("layout vectors: %d types on %d targets" % (len(TYPES), len(TARGETS)))


main()
