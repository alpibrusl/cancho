#!/usr/bin/env python3
"""regalloc_random.py -- random functions for the differential test of the register allocator (docs/native-backend.md section 17.9).

    tests/native/regalloc_random.py SEED FUNCTIONS OUT.ll

Writes a module of LLVM text with FUNCTIONS functions made from the seed (the same seed, the same module) and a `main` that calls each with three sets
of arguments and prints each result. A third of the functions are straight-line code (many values, some live to the end, so that there are more than
registers), a third loops in SSA form (`phi`s that swap and rotate, a diamond inside, values live across the edge back), and a third are written as the
front end writes (every variable an `alloca`, loops, a `switch`, a diamond), which `mem2reg` makes values first. Some call functions of the module that
take ten arguments or a `double`. Every operation is defined for every input: shifts are by less than the width, a divisor is at least one, and the
floating-point values stay finite (no `NaN`, whose sign two builds may print apart). tests/native/regalloc_random.sh builds each module three ways and
compares what they print."""
import random
import struct
import sys

INTS = ["i64", "i32", "i16", "i8"]
FPS = ["double", "float"]
BITS = {"i64": 64, "i32": 32, "i16": 16, "i8": 8, "i1": 1}


def fp_const(x):
    """LLVM's spelling of a floating-point constant: the sixteen hexadecimal digits of the double (exact for a float here)."""
    return "0x%016X" % struct.unpack("<Q", struct.pack("<d", x))[0]


def const(r, t):
    if t == "i1":
        return r.choice(["true", "false"])
    if t in FPS:
        return fp_const(r.choice([0.0, 0.5, 1.0, 1.5, 2.0, -3.0, 7.25, 100.0]))
    b = BITS[t]
    v = r.choice([0, 1, 2, 3, 7, 100, -1, -2, (1 << (b - 1)) - 1, -(1 << (b - 1)), r.randrange(-(1 << (b - 1)), 1 << (b - 1))])
    return str(v)


class Fn:
    def __init__(self, r, name):
        self.r = r
        self.name = name
        self.lines = []
        self.n = 0
        self.pool = []  # (name, type) usable here

    def new(self):
        self.n += 1
        return "%%v%d" % self.n

    def emit(self, s):
        self.lines.append("  " + s)

    def label(self, s):
        self.lines.append(s + ":")

    def pick(self, t, avoid_const=False):
        cands = [v for v, ty in self.pool if ty == t]
        # Mostly the newest, so that values die; sometimes an old one, so that some live long.
        if cands and (avoid_const or self.r.random() < 0.85):
            if self.r.random() < 0.6:
                return cands[-1 - self.r.randrange(min(4, len(cands)))]
            return self.r.choice(cands)
        if avoid_const:
            return None
        return const(self.r, t)

    def value(self, t, fp_safe=False):
        """One instruction (or a few) whose result has type `t`; it is added to the pool and its name answered."""
        r = self.r
        d = self.new()
        if t == "i1":
            k = r.random()
            if k < 0.6:
                ty = r.choice(INTS)
                pred = r.choice(["eq", "ne", "ugt", "uge", "ult", "ule", "sgt", "sge", "slt", "sle"])
                self.emit("%s = icmp %s %s %s, %s" % (d, pred, ty, self.pick(ty), self.pick(ty)))
            elif k < 0.8:
                ty = r.choice(FPS)
                pred = r.choice(["oeq", "one", "ogt", "oge", "olt", "ole", "ord", "ueq", "une", "ult"])
                self.emit("%s = fcmp %s %s %s, %s" % (d, pred, ty, self.pick(ty), self.pick(ty)))
            else:
                self.emit("%s = %s i1 %s, %s" % (d, r.choice(["and", "or", "xor"]), self.pick("i1"), self.pick("i1")))
        elif t in FPS:
            k = r.random()
            if fp_safe or k < 0.35:
                op = r.choice(["fadd", "fsub"])
                self.emit("%s = %s %s %s, %s" % (d, op, t, self.pick(t), fp_const(r.choice([0.5, 1.0, 2.0, -1.5]))))
            elif k < 0.55:
                self.emit("%s = %s %s %s, %s" % (d, r.choice(["fadd", "fsub"]), t, self.pick(t), self.pick(t)))
            elif k < 0.65:
                self.emit("%s = fmul %s %s, %s" % (d, t, self.pick(t), fp_const(r.choice([0.5, 1.5, -1.0, 0.25]))))
            elif k < 0.75:
                self.emit("%s = fneg %s %s" % (d, t, self.pick(t)))
            elif k < 0.85:
                ty = r.choice(["i64", "i32", "i8"])
                self.emit("%s = sitofp %s %s to %s" % (d, ty, self.pick(ty), t))
            elif k < 0.92:
                other = "float" if t == "double" else "double"
                src = self.pick(other, avoid_const=True)
                if src is None or t == "float":
                    # A double made a float could be too large for it; a float made a double never is.
                    self.emit("%s = fadd %s %s, %s" % (d, t, self.pick(t), fp_const(0.5)))
                else:
                    self.emit("%s = fpext float %s to double" % (d, src))
            else:
                c = self.pick("i1")
                self.emit("%s = select i1 %s, %s %s, %s %s" % (d, c, t, self.pick(t), t, self.pick(t)))
        else:
            k = r.random()
            b = BITS[t]
            if k < 0.45:
                op = r.choice(["add", "sub", "mul", "and", "or", "xor", "add", "sub"])
                self.emit("%s = %s %s %s, %s" % (d, op, t, self.pick(t), self.pick(t)))
            elif k < 0.55:
                op = r.choice(["shl", "lshr", "ashr"])
                if r.random() < 0.5:
                    amount = str(r.randrange(b))
                else:
                    amount = self.new()
                    self.emit("%s = and %s %s, %d" % (amount, t, self.pick(t), b - 1))
                self.emit("%s = %s %s %s, %s" % (d, op, t, self.pick(t), amount))
            elif k < 0.65:
                op = r.choice(["udiv", "sdiv", "srem"])
                m = self.new()
                self.emit("%s = and %s %s, %d" % (m, t, self.pick(t), min(127, (1 << (b - 1)) - 1)))
                dv = self.new()
                self.emit("%s = or %s %s, 1" % (dv, t, m))
                self.emit("%s = %s %s %s, %s" % (d, op, t, self.pick(t), dv))
            elif k < 0.8:
                ty = r.choice(INTS + ["i1"])
                if ty == t:
                    ty = "i1"
                if BITS[ty] > b:
                    self.emit("%s = trunc %s %s to %s" % (d, ty, self.pick(ty), t))
                else:
                    self.emit("%s = %s %s %s to %s" % (d, r.choice(["zext", "sext"]), ty, self.pick(ty), t))
            elif k < 0.9:
                c = self.pick("i1")
                self.emit("%s = select i1 %s, %s %s, %s %s" % (d, c, t, self.pick(t), t, self.pick(t)))
            elif k < 0.95 and t == "i64":
                if r.random() < 0.5:
                    args = ", ".join("i64 %s" % self.pick("i64") for _ in range(10))
                    self.emit("%s = call i64 @wide(%s)" % (d, args))
                else:
                    x = self.new()
                    self.emit("%s = call double @half(double %s, i64 %s)" % (x, self.pick("double"), self.pick("i64")))
                    self.pool.append((x, "double"))
                    self.emit("%s = bitcast double %s to i64" % (d, x))
            else:
                self.emit("%s = %s %s %s, %s" % (d, r.choice(["add", "xor"]), t, self.pick(t), const(r, t)))
        self.pool.append((d, t))
        return d

    def some(self, count, fp_safe=False):
        for _ in range(count):
            self.value(self.r.choice(["i64", "i64", "i64", "i32", "i8", "i16", "i1", "double", "double", "float"]), fp_safe)

    def fold(self, values, into="%h"):
        """The values folded into one i64, `into`: an i64 is multiplied and added, anything else made an i64 first (a double by its bits)."""
        acc = "0"
        for v, t in values:
            x = v
            if t == "i1" or t in INTS and t != "i64":
                x = self.new()
                self.emit("%s = zext %s %s to i64" % (x, t, v))
            elif t == "double":
                x = self.new()
                self.emit("%s = bitcast double %s to i64" % (x, v))
            elif t == "float":
                y = self.new()
                self.emit("%s = bitcast float %s to i32" % (y, v))
                x = self.new()
                self.emit("%s = zext i32 %s to i64" % (x, y))
            m = self.new()
            self.emit("%s = mul i64 %s, 31" % (m, acc))
            a = self.new()
            self.emit("%s = add i64 %s, %s" % (a, m, x))
            acc = a
        self.emit("%s = add i64 %s, 0" % (into, acc))
        return into

    def head(self):
        self.lines.append("define i64 @%s(i64 %%p0, i64 %%p1, i32 %%p2, double %%p3, i8 %%p4, i1 %%p5) {" % self.name)
        self.label("entry")
        self.pool = [("%p0", "i64"), ("%p1", "i64"), ("%p2", "i32"), ("%p3", "double"), ("%p4", "i8"), ("%p5", "i1")]
        f = self.new()
        self.emit("%s = fptrunc double %s to float" % (f, fp_const(1.25)))
        self.pool.append((f, "float"))

    def text(self):
        return "\n".join(self.lines) + "\n}\n"


def straight(r, name):
    f = Fn(r, name)
    f.head()
    f.some(r.randrange(15, 70))
    keep = [v for v in f.pool if r.random() < 0.3] or f.pool[-1:]
    f.fold(keep)
    f.emit("ret i64 %h")
    return f.text()


def ssa_loop(r, name):
    f = Fn(r, name)
    f.head()
    f.some(r.randrange(3, 15))
    before = list(f.pool)
    f.emit("%n = and i64 %p0, 15")
    f.emit("br label %head")
    f.label("head")
    k = r.randrange(2, 7)
    types = [r.choice(["i64", "i64", "i32", "i8", "double", "i1"]) for _ in range(k)]
    phis = ["%%a%d" % j for j in range(k)]
    inits = []
    for j in range(k):
        cands = [v for v, t in before if t == types[j]]
        inits.append(r.choice(cands) if cands and r.random() < 0.7 else const(r, types[j]))
    lines_at = len(f.lines)
    f.pool = list(before) + list(zip(phis, types)) + [("%i", "i64")]
    f.emit("%c = icmp slt i64 %i, %n")
    f.emit("br i1 %c, label %body, label %exit")
    f.label("body")
    f.some(r.randrange(2, 12), fp_safe=True)
    latch = "body"
    if r.random() < 0.5:
        # A diamond in the body, and a phi where its arms meet.
        cond = f.pick("i1")
        f.emit("br i1 %s, label %%yes, label %%no" % cond)
        saved = list(f.pool)
        f.label("yes")
        ya = f.value("i64")
        f.emit("br label %join")
        f.pool = list(saved)
        f.label("no")
        na = f.value("i64")
        f.emit("br label %join")
        f.pool = list(saved)
        f.label("join")
        f.emit("%%m = phi i64 [ %s, %%yes ], [ %s, %%no ]" % (ya, na))
        f.pool.append(("%m", "i64"))
        latch = "join"
    nexts = []
    for j in range(k):
        same = [p for p, t in zip(phis, types) if t == types[j] and p != phis[j]]
        if same and r.random() < 0.4:
            nexts.append(r.choice(same))  # a swap or a rotation of the phis
        elif types[j] == "double":
            nexts.append(f.value("double", fp_safe=True))
        else:
            nexts.append(f.pick(types[j]))
    f.emit("%i.next = add i64 %i, 1")
    f.emit("br label %head")
    head = ["  %%i = phi i64 [ 0, %%entry ], [ %%i.next, %%%s ]" % latch]
    for j in range(k):
        head.append("  %s = phi %s [ %s, %%entry ], [ %s, %%%s ]" % (phis[j], types[j], inits[j], nexts[j], latch))
    f.lines[lines_at:lines_at] = head
    f.label("exit")
    f.pool = list(before) + list(zip(phis, types)) + [("%i", "i64")]
    f.some(r.randrange(0, 6))
    keep = list(zip(phis, types)) + [v for v in before if r.random() < 0.3]
    f.fold(keep)
    f.emit("ret i64 %h")
    return f.text()


def variables(r, name):
    """The front end's way: each variable an alloca, read and written with load and store."""
    f = Fn(r, name)
    f.head()
    k = r.randrange(2, 7)
    types = [r.choice(["i64", "i64", "i32", "i8", "double", "i1"]) for _ in range(k)]
    vs = ["%%x%d" % j for j in range(k)]
    for j in range(k):
        f.emit("%s = alloca %s" % (vs[j], types[j]))
    f.emit("%i = alloca i64")
    f.emit("store i64 0, ptr %i")
    for j in range(k):
        f.emit("store %s %s, ptr %s" % (types[j], f.pick(types[j]), vs[j]))
    base = list(f.pool)
    f.emit("%n = and i64 %p1, 7")
    f.emit("br label %head")
    f.label("head")
    f.pool = list(base)
    f.emit("%iv = load i64, ptr %i")
    f.emit("%c = icmp slt i64 %iv, %n")
    f.emit("br i1 %c, label %body, label %exit")
    f.label("body")
    f.pool = list(base) + [("%iv", "i64")]

    def load_some():
        for j in range(k):
            if r.random() < 0.7:
                x = f.new()
                f.emit("%s = load %s, ptr %s" % (x, types[j], vs[j]))
                f.pool.append((x, types[j]))

    def store_some():
        for j in range(k):
            if r.random() < 0.5:
                f.emit("store %s %s, ptr %s" % (types[j], f.pick(types[j]), vs[j]))

    load_some()
    f.some(r.randrange(1, 8), fp_safe=True)
    store_some()
    shape = r.random()
    if shape < 0.4:
        f.emit("%%s = and i64 %s, 3" % "%iv")
        f.emit("switch i64 %s, label %%d [ i64 0, label %%c0 i64 1, label %%c1 i64 2, label %%c2 ]" % "%s")
        for lab in ["c0", "c1", "c2", "d"]:
            f.label(lab)
            f.pool = list(base) + [("%iv", "i64")]
            load_some()
            f.some(r.randrange(0, 3), fp_safe=True)
            store_some()
            f.emit("br label %latch")
    else:
        cond = f.pick("i1")
        f.emit("br i1 %s, label %%yes, label %%latch" % cond)
        f.label("yes")
        f.pool = list(base) + [("%iv", "i64")]
        load_some()
        f.some(r.randrange(0, 4), fp_safe=True)
        store_some()
        f.emit("br label %latch")
    f.label("latch")
    f.pool = list(base) + [("%iv", "i64")]
    f.emit("%i2 = add i64 %iv, 1")
    f.emit("store i64 %i2, ptr %i")
    f.emit("br label %head")
    f.label("exit")
    f.pool = list(base)
    got = []
    for j in range(k):
        x = f.new()
        f.emit("%s = load %s, ptr %s" % (x, types[j], vs[j]))
        got.append((x, types[j]))
    f.fold(got)
    f.emit("ret i64 %h")
    return f.text()


HELPERS = """define i64 @wide(i64 %a, i64 %b, i64 %c, i64 %d, i64 %e, i64 %f, i64 %g, i64 %h, i64 %i, i64 %j) {
entry:
  %1 = mul i64 %a, 3
  %2 = add i64 %1, %b
  %3 = xor i64 %2, %c
  %4 = sub i64 %3, %d
  %5 = add i64 %4, %e
  %6 = mul i64 %5, 5
  %7 = add i64 %6, %f
  %8 = xor i64 %7, %g
  %9 = add i64 %8, %h
  %10 = sub i64 %9, %i
  %11 = add i64 %10, %j
  ret i64 %11
}
define double @half(double %x, i64 %k) {
entry:
  %y = fmul double %x, 0x3FE0000000000000
  %m = and i64 %k, 7
  %z = sitofp i64 %m to double
  %r = fadd double %y, %z
  ret double %r
}
"""


def main():
    seed = int(sys.argv[1])
    count = int(sys.argv[2])
    r = random.Random(seed)
    parts = ['@fmt = private constant [5 x i8] c"%ld\\0A\\00"', "declare i32 @printf(ptr, ...)", HELPERS.rstrip("\n")]
    names = []
    for k in range(count):
        name = "f%d" % k
        names.append(name)
        parts.append([straight, ssa_loop, variables][k % 3](r, name).rstrip("\n"))
    calls = ["define i32 @main(i32 %argc, ptr %argv) {", "entry:"]
    n = 0
    for name in names:
        for _ in range(3):
            n += 1
            args = "i64 %d, i64 %d, i32 %d, double %s, i8 %d, i1 %s" % (
                r.randrange(-50, 50), r.randrange(-(1 << 40), 1 << 40), r.randrange(-1000, 1000),
                fp_const(r.choice([0.5, -2.0, 3.75, 10.0])), r.randrange(-128, 128), r.choice(["true", "false"]))
            calls.append("  %%r%d = call i64 @%s(%s)" % (n, name, args))
            calls.append("  %%q%d = call i32 (ptr, ...) @printf(ptr @fmt, i64 %%r%d)" % (n, n))
    calls.append("  ret i32 0")
    calls.append("}")
    parts.append("\n".join(calls))
    open(sys.argv[3], "w").write("\n".join(parts) + "\n")


if __name__ == "__main__":
    main()
