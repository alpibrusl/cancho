//! The tests of the command line in cancho (`examples/selfhost/cli.cho`, `docs/cli-in-cancho.md`): each builds it and runs it against the Rust
//! command it stands in for, or against the oracles of the stand-alone ports.

use super::*;

/// What `cli.cho fmt` does to one file: its standard output, standard error and exit status, and the file afterwards.
fn cli_fmt(exe: &Path, dir: &Path, text: &str, check: bool) -> (String, String, i32, String) {
    std::fs::write(dir.join("x.cho"), text).expect("the file is written");
    let mut command = Command::new(exe);
    command.current_dir(dir).arg("fmt");
    if check {
        command.arg("--check");
    }
    let run = command.arg("x.cho").output().expect("the command line runs");
    (
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
        run.status.code().unwrap_or(-1),
        std::fs::read_to_string(dir.join("x.cho")).expect("the file is read"),
    )
}

/// `examples/selfhost/cli.cho fmt` against `cancho fmt` (`cancho_syntax::format`, and the words `fmt_cli.rs` says): the repository's programs, the
/// same programs spoilt, and `FORMAT_EDGE`, each as a file, with and without `--check`: the output, the exit status, and the file afterwards.
#[test]
fn the_command_line_in_cancho_formats_like_cancho_fmt() {
    let exe = build("cli", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("files");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let mut programs: Vec<(String, String)> = sources();
    programs.extend(FORMAT_EDGE.iter().map(|s| (format!("edge case {s:.40?}"), (*s).to_owned())));
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut spoilt = Vec::new();
    for (n, (name, text)) in programs.iter().enumerate() {
        if ast_oracle::printed(text).starts_with("ERR") {
            continue;
        }
        for how in [n % 7, (n + 3) % 7] {
            spoilt.push((format!("{name} spoilt {how}"), spoil(text, how, &mut seed)));
        }
    }
    programs.extend(spoilt);
    let (mut different, mut formatted, mut refused, mut unchanged) = (Vec::new(), 0, 0, 0);
    for (name, text) in &programs {
        // A file that does not parse is refused by both, in different words (the rule and the span, or the diagnostic).
        if ast_oracle::printed(text).starts_with("ERR") {
            continue;
        }
        let want = ast_oracle::formatted(text);
        let expected = if let Some(why) = want.strip_prefix("CANNOT ") {
            refused += 1;
            (String::new(), format!("x.cho: not formatted: {}\n", why.trim_end()), 1, text.clone())
        } else if want == *text {
            unchanged += 1;
            (String::new(), String::new(), 0, text.clone())
        } else {
            formatted += 1;
            ("formatted x.cho\n".to_owned(), String::new(), 0, want.clone())
        };
        if cli_fmt(&exe, &dir, text, false) != expected {
            different.push(name.clone());
        }
        // Under `--check` nothing is written, and a file that would change is reported and makes the status 1.
        let checked = if want == *text || want.starts_with("CANNOT ") {
            expected
        } else {
            ("would reformat x.cho\n".to_owned(), String::new(), 1, text.clone())
        };
        if cli_fmt(&exe, &dir, text, true) != checked {
            different.push(format!("{name} (--check)"));
        }
    }
    assert!(
        different.is_empty(),
        "the command line in cancho and `cancho fmt` disagree about {} of {} runs:\n{}",
        different.len(),
        2 * programs.len(),
        different.join("\n")
    );
    eprintln!(
        "formatted {formatted}, refused {refused}, unchanged {unchanged}, of {}",
        programs.len()
    );
    assert!(formatted > 400, "only {formatted} programs were changed by the formatter");
    assert!(refused >= 10, "only {refused} programs were refused by the formatter");
    assert!(unchanged > 300, "only {unchanged} programs were already formatted");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// Run a command line, from `dir`, on `arguments`: its standard output, its exit status, and every file beneath `dir` afterwards (sorted).
fn run_in(exe: &Path, dir: &Path, arguments: &[&str]) -> (String, i32, Vec<(String, String)>) {
    let run =
        Command::new(exe).current_dir(dir).args(arguments).output().expect("the command runs");
    let mut files = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).expect("a directory is read") {
            let path = entry.expect("an entry").path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                files.push((
                    path.strip_prefix(dir).expect("beneath dir").display().to_string(),
                    text,
                ));
            }
        }
    }
    files.sort();
    (String::from_utf8_lossy(&run.stdout).into_owned(), run.status.code().unwrap_or(-1), files)
}

/// `cli.cho fmt` on directories, against `cancho fmt`: the order the files are found in, the names that are skipped (a dot at the front, a file
/// that is not `.cho`), a directory called `x.cho`, a link to a directory, a file that does not parse, and the repository's own directories.
#[test]
fn the_command_line_in_cancho_walks_directories_like_cancho_fmt() {
    let exe = build("cli-walk", &with_front_end("cli.cho"));
    let base = exe.parent().expect("a scratch directory").to_owned();
    let loose = "fn   main() -> [] int {\nreturn 0;\n}\n";
    let tree = |name: &str| -> PathBuf {
        let root = base.join(name);
        for dir in ["A/sub", "A/.hidden", "B/x.cho", "C", "D"] {
            std::fs::create_dir_all(root.join(dir)).expect("a directory is made");
        }
        for file in [
            "A/z.cho",
            "A/a.cho",
            "A/sub/m.cho",
            "A/.hidden/h.cho",
            "A/.dot.cho",
            "B/x.cho/in.cho",
            "B/notes.txt",
            "C/Q.CHO",
            "top.cho",
            "A/Zed.cho",
            "A/b-1.cho",
            "A/b_1.cho",
        ] {
            std::fs::write(root.join(file), loose).expect("a file is written");
        }
        std::fs::write(root.join("A/bad.cho"), "fn broken(").expect("a file is written");
        std::fs::write(root.join("A/ok.cho"), "fn f() -> [] int {\n    return 0;\n}\n")
            .expect("a file is written");
        std::os::unix::fs::symlink("A", root.join("link")).expect("a link is made");
        root
    };
    let (rust, mine) = (tree("rust"), tree("cancho"));
    let rust_binary = PathBuf::from(BIN);
    for arguments in [
        vec!["fmt", "--check", "A", "B", "C", "D", "top.cho", "."],
        vec!["fmt", "--check", "A/", "./B"],
        vec!["fmt", "D"],
        vec!["fmt", "A", "B", "C", "top.cho", "link"],
    ] {
        let want = run_in(&rust_binary, &rust, &arguments);
        let got = run_in(&exe, &mine, &arguments);
        assert_eq!(
            got,
            want,
            "`cancho {}` and the command line in cancho disagree",
            arguments.join(" ")
        );
    }
    // The repository's own directories, in the order they are found: the files `--check` reports are the same, and in the same order.
    let root = repo_root();
    let sample = ["std", "examples/selfhost", "packages", "tests/accept"];
    let check = |binary: &Path| {
        let run = Command::new(binary)
            .current_dir(&root)
            .arg("fmt")
            .arg("--check")
            .args(sample)
            .output()
            .expect("the command runs");
        (String::from_utf8_lossy(&run.stdout).into_owned(), run.status.code())
    };
    assert_eq!(check(&exe), check(&rust_binary));
    let _ = std::fs::remove_dir_all(&base);
}

/// `cli.cho print`, `print --output json` and `ingest`, against the oracles of the stand-alone ports (the printer, the JSON printer and the
/// ingester of the Rust front end): the repository's programs and the edge programs, each printed both ways, and the JSON of each, and the
/// inputs the ingester is tried on, ingested. A program the parser refuses is reported on the standard error (`x.cho: ERR rule from to`) and
/// makes the status 1; an ingest that is refused gives the same line on the standard output.
#[test]
fn the_command_line_in_cancho_prints_and_ingests_like_the_rust_front_end() {
    let exe = build("cli-print", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("files");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let run = |name: &str, text: &str, arguments: &[&str]| -> (String, String, i32) {
        std::fs::write(dir.join(name), text).expect("the file is written");
        let done = Command::new(&exe)
            .current_dir(&dir)
            .args(arguments)
            .output()
            .expect("the command line runs");
        (
            String::from_utf8_lossy(&done.stdout).into_owned(),
            String::from_utf8_lossy(&done.stderr).into_owned(),
            done.status.code().unwrap_or(-1),
        )
    };
    let mut programs = sources();
    programs.extend(PRINT_EDGE.iter().map(|s| (format!("print edge {s:.40?}"), (*s).to_owned())));
    programs.extend(JSON_EDGE.iter().map(|s| (format!("json edge {s:.40?}"), (*s).to_owned())));
    let (mut different, mut printed_ok, mut refused, mut ingested_ok) = (Vec::new(), 0, 0, 0);
    let mut inputs: Vec<(String, String)> = Vec::new();
    for (name, text) in &programs {
        for (form, want, arguments) in [
            ("text", ast_oracle::printed(text), vec!["print", "x.cho"]),
            ("json", ast_oracle::printed_json(text), vec!["print", "--output", "json", "x.cho"]),
        ] {
            let expected = if want.starts_with("ERR") {
                refused += 1;
                (String::new(), format!("x.cho: {want}"), 1)
            } else {
                printed_ok += 1;
                (want.clone(), String::new(), 0)
            };
            if run("x.cho", text, &arguments) != expected {
                different.push(format!("{name} (print, {form})"));
            }
        }
        let json = ast_oracle::printed_json(text);
        if !json.starts_with("ERR") {
            inputs.push((format!("{name} as json"), json));
        }
    }
    inputs.extend(INGEST_EDGE.iter().map(|s| (format!("ingest edge {s:.50?}"), (*s).to_owned())));
    for (name, json) in &inputs {
        let want = ast_oracle::ingested(json);
        let status = i32::from(want.starts_with("ERR"));
        ingested_ok += 1 - status;
        if run("y.json", json, &["ingest", "y.json"]) != (want, String::new(), status) {
            different.push(format!("{name} (ingest)"));
        }
    }
    assert!(
        different.is_empty(),
        "the command line in cancho and the Rust front end disagree about {} runs:\n{}",
        different.len(),
        different.join("\n")
    );
    eprintln!(
        "printed {printed_ok}, refused {refused}, ingested {ingested_ok}, of {} programs and {} inputs",
        programs.len(),
        inputs.len()
    );
    assert!(printed_ok > 800, "only {printed_ok} prints succeeded");
    assert!(refused >= 10, "only {refused} programs were refused");
    assert!(ingested_ok > 400, "only {ingested_ok} ingests succeeded");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// `cli.cho agent-guidelines` against the Rust binary: the same bytes (the text of `AGENTS.md`), status 0; an argument is a usage error.
#[test]
fn the_command_line_in_cancho_writes_the_agent_guidelines() {
    let exe = build("cli-guidelines", &with_front_end("cli.cho"));
    let want = Command::new(BIN).arg("agent-guidelines").output().expect("cancho runs");
    let got = Command::new(&exe).arg("agent-guidelines").output().expect("the command line runs");
    assert!(want.status.success());
    assert_eq!(got.status.code(), Some(0));
    assert!(
        got.stdout == want.stdout,
        "the guidelines differ ({} bytes against {})",
        got.stdout.len(),
        want.stdout.len()
    );
    assert!(got.stdout == std::fs::read(repo_root().join("AGENTS.md")).expect("AGENTS.md is read"));
    let extra =
        Command::new(&exe).args(["agent-guidelines", "x"]).output().expect("the command line runs");
    assert_eq!(extra.status.code(), Some(2));
    assert!(extra.stdout.is_empty());
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// `cli.cho --help`, `introspect` and `skill`, against the Rust binary: every form of the three, the path `skill` writes to, and the command lines
/// that are refused (the standard output, the standard error, the status, and the file written). The Rust binary also writes a `.cli/`
/// directory beside where it is run, which the command in cancho does not.
#[test]
fn the_command_line_in_cancho_answers_help_and_the_command_tree_like_cancho() {
    let exe = build("cli-tree", &with_front_end("cli.cho"));
    let run = |binary: &Path, arguments: &[&str]| {
        let dir = scratch("selfhost-tree-run");
        let _ = std::fs::remove_file(dir.join("out.md"));
        let done = Command::new(binary)
            .current_dir(&dir)
            .args(arguments)
            .output()
            .expect("the command runs");
        (
            String::from_utf8_lossy(&done.stdout).into_owned(),
            String::from_utf8_lossy(&done.stderr).into_owned(),
            done.status.code(),
            std::fs::read_to_string(dir.join("out.md")).ok(),
        )
    };
    let lines: &[&[&str]] = &[
        &["--help"],
        &["-h"],
        &["help"],
        &["introspect"],
        &["introspect", "--output", "json"],
        &["introspect", "--output=table"],
        &["introspect", "--output", "text"],
        &["introspect", "extra", "--output=json"],
        &["skill"],
        &["skill", "--output", "json"],
        &["skill", "--output=table"],
        &["skill", "out.md"],
        &["skill", "--output", "json", "out.md"],
        &["skill", "--output", "table", "out.md"],
        &["skill", "out.md", "--output", "text"],
        &["skill", "--output", "json", "a\"b\\c.md"],
        &["introspect", "--output"],
        &["introspect", "--output", "bogus"],
        &["skill", "--output=bogus"],
        &["skill", "--output"],
    ];
    let mut different = Vec::new();
    for arguments in lines {
        if run(&exe, arguments) != run(Path::new(BIN), arguments) {
            different.push(arguments.join(" "));
        }
    }
    assert!(
        different.is_empty(),
        "the command line in cancho and `cancho` disagree about:\n{}",
        different.join("\n")
    );
    // Two things the Rust command does that this one does not: it makes the directories above the file (the standard library has no
    // `mkdir`), and it says a skill file was written when it could not write it (the ACLI library drops the error). Here a file that cannot
    // be written is a status 3 and nothing on the standard output.
    let dir = scratch("selfhost-tree-lost");
    std::fs::write(dir.join("blocker"), "").expect("a file is written");
    let lost = Command::new(&exe)
        .current_dir(&dir)
        .args(["skill", "blocker/out.md"])
        .output()
        .expect("the command runs");
    assert_eq!(lost.status.code(), Some(3));
    assert!(lost.stdout.is_empty());
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// What `check --output json` answers for `files` (run in `dir`): the status and the first refusal as (rule, file, line, column), or none.
type Checked = (Option<i32>, Option<(String, String, u64, u64)>, String);

fn checked(binary: &Path, dir: &Path, flags: &[&str], files: &[&str]) -> Checked {
    let done = Command::new(binary)
        .current_dir(dir)
        .args(["check", "--output", "json"])
        .args(flags)
        .args(files)
        .output()
        .expect("check runs");
    let stdout = String::from_utf8_lossy(&done.stdout).into_owned();
    let first = serde_json::from_str::<serde_json::Value>(&stdout).ok().and_then(|v| {
        let first = v["refused"].get(0)?.clone();
        let position = &first["position"];
        Some((
            first["rule"].as_str()?.to_owned(),
            position["file"].as_str().unwrap_or("").to_owned(),
            position["line"].as_u64().unwrap_or(0),
            position["column"].as_u64().unwrap_or(0),
        ))
    });
    (done.status.code(), first, stdout)
}

/// `cli.cho check` against `cancho check --output json`, on the repository's programs and on programs of several files. The sentence in
/// `message` is the rule's, not the Rust command's per-site words (decided in `docs/cli-in-cancho.md` section 12), and the checker in cancho
/// stops at the first refusal, so what is compared is the status and the first refusal: its rule and its file are compared, and its line and column are counted where they differ (the checker in cancho and the Rust one name another span for some capability rules, or find another refusal first).
#[test]
fn the_command_line_in_cancho_checks_like_cancho_check() {
    let exe = build("cli-check", &with_front_end("cli.cho"));
    let dir = scratch("selfhost-check-run");
    let host = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux"
    };
    let mut cases: Vec<(String, Vec<(String, String)>)> = sources()
        .into_iter()
        .map(|(name, text)| (name, vec![("x.cho".to_owned(), text)]))
        .collect();
    for (n, group) in SEVERAL_FILES.iter().enumerate() {
        let files =
            group.iter().enumerate().map(|(k, t)| (format!("f{k}.cho"), (*t).to_owned())).collect();
        cases.push((format!("several files {n}"), files));
    }
    // The programs that Rust refuses and the checker in cancho does not (yet), and the programs it cannot judge.
    let (mut accepted, mut agreed, mut column, mut cannot, mut missed) =
        (0, 0, Vec::new(), Vec::new(), Vec::new());
    let (mut wrong, mut texts) = (Vec::new(), Vec::new());
    for (name, files) in &cases {
        for (path, text) in files {
            std::fs::write(dir.join(path), text).expect("a file is written");
        }
        let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let want = checked(Path::new(BIN), &dir, &[], &names);
        let got = checked(&exe, &dir, &["--host", host], &names);
        match (&want.1, &got) {
            _ if got.0 == Some(3) => cannot.push(name.clone()),
            (None, (Some(0), None, text)) if text == &want.2 => accepted += 1,
            (Some(w), (Some(1), Some(g), _)) if w.0 == g.0 && w.1 == g.1 => {
                if w.2 == g.2 && w.3 == g.3 {
                    agreed += 1;
                    // The text of the refusal: the place, then the line of the source and the caret under it, which are the same
                    // whatever the sentence is.
                    if !w.1.is_empty() {
                        let text = |binary: &Path, flags: &[&str]| -> Vec<String> {
                            let done = Command::new(binary)
                                .current_dir(&dir)
                                .arg("check")
                                .args(flags)
                                .args(&names)
                                .output()
                                .expect("check runs");
                            let stderr = String::from_utf8_lossy(&done.stderr).into_owned();
                            let first = stderr
                                .split("\n\n")
                                .next()
                                .unwrap_or("")
                                .trim_end()
                                .lines()
                                .map(str::to_owned)
                                .collect::<Vec<_>>();
                            let head = first[0].split(": error: ").next().unwrap_or("").to_owned();
                            vec![
                                head,
                                first[first.len() - 2].clone(),
                                first[first.len() - 1].clone(),
                            ]
                        };
                        if text(Path::new(BIN), &[]) != text(&exe, &["--host", host]) {
                            texts.push(name.clone());
                        }
                    }
                } else {
                    column.push(name.clone());
                }
            }
            (Some(_), (Some(0), None, _)) => missed.push(name.clone()),
            // A refusal about the program has no place in the Rust answer: only the rule and the status are compared.
            (Some(w), (Some(1), Some(g), _)) if w.1.is_empty() && w.0 == g.0 => agreed += 1,
            _ => wrong.push(format!("{name}: {:?} against {:?}", want.1, got.1)),
        }
    }
    assert!(
        wrong.is_empty(),
        "`check` in cancho and `cancho check` disagree about {} programs:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "accepted {accepted}, refused alike {agreed}, a different place {}, cannot judge {}, refusal missed {}, of {} programs",
        column.len(),
        cannot.len(),
        missed.len(),
        cases.len()
    );
    assert_eq!(
        missed,
        ["examples/tls_nb/gaps/g17_conn_payload.cho".to_owned()]
            .map(|p| repo_root().join(p).display().to_string()),
        "the refusals the checker in cancho misses"
    );
    assert!(
        accepted > 60 && agreed > 600,
        "the comparison covered {accepted} accepted and {agreed} refused programs"
    );
    assert!(
        column.len() * 20 < cases.len(),
        "the place of the first refusal differs in {} programs: {column:?}",
        column.len()
    );
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// `cli.cho ids` against `cancho ids`: the lines of identities, byte for byte, for the repository's programs and the programs of several
/// files, and, for a tenth of them, with the standard library (`--std` of the Rust command, `--std-dir std` of this one, which does not carry
/// the library: the files are read, in the order `--std` adds them, from the directory). A refused program is a status 1 and no lines. The checker
/// in cancho does not make every refusal (`examples/tls_nb/gaps/g17_conn_payload.cho`), and a body it has no code for is let through.
#[test]
fn the_command_line_in_cancho_writes_the_identities_like_cancho_ids() {
    let exe = build("cli-ids", &with_front_end("cli.cho"));
    let dir = scratch("selfhost-ids-run");
    let std_dir = repo_root().join("std");
    let std_dir = std_dir.to_str().expect("a path");
    let host = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux"
    };
    let mut cases: Vec<(String, Vec<(String, String)>)> = sources()
        .into_iter()
        .map(|(name, text)| (name, vec![("x.cho".to_owned(), text)]))
        .collect();
    for (n, group) in SEVERAL_FILES.iter().enumerate() {
        let files =
            group.iter().enumerate().map(|(k, t)| (format!("f{k}.cho"), (*t).to_owned())).collect();
        cases.push((format!("several files {n}"), files));
    }
    let run = |binary: &Path, flags: &[&str], names: &[&str]| -> (Option<i32>, String) {
        let done = Command::new(binary)
            .current_dir(&dir)
            .arg("ids")
            .args(flags)
            .args(names)
            .output()
            .expect("ids runs");
        (done.status.code(), String::from_utf8_lossy(&done.stdout).into_owned())
    };
    let (mut different, mut written, mut refused, mut with_std, mut missed) =
        (Vec::new(), 0, 0, 0, Vec::new());
    for (n, (name, files)) in cases.iter().enumerate() {
        for (path, text) in files {
            std::fs::write(dir.join(path), text).expect("a file is written");
        }
        let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let mut variants = vec![(vec![], vec!["--host", host])];
        if n % 10 == 0 {
            variants.push((vec!["--std"], vec!["--host", host, "--std-dir", std_dir]));
        }
        for (theirs, ours) in variants {
            let want = run(Path::new(BIN), &theirs, &names);
            let got = run(&exe, &ours, &names);
            if want.0 == Some(1) && got.0 == Some(0) {
                missed.push(name.clone());
            } else if want != (got.0, got.1.clone())
                && !(want.0 == Some(1) && got.0 == Some(1) && want.1 == got.1)
            {
                different.push(format!("{name} {theirs:?}"));
            } else if want.0 == Some(0) {
                written += 1;
                with_std += usize::from(!theirs.is_empty());
            } else {
                refused += 1;
            }
        }
    }
    assert!(
        different.is_empty(),
        "`ids` in cancho and `cancho ids` disagree about {} runs:\n{}",
        different.len(),
        different.join("\n")
    );
    eprintln!(
        "written {written} (with the library {with_std}), refused {refused}, refusal missed {}",
        missed.len()
    );
    // The refusals the checker in cancho does not make, or has no judgement of (a body it has no code for is let through): the program with
    // a `spawn` of a connection, one reference that escapes through inference, a tuple index that is no name, and a program of several files.
    let mut expected: Vec<String> = [
        "examples/tls_nb/gaps/g17_conn_payload.cho",
        "tests/reject/reference_escapes_via_inference.cho",
    ]
    .iter()
    .map(|p| repo_root().join(p).display().to_string())
    .collect();
    expected.push("edge case \"fn f() -> [] int { return t.0.1.4294967295; }\"".to_owned());
    expected.push("several files 39".to_owned());
    assert_eq!(missed, expected, "the refusals the checker in cancho does not make");
    assert!(
        written > 100 && with_std > 20,
        "only {written} runs wrote identities, {with_std} with the library"
    );
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// Run `exe` with no input for at most five seconds: its status and standard output, or `None` if it did not finish.
fn run_for(exe: &Path, dir: &Path) -> Option<(Option<i32>, Vec<u8>)> {
    let mut child = Command::new(exe)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the program runs");
    let start = std::time::Instant::now();
    loop {
        if child.try_wait().expect("the program is waited for").is_some() {
            let done = child.wait_with_output().expect("its output is read");
            return Some((done.status.code(), done.stdout));
        }
        if start.elapsed() > std::time::Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// `cli.cho build` against `cancho build`: the programs of the repository that the Rust command builds without the standard library and
/// that finish by themselves are built by both and run, and what they write and the status they end with are compared (the programs are not
/// the same bytes: the compilers are not). A program the compiler in cancho has no code for is a status 3. The file `-o` names is the only
/// file left behind, `--emit obj` makes an object file, and a refused program makes nothing.
#[test]
fn the_command_line_in_cancho_builds_like_cancho_build() {
    let exe = build("cli-build", &with_front_end("cli.cho"));
    let dir = scratch("selfhost-build-run");
    let (mut same, mut cannot, mut different, mut built) = (0, Vec::new(), Vec::new(), 0);
    for (name, text) in sources() {
        if name.contains("/selfhost/") || name.contains("/reject/") {
            continue;
        }
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the directory is made");
        std::fs::write(dir.join("x.cho"), &text).expect("a file is written");
        let theirs = Command::new(BIN)
            .current_dir(&dir)
            .args(["build", "x.cho", "-o", "rust.out"])
            .output()
            .expect("cancho runs");
        if !theirs.status.success() {
            continue;
        }
        let Some(want) = run_for(&dir.join("rust.out"), &dir) else { continue };
        built += 1;
        let mine = Command::new(&exe)
            .current_dir(&dir)
            .args(["build", "x.cho", "-o", "mine.out"])
            .output()
            .expect("the command line runs");
        match mine.status.code() {
            Some(3) => {
                cannot.push(name.clone());
                assert!(
                    !dir.join("mine.out").exists(),
                    "{name}: a program that cannot be judged made a file"
                );
            }
            Some(0) => {
                let got = run_for(&dir.join("mine.out"), &dir);
                if got.as_ref() == Some(&want) {
                    same += 1;
                } else {
                    different.push(name.clone());
                }
                let mut left: Vec<String> = std::fs::read_dir(&dir)
                    .expect("a directory is read")
                    .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
                    .collect();
                left.sort();
                assert_eq!(
                    left,
                    ["mine.out", "rust.out", "x.cho"],
                    "{name}: what the build left in the directory"
                );
            }
            other => different.push(format!(
                "{name}: status {other:?}: {}",
                String::from_utf8_lossy(&mine.stderr)
            )),
        }
    }
    eprintln!(
        "built by both {built}: the same {same}, cannot be judged {}, different {}",
        cannot.len(),
        different.len()
    );
    assert!(
        different.is_empty(),
        "the programs built by the command line in cancho behave differently from the Rust ones:\n{}",
        different.join("\n")
    );
    assert!(same > 60, "only {same} programs were built and run by both");
    // The options.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the directory is made");
    std::fs::write(
        dir.join("h.cho"),
        std::fs::read_to_string(repo_root().join("examples/hello.cho")).expect("hello is read"),
    )
    .expect("a file is written");
    let run = |arguments: &[&str]| {
        Command::new(&exe)
            .current_dir(&dir)
            .arg("build")
            .args(arguments)
            .output()
            .expect("the command line runs")
    };
    assert_eq!(run(&["h.cho"]).status.code(), Some(0));
    assert!(dir.join("h").exists(), "the default name is the stem of the first file");
    assert_eq!(run(&["--emit", "obj", "h.cho"]).status.code(), Some(0));
    let object = std::fs::read(dir.join("h.o")).expect("an object file");
    assert!(
        object.starts_with(b"\x7fELF") || object.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]),
        "h.o is an object file"
    );
    assert_eq!(run(&["-o", "named", "-l", "m", "-L", "/usr/lib", "h.cho"]).status.code(), Some(0));
    assert!(dir.join("named").exists());
    let left: Vec<String> = std::fs::read_dir(&dir)
        .expect("a directory is read")
        .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        left.iter().all(|n| ["h.cho", "h", "h.o", "named"].contains(&n.as_str())),
        "left over: {left:?}"
    );
    // A refused program makes nothing, and says why; a bad command line is a status 2.
    std::fs::write(dir.join("bad.cho"), "fn main(world: World) -> [] int { return x; }")
        .expect("a file is written");
    let refused = run(&["bad.cho"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(!dir.join("bad").exists() && !dir.join("bad.ll").exists());
    assert_eq!(run(&[]).status.code(), Some(2));
    assert_eq!(run(&["--emit", "wasm", "h.cho"]).status.code(), Some(2));
    assert_eq!(run(&["h.cho", "-o"]).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}
