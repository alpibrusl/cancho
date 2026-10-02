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

// A checked sum that vectorises. Per block of 4096 elements, add the low 32 bits and the (signed) high 32 bits of every
// element in two separate accumulators with no checks -- neither can overflow in a block, and every operation in the loop
// has a 64-bit SSE2 form -- then put the block's sum together with checked arithmetic, once: the multiplication below traps
// exactly when the true sum of the block does not fit an `int`. Nothing is unchecked that can overflow.
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
            var lo_sum = 0;
            var hi_sum = 0;
            var i = from;
            while i < to {
                let x = v[i];
                lo_sum = wrapping_add(lo_sum, x & 0xffff_ffff);
                hi_sum = wrapping_add(hi_sum, x >> 32);
                i = wrapping_add(i, 1);
            }
            // lo_sum is below 2^44 and hi_sum within +-2^43: nothing above wrapped
            let carry = lo_sum >> 32;
            let hi = hi_sum + carry;
            let low = lo_sum & 0xffff_ffff;
            total = total + (hi * 4294967296 + low);
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
