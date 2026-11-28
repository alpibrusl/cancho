#!/usr/bin/env python3
"""What clang's assembler makes of the sources the assembler of compiler/native/a64asm.cho is held to (docs/native-backend.md, N7). Run from the root:

    python3 tests/native/asm_vectors.py            # rewrite tests/native/asm/NAME.expected for every tests/native/asm/NAME.S
    python3 tests/native/asm_vectors.py --check    # exit 1 if one would change
    python3 tests/native/asm_vectors.py FILE.S     # print what FILE.S.expected would hold, and write nothing

It needs `clang` with the AArch64 target, `llvm-readelf` and `llvm-objcopy` ($CLANG, $READELF and $OBJCOPY name others). The test that reads the files
(tests/commands/native_asm.sh) needs none of them: tests/native/asm_check.cho prints the same lines from the assembler's own table, and the two are compared.

The lines:
    section NAME align N size N                      every section clang gave bytes to or the source named, `.text` first
    bytes HEX                                        the section's bytes, 32 to a line
    label NAME SECTION OFFSET local|global           every label with a name, in the order of the names
    relocation KIND SECTION OFFSET NAME ADDEND       in the order of the sections and offsets; NAME is a section for a label that is not global
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DIR = os.path.join(ROOT, "tests", "native", "asm")
CLANG = os.environ.get("CLANG", "clang")
READELF = os.environ.get("READELF", "llvm-readelf")
OBJCOPY = os.environ.get("OBJCOPY", "llvm-objcopy")
KINDS = {
    "R_AARCH64_ABS64": "abs64",
    "R_AARCH64_CALL26": "call26",
    "R_AARCH64_JUMP26": "jump26",
    "R_AARCH64_ADR_PREL_LO21": "adr21",
    "R_AARCH64_CONDBR19": "cond19",
    "R_AARCH64_LD_PREL_LO19": "literal19",
}


def run(*command):
    return subprocess.run(command, check=True, capture_output=True, text=True).stdout


def expected(source):
    """The lines for one source."""
    out = []
    with tempfile.TemporaryDirectory() as work:
        obj = os.path.join(work, "a.o")
        subprocess.run([CLANG, "--target=aarch64-none-elf", "-c", source, "-o", obj], check=True)
        # The sections with bytes of the program in them: number -> (name, size, alignment).
        sections = {}
        for line in run(READELF, "-S", "-W", obj).splitlines():
            m = re.match(r"\s*\[\s*(\d+)\]\s+(\S+)\s+PROGBITS\s+\S+\s+\S+\s+([0-9a-f]+)\s+\S+\s+(\S+)\s+\d+\s+\d+\s+(\d+)\s*$", line)
            if m and "A" in m.group(4):
                sections[int(m.group(1))] = (m.group(2), int(m.group(3), 16), int(m.group(5)))
        for number in sorted(sections):
            name, size, align = sections[number]
            out.append("section %s align %d size %d" % (name, align, size))
            raw = os.path.join(work, "raw")
            subprocess.run([OBJCOPY, "-O", "binary", "--only-section=" + name, obj, raw], check=True)
            data = open(raw, "rb").read() if size else b""
            if len(data) != size:
                sys.exit("%s: section %s is %d bytes and objcopy gave %d" % (source, name, size, len(data)))
            for i in range(0, size, 32):
                out.append("bytes " + data[i : i + 32].hex())
        labels = []
        for line in run(READELF, "-s", "-W", obj).splitlines():
            m = re.match(r"\s*\d+:\s+([0-9a-f]+)\s+\d+\s+NOTYPE\s+(LOCAL|GLOBAL)\s+\S+\s+(\d+)\s+(\S+)\s*$", line)
            # `$x` and `$d` say where code and data begin: they are clang's, not the source's.
            if m and not m.group(4).startswith("$") and int(m.group(3)) in sections:
                labels.append((m.group(4).encode(), sections[int(m.group(3))][0], int(m.group(1), 16), m.group(2).lower()))
        for name, section, offset, binding in sorted(labels):
            out.append("label %s %s %d %s" % (name.decode(), section, offset, binding))
        relocations = []
        section = None
        for line in run(READELF, "-r", "-W", obj).splitlines():
            m = re.match(r"Relocation section '\.rela(\S+)'", line)
            if m:
                section = m.group(1)
                continue
            m = re.match(r"([0-9a-f]{16})\s+[0-9a-f]+\s+(R_\S+)\s+[0-9a-f]+\s+(\S+) \+ ([0-9a-f]+)\s*$", line)
            if m:
                if m.group(2) not in KINDS:
                    sys.exit("%s: a relocation of a kind the assembler does not have: %s" % (source, m.group(2)))
                order = [n for n in sorted(sections) if sections[n][0] == section][0]
                relocations.append((order, int(m.group(1), 16), KINDS[m.group(2)], section, m.group(3), int(m.group(4), 16)))
            elif line.strip() and not line.lstrip().startswith("Offset") and not line.startswith("There are no relocations"):
                sys.exit("%s: a line of relocations this script cannot read: %s" % (source, line))
        for _, offset, kind, section, name, addend in sorted(relocations):
            out.append("relocation %s %s %d %s %d" % (kind, section, offset, name, addend))
    return "\n".join(out) + "\n"


def main():
    arguments = sys.argv[1:]
    if arguments and arguments[0] != "--check":
        sys.stdout.write(expected(arguments[0]))
        return 0
    changed = 0
    for name in sorted(os.listdir(DIR)):
        if not name.endswith(".S"):
            continue
        path = os.path.join(DIR, name[:-2] + ".expected")
        text = expected(os.path.join(DIR, name))
        old = open(path).read() if os.path.exists(path) else None
        if old != text:
            changed += 1
            if arguments:
                print("would change: " + os.path.relpath(path, ROOT))
            else:
                open(path, "w").write(text)
                print("wrote " + os.path.relpath(path, ROOT))
    if arguments and changed:
        return 1
    print("%d sources, %d changed" % (len([n for n in os.listdir(DIR) if n.endswith(".S")]), changed))
    return 0


if __name__ == "__main__":
    sys.exit(main())
