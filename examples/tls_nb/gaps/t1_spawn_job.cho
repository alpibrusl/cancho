// A unique reference to a struct that OWNS a capability crosses as a thread's one payload: the struct's Ffi is moved into the thread
// with it (the spawning side cannot use that Ffi again until the Job is taken apart after `join`), and the worker uses it through
// the field. Compare t3: a shared reference to an Ffi crosses and both sides keep using it, but then the payload carries nothing else.
edition 5;

extern fn basename[&f, &s](ffi: &f Ffi("tls"), text: &s [byte]) -> [ffi("tls")] int;

res struct Job {
    ffi: Ffi("tls"),
    name: Box[[byte]],
    out: int,
}

fn worker[&r](job: &!r Job) -> [ffi("tls")] int {
    job.out = basename(job.ffi, contents(job.name));
    return 0;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io);
    release(fs);
    release(args);
    release(net);
    release(clock);
    var hp = heap;
    var code = 1;
    borrow mut hp as &!h in {
        var job = Job { ffi: narrow(ffi, "tls"), name: box_slice(h, 4, byte_of(97)), out: 0 };
        var status = 1;
        borrow mut job as &!k in {
            let w = worker;
            let t = spawn(k, w);
            status = join(t);
        }
        let Job { ffi, name, out } = job;
        release(ffi);
        unbox_slice(h, name);
        if status == 0 && out != 0 {
            code = 0;
        }
    }
    release(hp);
    return code;
}
