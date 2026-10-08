#!/usr/bin/env python3
"""Mutation check of `conn_peer`, `std.addr` and `conns.peer` (docs/conn-peer.md §10), the shape of `scripts/tls_server_mutants.py`.

    python3 scripts/conn_peer_mutants.py [--only <text in a mutant's name>]

Each mutant is one deliberate bug at one site in `std/addr.cho`, `std/conns.cho`, or the two backends' `conn_peer`
(`crates/cancho-codegen/src/body/sockets.rs`, `crates/cancho-codegen-llvm/src/body/sockets.rs`). The file is changed in
place, `cargo test -p cancho --test conformance conn_peer` runs (it rebuilds the compiler, which embeds `std`, and builds
real programs with both backends), and the file is put back, whatever happens. A mutant is killed when that test run fails.
The unmutated tree is run first and must pass. A mutant that changes nothing any test can reach is in EQUIVALENT with the
argument, and must survive. Exit status 1 if a mutant survives, an `old` text does not occur exactly once, or a mutant
fails to compile (a mutant that does not build proves nothing: the test run says so in its output, and it counts as an
error here, not a kill).

Run it from a clean checkout of the change: it writes to the source files and restores them from a copy it holds in memory.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ADDR = "std/addr.cho"
CONNS = "std/conns.cho"
CRANE = "crates/cancho-codegen/src/body/sockets.rs"
LLVM = "crates/cancho-codegen-llvm/src/body/sockets.rs"

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    # ---- std.addr: the value ----
    ("decode: the port's bytes swapped", ADDR, "let port = int_of(raw[17]) << 8 | int_of(raw[18]);", "let port = int_of(raw[18]) << 8 | int_of(raw[17]);"),
    ("decode: a family of 5 accepted", ADDR, "if f != 4 && f != 6 {\n        return Parsed::Bad;", "if f != 4 && f != 5 && f != 6 {\n        return Parsed::Bad;"),
    ("decode: a short buffer accepted", ADDR, "if len(raw) < 19 {\n        return Parsed::Bad;", "if len(raw) < 18 {\n        return Parsed::Bad;"),
    ("decode: the IPv4 address read from the wrong byte", ADDR, "let w = int_of(raw[1]) << 24 | int_of(raw[2]) << 16 | int_of(raw[3]) << 8 | int_of(raw[4]);", "let w = int_of(raw[2]) << 24 | int_of(raw[3]) << 16 | int_of(raw[4]) << 8 | int_of(raw[5]);"),
    ("v6: the mapped range not normalised", ADDR, "    if mapped(a, b, c) {\n        return Peer { family: 4,", "    if false && mapped(a, b, c) {\n        return Peer { family: 4,"),
    ("mapped: the second word not looked at", ADDR, "return w0 == 0 && w1 == 0 && w2 == 0xffff;", "return w0 == 0 && w2 == 0xffff;"),
    ("mapped: ::fffe: taken for it too", ADDR, "return w0 == 0 && w1 == 0 && w2 == 0xffff;", "return w0 == 0 && w1 == 0 && (w2 == 0xffff || w2 == 0xfffe);"),
    ("from_parts: an IPv4 address built as IPv6", ADDR, "    if family == 4 {\n        return v4(w3 >> 24, w3 >> 16, w3 >> 8, w3, port);\n    }\n    return v6(", "    if family == 7 {\n        return v4(w3 >> 24, w3 >> 16, w3 >> 8, w3, port);\n    }\n    return v6("),
    ("v4: the octets not masked", ADDR, "let w = (a & 255) << 24 | (b & 255) << 16 | (c & 255) << 8 | d & 255;", "let w = a << 24 | b << 16 | c << 8 | d;"),
    ("v4: the port not masked", ADDR, "return Peer { family: 4, w0: 0, w1: 0, w2: 0, w3: w, port: port & 0xffff };", "return Peer { family: 4, w0: 0, w1: 0, w2: 0, w3: w, port: port };"),
    ("same: the port ignored", ADDR, "return same_address(a, b) && a.port == b.port;", "return same_address(a, b);"),
    ("same_address: the family ignored", ADDR, "return a.family == b.family && a.w0 == b.w0", "return a.w0 == b.w0"),
    # ---- std.addr: the key ----
    ("key: an IPv6 address keyed by its /32", ADDR, "bits: p.w0 << 32 | p.w1 }", "bits: p.w0 << 32 }"),
    ("key: an IPv6 address keyed whole", ADDR, "bits: p.w0 << 32 | p.w1 }", "bits: p.w0 << 32 | p.w3 }"),
    ("key: an IPv4 key in the IPv6 family", ADDR, "return Key { family: 4, bits: p.w3 };", "return Key { family: 6, bits: p.w3 };"),
    ("same_key: the family ignored", ADDR, "return a.family == b.family && a.bits == b.bits;", "return a.bits == b.bits;"),
    # ---- std.addr: text ----
    ("text: upper-case hex", ADDR, "return 'a' + (d - 10);", "return 'A' + (d - 10);"),
    ("text: the tie goes to the last run", ADDR, "if j - i > best_len {", "if j - i >= best_len {"),
    ("text: a lone zero group compressed", ADDR, "if best_len < 2 {\n        return put_groups", "if best_len < 1 {\n        return put_groups"),
    ("text: the shortest run is compressed", ADDR, "if j - i > best_len {\n                best = i;", "if best_len == 0 || j - i < best_len {\n                best = i;"),
    ("text: a zero group written empty", ADDR, "if d != 0 || started || shift == 0 {\n            p = put(out, p, hex_char(d));", "if d != 0 || started {\n            p = put(out, p, hex_char(d));"),
    ("text: leading zeros kept", ADDR, "if d != 0 || started || shift == 0 {", "if true {"),
    ("text: a short buffer accepted", ADDR, "pub fn text[&o](p: Peer, out: &!o [byte]) -> [] int {\n    if len(out) < 47 {", "pub fn text[&o](p: Peer, out: &!o [byte]) -> [] int {\n    if len(out) < 39 {"),
    ("text_port: no brackets", ADDR, "    if p.family == 6 {\n        q = put(out, q, '[');\n    }", ""),
    ("text_port: the port left out", ADDR, "    q = put(out, q, ':');\n    return put_dec(out, q, p.port);", "    return q;"),
    ("put_dec: a port of 0 left empty", ADDR, "if d != 0 || started || div == 1 {", "if d != 0 || started {"),
    # ---- std.addr: parse ----
    ("parse: a leading zero in an octet accepted", ADDR, "        if digits > 1 && int_of(s[first]) == '0' {\n            return 0 - 1;\n        }\n", ""),
    ("parse: an octet over 255 accepted", ADDR, "if digits == 0 || digits > 3 || n > 255 {", "if digits == 0 || digits > 3 || n > 999 {"),
    ("parse: a fifth digit accepted", ADDR, "if end == i || end - i > 4 || count > 7 {", "if end == i || end - i > 5 || count > 7 {"),
    ("parse: a second `::` accepted", ADDR, "                    if gap >= 0 {\n                        return Parsed::Bad;\n                    }\n                    gap = count;", "                    gap = count;"),
    ("parse: too few groups without `::` accepted", ADDR, "if gap < 0 && count != 8 {", "if gap < 0 && count > 8 {"),
    ("parse: a trailing single colon accepted", ADDR, "                    if end + 1 == len(s) {\n                        return Parsed::Bad;\n                    }\n                    i = end + 1;", "                    i = end + 1;\n                    if i == len(s) {\n                        more = false;\n                    }"),
    ("parse: a dotted quad that is not last accepted", ADDR, "if end != len(s) || count > 6 {", "if count > 6 {"),
    ("parse: groups after `::` put at the front", ADDR, "            if gap >= 0 && a >= gap {\n                at = a + 8 - count;", "            if gap >= 0 && a >= gap + 1 {\n                at = a + 8 - count;"),
    # ---- std.conns ----
    ("peer: a slot with nothing in it has a peer", CONNS, "    if ticket < 0 {\n        return Peered::Unavailable(9);\n    }\n    match conn_attach(ticket) {\n        Attached::Ok(c) => {\n            var conn = c;\n            var answer = Peered::Unavailable(9);", "    if ticket < 0 {\n        return Peered::Unavailable(0);\n    }\n    match conn_attach(ticket) {\n        Attached::Ok(c) => {\n            var conn = c;\n            var answer = Peered::Unavailable(9);"),
    ("peer: the connection not put back", CONNS, "                    answer = Peered::Unavailable(code);\n                }\n            }\n            let back = conn_detach(conn);\n            if back < 0 {\n                release_slot(table, slot);\n            } else {\n                vec.set(table.tickets, slot, back);\n            }", "                    answer = Peered::Unavailable(code);\n                }\n            }\n            let back = conn_detach(conn);\n            if back < 0 {\n                release_slot(table, slot);\n            }"),
    ("peer: the errno swallowed", CONNS, "                    answer = Peered::Unavailable(code);", "                    answer = Peered::Unavailable(22);"),
    ("peer: a buffer of 18 bytes", CONNS, "let raw = alloc_slice[r](19, byte_of(0));", "let raw = alloc_slice[r](18, byte_of(0));"),
    # ---- the builtin, Cranelift ----
    ("cranelift: the port read from the wrong bytes", CRANE, "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 2 + i);", "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 3 + i);"),
    ("cranelift: the IPv4 address read from the wrong bytes", CRANE, "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 4 + i)", "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 5 + i)"),
    ("cranelift: the IPv6 address read from the wrong bytes", CRANE, "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 8 + i);", "self.builder.ins().load(types::I8, MemFlags::trusted(), sa, 4 + i);"),
    ("cranelift: the family read from the other kernel's byte", CRANE, "let family_at = if darwin { 1 } else { 0 };", "let family_at = if darwin { 0 } else { 1 };"),
    ("cranelift: AF_INET6 is the other kernel's number", CRANE, "let inet6 = if darwin { 30 } else { 10 };", "let inet6 = if darwin { 10 } else { 30 };"),
    ("cranelift: a buffer of 18 bytes accepted", CRANE, "let small = self.builder.ins().icmp_imm(IntCC::SignedLessThan, room, 19);", "let small = self.builder.ins().icmp_imm(IntCC::SignedLessThan, room, 18);"),
    ("cranelift: the family byte left unwritten for IPv4", CRANE, "        let four = self.builder.ins().iconst(types::I8, 4);\n        self.builder.ins().store(MemFlags::trusted(), four, out, 0);", "        let four = self.builder.ins().iconst(types::I8, 4);\n        self.builder.ins().store(MemFlags::trusted(), four, out, 18);"),
    ("cranelift: the failure's errno not answered", CRANE, "self.builder.ins().brif(failed, merge, &[reason.into()], classify, &[]);", "self.builder.ins().brif(failed, merge, &[einval.into()], classify, &[]);"),
    # ---- the builtin, LLVM ----
    ("llvm: the port read from the wrong bytes", LLVM, "            let byte = self.load_field(&sa, 2 + i, \"i8\");\n            self.store_byte(&out, 17 + i, &byte);", "            let byte = self.load_field(&sa, 3 + i, \"i8\");\n            self.store_byte(&out, 17 + i, &byte);"),
    ("llvm: the IPv4 address read from the wrong bytes", LLVM, "let byte = self.load_field(&sa, 4 + i, \"i8\");", "let byte = self.load_field(&sa, 5 + i, \"i8\");"),
    ("llvm: the IPv6 address read from the wrong bytes", LLVM, "let byte = self.load_field(&sa, 8 + i, \"i8\");", "let byte = self.load_field(&sa, 4 + i, \"i8\");"),
    ("llvm: the family read from the other kernel's byte", LLVM, "let family_at = if darwin { 1 } else { 0 };", "let family_at = if darwin { 0 } else { 1 };"),
    ("llvm: AF_INET6 is the other kernel's number", LLVM, "let inet6 = if darwin { 30 } else { 10 };", "let inet6 = if darwin { 10 } else { 30 };"),
    ("llvm: a buffer of 18 bytes accepted", LLVM, "icmp slt i64 {}, 19\\n", "icmp slt i64 {}, 18\\n"),
    ("llvm: the IPv4 tail not zeroed", LLVM, "                self.store_byte(&out, 1 + i, \"0\");", "                self.store_byte(&out, 1 + i, \"1\");"),
    ("llvm: the failure's errno not answered", LLVM, "self.out.push_str(&format!(\"  store i64 {}, ptr {cell}\\n\", operand(&reason)));", "self.out.push_str(&format!(\"  store i64 {EINVAL}, ptr {cell}\\n\"));"),
]

# Mutants that cannot change anything a test can see, with the argument. They must survive.
EQUIVALENT = [
    ("parse: a bare colon at the start accepted", ADDR, "        } else if len(s) > 0 && int_of(s[0]) == ':' {\n            return Parsed::Bad;\n        }", "        }",
     "the first token would then be empty (`end == i`) and refused a few lines later; the check says it in the place a reader looks, and removing it changes no answer"),
]

# Mutants that only a Linux kernel can kill: on macOS a reset connection's `getpeername` fails with
# `EINVAL` (22), which is also the number these mutants answer instead of the real errno, so the
# tests there cannot tell them from the original. On Linux the expected errno is `ENOTCONN` (107).
LINUX_ONLY = {
    "peer: the errno swallowed",
    "cranelift: the failure's errno not answered",
    "llvm: the failure's errno not answered",
}


def read(path):
    with open(os.path.join(ROOT, path)) as f:
        return f.read()


def write(path, text):
    with open(os.path.join(ROOT, path), "w") as f:
        f.write(text)


def run_tests():
    p = subprocess.run(["cargo", "test", "-p", "cancho", "--test", "conformance", "conn_peer"], cwd=ROOT,
                       capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr


def main():
    only = None
    if "--only" in sys.argv:
        only = sys.argv[sys.argv.index("--only") + 1]
    code, out = run_tests()
    if code != 0:
        print("the unmutated tree does not pass:\n" + out[-3000:])
        return 2
    originals = {path: read(path) for path in {m[1] for m in MUTANTS + EQUIVALENT}}
    survived, errors, killed, linux_only = [], [], 0, []
    equivalent_ok = True
    try:
        for expect_kill, group in ((True, MUTANTS), (False, EQUIVALENT)):
            for entry in group:
                name, path, old, new = entry[:4]
                if only and only not in name:
                    continue
                text = originals[path]
                if text.count(old) != 1:
                    errors.append(f"{name}: `old` occurs {text.count(old)} times in {path}")
                    print(f"error      {name}: `old` occurs {text.count(old)} times")
                    continue
                write(path, text.replace(old, new, 1))
                try:
                    code, out = run_tests()
                finally:
                    write(path, text)
                built = "could not compile" not in out and "error[E" not in out and "error: could not" not in out
                if not built:
                    errors.append(f"{name}: does not build")
                    print(f"error      {name}: the mutant does not build\n{out[-1500:]}")
                elif expect_kill and code != 0:
                    killed += 1
                    print(f"killed     {name}", flush=True)
                elif expect_kill and name in LINUX_ONLY and sys.platform != "linux":
                    linux_only.append(name)
                    print(f"linux-only {name}: survives here, killed on Linux (see LINUX_ONLY)", flush=True)
                elif expect_kill:
                    survived.append(name)
                    print(f"SURVIVED   {name}", flush=True)
                elif code == 0:
                    print(f"equivalent {name}: {entry[4]}", flush=True)
                else:
                    equivalent_ok = False
                    print(f"NOT EQUIVALENT {name}: a test fails, so move it to MUTANTS", flush=True)
    finally:
        for path, text in originals.items():
            write(path, text)
    total = len([m for m in MUTANTS if not only or only in m[0]])
    print(f"{killed} of {total} mutants killed; {len(linux_only)} killed only on Linux; {len(survived)} survived; {len(errors)} errors")
    return 1 if survived or errors or not equivalent_ok else 0


if __name__ == "__main__":
    sys.exit(main())
