//! `docs/satisfy.md` (#406): the spec-to-implementation loop, through the
//! compiled binary, because the claim is about the CLI surface the loop's
//! participants actually use — a contract author, a candidate author,
//! and a caller reading the verdict.

use super::*;

const CONTRACT: &str = "fn test_double_doubles() -> [] int {\n    if double(3) != 6 { return 1; }\n    if double(0) != 0 { return 1; }\n    if double(-4) != -8 { return 1; }\n    return 0;\n}\n";

const GOOD_CANDIDATE: &str = "pub fn double(n: int) -> [] int { return n + n; }\n";

const WRONG_BODY: &str = "pub fn double(n: int) -> [] int { return n; }\n";

/// A candidate that checks and passes answers `satisfied`, exit 0, with
/// the recomputed `SigId` — the identity a caller takes away.
#[test]
fn a_satisfying_candidate_answers_satisfied_and_the_sig_id() {
    let dir = scratch("satisfy-good");
    let contract = dir.join("c.cho");
    let candidate = dir.join("k.cho");
    std::fs::write(&contract, CONTRACT).expect("a writable fixture");
    std::fs::write(&candidate, GOOD_CANDIDATE).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["satisfy".as_ref(), contract.as_os_str(), candidate.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert_eq!(out.status.code(), Some(0), "satisfaction is exit 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("satisfied"), "the verdict is named:\n{text}");
    assert!(text.contains("sig_id "), "the recomputed identity is part of the verdict:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A candidate with the right signature and the wrong body passes the
/// checker and fails the tests: exit 4, the same code `test` answers,
/// because satisfy is check plus test composed.
#[test]
fn a_wrong_body_fails_the_tests_with_tests_exit_code() {
    let dir = scratch("satisfy-wrong-body");
    let contract = dir.join("c.cho");
    let candidate = dir.join("k.cho");
    std::fs::write(&contract, CONTRACT).expect("a writable fixture");
    std::fs::write(&candidate, WRONG_BODY).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["satisfy".as_ref(), contract.as_os_str(), candidate.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert_eq!(out.status.code(), Some(4), "a failed test is exit 4, as `test` answers");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("not satisfied"), "the verdict is named:\n{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A candidate that does not check against the contract's calls is
/// refused by the ordinary checker before anything runs: exit 1,
/// the refusal vocabulary, located.
#[test]
fn a_candidate_that_does_not_check_is_a_refusal_not_a_run() {
    let dir = scratch("satisfy-refused");
    let contract = dir.join("c.cho");
    let candidate = dir.join("k.cho");
    std::fs::write(&contract, CONTRACT).expect("a writable fixture");
    // The contract calls `double` with one `int`; this candidate takes two.
    std::fs::write(&candidate, "pub fn double(n: int, m: int) -> [] int { return n; }\n")
        .expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["satisfy".as_ref(), contract.as_os_str(), candidate.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert_eq!(out.status.code(), Some(1), "a checker refusal is exit 1, as `check` answers");
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("2 were given") || text.contains("argument"),
        "the refusal is the ordinary arity vocabulary, located:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A contract with no tests checks nothing, and a verdict of `satisfied`
/// over nothing would be a lie: it is a usage error, exit 2, the code a
/// wrong command line already answers.
#[test]
fn a_contract_with_no_tests_cannot_be_satisfied() {
    let dir = scratch("satisfy-no-tests");
    let contract = dir.join("c.cho");
    let candidate = dir.join("k.cho");
    std::fs::write(&contract, "// no tests, no expectation\n").expect("a writable fixture");
    std::fs::write(&candidate, GOOD_CANDIDATE).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["satisfy".as_ref(), contract.as_os_str(), candidate.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert_eq!(out.status.code(), Some(2), "an empty contract is a usage error");
    let _ = std::fs::remove_dir_all(&dir);
}
