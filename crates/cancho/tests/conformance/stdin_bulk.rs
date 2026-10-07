//! `read_bytes` (`docs/standard-input.md` §7): standard input in bulk.
//!
//! `tests/programs/stdin_read_bytes.cho` reads standard input to its end through a buffer of a
//! given size and prints a count, a line count and an order-sensitive hash of what it saw. Each
//! test feeds it input whose answer is computed here, on both backends, and compares.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

fn build(tag: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(tag);
    let exe = dir.join(format!("probe-{backend}"));
    let out = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/stdin_read_bytes.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        out.status.success(),
        "`--backend {backend}`: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (dir, exe)
}

/// What the program prints for `input` read to the end, with `empty=ok ` first when the
/// buffer is empty.
fn expected(input: &[u8], chunk: usize) -> String {
    let mut sum: u64 = 0;
    let mut lines = 0;
    for &b in input {
        sum = (sum * 31 + u64::from(b) + 1) % 1_000_000_007;
        lines += u64::from(b == b'\n');
    }
    let empty = if chunk == 0 { "empty=ok " } else { "" };
    format!("{empty}bytes={} lines={lines} sum={sum} end=eof\n", input.len())
}

/// A deterministic stream with every byte value, newlines among them.
fn pattern(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as u8
        })
        .collect()
}

/// Run `exe args` with `input` delivered to its standard input as `pieces`
/// writes, with `pause` between them (none for a file-like burst).
fn run_fed(exe: &Path, args: &[&str], input: &[u8], pieces: usize, pause: Option<u64>) -> String {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the probe runs");
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        let step = data.len().div_ceil(pieces.max(1)).max(1);
        for piece in data.chunks(step) {
            if stdin.write_all(piece).is_err() {
                return;
            }
            let _ = stdin.flush();
            if let Some(ms) = pause {
                std::thread::sleep(std::time::Duration::from_millis(ms));
            }
        }
    });
    let out = child.wait_with_output().expect("the probe finishes");
    let _ = writer.join();
    assert!(out.status.success(), "{:?}", out.status);
    String::from_utf8(out.stdout).expect("the probe prints text")
}

#[test]
fn every_byte_arrives_once_in_order_for_any_buffer_size_and_input_size() {
    for backend in BACKENDS {
        let (dir, exe) = build(&format!("stdin-bulk-sizes-{backend}"), backend);
        // Sizes around the buffer sizes in play: empty, one byte, less than one chunk, exactly
        // one, one more, and several.
        for len in [0usize, 1, 5, 6, 4095, 4096, 4097, 65_535, 65_536, 65_537, 300_000] {
            let input = pattern(len);
            for chunk in [0usize, 1, 7, 4096, 65_536, 1 << 20] {
                let c = chunk.to_string();
                let got = run_fed(&exe, &[&c], &input, 1, None);
                assert_eq!(got, expected(&input, chunk), "{backend}: {len} bytes, buffer {chunk}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_large_input_is_read_whole() {
    // 24 MiB: far more than any internal buffer, and what `getchar` took a third of a second
    // to read on the machine that measured it.
    let input = pattern(24 << 20);
    for backend in BACKENDS {
        let (dir, exe) = build(&format!("stdin-bulk-large-{backend}"), backend);
        for chunk in ["65536", "999983"] {
            let got = run_fed(&exe, &[chunk], &input, 1, None);
            assert_eq!(got, expected(&input, 1), "{backend}: buffer {chunk}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn input_that_arrives_in_pieces_is_the_same_input() {
    // A pipe delivers what the writer wrote when it wrote it. `fread` asks again until the buffer
    // is full or the input ends, so pieces smaller than the buffer, larger than it, and straddling
    // it all give one answer -- the failure this guards against is a read that takes one piece for
    // the whole input and calls it the end.
    let input = pattern(200_000);
    for backend in BACKENDS {
        let (dir, exe) = build(&format!("stdin-bulk-pieces-{backend}"), backend);
        for (pieces, chunk) in [(40usize, "65536"), (40, "7"), (3, "4096"), (500, "65536")] {
            let got = run_fed(&exe, &[chunk], &input, pieces, Some(2));
            assert_eq!(got, expected(&input, 1), "{backend}: {pieces} pieces, buffer {chunk}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn it_shares_the_stream_with_getchar() {
    // `getchar` fills the stdio buffer with far more than the five bytes it hands out, and a read
    // that went around that buffer (`read(0)`) would lose them. The count is every byte.
    let input = pattern(100_000);
    for backend in BACKENDS {
        let (dir, exe) = build(&format!("stdin-bulk-mix-{backend}"), backend);
        for chunk in ["4096", "7", "1000000"] {
            let got = run_fed(&exe, &[chunk, "mix"], &input, 1, None);
            assert_eq!(got, expected(&input, 1), "{backend}: buffer {chunk}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_failed_read_is_failed_with_the_errno_and_end_is_not_a_failure() {
    for backend in BACKENDS {
        let (dir, exe) = build(&format!("stdin-bulk-failure-{backend}"), backend);
        let probe = exe.display().to_string();
        let run = |redirect: &str| {
            let out = Command::new("sh")
                .args(["-c", &format!("'{probe}' 64 {redirect}")])
                .output()
                .expect("sh runs");
            assert!(out.status.success(), "{redirect}: {:?}", out.status);
            String::from_utf8(out.stdout).expect("text")
        };
        // A closed descriptor is EBADF (9), a directory is EISDIR (21): both the same on
        // Linux and macOS.
        assert_eq!(run("<&-"), "bytes=0 lines=0 sum=0 end=failed:9\n", "{backend}: closed");
        assert_eq!(run("</"), "bytes=0 lines=0 sum=0 end=failed:21\n", "{backend}: a directory");
        // An empty file and /dev/null are the end of input, not a failure.
        assert_eq!(run("</dev/null"), "bytes=0 lines=0 sum=0 end=eof\n", "{backend}: /dev/null");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn the_row_says_standard_input_and_not_the_file_system() {
    // The point of a bulk read behind `Io`: the authority report still shows `io_read`, and
    // shows no `fs_read`, which is what reading `/dev/stdin` through `file_read` would show.
    let (effects, foreign, _) = authority_of("tests/programs/stdin_read_bytes.cho");
    assert!(effects.iter().any(|e| e == "io_read"), "{effects:?}");
    assert!(effects.iter().any(|e| e == "io_write"), "{effects:?}");
    assert!(!effects.iter().any(|e| e.starts_with("fs_") || e.starts_with("file_")), "{effects:?}");
    assert!(foreign.is_empty(), "{foreign:?}");
}

#[test]
fn a_program_that_never_reads_standard_input_cannot_call_it() {
    // `read_bytes` performs `io_read`: a row that says only `io_write` is refused.
    let dir = scratch("stdin-bulk-row");
    let file = dir.join("p.cho");
    std::fs::write(
        &file,
        "edition 7;\n\
         fn pull[&i](io: &!i Io) -> [io_write] int {\n\
             region a {\n\
                 let buf = alloc_slice[a](8, byte_of(0));\n\
                 match read_bytes(io, buf) {\n\
                     Read::Got(n) => { return n; }\n\
                     Read::End => { return 0; }\n\
                     Read::Failed(e) => { return 0 - e; }\n\
                 }\n\
             }\n\
         }\n\
         fn main(world: World) -> [] int { release(world); return 0; }\n",
    )
    .expect("a writable fixture");
    let out = Command::new(BIN).args(["check", "--std"]).arg(&file).output().expect("compiler");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("io_read"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
