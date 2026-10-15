//! A region allocation past the chunk (`docs/region-limit.md`): refused at
//! check time when its size is a constant, and a named trap at run time
//! when it is not. The run-time half is built and run on both backends.

use super::*;
use std::os::unix::process::ExitStatusExt;

/// A region allocation of `count` elements whose size the compiler cannot
/// see: the count is a variable the loop moves, so nothing folds it.
fn program(count: usize, fill: &str) -> String {
    format!(
        "fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(args); release(ffi); release(fs); release(heap); release(io);\n\
             var want = 0;\n\
             var i = 0;\n\
             while i < {count} {{ want = want + 1; i = i + 1; }}\n\
             var got = 0;\n\
             region r {{\n\
                 let t = alloc_slice[r](want, {fill});\n\
                 got = len(t);\n\
             }}\n\
             return got - want;\n\
         }}\n"
    )
}

/// Build `source` with `backend` and run it: (signal, exit code, stderr).
fn run(tag: &str, count: usize, fill: &str, backend: &str) -> (Option<i32>, Option<i32>, String) {
    let dir = scratch(&format!("region-limit-{tag}-{count}-{backend}"));
    let src = dir.join("p.cho");
    let source = program(count, fill);
    std::fs::write(&src, &source).expect("a writable fixture");
    let exe = dir.join("p");
    let build = Command::new(BIN)
        .args(["build".as_ref(), src.as_os_str(), "--backend".as_ref(), backend.as_ref()])
        .args(["-o".as_ref(), exe.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let out = Command::new(&exe).output().expect("the compiled program runs");
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.signal(), out.status.code(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn a_region_allocation_past_the_chunk_is_a_named_trap_on_both_backends() {
    for backend in ["cranelift", "llvm"] {
        // One element past the chunk, for ints and for bytes.
        for (tag, count, fill) in [("int", 8193, "0"), ("byte", 65537, "byte_of(0)")] {
            let (signal, code, stderr) = run(tag, count, fill, backend);
            assert_eq!(signal, Some(4), "{backend} {tag}: SIGILL, as every trap (exit {code:?})");
            assert!(
                stderr.contains("region chunk exhausted") && stderr.contains("box_slice"),
                "{backend} {tag}: the trap must name its cause and the remedy, said {stderr:?}"
            );
        }
    }
}

#[test]
fn a_region_allocation_of_exactly_the_chunk_runs_and_says_nothing() {
    for backend in ["cranelift", "llvm"] {
        for (tag, count, fill) in [("int", 8192, "0"), ("byte", 65536, "byte_of(0)")] {
            let (signal, code, stderr) = run(tag, count, fill, backend);
            assert_eq!((signal, code), (None, Some(0)), "{backend} {tag}: {stderr}");
            assert_eq!(stderr, "", "{backend} {tag}: a run that fits is silent");
        }
    }
}
