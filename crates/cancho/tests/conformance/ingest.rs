//! `docs/structured-ingest.md`: the JSON form of a unit, both directions,
//! through the binary an agent actually runs.

use super::*;

/// The loop the issue (#411) is about, end to end: print a real example as
/// JSON, ingest it, and require the canonical text back byte for byte.
#[test]
fn a_real_example_round_trips_through_the_binary() {
    let dir = scratch("ingest-cli");
    let source = repo_root().join("examples/hello.cho");
    let json = dir.join("hello.json");
    let out = Command::new(BIN)
        .arg("print")
        .arg(&source)
        .arg("--output")
        .arg("json")
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    std::fs::write(&json, &out.stdout).expect("a writable fixture");

    let ingested = Command::new(BIN).arg("ingest").arg(&json).output().expect("the compiler runs");
    assert!(ingested.status.success(), "{}", String::from_utf8_lossy(&ingested.stderr));
    let expected = Command::new(BIN).arg("print").arg(&source).output().expect("the compiler runs");
    assert_eq!(ingested.stdout, expected.stdout, "ingest did not give back the canonical text");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `ingest` is a fixed point: the text it answers, printed as JSON and
/// ingested again, is the same text.
#[test]
fn ingest_is_idempotent_through_the_binary() {
    let dir = scratch("ingest-fixed");
    let source = repo_root().join("examples/hello.cho");
    let first_json = dir.join("first.json");
    let out = Command::new(BIN)
        .arg("print")
        .arg(&source)
        .arg("--output")
        .arg("json")
        .output()
        .expect("the compiler runs");
    std::fs::write(&first_json, &out.stdout).expect("a writable fixture");
    let first =
        Command::new(BIN).arg("ingest").arg(&first_json).output().expect("the compiler runs");
    assert!(first.status.success());
    std::fs::write(dir.join("first.cho"), &first.stdout).expect("a writable fixture");
    let second_json = dir.join("second.json");
    let out = Command::new(BIN)
        .arg("print")
        .arg(dir.join("first.cho"))
        .arg("--output")
        .arg("json")
        .output()
        .expect("the compiler runs");
    std::fs::write(&second_json, &out.stdout).expect("a writable fixture");
    let second =
        Command::new(BIN).arg("ingest").arg(&second_json).output().expect("the compiler runs");
    assert_eq!(first.stdout, second.stdout, "the second pass moved");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The three refusals, each with its own rule and a position into the JSON:
/// the first contact an agent has with this path is a rejected file, and
/// that refusal is the loop.
#[test]
fn every_ingest_refusal_has_its_own_rule_and_a_position() {
    let dir = scratch("ingest-refused");
    let cases: &[(&str, &str, &str)] = &[
        ("not json at all", "ingest-json", "is not JSON"),
        ("an unknown vocabulary word", "ingest-node", "`fnx` is not an item kind"),
        ("a known word with the wrong shape", "ingest-arity", "the node needs a `name`"),
        ("a keyword as a name", "ingest-node", "is a keyword"),
        ("an edition field", "ingest-node", "no `edition` field"),
    ];
    for (tag, rule, why) in cases {
        let file = dir.join(format!("{tag}.json"));
        let text = match *tag {
            "not json at all" => "this is not json".to_owned(),
            "an unknown vocabulary word" => {
                r#"{"items": [{"kind": "fnx", "name": "f"}]}"#.to_owned()
            }
            "a known word with the wrong shape" => r#"{"items": [{"kind": "fn"}]}"#.to_owned(),
            "a keyword as a name" => r#"{"items": [{"kind": "fn", "name": "let"}]}"#.to_owned(),
            "an edition field" => r#"{"edition": 5, "items": []}"#.to_owned(),
            _ => unreachable!("a closed set"),
        };
        std::fs::write(&file, text).expect("a writable fixture");
        let out = Command::new(BIN)
            .arg("ingest")
            .arg(&file)
            .arg("--output")
            .arg("json")
            .output()
            .expect("the compiler runs");
        assert_eq!(out.status.code(), Some(1), "`{tag}` did not refuse");
        // The refusal is data (`docs/structured-ingest.md` §3): the same
        // shape `check --output json` answers, with the rule, the sentence
        // and a position into the JSON.
        let body = String::from_utf8_lossy(&out.stdout);
        assert!(body.contains("\"rule\": \""), "`{tag}`: {body}");
        assert!(body.contains(rule), "`{tag}`: {body}");
        assert!(body.contains(why), "`{tag}`: {body}");
        assert!(body.contains("\"position\": "), "`{tag}`: {body}");
        assert!(body.contains("\"line\": 1"), "`{tag}`: {body}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
