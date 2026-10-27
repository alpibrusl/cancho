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
fn own_source(root_module: &str) -> String {
    let root = repo_root();
    let mut files: Vec<PathBuf> = ["math", "bytes", "buffer", "io"]
        .iter()
        .map(|m| root.join(format!("std/{m}.cho")))
        .collect();
    let own = root.join("examples/selfhost");
    for module in [
        "lexcore",
        "tables",
        "runtime",
        "kinds",
        "rules",
        "ast",
        "pass1",
        "foreign",
        "builtins",
        "signatures",
        "types",
        "prims",
        "scope",
        "effects",
        "emit",
        "consteval",
        "instances",
        "floats",
        "floatcode",
        "fold",
        "threadcode",
        "codes",
        "externcode",
        "netcode",
        "platform",
        "wordcode",
        "cryptocode",
        "ex",
        "lookup",
        "atoms",
        "filecode",
        "narrowing",
        "allocs",
        "exprs",
        "body",
        "listing",
        "checker",
        "floatprint",
        "sink",
        "filewrite",
        "formatting",
        "jsonprint",
        "jsonread",
        "ingest",
        "blake3",
        "identity",
        "printing",
        "prune",
        "testscan",
        "layoutrep",
        "authwasi",
        "authority",
        "satisfytext",
        "driver",
    ] {
        files.push(own.join(format!("{module}.cho")));
    }
    files.push(own.join(format!("{root_module}.cho")));
    let target = super::target_line();
    let mut stream = format!("TARGET {target}\n");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a source file");
        stream.push_str(&format!("FILE {}\n{text}", text.len()));
    }
    stream
}

#[test]
fn the_compiler_in_cancho_compiles_itself_to_a_fixed_point() {
    let first = build("bootstrap-1", &with_front_end("compiler.cho"));
    let source = own_source("compiler");

    let ir_1 = answer(&first, &source);
    assert!(
        !ir_1.starts_with("ERR"),
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

/// The same, with no help but a `clang` to be run: the builder (`builder.cho`) reads a program, writes its IR
/// to a file and starts `clang` on it through its `Exec`. Built by Rust it is the first builder; it builds the
/// second from its own source, and the second builds the third. All three write the same IR.
#[test]
fn the_builder_in_cancho_builds_itself_with_no_help() {
    let first = build("bootstrap-builder", &with_front_end("builder.cho"));
    let source = own_source("builder");
    let dir = scratch("selfhost-bootstrap-builder-run");
    let clang = std::env::var("CLANG").unwrap_or_else(|_| "/usr/bin/clang".to_owned());

    let run = |builder: &Path, output: &Path| {
        let mut child = Command::new(builder)
            .arg(output)
            .arg(&clang)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the builder runs");
        child
            .stdin
            .take()
            .expect("a piped stdin")
            .write_all(source.as_bytes())
            .expect("the builder reads its input");
        let done = child.wait_with_output().expect("the builder finishes");
        assert!(
            done.status.success(),
            "the builder should build its source, but said:\n{}{}",
            String::from_utf8_lossy(&done.stdout),
            String::from_utf8_lossy(&done.stderr)
        );
    };

    let (second, third) = (dir.join("second"), dir.join("third"));
    run(&first, &second);
    run(&second, &third);
    let (ir_2, ir_3) = (
        std::fs::read(dir.join("second.ll")).expect("the first builder wrote the IR"),
        std::fs::read(dir.join("third.ll")).expect("the second builder wrote the IR"),
    );
    assert!(ir_2.len() > 100_000, "the IR is only {} bytes", ir_2.len());
    assert!(
        ir_2 == ir_3,
        "the builder built by itself writes different code from the one built by Rust: {} and {} bytes",
        ir_2.len(),
        ir_3.len()
    );

    // And what it builds runs: a program of its own.
    let program = "fn main(world: World) -> [] int { release(world); return 42; }";
    let target = super::target_line();
    let stream = format!("TARGET {target}\nFILE {}\n{program}", program.len());
    let made = dir.join("made");
    let mut child = Command::new(&third)
        .arg(&made)
        .arg(&clang)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the third builder runs");
    child.stdin.take().expect("a piped stdin").write_all(stream.as_bytes()).expect("input");
    assert!(child.wait().expect("the builder finishes").success());
    let ended = Command::new(&made).output().expect("the program runs");
    assert_eq!(ended.status.code(), Some(42));
}
