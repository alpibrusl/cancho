edition 5;

// `pin_check` -- `pin.is_public` against the original it was copied from, `lexsys-hooks`' `src/destination.ls`.
//
//     lex-sys run pin.ls /path/to/lexsys-hooks/src/destination.ls pin_check.ls --std      # exit 0: the two agree everywhere checked
//
// Checked: both edges of every range (the first address, the last, and the one on each side of them), every first octet with 17 samples
// of the other three, and 2,000,000 pseudo-random addresses. `test/pin_test.py` builds and runs it when a checkout of `lexsys-hooks` is
// beside this one.

import std.io;
import pin;
import destination;

fn next(x: int) -> [] int {
    return (x * 1103515245 + 12345) % 2147483648;
}

fn pack(a: int, b: int, c: int, d: int) -> [] int {
    return a * 16777216 + b * 65536 + c * 256 + d;
}

fn disagree(a: int) -> [] int {
    if pin.is_public(a) == destination.is_public(a) {
        return 0;
    }
    return 1;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    var bad = 0;
    var checked = 0;
    // The edges: the lower and upper bound of each range, and the neighbours.
    region r {
        let lows = alloc_slice[r](60, 0);
        // (first, second, third, fourth) of the first address of each range, then of the last
        var n = 0;
        // 0/8, 10/8, 127/8, 224/4, 240/4
        lows[0] = pack(0, 0, 0, 0);
        lows[1] = pack(0, 255, 255, 255);
        lows[2] = pack(10, 0, 0, 0);
        lows[3] = pack(10, 255, 255, 255);
        lows[4] = pack(127, 0, 0, 0);
        lows[5] = pack(127, 255, 255, 255);
        lows[6] = pack(224, 0, 0, 0);
        lows[7] = pack(255, 255, 255, 255);
        // 100.64/10, 169.254/16, 172.16/12, 192.168/16, 198.18/15
        lows[8] = pack(100, 64, 0, 0);
        lows[9] = pack(100, 127, 255, 255);
        lows[10] = pack(169, 254, 0, 0);
        lows[11] = pack(169, 254, 255, 255);
        lows[12] = pack(172, 16, 0, 0);
        lows[13] = pack(172, 31, 255, 255);
        lows[14] = pack(192, 168, 0, 0);
        lows[15] = pack(192, 168, 255, 255);
        lows[16] = pack(198, 18, 0, 0);
        lows[17] = pack(198, 19, 255, 255);
        // 192.0.0/24, 192.0.2/24, 192.88.99/24, 198.51.100/24, 203.0.113/24
        lows[18] = pack(192, 0, 0, 0);
        lows[19] = pack(192, 0, 0, 255);
        lows[20] = pack(192, 0, 2, 0);
        lows[21] = pack(192, 0, 2, 255);
        lows[22] = pack(192, 88, 99, 0);
        lows[23] = pack(192, 88, 99, 255);
        lows[24] = pack(198, 51, 100, 0);
        lows[25] = pack(198, 51, 100, 255);
        lows[26] = pack(203, 0, 113, 0);
        lows[27] = pack(203, 0, 113, 255);
        n = 28;
        var i = 0;
        while i < n {
            var d = 0 - 1;
            while d <= 1 {
                let a = lows[i] + d;
                if a >= 0 && a <= 4294967295 {
                    bad = bad + disagree(a);
                    checked = checked + 1;
                }
                d = d + 1;
            }
            i = i + 1;
        }
    }
    var a = 0;
    while a < 256 {
        var k = 0;
        while k < 17 {
            let b = (k * 37 + a) % 256;
            let c = (k * 91 + a * 3) % 256;
            let d = (k * 53 + 7) % 256;
            bad = bad + disagree(pack(a, b, c, d));
            checked = checked + 1;
            k = k + 1;
        }
        a = a + 1;
    }
    var x = 1;
    var j = 0;
    while j < 2000000 {
        x = next(x);
        let hi = x;
        x = next(x);
        let v = hi % 65536 * 65536 + x % 65536;
        bad = bad + disagree(v);
        checked = checked + 1;
        j = j + 1;
    }
    borrow mut io as &!i in {
        io.write_all(i, "pin_check: ");
        io.print_nat(i, checked);
        io.write_all(i, " addresses, ");
        io.print_nat(i, bad);
        io.write_all(i, " disagreements\n");
    }
    release(io);
    // An exit status is a byte: a count of disagreements that is a multiple of 256 would read as success.
    if bad > 0 {
        return 1;
    }
    return 0;
}
