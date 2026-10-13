//! The per-thread stack of region chunks (`docs/region-chunk-cache.md` §6):
//! nested regions in a loop reach the kernel O(1) times, each thread keeps
//! its own chunks and frees them when it ends, the stack holds at most 16,
//! and every chunk is cleared before it is kept or freed. Each on both
//! backends. The system-call and allocator counts are Linux's (`strace`, an
//! `LD_PRELOAD` shim); the programs' own answers are checked everywhere.

use super::*;

/// Three nested regions a round, each filled with a value of its own and
/// read back: the answer is how many rounds read something else (0).
const NESTED: &str = "\
fn work(rounds: int) -> [] int {
    var bad = 0;
    var i = 0;
    while i < rounds {
        region a {
            let x = alloc_slice[a](512, i + 1);
            region b {
                let y = alloc_slice[b](512, i + 2);
                region c {
                    let z = alloc_slice[c](512, i + 3);
                    if x[511] != i + 1 || y[0] != i + 2 || z[300] != i + 3 { bad = bad + 1; }
                }
            }
        }
        i = i + 1;
    }
    return bad;
}
";

fn nested_main(rounds: usize) -> String {
    format!(
        "{NESTED}\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             return work({rounds});\n\
         }}\n"
    )
}

/// Eight spawned threads and `main` each run [`NESTED`]; the answer is the
/// rounds that read the wrong value, summed.
fn threads_main(rounds: usize) -> String {
    let spawns: String = (0..8).map(|t| format!("    let h{t} = spawn({rounds}, w);\n")).collect();
    let joins: String = (0..8).map(|t| format!(" + join(h{t})")).collect();
    format!(
        "edition 4;\n{NESTED}\
         fn worker(rounds: int) -> [] int {{ return work(rounds); }}\n\
         fn main(world: World) -> [conc] int {{\n\
             let Split {{ io, ffi, fs, heap, args, net }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n\
             let w = worker;\n{spawns}\
             let bad = work({rounds}){joins};\n\
             return bad;\n\
         }}\n"
    )
}

/// A recursion `depth` regions deep, `times` times over, each region filled
/// with 7 and read back after the deeper ones have ended.
fn deep_main(depth: usize, times: usize) -> String {
    format!(
        "fn deep(depth: int) -> [] int {{\n\
             if depth == 0 {{ return 0; }}\n\
             var bad = 0;\n\
             region r {{\n\
                 let s = alloc_slice[r](512, 7);\n\
                 bad = deep(depth - 1);\n\
                 if s[0] != 7 || s[511] != 7 {{ bad = bad + 1; }}\n\
             }}\n\
             return bad;\n\
         }}\n\
         fn main(world: World) -> [] int {{\n\
             let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             var bad = 0;\n\
             var i = 0;\n\
             while i < {times} {{ bad = bad + deep({depth}); i = i + 1; }}\n\
             return bad;\n\
         }}\n"
    )
}

/// Build `source` on `backend` into `dir`.
fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let path = dir.join(format!("{name}.cho"));
    std::fs::write(&path, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let out = Command::new(BIN)
        .args(["build", "--backend", backend])
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{backend}: {}", String::from_utf8_lossy(&out.stderr));
    exe
}

/// Every program reads back exactly what it wrote, on both backends: a
/// reused chunk shows a region its own fill, never the last region's.
#[test]
fn regions_read_their_own_fill_on_both_backends() {
    let dir = scratch("region-cache-fill");
    for backend in ["llvm", "cranelift"] {
        for (name, source) in [
            ("nested", nested_main(2_000)),
            ("threads", threads_main(500)),
            ("deep", deep_main(40, 20)),
        ] {
            let exe = build(&dir, name, &source, backend);
            let run = Command::new(&exe).output().expect("the program runs");
            assert_eq!(
                run.status.code(),
                Some(0),
                "{backend} {name}: a region read another's bytes"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the shim counted: 64 KiB `malloc`s, their `free`s, and the `free`s
/// of a chunk whose first 4,096 bytes were not all zero.
#[cfg(target_os = "linux")]
#[derive(Debug, PartialEq)]
struct Counts {
    mallocs: u64,
    frees: u64,
    dirty: u64,
}

/// An `LD_PRELOAD` shim: counts `malloc(65536)`, the `free` of each such
/// chunk, and the chunks freed with a non-zero byte in their first page,
/// and writes the three to `$CHUNKS_OUT` at exit.
#[cfg(target_os = "linux")]
const SHIM: &str = r#"
#define _GNU_SOURCE
#include <dlfcn.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
static void *(*real_malloc)(size_t);
static void (*real_free)(void *);
static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static void *live[8192];
static int nlive;
static unsigned long mallocs, frees, dirty;
void *malloc(size_t n) {
    if (!real_malloc) real_malloc = dlsym(RTLD_NEXT, "malloc");
    void *p = real_malloc(n);
    if (n == 65536 && p) {
        pthread_mutex_lock(&mu);
        mallocs++;
        if (nlive < 8192) live[nlive++] = p;
        pthread_mutex_unlock(&mu);
    }
    return p;
}
void free(void *p) {
    if (!real_free) real_free = dlsym(RTLD_NEXT, "free");
    if (p) {
        pthread_mutex_lock(&mu);
        for (int i = 0; i < nlive; i++) {
            if (live[i] == p) {
                live[i] = live[--nlive];
                frees++;
                const unsigned char *b = p;
                for (int j = 0; j < 4096; j++) if (b[j]) { dirty++; break; }
                break;
            }
        }
        pthread_mutex_unlock(&mu);
    }
    real_free(p);
}
__attribute__((destructor)) static void report(void) {
    const char *path = getenv("CHUNKS_OUT");
    FILE *f = path ? fopen(path, "w") : 0;
    if (f) { fprintf(f, "%lu %lu %lu\n", mallocs, frees, dirty); fclose(f); }
}
"#;

#[cfg(target_os = "linux")]
fn shim(dir: &Path) -> PathBuf {
    let source = dir.join("chunks.c");
    std::fs::write(&source, SHIM).expect("a writable shim");
    let lib = dir.join("chunks.so");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let out = Command::new(&cc)
        .args(["-O2", "-shared", "-fPIC", "-o"])
        .arg(&lib)
        .arg(&source)
        .arg("-ldl")
        .output()
        .expect("a C compiler (the one `cancho build` links with)");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    lib
}

#[cfg(target_os = "linux")]
fn counted(exe: &Path, lib: &Path, dir: &Path) -> Counts {
    let report = dir.join("counts.txt");
    let _ = std::fs::remove_file(&report);
    let run = Command::new(exe)
        .env("LD_PRELOAD", lib)
        .env("CHUNKS_OUT", &report)
        .output()
        .expect("the program runs");
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
    let text = std::fs::read_to_string(&report).expect("the shim wrote its counts");
    let n: Vec<u64> = text.split_whitespace().map(|w| w.parse().expect("a count")).collect();
    Counts { mallocs: n[0], frees: n[1], dirty: n[2] }
}

/// §6: 2,000 rounds of three nested regions take three chunks from
/// `malloc` in all (6,000 before the stack), and no chunk is freed: they
/// stay on `main`'s thread until the process ends.
#[cfg(target_os = "linux")]
#[test]
fn nested_regions_in_a_loop_take_three_chunks_in_all() {
    let dir = scratch("region-cache-nested");
    let lib = shim(&dir);
    for backend in ["llvm", "cranelift"] {
        let exe = build(&dir, "nested", &nested_main(2_000), backend);
        assert_eq!(
            counted(&exe, &lib, &dir),
            Counts { mallocs: 3, frees: 0, dirty: 0 },
            "{backend}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §6: eight spawned threads and `main` each take their own three chunks
/// (27 in all: a thread that took another's would make it fewer), and each
/// spawned thread's three are freed, cleared, when it ends (24: the key's
/// destructor), before `join` answers.
#[cfg(target_os = "linux")]
#[test]
fn each_thread_keeps_its_own_chunks_and_frees_them_when_it_ends() {
    let dir = scratch("region-cache-threads");
    let lib = shim(&dir);
    for backend in ["llvm", "cranelift"] {
        let exe = build(&dir, "threads", &threads_main(500), backend);
        assert_eq!(
            counted(&exe, &lib, &dir),
            Counts { mallocs: 27, frees: 24, dirty: 0 },
            "{backend}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §2's bound: a recursion 40 regions deep, 100 times. The first descent
/// takes 40 chunks from `malloc`; each return keeps 16 and frees 24, all
/// cleared; each later descent takes the 16 back and 24 more. So
/// 40 + 99 × 24 = 2,416 `malloc`s and 100 × 24 = 2,400 `free`s, exactly: a
/// stack of 15 or 17 would give other numbers.
#[cfg(target_os = "linux")]
#[test]
fn a_thread_keeps_at_most_sixteen_chunks() {
    let dir = scratch("region-cache-bound");
    let lib = shim(&dir);
    for backend in ["llvm", "cranelift"] {
        let exe = build(&dir, "deep", &deep_main(40, 100), backend);
        assert_eq!(
            counted(&exe, &lib, &dir),
            Counts { mallocs: 2_416, frees: 2_400, dirty: 0 },
            "{backend}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// §5 gate 1: 20,000 rounds of three nested regions make O(1) `brk` calls
/// (one for each 128 KiB the heap first grows by, and start-up's), where
/// trimming made one or more a round. Under `strace`, which CI installs;
/// a machine without it says so and skips, and CI does not.
#[cfg(target_os = "linux")]
#[test]
fn nested_regions_in_a_loop_make_o1_brk_calls() {
    if Command::new("strace").arg("-V").output().is_err() {
        assert!(std::env::var_os("CI").is_none(), "CI installs strace for this test");
        eprintln!("strace is not installed; the brk count is not checked");
        return;
    }
    let dir = scratch("region-cache-brk");
    for backend in ["llvm", "cranelift"] {
        let exe = build(&dir, "nested", &nested_main(20_000), backend);
        let trace = dir.join("trace.txt");
        let run = Command::new("strace")
            .args(["-f", "-c", "-e", "trace=brk", "-o"])
            .arg(&trace)
            .arg(&exe)
            .output()
            .expect("strace runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{backend}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let summary = std::fs::read_to_string(&trace).expect("strace's summary");
        let calls: u64 = summary
            .lines()
            .find(|line| line.split_whitespace().last() == Some("brk"))
            .and_then(|line| line.split_whitespace().nth(3).and_then(|w| w.parse().ok()))
            .unwrap_or(0);
        assert!(calls <= 16, "{backend}: {calls} brk calls for 20,000 rounds\n{summary}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
