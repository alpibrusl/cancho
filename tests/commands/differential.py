"""differential.py -- `conformance/differential.rs` (deleted), for `docs/differential.md`: the constant folder against the compiled program.

    python3 differential.py CANCHO DIRECTORY

Every operator the folder evaluates, on every pair of boundary operands, gives the same answer folded as the compiled program gives at run time -- the same value, or a trap on both
sides. Three arms per case: the literal `(a) op (b)` in a function of its own (folded during lowering; a trap there is a `constant-traps` refusal, and `check --output json` reports every
one, so one run of `check` is the folder's whole trap set); the call `op_fn(a, b)` of a pure function on literal arguments; and the same operator on operands chosen by a loop counter the
compiler cannot see (a trap kills the process, so it reports on standard error, and the run is restarted past the case that killed it). Prints what it found, one line to a section; the
script that runs it compares the lines with tests/commands/differential.expected.
"""
import json
import os
import resource
import subprocess
import sys

cancho, work = sys.argv[1], sys.argv[2]

INTS = ["-9223372036854775808", "-9223372036854775807", "-64", "-63", "-2", "-1", "0", "1", "2", "3", "63", "64", "9223372036854775806", "9223372036854775807"]
FLOATS = ["0.0", "-0.0", "1.0", "-1.0", "0.1", "3.0", "1.0e308", "-1.0e308", "5.0e-324", "2.2250738585072014e-308", "(1.0 / 0.0)", "(-1.0 / 0.0)", "(0.0 / 0.0)", "(-(0.0 / 0.0))"]
BOOLS = ["false", "true"]
NAMES = {"+": "add", "-": "sub", "*": "mul", "/": "div", "%": "rem", "<<": "shl", ">>": "shr", "&": "band", "|": "bor", "^": "bxor", "==": "eq", "!=": "ne", "<": "lt", "<=": "le", ">": "gt", ">=": "ge", "&&": "land", "||": "lor"}
UNARY = {"-": "neg", "~": "bnot", "!": "not"}


class Case:
    def __init__(self, ty, ret, op, a, b):
        self.ty, self.ret, self.op, self.a, self.b = ty, ret, op, a, b

    def expr(self, a, b):
        if self.b is None:
            return f"{self.op[1:]}({a})"
        return f"({a}) {self.op} ({b})"

    def literal(self):
        return self.expr(self.a, self.b or "")

    def helper(self):
        if self.b is None:
            return f"{UNARY[self.op[1:]]}_{self.ty}"
        return f"{NAMES[self.op]}_{self.ty}"


def cases():
    out = []
    comparison = {"==", "!=", "<", "<=", ">", ">=", "&&", "||"}

    def binary(ty, ops, values):
        for op in ops:
            ret = "bool" if op in comparison else ty
            for a in values:
                for b in values:
                    out.append(Case(ty, ret, op, a, b))

    binary("int", ["+", "-", "*", "/", "%", "<<", ">>", "&", "|", "^", "==", "!=", "<", "<=", ">", ">="], INTS)
    binary("float", ["+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">="], FLOATS)
    binary("bool", ["==", "!=", "&&", "||"], BOOLS)
    for ty, op, values in [("int", "u-", INTS), ("int", "u~", INTS), ("float", "u-", FLOATS), ("bool", "u!", BOOLS)]:
        for a in values:
            out.append(Case(ty, ty, op, a, None))
    return out


def shown(ret, e):
    if ret == "int":
        return e
    if ret == "bool":
        return f"b2i({e})"
    return f"bits_of({e})"


def numbered(text):
    pairs = []
    for line in text.splitlines():
        k, v = line.split(" ")
        pairs.append((int(k), int(v)))
    return pairs


def run(args, **kw):
    return subprocess.run([cancho] + args, capture_output=True, text=True, **kw)


SHARED = "import std.io;\nfn b2i(b: bool) -> [] int { if b { return 1; } return 0; }\n"
cs = cases()
print(f"cases: {len(cs)}")

# The folder's trap set: one function per case, on line k + 1.
literal = [f"fn l{k}() -> [] {c.ret} {{ return {c.literal()}; }}" for k, c in enumerate(cs)]
probe = os.path.join(work, "literal.cho")
open(probe, "w").write("\n".join(literal) + "\nfn main(world: World) -> [] int { release(world); return 0; }\n")
checked = run(["check", probe, "--output", "json"])
report = json.loads(checked.stdout)
refused = report.get("refused", [])
others = [r for r in refused if r["rule"] != "constant-traps"]
print(f"literal cases refused for something other than trapping: {len(others)}")
for r in others[:5]:
    print("   ", r["rule"], r["position"]["line"], cs[r["position"]["line"] - 1].literal())
folder_traps = {r["position"]["line"] - 1 for r in refused if r["rule"] == "constant-traps"}
print(f"cases the folder refuses as traps: {len(folder_traps)}")

# The two folded arms, for every case that has a value.
helpers = {}
for c in cs:
    if c.b is None:
        helpers[c.helper()] = f"fn {c.helper()}(a: {c.ty}) -> [] {c.ret} {{ return {c.op[1:]}a; }}"
    else:
        helpers[c.helper()] = f"fn {c.helper()}(a: {c.ty}, b: {c.ty}) -> [] {c.ret} {{ return a {c.op} b; }}"
folded = SHARED
folded += "fn out[&i](i: &!i Io, k: int, v: int) -> [io_write] int {\n    io.print_int(i, k); io.space(i); io.print_int(i, v); io.newline(i); return 0;\n}\n"
folded += "\n".join(helpers.values()) + "\n"
calls = ""
for k, c in enumerate(cs):
    if k in folder_traps:
        continue
    args = f"{c.a}, {c.b}" if c.b is not None else c.a
    folded += literal[k] + "\n"
    folded += f"fn c{k}() -> [] {c.ret} {{ return {c.helper()}({args}); }}\n"
    calls += f"        out(i, {k}, {shown(c.ret, f'l{k}()')}); out(i, {k}, {shown(c.ret, f'c{k}()')});\n"
folded += (
    "fn main(world: World) -> [] int {\n"
    "    let Split { io, ffi, fs, heap, args } = split(world);\n"
    "    release(args); release(heap); release(fs); release(ffi);\n"
    f"    borrow mut io as &!i in {{\n{calls}    }}\n    release(io);\n    return 0;\n}}\n"
)
folded_path = os.path.join(work, "folded.cho")
open(folded_path, "w").write(folded)
valued = len(cs) - len(folder_traps)
authority = run(["authority", folded_path, "--std", "--output", "json"])
if '"folded_calls"' in authority.stdout:
    print("call arms folded:", f'"folded_calls": {valued},' in authority.stdout)
else:
    print("call arms folded: not reported by this compiler")
exe = os.path.join(work, "folded")
built = run(["build", "--std", folded_path, "-o", exe])
if built.returncode != 0:
    print("the folded program does not build:\n" + built.stderr)
    sys.exit(0)
ran = subprocess.run([exe], capture_output=True, text=True)
print(f"the folded program exits {ran.returncode}")
at_compile_time = {}
disagree = []
for k, v in numbered(ran.stdout):
    if k in at_compile_time and at_compile_time[k] != v:
        disagree.append(f"the two halves of the folder disagree on `{cs[k].literal()}` ({cs[k].ty}): lowering says {at_compile_time[k]}, the call arm says {v}")
    at_compile_time[k] = v
print(f"folded values: {len(at_compile_time)} of {valued}")

# Run time: operands the compiler cannot see.
runtime = SHARED
runtime += """fn digits[&o](buf: &!o [byte], at: int, n: int) -> [] int {
    var p = at;
    var m = n;
    if m > 0 { m = -m; }
    if m == 0 { p = p - 1; buf[p] = byte_of('0'); }
    while m != 0 {
        p = p - 1;
        buf[p] = byte_of('0' - m % 10);
        m = m / 10;
    }
    if n < 0 { p = p - 1; buf[p] = byte_of('-'); }
    return p;
}
fn out[&i](i: &!i Io, k: int, v: int) -> [err_write] int {
    region r {
        let buf = alloc_slice[r](48, byte_of(0));
        buf[47] = byte_of('\\n');
        var p = digits(buf, 47, v);
        p = p - 1;
        buf[p] = byte_of(' ');
        p = digits(buf, p, k);
        io.error_all(i, buf[p..48]);
    }
    return 0;
}
fn first[&a](a: &a Args) -> [args] int {
    let s = arg(a, 1);
    var n = 0;
    var p = 0;
    while p < len(s) { n = n * 10 + (int_of(s[p]) - '0'); p = p + 1; }
    return n;
}
"""
for name, ty, values in [("ival", "int", INTS), ("fval", "float", FLOATS), ("bval", "bool", BOOLS)]:
    runtime += f"fn {name}(k: int) -> [] {ty} {{\n"
    for i, v in enumerate(values):
        runtime += f"    if k == {i} {{ return {v}; }}\n"
    runtime += f"    return {values[0]};\n}}\n"
runtime += "fn case(k: int) -> [] int {\n"
start = 0
while start < len(cs):
    c = cs[start]
    count = 0
    while start + count < len(cs) and cs[start + count].ty == c.ty and cs[start + count].op == c.op:
        count += 1
    getter, n = {"int": ("ival", len(INTS)), "float": ("fval", len(FLOATS)), "bool": ("bval", len(BOOLS))}[c.ty]
    if c.b is None:
        e = c.expr(f"{getter}(j)", "")
    else:
        e = c.expr(f"{getter}(j / {n})", f"{getter}(j % {n})")
    runtime += f"    if k < {start + count} {{\n        let j = k - {start};\n        return {shown(c.ret, e)};\n    }}\n"
    start += count
runtime += (
    "    return 0;\n}\n"
    "fn main(world: World) -> [] int {\n"
    "    let Split { io, ffi, fs, heap, args } = split(world);\n"
    "    release(heap); release(fs); release(ffi);\n"
    "    var k = 0;\n"
    "    borrow args as &a in { k = first(a); }\n"
    "    release(args);\n"
    f"    borrow mut io as &!i in {{\n        while k < {len(cs)} {{ out(i, k, case(k)); k = k + 1; }}\n    }}\n"
    "    release(io);\n    return 0;\n}\n"
)
runtime_path = os.path.join(work, "runtime.cho")
open(runtime_path, "w").write(runtime)
exe = os.path.join(work, "runtime")
built = run(["build", "--std", runtime_path, "-o", exe])
if built.returncode != 0:
    print("the run-time program does not build:\n" + built.stderr)
    sys.exit(0)


def without_a_core_dump():
    # A trap is a crash, and a crash may cost a core dump (544 ms each on a runner that pipes it to systemd-coredump): limit 1 is the value the kernel does not pipe. A hard limit of 0 is left alone.
    soft, hard = resource.getrlimit(resource.RLIMIT_CORE)
    if hard != 0:
        resource.setrlimit(resource.RLIMIT_CORE, (1, 1))


at_run_time = {}
runtime_traps = set()
unpredicted = []
nxt = 0
while nxt < len(cs):
    p = subprocess.run([exe, str(nxt)], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, timeout=60, preexec_fn=without_a_core_dump)
    reached = nxt
    for k, v in numbered(p.stderr):
        at_run_time[k] = v
        reached = k + 1
    if p.returncode == 0:
        if reached != len(cs):
            print(f"a clean exit must have run every case: it reached {reached}")
        break
    if p.returncode > 0:
        print(f"case {reached} ended with the exit status {p.returncode}, not a signal")
        break
    runtime_traps.add(reached)
    if reached not in folder_traps:
        unpredicted.append(f"`{cs[reached].literal()}` ({cs[reached].ty})")
        if len(unpredicted) > 20:
            break
    nxt = reached + 1
print(f"cases that trap at run time: {len(runtime_traps)}")

# The comparison.
for k, c in enumerate(cs):
    what = f"`{c.literal()}` ({c.ty})"
    folder, run_time = k in folder_traps, k in runtime_traps
    if folder and not run_time:
        disagree.append(f"{what}: the folder refuses it as a trap, the program answers {at_run_time.get(k)}")
    elif run_time and not folder:
        disagree.append(f"{what}: the folder answers {at_compile_time.get(k)}, the program traps")
    elif not folder and at_compile_time.get(k) != at_run_time.get(k):
        disagree.append(f"{what}: folded {at_compile_time.get(k)}, at run time {at_run_time.get(k)}")
print(f"cases that disagree: {len(disagree)}")
for line in sorted(disagree)[:40]:
    print("   ", line)
