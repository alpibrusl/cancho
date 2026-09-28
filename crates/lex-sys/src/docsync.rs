//! `lex-sys docsync` — generated documentation blocks with a `--check` twin.
//!
//! Ported from `lex-lang`'s `lex doc-sync` (`crates/lex-cli/src/doc_sync.rs`)
//! — same manifest shape, same marker syntax, same `--check` contract — not
//! shared code (`docs/vcs.md` §5 already set the precedent for this
//! repository: share the idea, not a `Cargo.toml` edge to a sibling repo).
//!
//! The drift problem this closes: a handful of numbers in `README.md`
//! (crate count, example count, the `Net` outbound/inbound tally) are
//! facts about the current repository, not design narrative, and nothing
//! failed when they went stale — the outbound/inbound count in
//! `docs/net.md` and `docs/README.md` has been hand-edited twice in this
//! repository's history for exactly that reason. The fix: generate the
//! mechanical facts, splice them into a marked region, and check them in
//! CI, the same way `docs/AGENT.md`'s generated tables already work in
//! `lex-lang`.
//!
//! ```sh
//! lex-sys docsync            # regenerate every target in docsync.toml
//! lex-sys docsync --check    # CI: fail on drift, naming each stale target
//! ```
//!
//! `docsync.toml`, at the repo root (or passed explicitly):
//!
//! ```toml
//! # A whole file owned by a generator:
//! [[file]]
//! path = "SOME_GENERATED.md"
//! command = "lex-sys some-subcommand"
//!
//! # A marked region inside a hand-written doc:
//! [[block]]
//! id = "repo-stats"
//! path = "README.md"
//! command = "cargo run -q -p lex-sys -- repo-stats"
//! ```
//!
//! Block targets replace the lines between `<!-- docsync:begin ID -->` and
//! `<!-- docsync:end ID -->` (matched by substring, so any comment syntax
//! that can carry those tokens works). The markers themselves stay, so
//! regeneration is idempotent and the surrounding prose remains
//! hand-written. Generator stdout is the block body; a trailing newline is
//! normalized. A failing generator, a missing marker pair, or a duplicate
//! id is an error naming the target — refuse, don't guess.
//!
//! This module deliberately owns only the splice mechanism and the
//! manifest runner. What each generator prints — `repo_stats::render` for
//! the one target this repository has today — is ordinary Rust living
//! beside `main`'s other subcommands, the same way `lex agent-guidelines`
//! is just a subcommand that prints a string. A mechanical count belongs
//! in a generated block; design narrative never does, and this mechanism
//! must never be asked to own the latter — `lex-lang`'s own
//! `docsync.toml` draws the identical line, owning three tables in
//! `docs/AGENT.md` and nothing else in that repository's docs.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    file: Vec<FileTarget>,
    #[serde(default)]
    block: Vec<BlockTarget>,
}

#[derive(Debug, Deserialize)]
struct FileTarget {
    path: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct BlockTarget {
    id: String,
    path: String,
    command: String,
}

/// What went wrong, classified so the caller can pick the right exit code
/// (`docs/agent-errors.md`'s own discipline: a refusal and an environment
/// failure are not the same thing and should not share an exit status).
#[derive(Debug)]
pub enum DocsyncError {
    /// A bad flag or manifest path — the command line was wrong.
    Usage(String),
    /// The manifest or a target names something that does not hold: a
    /// missing marker, a duplicate id, or (under `--check`) real drift.
    /// The kind of failure a linter reports, not a crash.
    Refused(String),
    /// The generator could not run, or a file could not be read/written —
    /// the environment failed, not the content.
    Environment(String),
}

/// `Ok(message)` on success (what to print), `Err` classified above.
pub fn cmd_docsync(args: &[String]) -> Result<String, DocsyncError> {
    let mut check = false;
    let mut manifest_path: Option<PathBuf> = None;
    for a in args {
        match a.as_str() {
            "--check" => check = true,
            "--help" | "-h" => {
                return Ok("usage: lex-sys docsync [--check] [manifest]\n\
                    \n\
                    Regenerate (or, with --check, verify) every generated doc target\n\
                    listed in docsync.toml. See `crates/lex-sys/src/docsync.rs`'s\n\
                    module docs for the manifest format."
                    .to_string());
            }
            other if !other.starts_with('-') => manifest_path = Some(PathBuf::from(other)),
            other => {
                return Err(DocsyncError::Usage(format!(
                    "unknown flag `{other}`; usage: lex-sys docsync [--check] [manifest]"
                )));
            }
        }
    }
    let manifest_path = manifest_path.unwrap_or_else(|| PathBuf::from("docsync.toml"));
    let root = manifest_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let raw = fs::read_to_string(&manifest_path).map_err(|e| {
        DocsyncError::Environment(format!("cannot read manifest {}: {e}", manifest_path.display()))
    })?;
    let manifest: Manifest = toml::from_str(&raw).map_err(|e| {
        DocsyncError::Refused(format!("invalid manifest {}: {e}", manifest_path.display()))
    })?;
    if manifest.file.is_empty() && manifest.block.is_empty() {
        return Err(DocsyncError::Refused(format!(
            "{} declares no [[file]] or [[block]] targets",
            manifest_path.display()
        )));
    }
    {
        let mut seen = std::collections::BTreeSet::new();
        for b in &manifest.block {
            if !seen.insert((&b.path, &b.id)) {
                return Err(DocsyncError::Refused(format!(
                    "duplicate block id `{}` for {}",
                    b.id, b.path
                )));
            }
        }
    }

    let mut drifted: Vec<String> = Vec::new();
    for t in &manifest.file {
        let target = root.join(&t.path);
        let generated = normalize_trailing(run_generator(&root, &t.command, &t.path)?);
        let current = fs::read_to_string(&target).unwrap_or_default();
        apply_or_report(check, &target, &t.path, &current, &generated, &mut drifted)?;
    }
    for b in &manifest.block {
        let target = root.join(&b.path);
        let current = fs::read_to_string(&target).map_err(|e| {
            DocsyncError::Environment(format!("cannot read block target {}: {e}", target.display()))
        })?;
        let body = run_generator(&root, &b.command, &b.path)?;
        let updated = splice_block(&current, &b.id, &body)
            .map_err(|e| DocsyncError::Refused(format!("in {}: {e}", target.display())))?;
        let label = format!("{}#{}", b.path, b.id);
        apply_or_report(check, &target, &label, &current, &updated, &mut drifted)?;
    }

    if check {
        if drifted.is_empty() {
            Ok("docsync: all targets current".to_string())
        } else {
            Err(DocsyncError::Refused(format!(
                "docsync: {} target(s) drifted from their generators: {}\n  fix: lex-sys docsync",
                drifted.len(),
                drifted.join(", ")
            )))
        }
    } else if drifted.is_empty() {
        Ok("docsync: all targets already current".to_string())
    } else {
        Ok(drifted
            .iter()
            .map(|d| format!("docsync: regenerated {d}"))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

fn apply_or_report(
    check: bool,
    target: &Path,
    label: &str,
    current: &str,
    updated: &str,
    drifted: &mut Vec<String>,
) -> Result<(), DocsyncError> {
    if current == updated {
        return Ok(());
    }
    if check {
        drifted.push(label.to_string());
    } else {
        fs::write(target, updated).map_err(|e| {
            DocsyncError::Environment(format!("cannot write {}: {e}", target.display()))
        })?;
        drifted.push(label.to_string());
    }
    Ok(())
}

/// Run one generator through the shell with the manifest's directory as
/// cwd. Stdout is the product; a non-zero exit is an error naming the
/// target, with the generator's stderr attached.
fn run_generator(root: &Path, command: &str, label: &str) -> Result<String, DocsyncError> {
    let out =
        Command::new("sh").arg("-c").arg(command).current_dir(root).output().map_err(|e| {
            DocsyncError::Environment(format!(
                "generator for {label} failed to spawn: `{command}`: {e}"
            ))
        })?;
    if !out.status.success() {
        return Err(DocsyncError::Refused(format!(
            "generator for {label} exited {:?}: `{command}`\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    String::from_utf8(out.stdout).map_err(|_| {
        DocsyncError::Refused(format!("generator for {label} produced non-UTF-8 output"))
    })
}

fn normalize_trailing(mut s: String) -> String {
    while s.ends_with('\n') {
        s.pop();
    }
    s.push('\n');
    s
}

/// Replace the lines strictly between the begin/end markers for `id`,
/// keeping the marker lines themselves. Exactly one begin and one end, in
/// order — anything else is an error, not a guess.
fn splice_block(doc: &str, id: &str, body: &str) -> Result<String, String> {
    let begin_tok = format!("docsync:begin {id}");
    let end_tok = format!("docsync:end {id}");
    let lines: Vec<&str> = doc.split_inclusive('\n').collect();
    let idx = |tok: &str| -> Result<usize, String> {
        let hits: Vec<usize> =
            lines.iter().enumerate().filter(|(_, l)| l.contains(tok)).map(|(i, _)| i).collect();
        match hits.as_slice() {
            [i] => Ok(*i),
            [] => Err(format!("marker `{tok}` not found")),
            _ => Err(format!("marker `{tok}` appears {} times; must be unique", hits.len())),
        }
    };
    let b = idx(&begin_tok)?;
    let e = idx(&end_tok)?;
    if e <= b {
        return Err(format!("marker `{end_tok}` appears before `{begin_tok}`"));
    }
    let mut out = String::new();
    for l in &lines[..=b] {
        out.push_str(l);
    }
    let mut body = body.trim_end_matches('\n').to_string();
    if !body.is_empty() {
        body.push('\n');
    }
    out.push_str(&body);
    for l in &lines[e..] {
        out.push_str(l);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_replaces_only_the_marked_region() {
        let doc = "intro\n<!-- docsync:begin x -->\nOLD\n<!-- docsync:end x -->\noutro\n";
        let got = splice_block(doc, "x", "NEW LINE 1\nNEW LINE 2\n").unwrap();
        assert_eq!(
            got,
            "intro\n<!-- docsync:begin x -->\nNEW LINE 1\nNEW LINE 2\n<!-- docsync:end x -->\noutro\n"
        );
        // Idempotent: splicing the same body again changes nothing.
        assert_eq!(splice_block(&got, "x", "NEW LINE 1\nNEW LINE 2\n").unwrap(), got);
    }

    #[test]
    fn splice_works_with_non_html_comment_syntax() {
        let doc = "# docsync:begin cfg\nold\n# docsync:end cfg\n";
        let got = splice_block(doc, "cfg", "new\n").unwrap();
        assert_eq!(got, "# docsync:begin cfg\nnew\n# docsync:end cfg\n");
    }

    #[test]
    fn splice_refuses_missing_or_duplicate_markers() {
        assert!(splice_block("no markers\n", "x", "b").is_err());
        let dup = "<!-- docsync:begin x -->\n<!-- docsync:end x -->\n<!-- docsync:begin x -->\n";
        assert!(splice_block(dup, "x", "b").is_err());
        let reversed = "<!-- docsync:end x -->\n<!-- docsync:begin x -->\n";
        assert!(splice_block(reversed, "x", "b").is_err());
    }

    #[test]
    fn end_to_end_regenerate_and_check() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("doc.md");
        fs::write(&doc, "head\n<!-- docsync:begin gen -->\nstale\n<!-- docsync:end gen -->\n")
            .unwrap();
        let whole = dir.path().join("WHOLE.md");
        fs::write(&whole, "stale whole\n").unwrap();
        let manifest = dir.path().join("docsync.toml");
        fs::write(
            &manifest,
            "[[file]]\npath = \"WHOLE.md\"\ncommand = \"printf 'fresh whole\\n'\"\n\n\
             [[block]]\nid = \"gen\"\npath = \"doc.md\"\ncommand = \"printf 'fresh\\n'\"\n",
        )
        .unwrap();
        let mpath = manifest.to_string_lossy().to_string();

        // --check on stale targets fails and names both.
        let err = match cmd_docsync(&["--check".into(), mpath.clone()]) {
            Err(DocsyncError::Refused(msg)) => msg,
            other => panic!("expected a Refused error, got {}", other.is_ok()),
        };
        assert!(err.contains("WHOLE.md"), "{err}");
        assert!(err.contains("doc.md#gen"), "{err}");
        assert!(err.contains("fix: lex-sys docsync"), "{err}");

        // Regenerate, then --check passes and content is right.
        cmd_docsync(std::slice::from_ref(&mpath)).unwrap();
        assert_eq!(fs::read_to_string(&whole).unwrap(), "fresh whole\n");
        assert_eq!(
            fs::read_to_string(&doc).unwrap(),
            "head\n<!-- docsync:begin gen -->\nfresh\n<!-- docsync:end gen -->\n"
        );
        cmd_docsync(&["--check".into(), mpath]).unwrap();
    }

    #[test]
    fn failing_generator_is_an_error_naming_the_target() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("docsync.toml");
        fs::write(&manifest, "[[file]]\npath = \"X.md\"\ncommand = \"echo boom >&2; exit 3\"\n")
            .unwrap();
        let err = match cmd_docsync(&[manifest.to_string_lossy().to_string()]) {
            Err(DocsyncError::Refused(msg)) => msg,
            other => panic!("expected a Refused error, got {}", other.is_ok()),
        };
        assert!(err.contains("X.md"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }
}
