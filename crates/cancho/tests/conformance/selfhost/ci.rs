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
