#!/usr/bin/env python3
"""Writes par1.cho, par2.cho and par4.cho: P threads, each summing its own 128 KB boxed slice 12,207 times (the
cache-resident `reduce` of benches/, wrapping adds so LLVM vectorises it), each given its own struct by `&!`
reference (`tests/accept/spawn_struct_ref.cho`). Total work is P times one thread's, so perfect scaling keeps the
wall time constant.

    python3 benches/parallel/gen_threads.py /tmp/par
    for p in 1 2 4; do cancho build --std --backend llvm /tmp/par/par$p.cho -o /tmp/par/par$p; done
    for p in 1 2 4; do (time taskset -c 0-3 /tmp/par/par$p); done
"""
import os
import sys

ROUNDS, N = 12207, 16384


def program(p):
    s = """edition 5;

res struct Worker {
    data: Box[[int]],
    out: int,
}

fn work[&r](w: &!r Worker) -> [] int {
    var total = 0;
    var r = 0;
    while r < %d {
        var i = 0;
        while i < len(contents(w.data)) {
            total = wrapping_add(total, contents(w.data)[i]);
            i = wrapping_add(i, 1);
        }
        r = wrapping_add(r, 1);
    }
    w.out = total;
    return 0;
}

fn make[&h](heap: &!h Heap) -> [heap] Worker {
    let held = box_slice(heap, %d, 0);
    borrow mut held as &!b in {
        var i = 0;
        while i < %d {
            contents(b)[i] = i %% 3 - 1;
            i = i + 1;
        }
    }
    return Worker { data: held, out: 0 };
}

// Take a worker apart: free its slice; 1 if its thread computed the wrong answer.
fn finish[&h](heap: &!h Heap, w: Worker) -> [heap] int {
    let Worker { data, out } = w;
    unbox_slice(heap, data);
    if out != 0 - %d {
        return 1;
    }
    return 0;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(io);
    var h0 = heap;
    var code = 0;
    borrow mut h0 as &!h in {
""" % (ROUNDS, N, N, ROUNDS)
    for i in range(p):
        s += "        var w%d = make(h);\n" % i
    ind = "        "
    for i in range(p):
        s += ind + "borrow mut w%d as &!k%d in {\n" % (i, i)
        ind += "    "
    for i in range(p):
        s += ind + "let f%d = work;\n" % i          # one function value per spawn: see tests/accept/spawn_struct_ref.cho
    for i in range(p):
        s += ind + "let t%d = spawn(k%d, f%d);\n" % (i, i, i)
    for i in range(p):
        s += ind + "let s%d = join(t%d);\n" % (i, i)
    for i in range(p):
        ind = ind[:-4]
        s += ind + "}\n"
    for i in range(p):
        s += "        code = code + finish(h, w%d);\n" % i
    s += """    }
    release(h0);
    return code;
}
"""
    return s


out = sys.argv[1] if len(sys.argv) > 1 else "."
os.makedirs(out, exist_ok=True)
for p in (1, 2, 4):
    with open(os.path.join(out, "par%d.cho" % p), "w") as f:
        f.write(program(p))
