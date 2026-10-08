//! `cancho vcs sync` — the store rides inside git (`docs/vcs.md` §8's next
//! row, issue #403): recompute every declaration's identity from the
//! working tree, write the index and the op log into `.cancho/` as plain
//! text, and answer the semantic diff against what the repository last
//! committed.
//!
//! The index is one sorted line per declaration —
//! `fn <module>::<name> <sig> <body>` — deliberately line-oriented and
//! key-sorted, so an ordinary `git merge` of two branches that both
//! touched different functions resolves as text and a conflict lands on
//! exactly the declarations that genuinely collided. The op log is the
//! same `OperationRecord` files `vcs publish` writes, keyed by `OpId`,
//! so a PR's semantic diff ("this changes the `SigId` of `fn execute`;
//! N callers by `SigId`") is visible in the file list before it is
//! visible anywhere else.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use cancho_id::identify;
use cancho_vcs::{OpLog, Operation, OperationKind, OperationRecord};

use crate::vcs_cli::EDITION;
use crate::vcs_cli::parse_texts;
use crate::{Failure, STD, environment, usage};

/// One line of the index: what a declaration is, at the module and
/// name a caller writes. The hashes are `cancho-id`'s own, so the index
/// is a function of the source alone and never of the order files were
/// walked in.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct IndexEntry {
    module: String,
    name: String,
    sig: String,
    body: String,
}

impl IndexEntry {
    fn parse(line: &str) -> Option<Self> {
        // `fn <module> <name> <sig> <body>`, four space-delimited fields
        // after the `fn ` tag. The module is empty for the root module,
        // so the line reads `fn  name ...` — one space for the separator,
        // one for the empty field. Splitting on single spaces keeps that
        // unambiguous: a name or hash never contains one.
        let rest = line.strip_prefix("fn ")?;
        let mut fields = rest.split(' ');
        let module = fields.next()?.to_owned();
        let name = fields.next()?.to_owned();
        let sig = fields.next()?.to_owned();
        let body = fields.next()?.to_owned();
        if name.is_empty() || sig.is_empty() || body.is_empty() {
            return None;
        }
        Some(Self { module, name, sig, body })
    }

    fn write(&self) -> String {
        format!("fn {} {} {} {}", self.module, self.name, self.sig, self.body)
    }
}

/// The index as a whole: sorted, so the file is a function of the
/// program and never of the walk order.
struct Index(BTreeMap<(String, String), IndexEntry>);

impl Index {
    fn parse(text: &str) -> Self {
        let mut map = BTreeMap::new();
        for line in text.lines() {
            if let Some(entry) = IndexEntry::parse(line) {
                map.insert((entry.module.clone(), entry.name.clone()), entry);
            }
        }
        Self(map)
    }

    fn from_source(
        dir: &Path,
        with_std: bool,
        std: &[(&str, &str)],
    ) -> io::Result<(Self, Vec<String>)> {
        // One file at a time, because identity is per-program and a
        // directory is not one program: two files may declare helpers of
        // the same name today (the pre-module era) and a merged walk
        // would refuse them rather than index them.
        let mut map = BTreeMap::new();
        let mut problems = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();
        collect_cho(dir, &mut files)?;
        files.sort();
        for path in files {
            let source = fs::read_to_string(&path)?;
            let file_name = path.display().to_string();
            let mut named = vec![(file_name.clone(), source)];
            if with_std {
                named.extend(std.iter().map(|(n, t)| (n.to_string(), t.to_string())));
            }
            match parse_texts(&named) {
                Ok((ast, _)) => {
                    for f in identify(&ast).functions {
                        let entry = IndexEntry {
                            module: f.module.clone(),
                            name: f.name.clone(),
                            sig: f.sig.to_hex(),
                            body: f.body.to_hex(),
                        };
                        map.insert((f.module, f.name), entry);
                    }
                }
                Err(rendered) => {
                    // A file that does not check is a fact to report, not a
                    // reason to refuse the whole sync: the index records
                    // what does exist, and the problem names the file.
                    problems.push(rendered);
                }
            }
        }
        Ok((Self(map), problems))
    }
}

/// Every `.cho` under `dir`, recursively, in sorted order.
fn collect_cho(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            // `.cancho` is the store itself; `.git` is not source; a
            // target directory is never source.
            if name == ".cancho" || name == ".git" || name == "target" {
                continue;
            }
            collect_cho(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "cho") {
            out.push(path);
        }
    }
    Ok(())
}

/// What changed between the committed index and the working tree,
/// as the `OperationKind` vocabulary the op log already speaks.
enum Semantic {
    Unchanged,
    Added,
    BodyChanged { sig: String, body: String, from: String },
    SignatureChanged,
}

fn classify(old: Option<&IndexEntry>, new: &IndexEntry) -> Semantic {
    match old {
        None => Semantic::Added,
        Some(old) if old.sig == new.sig && old.body == new.body => Semantic::Unchanged,
        Some(old) if old.sig == new.sig => Semantic::BodyChanged {
            sig: old.sig.clone(),
            from: old.body.clone(),
            body: new.body.clone(),
        },
        Some(_) => Semantic::SignatureChanged,
    }
}

/// `cancho vcs sync [--dir <dir>] [--std] [--check]`.
///
/// Writes `.cancho/ids.txt` beside the sources and the op records for
/// what moved. `--check` writes nothing and fails, exit 1, on any
/// semantic change — the CI shape, so a PR that changes a `SigId` is a
/// red diff rather than a silent rewrite of the index.
pub fn cmd_sync(args: &[String]) -> Result<std::process::ExitCode, Failure> {
    let mut dir = PathBuf::from(".");
    let mut with_std = false;
    let mut check = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--dir" => {
                let Some(value) = it.next() else {
                    return Err(usage("`--dir` needs a directory"));
                };
                dir = PathBuf::from(value);
            }
            "--std" => with_std = true,
            "--check" => check = true,
            other => {
                return Err(usage(format!("unknown `vcs sync` option `{other}`")));
            }
        }
    }
    let store = dir.join(".cancho");
    let index_path = store.join("ids.txt");

    let std: Vec<(&str, &str)> =
        if with_std { STD.iter().map(|(n, t)| (*n, *t)).collect() } else { Vec::new() };
    let (index, problems) = Index::from_source(&dir, with_std, &std)
        .map_err(|e| environment(format!("cannot walk `{}`: {e}", dir.display())))?;

    let old = if index_path.exists() {
        Index::parse(
            &fs::read_to_string(&index_path)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", index_path.display())))?,
        )
    } else {
        Index(BTreeMap::new())
    };

    // The semantic diff, in the op log's own vocabulary. Removals are
    // what the old index has and the new does not.
    let mut added = 0usize;
    let mut body_changed = 0usize;
    let mut sig_changed = 0usize;
    let mut removed = 0usize;
    let mut records: Vec<OperationRecord> = Vec::new();
    for ((module, name), new_entry) in &index.0 {
        match classify(old.0.get(&(module.clone(), name.clone())), new_entry) {
            Semantic::Unchanged => {}
            Semantic::Added => {
                added += 1;
                records.push(OperationRecord::new(Operation::new(
                    OperationKind::AddFunction {
                        sig_id: new_entry.sig.clone(),
                        stage_id: new_entry.body.clone(),
                        effects: Default::default(),
                        in_file: None,
                    },
                    EDITION,
                    [],
                )));
            }
            Semantic::BodyChanged { sig, body, from } => {
                body_changed += 1;
                records.push(OperationRecord::new(Operation::new(
                    OperationKind::ModifyBody {
                        sig_id: sig,
                        from_stage_id: from,
                        to_stage_id: body,
                    },
                    EDITION,
                    [],
                )));
            }
            Semantic::SignatureChanged => {
                sig_changed += 1;
            }
        }
    }
    for (key, old_entry) in &old.0 {
        if !index.0.contains_key(key) {
            removed += 1;
            records.push(OperationRecord::new(Operation::new(
                OperationKind::RemoveFunction {
                    sig_id: old_entry.sig.clone(),
                    last_stage_id: old_entry.body.clone(),
                },
                EDITION,
                [],
            )));
        }
    }

    // The report, whatever the mode: the semantic facts first.
    if added > 0 || body_changed > 0 || sig_changed > 0 || removed > 0 {
        println!("semantic diff against the committed index:");
        if added > 0 {
            println!("  {added} declaration(s) added");
        }
        if body_changed > 0 {
            println!("  {body_changed} body change(s)");
        }
        if sig_changed > 0 {
            println!("  {sig_changed} signature change(s) — callers by `SigId` affected");
        }
        if removed > 0 {
            println!("  {removed} declaration(s) removed");
        }
    } else {
        println!("no semantic change: the index matches the source");
    }
    for problem in &problems {
        println!("warning: {problem}");
    }

    if check {
        if added > 0 || body_changed > 0 || sig_changed > 0 || removed > 0 {
            return Ok(std::process::ExitCode::from(1u8));
        }
        return Ok(std::process::ExitCode::SUCCESS);
    }

    // Write the new index, plain text, sorted.
    fs::create_dir_all(&store)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", store.display())))?;
    let mut text = String::new();
    for entry in index.0.values() {
        text.push_str(&entry.write());
        text.push('\n');
    }
    fs::write(&index_path, text)
        .map_err(|e| environment(format!("cannot write `{}`: {e}", index_path.display())))?;

    // The op records, loose files keyed by `OpId`, exactly what
    // `vcs publish` already writes.
    let op_log = OpLog::open(&store)
        .map_err(|e| environment(format!("cannot open op log at `{}`: {e}", store.display())))?;
    for record in &records {
        op_log.put(record).map_err(|e| environment(format!("cannot write to the op log: {e}")))?;
    }
    if !records.is_empty() {
        println!(
            "{} operation record(s) appended to {}",
            records.len(),
            store.join("ops").display()
        );
    }
    Ok(std::process::ExitCode::SUCCESS)
}
