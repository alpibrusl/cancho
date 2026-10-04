//~ STDOUT claimed
//~ STDOUT nothing arrived
//~ STDOUT released

// `docs/signals.md` section 2: claim two signals, look, close. No signal is
// sent, so the look finds nothing; the claim and the close each succeed. The
// conformance suite (`conformance/signals.rs`) is where signals are sent.
edition 6;

import std.io;
import std.signals as sg;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    let stop = narrow(signals, "TERM,INT");
    var status = 1;
    borrow mut io as &!i in {
        borrow stop as &s in {
            match signals_watch(s) {
                Watching::Ok(w) => {
                    var watch = w;
                    io.write_all(i, "claimed\n");
                    borrow mut watch as &!wh in {
                        if sg.any(signals_pending(wh), sg.stop_signals()) {
                            io.write_all(i, "a stop arrived\n");
                        } else {
                            io.write_all(i, "nothing arrived\n");
                        }
                    }
                    if signals_close(watch) == 0 {
                        io.write_all(i, "released\n");
                        status = 0;
                    }
                }
                Watching::Failed(e) => {
                    io.write_all(i, "refused\n");
                }
            }
        }
    }
    release(stop);
    release(io);
    return status;
}
