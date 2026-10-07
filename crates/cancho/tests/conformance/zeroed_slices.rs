//! Zero-filled heap slices (`docs/zeroed-slices.md`): `box_slice(h, n, 0)` (and `byte_of(0)`, `false`,
//! `0.0`) is `calloc`, with no fill loop; every other fill keeps the loop. Both backends.
//!
//! Judged from outside: what the program reads back, and -- on Linux, where `/proc` says it -- how much
//! of a large zeroed slice ever became resident.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}.cho"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
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
        "`--backend {backend}`: {}\n{source}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// Every zero fill reads back zero, even in memory just freed full of nines; a non-zero fill, a zero computed at run time and `-0.0` (whose sign
/// bit is set, so it is not all-zero) are still written by the loop and read back as themselves. The
/// exit status counts the elements that were wrong.
const READ_BACK: &str = "edition 5;
fn zero_at_run_time(n: int) -> [] int {
    return n - n;
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(ffi); release(fs); release(args); release(net); release(clock);
    var bad = 0;
    let n = 5000;
    borrow mut heap as &!h in {
        // Freed memory full of nines, which the allocator hands straight back: a zero fill that skipped
        // the loop without asking for zeroed memory would read nines here.
        let dirty = box_slice(h, n, 9);
        unbox_slice(h, dirty);
        let ints = box_slice(h, n, 0);
        let bytes = box_slice(h, n, byte_of(0));
        let bools = box_slice(h, n, false);
        let floats = box_slice(h, n, 0.0);
        let sevens = box_slice(h, n, 7);
        let fives = box_slice(h, n, byte_of(5));
        let computed = box_slice(h, n, zero_at_run_time(n));
        let negative = box_slice(h, n, -0.0);
        borrow ints as &r in { var i = 0; while i < n { if contents(r)[i] != 0 { bad = bad + 1; } i = i + 1; } }
        borrow bytes as &r in { var i = 0; while i < n { if int_of(contents(r)[i]) != 0 { bad = bad + 1; } i = i + 1; } }
        borrow bools as &r in { var i = 0; while i < n { if contents(r)[i] { bad = bad + 1; } i = i + 1; } }
        borrow floats as &r in { var i = 0; while i < n { if bits_of(contents(r)[i]) != 0 { bad = bad + 1; } i = i + 1; } }
        borrow sevens as &r in { var i = 0; while i < n { if contents(r)[i] != 7 { bad = bad + 1; } i = i + 1; } }
        borrow fives as &r in { var i = 0; while i < n { if int_of(contents(r)[i]) != 5 { bad = bad + 1; } i = i + 1; } }
        borrow computed as &r in { var i = 0; while i < n { if contents(r)[i] != 0 { bad = bad + 1; } i = i + 1; } }
        borrow negative as &r in { var i = 0; while i < n { if bits_of(contents(r)[i]) >= 0 { bad = bad + 1; } i = i + 1; } }
        unbox_slice(h, ints); unbox_slice(h, bytes); unbox_slice(h, bools); unbox_slice(h, floats);
        unbox_slice(h, sevens); unbox_slice(h, fives); unbox_slice(h, computed); unbox_slice(h, negative);
    }
    release(heap);
    if bad > 100 {
        return 100;
    }
    return bad;
}
";

#[test]
fn every_fill_reads_back_as_itself_on_both_backends() {
    for backend in BACKENDS {
        let dir = scratch(&format!("zeroed-read-back-{backend}"));
        let exe = build(&dir, "read_back", READ_BACK, backend);
        let out = Command::new(&exe).output().expect("the program runs");
        assert_eq!(out.status.code(), Some(0), "{backend}: elements read back wrong");
    }
}

/// 64 Mi `int`s (512 MiB) filled with zero, one element written and read, then `ready` on standard error
/// (unbuffered, so it arrives at once) and a wait on standard input, so the test can read the process's
/// peak resident memory from `/proc` before it exits.
const UNTOUCHED: &str = "edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(args); release(net); release(clock);
    var answer = 0;
    borrow mut heap as &!h in {
        let big = box_slice(h, 67108864, 0);
        borrow mut big as &!w in {
            contents(w)[12345] = 7;
            answer = contents(w)[12345] + contents(w)[67108863];
        }
        borrow mut io as &!i in {
            write_err(i, \"ready\\n\");
            getchar(i);
        }
        unbox_slice(h, big);
    }
    release(heap);
    release(io);
    return answer;
}
";

/// Kilobytes from a `/proc/<pid>/status` line such as `VmHWM:   1234 kB`.
fn status_kb(pid: u32, key: &str) -> u64 {
    let status =
        std::fs::read_to_string(format!("/proc/{pid}/status")).expect("/proc is readable on Linux");
    let line =
        status.lines().find(|l| l.starts_with(key)).expect("the key is in /proc/<pid>/status");
    line.split_whitespace().nth(1).and_then(|n| n.parse().ok()).expect("a number of kB")
}

#[test]
fn a_large_zeroed_slice_is_not_made_resident_by_filling_it() {
    if !cfg!(target_os = "linux") {
        // macOS has no `/proc`; the read-back test above still covers its correctness.
        return;
    }
    use std::io::{BufRead, BufReader};
    for backend in BACKENDS {
        let dir = scratch(&format!("zeroed-untouched-{backend}"));
        let exe = build(&dir, "untouched", UNTOUCHED, backend);
        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the program starts");
        let mut line = String::new();
        BufReader::new(child.stderr.as_mut().expect("piped"))
            .read_line(&mut line)
            .expect("the program says it is ready");
        assert_eq!(line, "ready\n", "{backend}");
        let peak = status_kb(child.id(), "VmHWM:");
        drop(child.stdin.take());
        let status = child.wait().expect("the program exits");
        assert_eq!(status.code(), Some(7), "{backend}: the written element reads back");
        // A filled 512 MiB slice is 524,288 kB resident. Untouched zero pages are not resident at all.
        assert!(
            peak < 64 * 1024,
            "{backend}: peak resident {peak} kB for a zeroed slice nobody filled"
        );
    }
}
