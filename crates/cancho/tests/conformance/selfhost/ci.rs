//! `cli.cho ci` (`examples/selfhost/clici.cho`, `docs/agent-ci.md`): the gate as one command, checked against the three commands it runs.

use super::*;

/// The answer of `ci --output json` run in `dir`: its exit status and the answer read by a strict JSON reader.
fn ci(exe: &Path, dir: &Path, arguments: &[&str]) -> (i32, serde_json::Value) {
    let done = Command::new(exe)
        .current_dir(dir)
        .arg("ci")
        .arg("--output")
        .arg("json")
        .args(arguments)
        .output()
        .expect("the command runs");
    let text = String::from_utf8_lossy(&done.stdout).into_owned();
    let answer = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("`ci --output json {arguments:?}` is not JSON ({e}): {text}"));
    (done.status.code().unwrap_or(-1), answer)
}

/// The status of the step called `name` in an answer.
fn status(answer: &serde_json::Value, name: &str) -> String {
    answer["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|s| s["name"] == name))
        .and_then(|s| s["status"].as_str())
        .unwrap_or("absent")
        .to_owned()
}

/// Every rule a step reports.
fn rules(answer: &serde_json::Value, name: &str) -> Vec<String> {
    answer["steps"]
        .as_array()
        .and_then(|steps| steps.iter().find(|s| s["name"] == name))
        .and_then(|s| s["findings"].as_array())
        .map(|f| f.iter().filter_map(|x| x["rule"].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// A passing program, a badly formatted one, a refused one, and a test that passes and one that fails: the status of every step, the rules of every finding,
/// the exit status, and that each answer is JSON a strict reader accepts.
#[test]
fn ci_in_cancho_says_what_fmt_check_and_test_say() {
    let exe = build("cli-ci", &with_front_end("cli.cho"));
    let dir = exe.parent().expect("a scratch directory").join("ci");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    // `ci` runs this command line again, by the name it was run by, and `Exec` refuses a name with `..` in it.
    let exe = exe.canonicalize().expect("a path without links");
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect("a file");
    let good = std::fs::read_to_string(repo_root().join("examples/hello.cho")).expect("hello");
    write("good.cho", &good);
    write("loose.cho", &good.replace("fn main", "fn   main"));
    write("refused.cho", "fn main(world: World) -> [] int {\n    return undefined_name;\n}\n");
    write(
        "lib.cho",
        "edition 7;\nmodule lib;\n\npub fn double(n: int) -> [] int {\n    return n * 2;\n}\n",
    );
    let test = |name: &str, want: i32| {
        write(
            name,
            &format!(
                "edition 7;\nmodule {}; \n\nimport lib;\n\npub fn test_double() -> [] int {{\n    if lib.double(2) == {want} {{\n        return 0;\n    }}\n    return 1;\n}}\n",
                name.trim_end_matches(".cho")
            ),
        );
    };
    test("t_pass.cho", 4);
    test("t_fail.cho", 5);
    let fmt = Command::new(&exe)
        .current_dir(&dir)
        .args(["fmt", "lib.cho", "t_pass.cho", "t_fail.cho", "good.cho"])
        .output()
        .expect("fmt runs");
    assert_eq!(fmt.status.code(), Some(0), "{}", String::from_utf8_lossy(&fmt.stderr));

    // Before there is a baseline, a program that checks still fails the authority step, and `--accept-authority` needs its reason.
    let (code, answer) = ci(&exe, &dir, &["good.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(rules(&answer, "authority"), ["ci-authority-baseline-missing"], "{answer}");
    let bare = Command::new(&exe)
        .current_dir(&dir)
        .args(["ci", "--accept-authority", "good.cho"])
        .output()
        .expect("runs");
    assert_eq!(bare.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bare.stderr).contains("ci-accept-needs-reason"));
    let (code, answer) = ci(&exe, &dir, &["--accept-authority", "--reason", "first", "good.cho"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(rules(&answer, "authority"), ["ci-authority-accepted"], "{answer}");

    let (code, answer) = ci(&exe, &dir, &["good.cho"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(
        (status(&answer, "fmt"), status(&answer, "check"), status(&answer, "test")),
        ("pass".into(), "pass".into(), "skipped".into())
    );

    let (code, answer) = ci(&exe, &dir, &["loose.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(answer["ok"], false);
    assert_eq!(rules(&answer, "fmt"), ["ci-not-formatted"], "{answer}");

    let (code, answer) = ci(&exe, &dir, &["refused.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(status(&answer, "check"), "fail");
    assert!(!rules(&answer, "check").is_empty(), "{answer}");
    assert_eq!(status(&answer, "test"), "skipped");

    let (code, answer) = ci(&exe, &dir, &["--test", "lib.cho", "--test", "t_pass.cho"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(
        (status(&answer, "check"), status(&answer, "test")),
        ("skipped".into(), "pass".into())
    );

    let (code, answer) = ci(&exe, &dir, &["--test", "lib.cho", "--test", "t_fail.cho"]);
    assert_eq!(code, 4, "{answer}");
    assert_eq!(rules(&answer, "test"), ["ci-test-failed"], "{answer}");

    // The first failing step's status wins: a loose file and a failing test is 1, not 4.
    let (code, answer) =
        ci(&exe, &dir, &["--test", "lib.cho", "--test", "t_fail.cho", "loose.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(status(&answer, "fmt"), "fail");

    authority_baseline(&exe, &dir, &good);

    // Nothing to do is a usage error, and a name with `..` is refused rather than reaching `Exec`.
    let none = Command::new(&exe).current_dir(&dir).arg("ci").output().expect("runs");
    assert_eq!(none.status.code(), Some(2));
    let dotted = dir.join("..").join(exe.file_name().expect("a file name"));
    let said =
        Command::new(&dotted).current_dir(&dir).args(["ci", "good.cho"]).output().expect("runs");
    assert_eq!(said.status.code(), Some(3), "{said:?}");
    assert!(String::from_utf8_lossy(&said.stderr).contains("ci-self-path"));
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// The baseline (`docs/agent-ci.md` section 3): a program that gains authority fails, one that loses it passes (and fails under `--strict`), the reason of an
/// acceptance is written beside what it admits, and a baseline that is another program's or not a baseline is a finding and not a crash.
fn authority_baseline(exe: &Path, base: &Path, hello: &str) {
    let dir = base.join("authority");
    std::fs::create_dir_all(&dir).expect("the directory is made");
    let program = dir.join("prog.cho");
    let baseline = dir.join("cancho.authority.json");
    let narrow = hello.to_owned();
    // The same program that also reads the console: one more label.
    let wide = narrow.replace(
        "fn write_all[&r, &i](io: &!i Io, s: &r [byte]) -> [io_write] int {\n    var n = 0;",
        "fn write_all[&r, &i](io: &!i Io, s: &r [byte]) -> [io_write, io_read] int {\n    var n = getchar(io) * 0;",
    );
    assert_ne!(wide, narrow, "the program to widen is not the one this test was written for");
    std::fs::write(&program, &narrow).expect("a program");

    let (code, _) = ci(exe, &dir, &["--accept-authority", "--reason", "hello", "prog.cho"]);
    assert_eq!(code, 0);
    let text = std::fs::read_to_string(&baseline).expect("a baseline");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("a baseline is JSON");
    assert_eq!(parsed["format"], 1);
    assert_eq!(parsed["program"], "prog");
    assert_eq!(parsed["accepted"].as_array().map(Vec::len), Some(0));
    assert!(text.contains("\"io_write\"") && !text.contains("io_read"), "{text}");
    let (code, answer) = ci(exe, &dir, &["prog.cho"]);
    assert_eq!(code, 0, "{answer}");

    // A widening fails, and says which fact.
    std::fs::write(&program, &wide).expect("a program");
    let (code, answer) = ci(exe, &dir, &["prog.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(rules(&answer, "authority"), ["ci-authority-widened"], "{answer}");
    assert!(answer.to_string().contains("io_read"), "{answer}");
    let before = std::fs::read_to_string(&baseline).expect("a baseline");
    assert_eq!(before, text, "a failing run must not touch the baseline");

    // Accepting it records the reason beside the label, and the run then passes.
    let (code, _) =
        ci(exe, &dir, &["--accept-authority", "--reason", "it reads the console", "prog.cho"]);
    assert_eq!(code, 0);
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&baseline).expect("a baseline"))
            .expect("a baseline is JSON");
    let accepted = after["accepted"].as_array().expect("accepted is a list");
    assert_eq!(accepted.len(), 1, "{after}");
    assert!(accepted[0]["fact"].as_str().unwrap_or("").contains("io_read"), "{after}");
    assert_eq!(accepted[0]["reason"], "it reads the console");
    assert_eq!(ci(exe, &dir, &["prog.cho"]).0, 0);

    // A narrowing passes and is said; `--strict` makes it fail. An earlier acceptance is kept when the baseline is written again.
    std::fs::write(&program, &narrow).expect("a program");
    let (code, answer) = ci(exe, &dir, &["prog.cho"]);
    assert_eq!(code, 0, "{answer}");
    assert_eq!(rules(&answer, "authority"), ["ci-authority-narrowed"], "{answer}");
    assert_eq!(ci(exe, &dir, &["--strict", "prog.cho"]).0, 1);
    let (code, _) = ci(exe, &dir, &["--accept-authority", "--reason", "back", "prog.cho"]);
    assert_eq!(code, 0);
    let kept: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&baseline).expect("a baseline"))
            .expect("a baseline is JSON");
    assert_eq!(kept["accepted"].as_array().map(Vec::len), Some(1), "{kept}");

    // Another program's baseline, and a file that is not one.
    std::fs::copy(&program, dir.join("other.cho")).expect("a copy");
    let (code, answer) = ci(exe, &dir, &["other.cho"]);
    assert_eq!(code, 1, "{answer}");
    assert_eq!(rules(&answer, "authority"), ["ci-authority-baseline-stale"], "{answer}");
    for bad in ["not json", "{}", "{\n  \"format\": 2,\n  \"program\": \"prog\"\n}", ""] {
        std::fs::write(&baseline, bad).expect("a file");
        let (code, answer) = ci(exe, &dir, &["prog.cho"]);
        assert_eq!(code, 1, "{bad:?}: {answer}");
        assert_eq!(rules(&answer, "authority"), ["ci-baseline-unreadable"], "{bad:?}: {answer}");
        // It is not overwritten by an acceptance either.
        let (code, _) = ci(exe, &dir, &["--accept-authority", "--reason", "x", "prog.cho"]);
        assert_eq!(code, 1, "{bad:?}");
        assert_eq!(std::fs::read_to_string(&baseline).expect("a baseline"), bad);
    }
}
