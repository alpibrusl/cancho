//! The compiler written in cancho compiles itself (`docs/self-hosting.md` section 6, stage 4, the bootstrap).
//!
//! Three compilers, each built from the same source:
//!
//! 1. the first is built by the Rust compiler from `compiler.cho` and its modules;
//! 2. it reads that source (and the `std` modules it imports) and writes LLVM IR, which `clang` builds:
//!    the second compiler, which no Rust compiler had a hand in;
//! 3. the second reads the same source and writes IR again.
//!
//! The two IR texts must be the same, byte for byte: a compiler that was made by itself and compiles the
//! way the one that made it does. Anything it got wrong while compiling itself would show here, as a text
//! that differs or a program that does not build or run.

use super::*;

/// The program the compiler compiles: the `std` modules it imports, then its own, then its root, as
/// `FILE <length>` lines and the files (the stream `driver.cho` reads).
fn own_source() -> String {
    let root = repo_root();
    let mut files: Vec<PathBuf> = ["math", "bytes", "buffer", "io"]
        .iter()
        .map(|m| root.join(format!("std/{m}.cho")))
        .collect();
    let own = root.join("examples/selfhost");
    for module in [
        "lexcore", "tables", "kinds", "rules", "ast", "pass1", "foreign", "builtins", "types",
        "prims", "scope", "effects", "emit", "exprs", "body", "listing", "checker", "driver",
        "compiler",
    ] {
        files.push(own.join(format!("{module}.cho")));
    }
    let mut stream = String::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a source file");
        stream.push_str(&format!("FILE {}\n{text}", text.len()));
    }
    stream
}

#[test]
fn the_compiler_in_cancho_compiles_itself_to_a_fixed_point() {
    let first = build("bootstrap-1", &with_front_end("compiler.cho"));
    let source = own_source();

    let ir_1 = answer(&first, &source);
    assert!(
        ir_1.starts_with("declare"),
        "the first compiler should write code for its own source, but answered:\n{}",
        &ir_1[..ir_1.len().min(300)]
    );

    // Not an empty module: the compiler's own functions are most of 600, and what is left out is `std.math`.
    let defined = ir_1.lines().filter(|l| l.starts_with("define ")).count();
    assert!(defined >= 600, "only {defined} functions were written");

    let dir = scratch("selfhost-bootstrap-2");
    let second = dir.join("second");
    let text = dir.join("second.ll");
    std::fs::write(&text, &ir_1).expect("the scratch directory is writable");
    let cc = std::env::var("CLANG").unwrap_or_else(|_| "clang".to_owned());
    let built = Command::new(cc)
        .args(["-O1", "-Wno-override-module"])
        .arg(&text)
        .args(["-o".as_ref(), second.as_os_str()])
        .output()
        .expect("clang runs");
    assert!(
        built.status.success(),
        "clang should build the compiler the first one wrote, but said:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let ir_2 = answer(&second, &source);
    assert!(
        ir_1 == ir_2,
        "the compiler built by itself writes different code from the one built by Rust: {} and {} bytes, first difference at byte {}",
        ir_1.len(),
        ir_2.len(),
        ir_1.bytes()
            .zip(ir_2.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(ir_1.len().min(ir_2.len()))
    );
}
