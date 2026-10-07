//! Checked output (`docs/checked-output.md`, issue #215): `flush_out` tells a program whether what it
//! wrote to standard output arrived.
//!
//! Each program is judged from outside: its standard output is pointed at `/dev/full`, at a closed
//! descriptor, at a pipe and at `/dev/null` by a shell, and the answer the program got is read back as
//! its exit status. Both backends, the same expectations.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// Writes six bytes, or 100,000 with an argument (so that `fwrite` itself fails while the buffer drains
/// and only the stream's error indicator remembers it), flushes twice, and exits `100 + errno` of the
/// first answer (`100` for `Ok`), or `99` if the second flush disagreed with a failed first.
const PROBE: &str = "edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(net); release(clock);
    var big = false;
    borrow args as &g in {
        big = arg_count(g) > 1;
    }
    release(args);
    var status = 0;
    borrow mut io as &!i in {
        if big {
            region a {
                let block = alloc_slice[a](50000, byte_of(120));
                write_bytes(i, block);
                write_bytes(i, block);
            }
        } else {
            write_bytes(i, \"hello\\n\");
        }
        var first = 0;
        match flush_out(i) {
            Done::Ok(n) => { first = 0; }
            Done::Failed(e) => { first = e; }
        }
        var second = 0;
        match flush_out(i) {
            Done::Ok(n) => { second = 0; }
            Done::Failed(e) => { second = e; }
        }
        status = 100 + first;
        if first != 0 && second == 0 {
            status = 99;
        }
    }
    release(io);
    return status;
}
";

fn build(dir: &Path, backend: &str) -> PathBuf {
    let file = dir.join("probe.cho");
    std::fs::write(&file, PROBE).expect("a writable fixture");
    let exe = dir.join(format!("probe-{backend}"));
    let out = Command::new(BIN)
        .args([
            "build".as_ref(),
            file.as_os_str(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(
        out.status.success(),
        "`--backend {backend}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// Run `exe` (with `arg`, if any) under `sh` with standard output redirected by `redirect`, and answer
/// its exit status and what reached a pipe when the redirect is one.
fn run(exe: &Path, arg: &str, redirect: &str) -> (i32, Vec<u8>) {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("\"$0\" {arg} {redirect}"))
        .arg(exe)
        .stdout(Stdio::piped())
        .output()
        .expect("sh runs");
    (out.status.code().expect("an exit status, not a signal"), out.stdout)
}

/// Whether this machine has `/dev/full`. Linux does and macOS does not (there the shell's redirect
/// itself fails, before the program runs). On Linux its absence is a failure, not a skip, so the
/// `ENOSPC` cases can never quietly stop running where they can run; on macOS the closed descriptor
/// below is the case that observes a failed write.
fn has_full_device() -> bool {
    let present = Path::new("/dev/full").exists();
    if cfg!(target_os = "linux") {
        assert!(present, "/dev/full is missing on Linux");
    }
    present
}

#[test]
fn a_full_device_is_enospc_on_both_backends() {
    if !has_full_device() {
        return;
    }
    for backend in BACKENDS {
        let dir = scratch(&format!("checked-output-full-{backend}"));
        let exe = build(&dir, backend);
        assert_eq!(run(&exe, "", "> /dev/full").0, 100 + 28, "{backend}");
    }
}

#[test]
fn a_closed_stdout_is_ebadf_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("checked-output-closed-{backend}"));
        let exe = build(&dir, backend);
        assert_eq!(run(&exe, "", ">&-").0, 100 + 9, "{backend}");
    }
}

#[test]
fn an_earlier_failure_is_still_reported_and_keeps_being_reported() {
    // `fflush` alone answers 0 here: the failed bytes were discarded and the buffer is empty. The
    // stream's error indicator is what remembers, and it is not cleared, so both flushes say `EIO`.
    for backend in BACKENDS {
        let dir = scratch(&format!("checked-output-earlier-{backend}"));
        let exe = build(&dir, backend);
        if has_full_device() {
            assert_eq!(run(&exe, "big", "> /dev/full").0, 100 + 5, "{backend}: /dev/full");
        }
        assert_eq!(run(&exe, "big", ">&-").0, 100 + 5, "{backend}: closed");
    }
}

#[test]
fn a_working_stream_is_ok_and_the_bytes_arrive() {
    for backend in BACKENDS {
        let dir = scratch(&format!("checked-output-ok-{backend}"));
        let exe = build(&dir, backend);
        let (status, bytes) = run(&exe, "", "");
        assert_eq!((status, bytes.as_slice()), (100, b"hello\n".as_slice()), "{backend}: pipe");
        let (status, bytes) = run(&exe, "big", "");
        assert_eq!((status, bytes.len()), (100, 100_000), "{backend}: pipe, big");
        assert_eq!(run(&exe, "", "> /dev/null").0, 100, "{backend}: /dev/null");
    }
}

#[test]
fn flushing_reports_io_write_and_nothing_else() {
    let report = authority_json(
        "edition 5;
fn flush[&i](io: &!i Io) -> [io_write] int {
    match flush_out(io) {
        Done::Ok(n) => { return 0; }
        Done::Failed(e) => { return e; }
    }
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(net); release(clock);
    var code = 0;
    borrow mut io as &!i in { code = flush(i); }
    release(io);
    return code;
}
",
        "checked-output-authority",
    );
    let effects = report.lines().find(|l| l.contains("\"effects\"")).expect("an effects line");
    assert_eq!(effects.trim(), "\"effects\": [\"io_write\"],", "{report}");
}
