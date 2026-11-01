//! The ratchet on the refusal message (D4 of `docs/diagnostic-messages.md`): the `message` of `check --output json` in the command line in cancho
//! is held to the text each fixture of `tests/reject/` expects.

use super::*;

/// The text a fixture expects of the Rust command's message (the first `//~ ERROR` line), or none.
fn expected_message(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.strip_prefix("//~ ERROR ")).map(str::trim)
}

/// The `message` of the first refusal `check --output json` gives for `program`, or what it did instead.
fn first_message(exe: &Path, host: &str, program: &Path) -> Result<String, String> {
    let done = Command::new(exe)
        .args(["check", "--output", "json", "--host", host])
        .arg(program)
        .output()
        .expect("check runs");
    let json: serde_json::Value = serde_json::from_slice(&done.stdout)
        .map_err(|_| format!("no JSON (status {:?})", done.status.code()))?;
    json["refused"][0]["message"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("no refusal (status {:?})", done.status.code()))
}

/// The `message` of every refusal in `tests/reject/` contains the text its fixture expects. Not yet: `message_gaps.txt` lists the fixtures it does not,
/// each of which must still differ, so the list only shrinks: a fixture that matches and is listed fails (take it out), and so does one that differs and
/// is not. `CANCHO_REGENERATE_MESSAGE_GAPS=1` rewrites the list from what is measured (for the first writing, or to check a shrink; read the diff).
///
/// A fixture with a `//~ TARGET` line is the Rust compiler's (the same exclusion as `refusals_agree_with_the_rust_front_end`), and one with no `//~ ERROR`
/// line has nothing to hold the message to.
#[test]
fn the_message_of_a_refusal_holds_the_text_its_fixture_expects() {
    let exe = build("cli-messages", &with_front_end("cli.cho"));
    let host = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux"
    };
    let gap_file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/selfhost/message_gaps.txt");
    let gaps: std::collections::BTreeSet<&str> = include_str!("message_gaps.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    let mut fixtures: Vec<(PathBuf, String)> = Vec::new();
    for entry in std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reject"))
        .expect("tests/reject")
    {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|e| e != "cho") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a fixture");
        if text.lines().any(|l| l.starts_with("//~ TARGET ")) {
            continue;
        }
        if let Some(want) = expected_message(&text) {
            let want = want.to_owned();
            fixtures.push((path, want));
        }
    }
    fixtures.sort();
    assert!(fixtures.len() > 300, "tests/reject/ is read: {} fixtures", fixtures.len());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for chunk in fixtures.chunks(fixtures.len().div_ceil(8)) {
            let (exe, results) = (&exe, &results);
            scope.spawn(move || {
                for (program, want) in chunk {
                    let name = program.file_name().and_then(|n| n.to_str()).expect("a name");
                    let got = first_message(exe, host, program);
                    let matches = got.as_ref().is_ok_and(|m| m.contains(want.as_str()));
                    results.lock().expect("a lock").push((
                        name.to_owned(),
                        matches,
                        want.clone(),
                        got,
                    ));
                }
            });
        }
    });
    let mut results = results.into_inner().expect("a lock");
    results.sort();
    if std::env::var_os("CANCHO_REGENERATE_MESSAGE_GAPS").is_some() {
        let mut text = String::from(
            "# The programs of tests/reject whose refusal `message` in the command line in cancho does not contain the text of their `//~ ERROR` line\n\
             # (docs/diagnostic-messages.md D4). `the_message_of_a_refusal_holds_the_text_its_fixture_expects` holds the rest to it and every program\n\
             # listed here to still differing, so the list only shrinks. Remove a line when its program matches. Written by\n\
             # `CANCHO_REGENERATE_MESSAGE_GAPS=1 cargo test -p cancho --test conformance the_message_of_a_refusal`.\n",
        );
        for (name, matches, _, _) in &results {
            if !matches {
                text.push_str(name);
                text.push('\n');
            }
        }
        std::fs::write(&gap_file, text).expect("message_gaps.txt is written");
        return;
    }
    let wrong: Vec<String> = results
        .iter()
        .filter(|(name, matches, _, _)| *matches == gaps.contains(name.as_str()))
        .map(|(name, matches, want, got)| {
            if *matches {
                format!("{name}: now says {want:?}; take it out of message_gaps.txt")
            } else {
                format!("{name}: expected {want:?} in the message, got {got:?}")
            }
        })
        .collect();
    let stale: Vec<&str> =
        gaps.iter().copied().filter(|g| !results.iter().any(|(n, ..)| n == g)).collect();
    assert!(stale.is_empty(), "message_gaps.txt names no fixture: {stale:?}");
    assert!(wrong.is_empty(), "{} fixtures:\n{}", wrong.len(), wrong.join("\n"));
}
