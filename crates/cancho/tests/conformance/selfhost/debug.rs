//! `cli.cho build -g` (`examples/selfhost/cidebug.cho`, `docs/agent-debug.md` section 3): line tables a debugger reads, and nothing else changed.

use super::*;

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn said(command: &mut Command) -> String {
    let done = command.output().expect("the command runs");
    format!("{}{}", String::from_utf8_lossy(&done.stdout), String::from_utf8_lossy(&done.stderr))
}

/// The 1-based number of the first line of `text` that has `needle` in it.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines().position(|l| l.contains(needle)).expect("the line is there") + 1
}

const CHAIN: &str = "fn add_one(n: int) -> [] int {\n    let m = n + 1;\n    return m;\n}\n\nfn twice(n: int) -> [] int {\n    let a = add_one(n);\n    let b = add_one(a);\n    return b;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    release(io);\n    let r = twice(40);\n    if r == 42 {\n        return 0;\n    }\n    return 1;\n}\n";

/// A program built with `-g` stops where the source says, under `gdb`, with the names the programmer wrote; the code is the code of the build without it;
/// and the debug information is there (clang drops all of it, and exits 0, when one call is left without a location).
#[test]
fn a_program_built_with_g_has_line_tables() {
    if !have("gdb") || !have("llvm-dwarfdump") || !have("objdump") {
        eprintln!("skipped: needs gdb, llvm-dwarfdump and objdump");
        return;
    }
    let exe = build("cli-debug", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("g");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let hello = std::fs::read_to_string(repo_root().join("examples/hello.cho")).expect("hello");
    std::fs::write(dir.join("hello.cho"), &hello).expect("a file");
    std::fs::write(dir.join("chain.cho"), CHAIN).expect("a file");
    let cli = |arguments: &[&str]| {
        let done = Command::new(&exe).current_dir(&dir).args(arguments).output().expect("runs");
        assert_eq!(
            done.status.code(),
            Some(0),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    cli(&["build", "-g", "-o", "hello_g", "hello.cho"]);
    cli(&["build", "-g", "-o", "chain_g", "chain.cho"]);
    cli(&["build", "-o", "chain_n", "chain.cho"]);

    // Where `gdb` stops, and what it calls the frames.
    let stopped = said(Command::new("gdb").current_dir(&dir).args([
        "-batch",
        "-ex",
        "break write_all",
        "-ex",
        "run",
        "-ex",
        "bt",
        "hello_g",
    ]));
    let putchar = line_of(&hello, "putchar(io");
    let call = line_of(&hello, "written = write_all");
    assert!(stopped.contains(&format!("write_all () at hello.cho:{putchar}")), "{stopped}");
    assert!(stopped.contains(&format!("main () at hello.cho:{call}")), "{stopped}");

    // A function is at the line of its declaration, whatever the optimiser did to it.
    let located = said(Command::new("gdb").current_dir(&dir).args([
        "-batch",
        "-ex",
        "info line twice",
        "chain_g",
    ]));
    let declared = line_of(CHAIN, "fn twice");
    assert!(located.contains(&format!("Line {declared} of \"chain.cho\"")), "{located}");

    // The debug information is there for the functions the optimiser kept apart (one that was folded into the C `main` has no subprogram): a call without a location would have dropped it all.
    let info =
        said(Command::new("llvm-dwarfdump").current_dir(&dir).args(["--debug-info", "chain_g"]));
    for name in ["add_one", "twice"] {
        assert!(info.contains(&format!("DW_AT_name\t(\"{name}\")")), "{name}: {info}");
    }

    // And the code is the same: the disassembly of the build with `-g` and the one without, apart from the header that names the file.
    let code = |name: &str| {
        let text = said(Command::new("objdump").current_dir(&dir).args([
            "-d",
            "-j",
            ".text",
            "--no-show-raw-insn",
            name,
        ]));
        text.lines().skip(3).map(str::to_owned).collect::<Vec<_>>().join("\n")
    };
    let (with, without) = (code("chain_g"), code("chain_n"));
    assert!(with.lines().count() > 20, "{with}");
    assert_eq!(with, without, "-g changed the code");

    // It still runs.
    for program in ["hello_g", "chain_g", "chain_n"] {
        let ran = Command::new(dir.join(program)).output().expect("runs");
        assert_eq!(ran.status.code(), Some(0), "{program}");
    }
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}
