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
        "binops",
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
        "version",
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

/// The files of the command line in cancho, `examples/selfhost/cancho.files`: `cli.cho` and everything it imports, by the `module` line of each file (a file without one is
/// named by its stem). `scripts/build-standalone.sh` reads the list, so it must be the closure: a module added to the command line and not to the list would be built
/// by nobody. `CANCHO_REGENERATE=1` writes it.
#[test]
fn the_file_list_of_the_standalone_build_is_the_import_closure_of_cli() {
    let dir = repo_root().join("examples/selfhost");
    let mut by_module: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("the directory is read") {
        let path = entry.expect("an entry").path();
        let Some(file) = path.file_name().and_then(|f| f.to_str()).map(str::to_owned) else {
            continue;
        };
        if !file.ends_with(".cho") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a file is read");
        let module = text
            .lines()
            .find_map(|l| l.strip_prefix("module ").and_then(|m| m.strip_suffix(';')))
            .map_or_else(|| file.trim_end_matches(".cho").to_owned(), str::to_owned);
        by_module.insert(module, file);
    }
    let lookup = |m: &str| {
        by_module.get(m).or_else(|| by_module.get(m.trim_start_matches("selfhost."))).cloned()
    };
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut todo = vec!["cli".to_owned()];
    while let Some(module) = todo.pop() {
        let Some(file) = lookup(&module) else { continue };
        if !seen.insert(file.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(dir.join(&file)).expect("a file is read");
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("import ") {
                todo.push(rest.split([';', ' ']).next().unwrap_or("").to_owned());
            }
        }
    }
    let want: String = seen.iter().map(|f| format!("{f}\n")).collect();
    let list = dir.join("cancho.files");
    if std::env::var_os("CANCHO_REGENERATE").is_some() {
        std::fs::write(&list, &want).expect("the list is written");
    }
    let have = std::fs::read_to_string(&list).expect("the list is read");
    assert!(
        have == want,
        "examples/selfhost/cancho.files is not the import closure of cli.cho: run this test with CANCHO_REGENERATE=1 and commit the result"
    );
    assert!(seen.len() > 70, "only {} files in the closure", seen.len());
}

/// `scripts/build-standalone.sh` (`docs/standalone-cancho.md` D5, slice 4): the command line in cancho, stamped, is built by the Rust one (stage 1), by itself (stage 2) and
/// by that (stage 3), and stage 2 and stage 3 are the same bytes. The binary it leaves reports the stamp, builds a program and a project, and is the second stage's. The gate can fail:
/// with stage 3 written under another name (which changes one byte of the executable) the script refuses and leaves no `cancho`.
#[test]
fn the_standalone_build_reproduces_itself() {
    let (_, rev, host) = rust_stamp();
    let dir = scratch("selfhost-standalone");
    let script = repo_root().join("scripts/build-standalone.sh");
    let run = |out: &Path, name3: Option<&str>| {
        let mut command = Command::new("sh");
        command.arg(&script).arg("--out").arg(out).args([
            "--rev",
            &rev,
            "--host",
            &host,
            "--cancho0",
            BIN,
        ]);
        if let Some(name) = name3 {
            command.env("STANDALONE_STAGE3_NAME", name);
        }
        command.env("CANCHO_CACHE", dir.join("cache")).output().expect("the script runs")
    };
    let good = dir.join("good");
    let done = run(&good, None);
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    let cancho = good.join("cancho");
    let said = Command::new(&cancho).arg("--version").output().expect("the binary runs");
    let (version, _, _) = rust_stamp();
    assert_eq!(
        String::from_utf8_lossy(&said.stdout),
        format!("cancho {version} (rev {rev}, host {host})\n")
    );
    // It builds with its own embedded library, with nothing but the system around it, and the program runs.
    let program = dir.join("p.cho");
    std::fs::write(&program, "edition 7;\nimport std.math;\nfn main(world: World) -> [] int { release(world); return math.abs(0 - 4); }\n").expect("a program");
    let exe = dir.join("p");
    let built = Command::new(&cancho)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("build")
        .arg(&program)
        .args(["--std", "-o"])
        .arg(&exe)
        .output()
        .expect("the binary builds");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    assert_eq!(Command::new(&exe).status().expect("it runs").code(), Some(4));
    let stages =
        |n: u32| std::fs::read(good.join(format!("stages/{n}/cancho"))).expect("a stage is read");
    assert_eq!(stages(2), stages(3), "stages 2 and 3 differ");
    assert_eq!(std::fs::read(&cancho).expect("the result"), stages(2), "the result is not stage 2");
    // The gate can fail.
    let bad = dir.join("bad");
    let refused = run(&bad, Some("other"));
    assert_eq!(refused.status.code(), Some(1), "{}", String::from_utf8_lossy(&refused.stderr));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("does not reproduce itself"));
    assert!(!bad.join("cancho").exists(), "a failed build left a cancho");
    let _ = std::fs::remove_dir_all(&dir);
}
