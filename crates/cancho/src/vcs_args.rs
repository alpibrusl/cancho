//! Argument parsing and output for the repository commands of `cancho vcs`
//! (`docs/vcs-cli.md` section 4). `--output json` is the stable interface:
//! one JSON document on stdout, `{"ok": true, "command": .., ..}` or
//! `{"ok": false, "command": .., "refused": {"tag": .., "message": .., ..}}`;
//! text is a rendering of the same result. A refusal exits 1 and an
//! environment failure (the disk) exits 3, in both forms.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cancho_vcs::{Refusal, Repo, VcsResult};
use serde_json::{Map, Value, json};

use crate::{EXIT_ENVIRONMENT, EXIT_REFUSED, Failure, usage};

pub(crate) struct Parsed {
    pub pos: Vec<String>,
    vals: HashMap<String, Vec<String>>,
    flags: HashSet<String>,
}

impl Parsed {
    pub fn value(&self, key: &str) -> Option<&str> {
        self.vals.get(key).and_then(|v| v.last()).map(String::as_str)
    }
    pub fn flag(&self, key: &str) -> bool {
        self.flags.contains(key)
    }
    pub fn json(&self) -> bool {
        self.value("--output") == Some("json")
    }
}

/// `--output`, `--repo` and `--dry-run` are accepted by every command.
pub(crate) fn parse(
    args: &[String],
    with_value: &[&str],
    flags: &[&str],
) -> Result<Parsed, Failure> {
    let mut p = Parsed { pos: vec![], vals: HashMap::new(), flags: HashSet::new() };
    let mut it = args.iter();
    let mut options = true;
    while let Some(arg) = it.next() {
        if !options || !arg.starts_with("--") {
            p.pos.push(arg.clone());
            continue;
        }
        if arg == "--" {
            options = false;
            continue;
        }
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_owned())),
            None => (arg.as_str(), None),
        };
        if with_value.contains(&key) || key == "--output" || key == "--repo" {
            let v = match inline {
                Some(v) => v,
                None => {
                    it.next().cloned().ok_or_else(|| usage(format!("`{key}` needs a value")))?
                }
            };
            if key == "--output" && v != "json" && v != "text" {
                return Err(usage("`--output` is `json` or `text`"));
            }
            p.vals.entry(key.to_owned()).or_default().push(v);
        } else if flags.contains(&key) || key == "--dry-run" {
            p.flags.insert(key.to_owned());
        } else {
            return Err(usage(format!("unknown option `{key}`")));
        }
    }
    Ok(p)
}

pub(crate) type Done = (Value, String);

/// Run one command body and print its result in the requested form.
pub(crate) fn run(
    name: &str,
    p: &Parsed,
    body: impl FnOnce() -> VcsResult<Done>,
) -> Result<ExitCode, Failure> {
    match body() {
        Ok((value, text)) => {
            if p.json() {
                let mut obj = Map::new();
                obj.insert("ok".into(), json!(true));
                obj.insert("command".into(), json!(name));
                if let Value::Object(m) = value {
                    obj.extend(m);
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default()
                );
            } else {
                print!("{text}");
                if !text.ends_with('\n') && !text.is_empty() {
                    println!();
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(r) => {
            let code = if r.environment { EXIT_ENVIRONMENT } else { EXIT_REFUSED };
            if p.json() {
                let mut refused = Map::new();
                refused.insert("tag".into(), json!(r.tag));
                refused.insert("message".into(), json!(r.message));
                if let Value::Object(m) = r.detail {
                    refused.extend(m);
                }
                let doc = json!({"ok": false, "command": name, "refused": Value::Object(refused)});
                println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
            } else {
                eprintln!("refused [{}]: {}", r.tag, r.message);
            }
            Ok(ExitCode::from(code))
        }
    }
}

pub(crate) fn open(p: &Parsed) -> VcsResult<Repo> {
    Repo::open(Path::new(p.value("--repo").unwrap_or(".")))
}

/// A path argument as a path relative to the repository root: relative
/// arguments are taken from the current directory, `..` is resolved
/// lexically, and leaving the root is `path-escapes`. The root itself is "".
pub(crate) fn rel_path(repo: &Repo, arg: &str) -> VcsResult<String> {
    let cwd = std::env::current_dir()
        .map_err(|e| Refusal::io("cannot read the current directory", &e))?;
    let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let joined = if Path::new(arg).is_absolute() { PathBuf::from(arg) } else { cwd.join(arg) };
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for c in joined.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    let normal: PathBuf = parts.iter().collect();
    let rel = normal
        .strip_prefix(&repo.root)
        .map_err(|_| Refusal::new("path-escapes", format!("`{arg}` is outside the repository")))?;
    let s =
        rel.to_str().ok_or_else(|| Refusal::new("path-unsupported", "a path that is not UTF-8"))?;
    Ok(s.replace('\\', "/"))
}

pub(crate) fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}

/// The bytes of a tree entry: from the store, or, for an entry that only
/// exists in the working copy so far, from its file (checked against the
/// entry's id). Reading never writes to the store.
pub(crate) fn entry_bytes(repo: &Repo, e: &cancho_vcs::Entry) -> VcsResult<Vec<u8>> {
    if repo.store.has(cancho_vcs::Kind::Blob, &e.blob) {
        return repo.store.get(cancho_vcs::Kind::Blob, &e.blob);
    }
    let abs = repo.root.join(&e.path);
    let bytes = if e.mode == cancho_vcs::Mode::Link {
        std::fs::read_link(&abs)
            .map_err(|er| Refusal::io(format!("cannot read link {}", e.path), &er))?
            .to_string_lossy()
            .into_owned()
            .into_bytes()
    } else {
        std::fs::read(&abs).map_err(|er| Refusal::io(format!("cannot read {}", e.path), &er))?
    };
    if cancho_vcs::object_id(cancho_vcs::Kind::Blob, &bytes) != e.blob {
        return Err(Refusal::new(
            "dirty-working-copy",
            format!("`{}` changed while it was being read", e.path),
        ));
    }
    Ok(bytes)
}
