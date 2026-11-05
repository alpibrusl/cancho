//! `cli.cho trace` (`examples/selfhost/clitrace.cho`, `docs/agent-debug.md`): one run under a debugger, one answer.

use super::*;

const BOOM: &str = "fn grow(n: int) -> [] int {\n    var x = 1;\n    var i = 0;\n    while i < n {\n        x = x * 3;\n        i = i + 1;\n    }\n    return x;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    release(io);\n    let r = grow(100);\n    if r > 0 {\n        return 0;\n    }\n    return 1;\n}\n";

fn line_of(text: &str, needle: &str) -> u64 {
    text.lines().position(|l| l.contains(needle)).expect("the line is there") as u64 + 1
}

fn trace(exe: &Path, dir: &Path, arguments: &[&str]) -> (i32, Option<serde_json::Value>, String) {
    let done =
        Command::new(exe).current_dir(dir).arg("trace").args(arguments).output().expect("runs");
    let out = String::from_utf8_lossy(&done.stdout).into_owned();
    (
        done.status.code().unwrap_or(-1),
        serde_json::from_str(&out).ok(),
        String::from_utf8_lossy(&done.stderr).into_owned(),
    )
}

#[test]
fn trace_in_cancho_answers_where_a_program_stopped_and_how() {
    if Command::new("gdb").arg("--version").output().is_err()
        || Command::new("timeout").arg("--version").output().is_err()
    {
        eprintln!("skipped: needs gdb and timeout");
        return;
    }
    let exe = build("cli-trace", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("t");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    // `trace` runs this command line again by the name it was run by, which `Exec` refuses with `..` in it.
    let exe = exe.canonicalize().expect("a path");
    let hello = std::fs::read_to_string(repo_root().join("examples/hello.cho")).expect("hello");
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect("a file");
    write("hello.cho", &hello);
    write("boom.cho", BOOM);

    // A breakpoint at a line: where it stopped, and the stack of the program's own functions, by the names and lines of the source.
    let putchar = line_of(&hello, "putchar(io");
    let call = line_of(&hello, "written = write_all");
    let place = format!("hello.cho:{putchar}");
    let (code, answer, said) = trace(&exe, &dir, &["--at", &place, "hello.cho"]);
    assert_eq!(code, 0, "{said}");
    let answer = answer.expect("JSON");
    assert_eq!(answer["stopped"], "breakpoint");
    assert_eq!(answer["optimization"], "O0");
    assert_eq!(
        answer["at"],
        serde_json::json!({"function": "write_all", "file": "hello.cho", "line": putchar})
    );
    assert_eq!(answer["stack"][0], answer["at"]);
    assert_eq!(
        answer["stack"][1],
        serde_json::json!({"function": "main", "file": "hello.cho", "line": call})
    );
    assert_eq!(
        answer["stack"].as_array().map(Vec::len),
        Some(2),
        "frames of the C library and of the entry are not the program's: {answer}"
    );
    assert!(answer["exit_status"].is_null());
    let (_, one, _) = trace(&exe, &dir, &["--at", &place, "--max-frames", "1", "hello.cho"]);
    assert_eq!(one.expect("JSON")["stack"].as_array().map(Vec::len), Some(1));

    // A trap is an answer (exit 0), with the signal and the line of the statement that trapped.
    let (code, trap, said) = trace(&exe, &dir, &["boom.cho"]);
    assert_eq!(code, 0, "{said}");
    let trap = trap.expect("JSON");
    assert_eq!(
        (trap["stopped"].as_str(), trap["signal"].as_str()),
        (Some("trap"), Some("SIGILL")),
        "{trap}"
    );
    assert_eq!(trap["at"]["function"], "grow");
    assert_eq!(trap["at"]["line"], line_of(BOOM, "x = x * 3"));
    assert_eq!(trap["stack"][1]["function"], "main");

    // No place asked for and no trap: the program ends, with its status.
    let (code, ended, _) = trace(&exe, &dir, &["hello.cho"]);
    assert_eq!(code, 0);
    let ended = ended.expect("JSON");
    assert_eq!((ended["stopped"].as_str(), ended["exit_status"].as_u64()), (Some("exit"), Some(0)));
    write(
        "three.cho",
        &hello.replace(
            "if written == 14 {\n        return 0;",
            "if written == 14 {\n        return 3;",
        ),
    );
    let (code, three, _) = trace(&exe, &dir, &["three.cho"]);
    assert_eq!(code, 0);
    assert_eq!(three.expect("JSON")["exit_status"], 3);

    // A place that is never reached is a finding, not an answer; a bad place is a usage error; a refused program is refused.
    let (code, none, said) = trace(&exe, &dir, &["--at", "hello.cho:9999", "hello.cho"]);
    assert_eq!((code, none.is_none()), (1, true));
    assert!(said.contains("trace-not-reached"), "{said}");
    for bad in ["hello.cho", "hello.cho:", ":3", "hello.cho:x", "hello.cho:3x"] {
        assert_eq!(trace(&exe, &dir, &["--at", bad, "hello.cho"]).0, 2, "{bad}");
    }
    assert_eq!(trace(&exe, &dir, &[]).0, 2);
    assert_eq!(trace(&exe, &dir, &["--timeout", "0", "hello.cho"]).0, 2);
    write("refused.cho", "fn main(world: World) -> [] int {\n    return undefined_name;\n}\n");
    assert_eq!(trace(&exe, &dir, &["refused.cho"]).0, 1);

    // A program that does not stop is stopped after the time allowed.
    write(
        "spin.cho",
        &BOOM.replace("x = x * 3;", "x = x + 0;").replace("i = i + 1;", "i = i + 0;"),
    );
    let started = std::time::Instant::now();
    let (code, none, said) = trace(&exe, &dir, &["--timeout", "2", "spin.cho"]);
    assert_eq!((code, none.is_none()), (1, true), "{said}");
    assert!(said.contains("trace-timeout"), "{said}");
    assert!(started.elapsed().as_secs() < 60);

    // Nothing is left beside the program.
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .expect("a directory")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.contains(".cancho-trace"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}
