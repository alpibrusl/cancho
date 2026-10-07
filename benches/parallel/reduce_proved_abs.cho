// `reduce` -- a memory-fed reduction, which is the shape a GPU runs.
//
// `sum` is arithmetic with no memory traffic and bounds the answer from
// above; this is the loop `benches/reduce.c` already gives to clang, so
// the two languages can be compared on the same algorithm rather than on
// the same words. `docs/gpu.md` §2 is what the comparison is for: a GPU
// has no trap, so the only points reachable there are the wrapping ones,
// and the question is whether deleting the trap is enough to reach C.
//
// 200 rounds over a million elements.
//
// This is the **checked** half of a pair: `+` traps on overflow, which is
// `defined-behaviour.md` §2.1 and the reason this language exists.

fn fill[&h](heap: &!h Heap, n: int) -> [heap] Box[[int]] {
    let held = box_slice(heap, n, 0);
    borrow mut held as &!b in {
        let v = contents(b);
        var i = 0;
        while i < n {
            // The same values `reduce.c` uses: -1, 0, 1 repeating, so
            // nothing can be constant-folded and nothing overflows.
            v[i] = i % 3 - 1;
            i = i + 1;
        }
    }
    return held;
}

// The largest absolute value in v[from..to] (a plain loop over comparisons).
fn widest[&b](v: &b [int], from: int, to: int) -> [] int {
    var m = 0;
    var i = from;
    while i < to {
        var x = v[i];
        if x < 0 {
            x = 0 - x;
        }
        if x > m {
            m = x;
        }
        i = i + 1;
    }
    return m;
}

// The sum of v[from..to] with no overflow check: only called when `widest` has shown it cannot overflow.
fn sum_wrapping[&b](v: &b [int], from: int, to: int) -> [] int {
    var total = 0;
    var i = from;
    while i < to {
        total = wrapping_add(total, v[i]);
        i = wrapping_add(i, 1);
    }
    return total;
}

// The sum of v[from..to], checked.
fn sum_checked[&b](v: &b [int], from: int, to: int) -> [] int {
    var total = 0;
    var i = from;
    while i < to {
        total = total + v[i];
        i = i + 1;
    }
    return total;
}

// A checked sum that is fast when it can be: per block of 4096 elements, find the widest value (one
// vectorisable pass over data that is then in the L1 cache); if block_len * widest cannot reach 2^62 the
// block cannot overflow and is summed without checks (vectorised); otherwise it is summed with them.
// The blocks' sums are added with the checked `+`, so an overflow anywhere still traps.
fn run[&b](v: &b [int], rounds: int) -> [] int {
    var total = 0;
    var r = 0;
    while r < rounds {
        var from = 0;
        while from < len(v) {
            var to = from + 4096;
            if to > len(v) {
                to = len(v);
            }
            let m = widest(v, from, to);
            if m < 1125899906842624 {
                total = total + sum_wrapping(v, from, to);
            } else {
                total = total + sum_checked(v, from, to);
            }
            from = to;
        }
        r = r + 1;
    }
    return total;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    release(io);
    release(ffi);

    var total = 0;
    borrow mut heap as &!h in {
        let held = fill(h, 1000000);
        borrow held as &b in {
            total = run(contents(b), 200);
        }
        unbox_slice(h, held);
    }
    release(heap);
    // -1, 0, 1 repeating over a million elements sums to -1 per round.
    return total - (0 - 200);
}
