//! The ACLI surface stays honest about the dispatch table (mirroring
//! lex-lang's `readme_commands.rs::every_dispatched_command_is_documented`):
//! a command the binary dispatches but `cancho skill`/`introspect` don't
//! advertise is invisible to an agent that only reads the generated
//! surface, and the reverse (advertised but not dispatched) would be a
//! lie the generator told on this binary's behalf. Both are read straight
//! from the source rather than a curated second list, so neither can
//! drift the way `docs/benchmarks-game.md`'s sqrt paragraph did.

use super::*;
use std::collections::BTreeSet;

/// The set of command tokens the binary dispatches, read straight from
/// the `match command.as_str() { … }` table in `src/main.rs`. An arm
/// counts only when everything left of `=>` is string literals joined
/// by `|` -- which excludes a nested `match` inside a handler body.
fn dispatch_commands() -> BTreeSet<String> {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("read main.rs");

    let start = src.find("match command.as_str()").expect("dispatch match not found");
    let end = src[start..].find("other =>").map(|o| start + o).unwrap_or(src.len());
    let region = &src[start..end];

    let mut cmds = BTreeSet::new();
    for line in region.lines() {
        let Some(arrow) = line.find("=>") else {
            continue;
        };
        let lhs = &line[..arrow];
        let literals = quoted_literals(lhs);
        if literals.is_empty() {
            continue;
        }
        let mut residue = lhs.to_string();
        for lit in &literals {
            residue = residue.replacen(&format!("\"{lit}\""), "", 1);
        }
        if residue.trim().chars().all(|c| c == '|' || c.is_whitespace()) {
            for lit in literals {
                cmds.insert(lit);
            }
        }
    }
    assert!(
        cmds.contains("check") && cmds.contains("build") && cmds.contains("vcs"),
        "dispatch parse looks wrong; got {cmds:?}"
    );
    cmds
}

/// Extract the contents of every double-quoted literal in `s`.
fn quoted_literals(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '"' {
            let mut lit = String::new();
            for (_, c2) in chars.by_ref() {
                if c2 == '"' {
                    break;
                }
                lit.push(c2);
            }
            out.push(lit);
        }
    }
    out
}

/// Top-level command tokens in `USAGE`'s own `usage:` block: the second
/// word of each `    cancho <cmd> ...` line.
fn usage_commands() -> BTreeSet<String> {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("read main.rs");
    let start = src.find("usage:\n").expect("usage: block not found") + "usage:\n".len();
    let end = src[start..].find("\n\noptions:").map(|o| start + o).unwrap_or(src.len());
    let region = &src[start..end];

    let mut cmds = BTreeSet::new();
    for line in region.lines() {
        let Some(rest) = line.trim_start().strip_prefix("cancho ") else {
            continue;
        };
        let tok: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        let tok = if tok.is_empty() && rest.starts_with("--") {
            rest.split_whitespace().next().unwrap_or("").trim_start_matches('-').to_string()
        } else {
            tok
        };
        if !tok.is_empty() {
            cmds.insert(tok);
        }
    }
    cmds
}

/// Command tokens advertised by `cancho skill`: the word after
/// ``- `cancho `` in each bullet of "Available commands".
fn skill_commands(out: &str) -> BTreeSet<String> {
    let mut cmds = BTreeSet::new();
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("- `cancho ") else {
            continue;
        };
        let tok: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        if !tok.is_empty() {
            cmds.insert(tok);
        }
    }
    cmds
}

/// Self-describing meta commands that needn't appear in the command
/// listings: `--version`/`-V` and `--help`/`-h`/`help` are output modes,
/// `introspect`/`skill` *are* the surfaces being checked against.
const META: &[&str] = &["help", "introspect", "skill"];

/// `cancho <args>`'s stdout, run in a scratch directory so the ACLI
/// side effect (writing/refreshing `.cli/`) never touches the source
/// tree a test run happens to be built from. `tag` names that
/// directory -- unique per call site, like every other test here,
/// since `scratch` itself `rm -rf`s its directory first and two tests
/// sharing one name race under `cargo test`'s default parallelism.
fn run_stdout(tag: &str, args: &[&str]) -> String {
    let dir = scratch(tag);
    let out = Command::new(BIN).args(args).current_dir(&dir).output().expect("spawn cancho");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn every_dispatched_command_is_documented() {
    let dispatched: BTreeSet<String> = dispatch_commands()
        .into_iter()
        .filter(|c| !c.starts_with('-') && !META.contains(&c.as_str()))
        .collect();
    let usage = usage_commands();
    let skill = skill_commands(&run_stdout("agent-cli-documented", &["skill"]));

    let missing_usage: Vec<&String> = dispatched.iter().filter(|c| !usage.contains(*c)).collect();
    let missing_skill: Vec<&String> = dispatched.iter().filter(|c| !skill.contains(*c)).collect();
    assert!(
        missing_usage.is_empty(),
        "USAGE omits dispatched commands: {missing_usage:?} -- add them to the `usage:` block"
    );
    assert!(
        missing_skill.is_empty(),
        "`cancho skill` omits dispatched commands: {missing_skill:?} -- \
         add a CommandInfo in acli.rs::commands()"
    );
}

#[test]
fn introspect_names_every_registered_command() {
    let out = run_stdout("agent-cli-introspect", &["introspect", "--output", "json"]);
    let tree: serde_json::Value = serde_json::from_str(&out).expect("introspect prints JSON");
    assert_eq!(tree["data"]["name"], "cancho");
    let names: BTreeSet<String> = tree["data"]["commands"]
        .as_array()
        .expect("a commands array")
        .iter()
        .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    for expected in ["build", "check", "run", "ids", "authority", "layout", "print", "vcs"] {
        assert!(names.contains(expected), "introspect is missing `{expected}`: {names:?}");
    }
}
