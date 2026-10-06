edition 7;
// AES-128-GCM seal, hardware path against software path, at 64 and 16,384
// bytes: bytes sealed and milliseconds for each.
import std.gcm;
import std.io;

fn run[&i, &k](io: &!i Io, clock: &k Clock, size: int, n: int) -> [io_write, clock] int {
    region r {
        let key = alloc_slice[r](16, byte_of(7));
        let nonce = alloc_slice[r](12, byte_of(1));
        let aad = alloc_slice[r](13, byte_of(23));
        let text = alloc_slice[r](size, byte_of(42));
        let out = alloc_slice[r](size + 16, byte_of(0));
        let ctx = alloc_slice[r](gcm.context_len(), 0);
        let hw = alloc_slice[r](gcm.hw_len(), byte_of(0));
        gcm.prepare(key, ctx, hw);
        var t0 = clock_ms(clock);
        var k = 0;
        while k < n {
            gcm.seal_with(ctx, hw, nonce, aad, text, out);
            k = k + 1;
        }
        let fast = clock_ms(clock) - t0;
        t0 = clock_ms(clock);
        k = 0;
        while k < n {
            gcm.seal_software(ctx, nonce, aad, text, out);
            k = k + 1;
        }
        let slow = clock_ms(clock) - t0;
        io.print_int(io, size);
        io.space(io);
        io.print_int(io, n);
        io.space(io);
        io.print_int(io, fast);
        io.space(io);
        io.print_int(io, slow);
        io.newline(io);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(signals);
    release(exec);
    borrow mut io as &!i in {
        borrow clock as &k in {
            run(i, k, 64, 200000);
            run(i, k, 16384, 4000);
        }
    }
    release(io);
    release(clock);
    return 0;
}
