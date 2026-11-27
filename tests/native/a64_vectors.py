#!/usr/bin/env python3
"""The vectors of the AArch64 encoder (docs/native-backend.md section 11): for every form of compiler/native/a64.cho, operand choices written as assembly,
assembled by clang, and the words it made written to tests/native/a64.vectors. Run from the root of the repository:

    python3 tests/native/a64_vectors.py            # rewrite tests/native/a64.vectors and compiler/native/a64sys.cho's table
    python3 tests/native/a64_vectors.py --check    # exit 1 if either would change

It needs `clang` with the AArch64 target and `llvm-objcopy` ($CLANG and $OBJCOPY name others). The test that reads the file (tests/commands/a64.cho) needs
neither. What is here is a second implementation on purpose: this script knows how each form is *written*, clang knows what it *is*, and neither is the
encoder. The choices come from a generator of its own (not Python's, whose sequence is a property of the version), so the file is the same on every machine.

A line of the file is `form operand... word`, the operands in the order of the function's parameters and the word in hexadecimal, then ` ; ` and the
assembly it came from. The system registers `Cpu` accepts (compiler/bare.cho) are each a `system_register` line and a row of the table.
"""
import os, re, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "tests", "native", "a64.vectors")
SYS = os.path.join(ROOT, "compiler", "native", "a64sys.cho")
CLANG = os.environ.get("CLANG", "clang")
OBJCOPY = os.environ.get("OBJCOPY", "llvm-objcopy")
PER_FORM = 150

state = 0x243F6A8885A308D3


def rnd(n):
    """0 <= value < n, from a 64-bit linear congruential generator."""
    global state
    state = (state * 6364136223846793005 + 1442695040888963407) & (2**64 - 1)
    return (state >> 33) % n


def signed(bits):
    edge = [-(1 << (bits - 1)), (1 << (bits - 1)) - 1, 0, -1, 1]
    return edge[rnd(len(edge))] if rnd(4) == 0 else rnd(1 << bits) - (1 << (bits - 1))


def unsigned(bits):
    edge = [0, (1 << bits) - 1, 1]
    return edge[rnd(len(edge))] if rnd(4) == 0 else rnd(1 << bits)


def reg():
    return 31 if rnd(8) == 0 else rnd(31)


def x(r, sf=1, sp=False):
    if r == 31:
        return ("sp" if sf else "wsp") if sp else ("xzr" if sf else "wzr")
    return ("x" if sf else "w") + str(r)


def f(r, double):
    return ("d" if double else "s") + str(r)


COND = "eq ne hs lo mi pl vs vc hi ls ge lt gt le al nv".split()
vectors = []  # (form, operands, assembly)


def add(form, operands, text):
    vectors.append((form, [int(o) for o in operands], text))


def rel(words):
    """An offset of that many instructions, as the assembler is told it."""
    return ".+%d" % (4 * words) if words >= 0 else ".-%d" % (-4 * words)


def decode_mask(n, immr, imms):
    """The Arm pseudocode DecodeBitMasks, for the 64-bit value: None if the bits say none."""
    key = (n << 6) | (~imms & 63)
    length = key.bit_length() - 1
    if length < 1:
        return None
    size = 1 << length
    s, r = imms & (size - 1), immr & (size - 1)
    if s == size - 1:
        return None
    element = (1 << (s + 1)) - 1
    element = ((element >> r) | (element << (size - r))) & ((1 << size) - 1)
    value = 0
    for i in range(0, 64, size):
        value |= element << i
    return value


def generate():
    for _ in range(PER_FORM):
        sf, rd, rn, rm, ra = rnd(2), reg(), reg(), reg(), reg()
        opc = [0, 2, 3][rnd(3)]
        hw = rnd(4 if sf else 2)
        imm = unsigned(16)
        add("movw", [sf, opc, rd, imm, hw], "%s %s, #%d, lsl #%d" % (["movn", "", "movz", "movk"][opc], x(rd, sf), imm, 16 * hw))
        off = signed(21)
        add("adr", [0, rd, off], "adr %s, %s" % (x(rd), ".+%d" % off if off >= 0 else ".-%d" % -off))
        sub, s, sh, imm = rnd(2), rnd(2), rnd(2), unsigned(12)
        name = ["add", "sub"][sub] + ("s" if s else "")
        add("addsub_imm", [sf, sub, s, rd, rn, imm, sh], "%s %s, %s, #%d%s" % (name, x(rd, sf, sp=not s), x(rn, sf, sp=True), imm, ", lsl #12" if sh else ""))
        shift, amount = rnd(3), unsigned(5 + sf)
        add("addsub_shift", [sf, sub, s, rd, rn, rm, shift, amount], "%s %s, %s, %s, %s #%d" % (name, x(rd, sf), x(rn, sf), x(rm, sf), ["lsl", "lsr", "asr"][shift], amount))
        option, amount = rnd(8), rnd(5)
        wide = sf and option in (3, 7)
        add("addsub_ext", [sf, sub, s, rd, rn, rm, option, amount], "%s %s, %s, %s, %s #%d" % (name, x(rd, sf, sp=not s), x(rn, sf, sp=True), x(rm, 1 if wide else 0), "uxtb uxth uxtw uxtx sxtb sxth sxtw sxtx".split()[option], amount))
        lop, invert, shift, amount = rnd(4), rnd(2), rnd(4), unsigned(5 + sf)
        lname = [["and", "orr", "eor", "ands"], ["bic", "orn", "eon", "bics"]][invert][lop]
        add("logic_shift", [sf, lop, invert, rd, rn, rm, shift, amount], "%s %s, %s, %s, %s #%d" % (lname, x(rd, sf), x(rn, sf), x(rm, sf), ["lsl", "lsr", "asr", "ror"][shift], amount))
        value = None
        while value is None:
            n, immr, imms = (rnd(2) if sf else 0), rnd(64), rnd(64)
            value = decode_mask(n, immr, imms)
        if not sf:
            value &= 0xFFFFFFFF
        add("logic_imm", [sf, lop, rd, rn, value if value < 2**63 else value - 2**64], "%s %s, %s, #0x%x" % (["and", "orr", "eor", "ands"][lop], x(rd, sf, sp=lop != 3), x(rn, sf), value))
        bop, immr, imms = rnd(3), unsigned(5 + sf), unsigned(5 + sf)
        add("bitfield", [sf, bop, rd, rn, immr, imms], "%s %s, %s, #%d, #%d" % (["sbfm", "bfm", "ubfm"][bop], x(rd, sf), x(rn, sf), immr, imms))
        kind, amount = rnd(3), unsigned(5 + sf)
        add("shift_imm", [sf, kind, rd, rn, amount], "%s %s, %s, #%d" % (["lsl", "lsr", "asr"][kind], x(rd, sf), x(rn, sf), amount))
        sgn, bits = rnd(2), [8, 16, 32][rnd(3)]
        if bits == 32:
            add("extend", [1, 1, 32, rd, rn], "sxtw %s, %s" % (x(rd), x(rn, 0)))
        elif sgn:
            add("extend", [sf, 1, bits, rd, rn], "sxt%s %s, %s" % ("b" if bits == 8 else "h", x(rd, sf), x(rn, 0)))
        else:
            add("extend", [sf, 0, bits, rd, rn], "uxt%s %s, %s" % ("b" if bits == 8 else "h", x(rd, 0), x(rn, 0)))
        lsb = unsigned(5 + sf)
        add("extr", [sf, rd, rn, rm, lsb], "extr %s, %s, %s, #%d" % (x(rd, sf), x(rn, sf), x(rm, sf), lsb))
        op = [2, 3, 8, 9, 10, 11][rnd(6)]
        add("dp2", [sf, op, rd, rn, rm], "%s %s, %s, %s" % ({2: "udiv", 3: "sdiv", 8: "lslv", 9: "lsrv", 10: "asrv", 11: "rorv"}[op], x(rd, sf), x(rn, sf), x(rm, sf)))
        op = rnd(6 if sf else 5)
        if op == 3 and not sf:
            op = 4
        names = ["rbit", "rev16", "rev32" if sf else "rev", "rev", "clz", "cls"]
        add("dp1", [sf, op, rd, rn], "%s %s, %s" % (names[op], x(rd, sf), x(rn, sf)))
        add("madd", [sf, sub, rd, rn, rm, ra], "%s %s, %s, %s, %s" % (["madd", "msub"][sub], x(rd, sf), x(rn, sf), x(rm, sf), x(ra, sf)))
        u = rnd(2)
        add("mulh", [u, rd, rn, rm], "%s %s, %s, %s" % (["smulh", "umulh"][u], x(rd), x(rn), x(rm)))
        cop, o2, cond = rnd(2), rnd(2), rnd(16)
        add("csel", [sf, cop, o2, rd, rn, rm, cond], "%s %s, %s, %s, %s" % ([["csel", "csinc"], ["csinv", "csneg"]][cop][o2], x(rd, sf), x(rn, sf), x(rm, sf), COND[cond]))
        link, off = rnd(2), signed(26)
        add("branch", [link, off], "%s %s" % (["b", "bl"][link], rel(off)))
        off = signed(19)
        add("branch_cond", [cond, off], "b.%s %s" % (COND[cond], rel(off)))
        nz = rnd(2)
        add("compare_branch", [sf, nz, rd, off], "%s %s, %s" % (["cbz", "cbnz"][nz], x(rd, sf), rel(off)))
        b, off = unsigned(6), signed(14)
        add("bit_branch", [nz, rd, b, off], "%s %s, #%d, %s" % (["tbz", "tbnz"][nz], x(rd, 1 if b > 31 else 0), b, rel(off)))
        op = rnd(3)
        add("branch_reg", [op, rn], "%s %s" % (["br", "blr", "ret"][op], x(rn)))
        imm = unsigned(16)
        add("udf", [imm], "udf #%d" % imm)
        e = rnd(2)
        add("exception", [e, imm], "%s #%d" % (["svc", "brk"][e], imm))
        h = unsigned(7)
        add("hint", [h], "hint #%d" % h)
        bop, crm = 4 + rnd(3), unsigned(4)
        add("barrier", [bop, crm], "%s #%d" % ({4: "dsb", 5: "dmb", 6: "isb"}[bop], crm))
        o0, op1, crn, crm, op2, read = rnd(2), rnd(8), rnd(16), rnd(16), rnd(8), rnd(2)
        name = "S%d_%d_C%d_C%d_%d" % (2 + o0, op1, crn, crm, op2)
        add("system_register", [read, rd, o0 << 14 | op1 << 11 | crn << 7 | crm << 3 | op2], ("mrs %s, %s" % (x(rd), name)) if read else ("msr %s, %s" % (name, x(rd))))
        add("sys", [op1, crn, crm, op2, rd], "sys #%d, C%d, C%d, #%d, %s" % (op1, crn, crm, op2, x(rd)))
        # Loads and stores. An access that also changes its base does not name the base as what is moved.
        v = rnd(2)
        size = 2 + rnd(2) if v else rnd(4)
        opc = rnd(2) if v else rnd(4)
        if not v and (opc >= 2 and size == 3 or opc == 3 and size == 2):
            opc = 1
        def access(size, v, opc):
            if v:
                return ("ldr" if opc else "str"), "ds"[size == 2]
            stem = ["str", "ldr", "ldrs", "ldrs"][opc]
            suffix = ["b", "h", "", ""][size]
            if opc >= 2 and size == 2:
                suffix = "w"
            return stem + suffix, None
        mnemonic, fp = access(size, v, opc)
        def rt_name(rt):
            if v:
                return ("d" if size == 3 else "s") + str(rt)
            return x(rt, 1 if size == 3 or opc == 2 else 0)
        index = unsigned(12)
        add("load_store", [size, v, opc, rd, rn, index], "%s %s, [%s, #%d]" % (mnemonic, rt_name(rd), x(rn, sp=True), index << size))
        mode, off = [0, 1, 3][rnd(3)], signed(9)
        base = rn if mode == 0 or rn != rd or v else (rn + 1) % 31
        um = mnemonic[:2] + "u" + mnemonic[2:] if mode == 0 else mnemonic
        where = ["[%s, #%d]", "[%s], #%d", None, "[%s, #%d]!"][mode] % (x(base, sp=True), off)
        add("load_store_offset", [size, v, opc, mode, rd, base, off], "%s %s, %s" % (um, rt_name(rd), where))
        option, scaled = [2, 3, 6, 7][rnd(4)], rnd(2)
        ext = {2: "uxtw", 3: "lsl", 6: "sxtw", 7: "sxtx"}[option]
        index_reg = x(rm, 1 if option in (3, 7) else 0)
        if scaled:
            where = "[%s, %s, %s #%d]" % (x(rn, sp=True), index_reg, ext, size)
        elif option == 3:
            where = "[%s, %s]" % (x(rn, sp=True), index_reg)
        else:
            where = "[%s, %s, %s]" % (x(rn, sp=True), index_reg, ext)
        add("load_store_reg", [size, v, opc, rd, rn, rm, option, scaled], "%s %s, %s" % (mnemonic, rt_name(rd), where))
        wide, pmode, load, index = rnd(2), 1 + rnd(3), rnd(2), signed(7)
        r1, r2, base = rnd(31), rnd(31), reg()
        if r1 == r2:
            r2 = (r1 + 1) % 31
        if not v and pmode != 2 and base in (r1, r2):
            base = 31
        pair = (lambda r: ("d" if wide else "s") + str(r)) if v else (lambda r: x(r, wide))
        scale = 4 << wide
        where = [None, "[%s], #%d", "[%s, #%d]", "[%s, #%d]!"][pmode] % (x(base, sp=True), index * scale)
        add("load_store_pair", [wide, v, pmode, load, r1, r2, base, index], "%s %s, %s, %s" % (["stp", "ldp"][load], pair(r1), pair(r2), where))
        off = signed(19)
        add("load_literal", [wide, v, rd, off], "ldr %s, %s" % ((("d" if wide else "s") + str(rd)) if v else x(rd, wide), rel(off)))
        esize, load, ordered = rnd(4), rnd(2), rnd(2)
        suffix = ["b", "h", "", ""][esize]
        rt, base, rs = rnd(31), reg(), rnd(31)
        while rs in (rt, base):
            rs = (rs + 1) % 31
        if load:
            add("exclusive", [esize, 1, ordered, 31, rt, base], "%s%s %s, [%s]" % (["ldxr", "ldaxr"][ordered], suffix, x(rt, esize == 3), x(base, sp=True)))
        else:
            add("exclusive", [esize, 0, ordered, rs, rt, base], "%s%s %s, %s, [%s]" % (["stxr", "stlxr"][ordered], suffix, x(rs, 0), x(rt, esize == 3), x(base, sp=True)))
        add("ordered", [esize, load, rt, base], "%s%s %s, [%s]" % (["stlr", "ldar"][load], suffix, x(rt, esize == 3), x(base, sp=True)))
        # Floating point.
        dbl, op = rnd(2), rnd(4)
        add("fp2", [dbl, op, rd, rn, rm], "%s %s, %s, %s" % (["fmul", "fdiv", "fadd", "fsub"][op], f(rd, dbl), f(rn, dbl), f(rm, dbl)))
        op = rnd(5)
        if op == 4:
            add("fp1", [dbl, 4, rd, rn], "fcvt %s, %s" % (f(rd, not dbl), f(rn, dbl)))
        else:
            add("fp1", [dbl, op, rd, rn], "%s %s, %s" % (["fmov", "fabs", "fneg", "fsqrt"][op], f(rd, dbl), f(rn, dbl)))
        if rnd(4) == 0:
            add("fcmp", [dbl, rn, 0, 1], "fcmp %s, #0.0" % f(rn, dbl))
        else:
            add("fcmp", [dbl, rn, rm, 0], "fcmp %s, %s" % (f(rn, dbl), f(rm, dbl)))
        add("fcsel", [dbl, rd, rn, rm, cond], "fcsel %s, %s, %s, %s" % (f(rd, dbl), f(rn, dbl), f(rm, dbl), COND[cond]))
        op = rnd(6)
        if op < 2:
            add("fp_int", [sf, dbl, op, rd, rn], "%s %s, %s" % (["scvtf", "ucvtf"][op], f(rd, dbl), x(rn, sf)))
        elif op < 4:
            add("fp_int", [sf, dbl, op, rd, rn], "%s %s, %s" % (["fcvtzs", "fcvtzu"][op - 2], x(rd, sf), f(rn, dbl)))
        elif op == 4:
            add("fp_int", [dbl, dbl, 4, rd, rn], "fmov %s, %s" % (x(rd, dbl), f(rn, dbl)))
        else:
            add("fp_int", [dbl, dbl, 5, rd, rn], "fmov %s, %s" % (f(rd, dbl), x(rn, dbl)))
    # The forms that are few are written out whole.
    add("eret", [], "eret")
    add("adr", [1, 3, 0], "adrp x3, .")
    for n, name in enumerate(["nop", "yield", "wfe", "wfi"]):
        add("hint", [n], name)
    add("barrier", [6, 15], "isb")
    add("barrier", [4, 7], "dsb nsh")
    add("sys", [0, 8, 7, 0, 31], "tlbi vmalle1")
    add("udf", [0xC11F], "udf #0xc11f")


def registers():
    """The names `Cpu` accepts, from compiler/bare.cho's three lists: (name, readable, writable)."""
    text = open(os.path.join(ROOT, "compiler", "bare.cho")).read()
    lists = re.findall(r'fn (both|read_only|write_only)\(\) -> \[\] &static \[byte\] \{\n    return "([^"]*)";', text)
    assert len(lists) == 3, "compiler/bare.cho no longer has the three lists"
    out = []
    for which, names in lists:
        for name in names.split():
            out.append((name, which != "write_only", which != "read_only"))
    return out


def assemble(lines):
    """The word clang makes of each line of assembly."""
    with tempfile.TemporaryDirectory() as d:
        src, obj, raw = (os.path.join(d, n) for n in ("v.s", "v.o", "v.bin"))
        open(src, "w").write("".join(l + "\n" for l in lines))
        done = subprocess.run([CLANG, "--target=aarch64-none-elf", "-march=armv8-a", "-c", src, "-o", obj], capture_output=True, text=True)
        if done.returncode != 0:
            bad = sorted({int(m.group(1)) for m in re.finditer(r"v\.s:(\d+):\d+: error", done.stderr)})
            sys.exit("clang refused %d lines; the first: %s\n%s" % (len(bad), lines[bad[0] - 1] if bad else "?", done.stderr[:600]))
        subprocess.run([OBJCOPY, "-O", "binary", "--only-section=.text", obj, raw], check=True)
        data = open(raw, "rb").read()
    assert len(data) == 4 * len(lines), "clang made %d words of %d lines" % (len(data) // 4, len(lines))
    return [int.from_bytes(data[4 * i:4 * i + 4], "little") for i in range(len(lines))]


def main():
    generate()
    regs = registers()
    table = {}
    for name, readable, writable in regs:
        if readable:
            add("system_register", [1, 9, -1], "mrs x9, %s" % name)
        if writable:
            add("system_register", [0, 9, -1], "msr %s, x9" % name)
    words = assemble([v[2] for v in vectors])
    lines = []
    for (form, operands, text), word in zip(vectors, words):
        if form == "system_register" and operands[2] < 0:
            # A named register: its fifteen bits are what clang put in the word.
            operands[2] = (word >> 5) & 0x7FFF
            name = text.split()[-1] if text.startswith("mrs") else text.split()[1].rstrip(",")
            assert table.setdefault(name, operands[2]) == operands[2]
        lines.append("%s %s%08x ; %s\n" % (form, "".join("%d " % o for o in operands), word, text))
    vectors_text = "".join(lines)
    names = "".join(" %s=%d" % (name, table[name]) for name, _, _ in regs) + " "
    sys_text = open(SYS).read() if os.path.exists(SYS) else ""
    new_sys = re.sub(r'(fn table\(\) -> \[\] &static \[byte\] \{\n    return ")[^"]*(";)', lambda m: m.group(1) + names + m.group(2), sys_text)
    if "--check" in sys.argv:
        same = os.path.exists(OUT) and open(OUT).read() == vectors_text and new_sys == sys_text
        print("a64 vectors: %d, %s" % (len(lines), "unchanged" if same else "WOULD CHANGE"))
        sys.exit(0 if same else 1)
    open(OUT, "w").write(vectors_text)
    if sys_text:
        open(SYS, "w").write(new_sys)
    forms = sorted({v[0] for v in vectors})
    print("a64 vectors: %d of %d forms, %d system registers" % (len(lines), len(forms), len(regs)))


main()
