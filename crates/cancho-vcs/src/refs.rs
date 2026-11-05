//! Refs, the HEAD pointer and the locks (`docs/vcs-cli.md` sections 3 and 5).
//!
//! A ref is a file holding one change id. It moves only under its lock file
//! and only if the current value is the one the caller expected, so two
//! agents racing on a branch cannot both win silently: the loser gets
//! `ref-moved` and the winner's id. Every move appends a line to
//! `<ref>.log` (old, new, time, note), the replacement for a reflog. A lock
//! left by a killed process is reported with its pid and age and removed
//! only by the explicit `unlock`, never silently.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::error::{Refusal, Result};
use crate::fsx::{self, failpoint};
use crate::object::is_id;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    Any,
    Absent,
    Id(String),
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `heads/main`, `tags/v1`: a ref name below `refs/`.
pub fn check_ref_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.ends_with(".lock")
        && !name.ends_with(".log")
        && name.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        });
    if ok { Ok(()) } else { Err(Refusal::new("bad-ref-name", format!("`{name}` is not a ref name"))) }
}

fn ref_path(vcs: &Path, name: &str) -> PathBuf {
    vcs.join("refs").join(name)
}

fn read_trimmed(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s.trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Refusal::io(format!("cannot read {}", path.display()), &e)),
    }
}

pub fn read_ref(vcs: &Path, name: &str) -> Result<Option<String>> {
    check_ref_name(name)?;
    match read_trimmed(&ref_path(vcs, name))? {
        Some(id) if is_id(&id) => Ok(Some(id)),
        Some(_) => Err(Refusal::new("object-corrupt", format!("ref `{name}` does not hold a change id"))),
        None => Ok(None),
    }
}

/// A lock file: created exclusively, holds `pid time`.
pub struct LockGuard {
    path: PathBuf,
}

impl LockGuard {
    pub fn acquire(path: &Path) -> Result<LockGuard> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Refusal::io("cannot create the lock directory", &e))?;
        }
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut f) => {
                let _ = write!(f, "{} {}", std::process::id(), now());
                failpoint();
                Ok(LockGuard { path: path.to_path_buf() })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let text = fs::read_to_string(path).unwrap_or_default();
                let mut it = text.split_whitespace();
                let pid: Option<u32> = it.next().and_then(|s| s.parse().ok());
                let since: Option<i64> = it.next().and_then(|s| s.parse().ok());
                let age = since.map(|t| (now() - t).max(0));
                Err(Refusal::new(
                    "lock-held",
                    format!(
                        "`{}` is locked by pid {} for {}s; if that process is gone, run `cancho vcs unlock`",
                        path.display(),
                        pid.map_or("?".to_owned(), |p| p.to_string()),
                        age.map_or("?".to_owned(), |a| a.to_string())
                    ),
                )
                .with(json!({"lock": path.display().to_string(), "pid": pid, "age_seconds": age})))
            }
            Err(e) => Err(Refusal::io(format!("cannot create lock {}", path.display()), &e)),
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// The working-copy lock, taken by record, switch, restore and revert.
pub fn work_lock(vcs: &Path) -> Result<LockGuard> {
    LockGuard::acquire(&vcs.join("lock"))
}

/// Move `name` to `new` if it currently matches `expect`.
pub fn update_ref(vcs: &Path, name: &str, new: &str, expect: &Expect, note: &str) -> Result<()> {
    check_ref_name(name)?;
    let path = ref_path(vcs, name);
    let _lock = LockGuard::acquire(&path.with_extension_suffix("lock"))?;
    let current = read_ref(vcs, name)?;
    let ok = match expect {
        Expect::Any => true,
        Expect::Absent => current.is_none(),
        Expect::Id(want) => current.as_deref() == Some(want.as_str()),
    };
    if !ok {
        return Err(Refusal::new(
            "ref-moved",
            format!("`{name}` is at {}, not where it was expected", current.as_deref().unwrap_or("nothing")),
        )
        .with(json!({"ref": name, "current": current, "expected": match expect {
            Expect::Any => None, Expect::Absent => Some("absent".to_owned()), Expect::Id(i) => Some(i.clone()) }})));
    }
    if current.as_deref() == Some(new) {
        return Ok(());
    }
    failpoint();
    fsx::write_atomic(&vcs.join("tmp"), &path, format!("{new}\n").as_bytes(), false, true)
        .map_err(|e| Refusal::io(format!("cannot write ref {name}"), &e))?;
    append_log(&path, current.as_deref(), new, note)
}

fn append_log(path: &Path, old: Option<&str>, new: &str, note: &str) -> Result<()> {
    let log = path.with_extension_suffix("log");
    let line = format!("{} {} {} {}\n", old.unwrap_or("-"), new, now(), note.replace('\n', " "));
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| Refusal::io("cannot open the ref log", &e))?;
    f.write_all(line.as_bytes()).map_err(|e| Refusal::io("cannot write the ref log", &e))?;
    f.sync_all().map_err(|e| Refusal::io("cannot sync the ref log", &e))?;
    failpoint();
    Ok(())
}

pub fn delete_ref(vcs: &Path, name: &str, expect: &Expect) -> Result<()> {
    check_ref_name(name)?;
    let path = ref_path(vcs, name);
    let _lock = LockGuard::acquire(&path.with_extension_suffix("lock"))?;
    let current = read_ref(vcs, name)?;
    if let Expect::Id(want) = expect
        && current.as_deref() != Some(want.as_str())
    {
        return Err(Refusal::new("ref-moved", format!("`{name}` is not at the expected change"))
            .with(json!({"ref": name, "current": current})));
    }
    if current.is_some() {
        append_log(&path, current.as_deref(), "-", "deleted")?;
        fs::remove_file(&path).map_err(|e| Refusal::io("cannot delete the ref", &e))?;
    }
    Ok(())
}

/// Refs below `refs/<prefix>`, as `(name, id)`, sorted.
pub fn list_refs(vcs: &Path, prefix: &str) -> Result<Vec<(String, String)>> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = format!("{rel}{name}");
            if e.path().is_dir() {
                walk(&e.path(), &format!("{path}/"), out);
            } else if !(name.ends_with(".lock") || name.ends_with(".log")) {
                out.push(path);
            }
        }
    }
    let mut names = Vec::new();
    walk(&vcs.join("refs").join(prefix), &format!("{prefix}/"), &mut names);
    names.sort();
    let mut out = Vec::new();
    for n in names {
        if let Some(id) = read_ref(vcs, &n)? {
            out.push((n, id));
        }
    }
    Ok(out)
}

/// The ref log of `name`, oldest first: `(old, new, time, note)`.
pub fn read_log(vcs: &Path, name: &str) -> Result<Vec<(String, String, i64, String)>> {
    check_ref_name(name)?;
    let text = fs::read_to_string(ref_path(vcs, name).with_extension_suffix("log")).unwrap_or_default();
    Ok(text
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(4, ' ');
            Some((
                it.next()?.to_owned(),
                it.next()?.to_owned(),
                it.next()?.parse().ok()?,
                it.next().unwrap_or("").to_owned(),
            ))
        })
        .collect())
}

/// Locks present in the store: `(path, pid, age)`.
pub fn locks(vcs: &Path) -> Vec<(PathBuf, Option<u32>, Option<i64>)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.file_name().is_some_and(|n| n == "lock") || p.to_string_lossy().ends_with(".lock") {
                out.push(p);
            }
        }
    }
    let mut found = Vec::new();
    walk(&vcs.join("refs"), &mut found);
    let top = vcs.join("lock");
    if top.is_file() {
        found.push(top);
    }
    found
        .into_iter()
        .map(|p| {
            let text = fs::read_to_string(&p).unwrap_or_default();
            let mut it = text.split_whitespace();
            let pid = it.next().and_then(|s| s.parse().ok());
            let age = it.next().and_then(|s| s.parse::<i64>().ok()).map(|t| (now() - t).max(0));
            (p, pid, age)
        })
        .collect()
}

trait SuffixExt {
    fn with_extension_suffix(&self, suffix: &str) -> PathBuf;
}

impl SuffixExt for Path {
    /// `refs/heads/main` -> `refs/heads/main.lock` (the name keeps any dots).
    fn with_extension_suffix(&self, suffix: &str) -> PathBuf {
        let mut s = self.as_os_str().to_os_string();
        s.push(".");
        s.push(suffix);
        PathBuf::from(s)
    }
}
