//! A repository: `.cancho/vcs/` beside a working copy (`docs/vcs-cli.md`
//! sections 3 to 5). Opening, HEAD, resolving a name to a change, scanning
//! the working copy, and writing a tree's files back.

use std::collections::{BinaryHeap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::change::Change;
use crate::error::{Refusal, Result};
use crate::fsx;
use crate::object::{Kind, MAX_BLOB, Store, is_id, object_id};
use crate::refs::{self, Expect};
use crate::tree::{Entry, Mode, Tree, check_path};

pub const FORMAT: &str = "cancho-vcs 1\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached(String),
}

pub struct Repo {
    pub root: PathBuf,
    pub vcs: PathBuf,
    pub store: Store,
}

/// What `.canchoignore` excludes, besides `.cancho` and `.git` which are
/// never versioned. A line is a directory name (`target/`), a suffix
/// pattern (`*.o`), a root-relative path prefix (`a/b`), or a bare name that
/// matches any component. There is no negation and no `**`.
pub struct Ignore {
    dirs: Vec<String>,
    suffixes: Vec<String>,
    prefixes: Vec<String>,
    names: Vec<String>,
}

impl Ignore {
    pub fn load(root: &Path) -> Ignore {
        let mut ig = Ignore { dirs: vec![], suffixes: vec![], prefixes: vec![], names: vec![] };
        let text = fs::read_to_string(root.join(".canchoignore")).unwrap_or_default();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            if let Some(d) = line.strip_suffix('/') {
                ig.dirs.push(d.to_owned());
            } else if let Some(s) = line.strip_prefix('*') {
                ig.suffixes.push(s.to_owned());
            } else if line.contains('/') {
                ig.prefixes.push(line.trim_start_matches('/').to_owned());
            } else {
                ig.names.push(line.to_owned());
            }
        }
        ig
    }

    fn skips(&self, rel: &str, is_dir: bool) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if rel == ".cancho" || rel == ".git" {
            return true;
        }
        self.names.iter().any(|n| n == name)
            || (is_dir && self.dirs.iter().any(|d| d == name))
            || self.suffixes.iter().any(|s| name.ends_with(s.as_str()))
            || self.prefixes.iter().any(|p| rel == p || rel.starts_with(&format!("{p}/")))
    }
}

#[derive(Clone, Debug)]
pub struct StatusEntry {
    pub path: String,
    pub kind: crate::tree::ChangeKind,
}

impl Repo {
    pub fn init(root: &Path, branch: &str) -> Result<(Repo, bool)> {
        refs::check_ref_name(&format!("heads/{branch}"))?;
        let vcs = root.join(".cancho").join("vcs");
        let created = !vcs.join("format").is_file();
        if created {
            for d in ["objects", "refs/heads", "refs/tags", "tmp"] {
                fs::create_dir_all(vcs.join(d))
                    .map_err(|e| Refusal::io("cannot create the store", &e))?;
            }
            fsx::write_atomic(
                &vcs.join("tmp"),
                &vcs.join("HEAD"),
                format!("ref: refs/heads/{branch}\n").as_bytes(),
                false,
                true,
            )
            .map_err(|e| Refusal::io("cannot write HEAD", &e))?;
            // The format file is written last: a store without it is "not a repository".
            fsx::write_atomic(
                &vcs.join("tmp"),
                &vcs.join("format"),
                FORMAT.as_bytes(),
                false,
                true,
            )
            .map_err(|e| Refusal::io("cannot write the format file", &e))?;
        }
        Ok((Repo { root: root.to_path_buf(), store: Store::new(&vcs), vcs }, created))
    }

    pub fn open(start: &Path) -> Result<Repo> {
        let mut dir = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
        loop {
            let vcs = dir.join(".cancho").join("vcs");
            if let Ok(f) = fs::read_to_string(vcs.join("format")) {
                if f != FORMAT {
                    return Err(Refusal::new(
                        "not-a-repository",
                        format!("unknown store format `{}`", f.trim()),
                    ));
                }
                return Ok(Repo { root: dir, store: Store::new(&vcs), vcs });
            }
            if !dir.pop() {
                return Err(Refusal::new(
                    "not-a-repository",
                    format!("no `.cancho/vcs` at or above `{}`", start.display()),
                ));
            }
        }
    }

    pub fn exists_at(start: &Path) -> bool {
        Repo::open(start).is_ok()
    }

    pub fn head(&self) -> Result<Head> {
        let text = fs::read_to_string(self.vcs.join("HEAD"))
            .map_err(|e| Refusal::io("cannot read HEAD", &e))?;
        let text = text.trim();
        if let Some(r) = text.strip_prefix("ref: refs/") {
            Ok(Head::Branch(r.to_owned()))
        } else if is_id(text) {
            Ok(Head::Detached(text.to_owned()))
        } else {
            Err(Refusal::new("object-corrupt", "HEAD is neither a ref nor a change id"))
        }
    }

    pub fn set_head(&self, head: &Head) -> Result<()> {
        let text = match head {
            Head::Branch(b) => format!("ref: refs/{b}\n"),
            Head::Detached(id) => format!("{id}\n"),
        };
        fsx::write_atomic(
            &self.vcs.join("tmp"),
            &self.vcs.join("HEAD"),
            text.as_bytes(),
            false,
            true,
        )
        .map_err(|e| Refusal::io("cannot write HEAD", &e))
    }

    pub fn head_change_id(&self) -> Result<Option<String>> {
        match self.head()? {
            Head::Branch(b) => refs::read_ref(&self.vcs, &b),
            Head::Detached(id) => Ok(Some(id)),
        }
    }

    pub fn load_change(&self, id: &str) -> Result<Change> {
        Change::load(&self.store, id)
    }

    pub fn tree_of(&self, change: Option<&str>) -> Result<Tree> {
        match change {
            None => Ok(Tree::default()),
            Some(id) => Tree::load(&self.store, &self.load_change(id)?.tree),
        }
    }

    pub fn head_tree(&self) -> Result<Tree> {
        self.tree_of(self.head_change_id()?.as_deref())
    }

    /// `HEAD`, `HEAD~2`, a branch or tag name, a full id, or a unique id prefix (6+ hex).
    pub fn resolve(&self, spec: &str) -> Result<String> {
        let (base, back) = match spec.split_once('~') {
            Some((b, n)) => (
                b,
                n.parse::<usize>()
                    .map_err(|_| Refusal::new("bad-revision", format!("`{spec}`")))?,
            ),
            None => (spec, 0),
        };
        let mut id = self.resolve_one(base)?;
        for _ in 0..back {
            let c = self.load_change(&id)?;
            id = c.parents.first().cloned().ok_or_else(|| {
                Refusal::new("bad-revision", format!("`{spec}` goes back past the first change"))
            })?;
        }
        Ok(id)
    }

    fn resolve_one(&self, name: &str) -> Result<String> {
        if name == "HEAD" || name == "@" {
            return self
                .head_change_id()?
                .ok_or_else(|| Refusal::new("bad-revision", "HEAD has no change yet"));
        }
        if is_id(name) && self.store.has(Kind::Change, name) {
            return Ok(name.to_owned());
        }
        for candidate in [format!("heads/{name}"), format!("tags/{name}"), name.to_owned()] {
            if refs::check_ref_name(&candidate).is_ok()
                && let Some(id) = refs::read_ref(&self.vcs, &candidate)?
            {
                return Ok(id);
            }
        }
        if name.len() >= 6 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            let hits: Vec<String> = self
                .store
                .list(Kind::Change)?
                .into_iter()
                .filter(|i| i.starts_with(name))
                .collect();
            match hits.len() {
                1 => return Ok(hits[0].clone()),
                0 => {}
                _ => {
                    return Err(Refusal::new("bad-revision", format!("`{name}` is ambiguous"))
                        .with(json!({"candidates": hits})));
                }
            }
        }
        Err(Refusal::new("bad-revision", format!("`{name}` names no change, branch or tag")))
    }

    /// Changes reachable from `from`, newest first by time (ties by id).
    pub fn history(&self, from: &str) -> Result<Vec<(String, Change)>> {
        let mut seen = HashSet::new();
        let mut heap = BinaryHeap::new();
        let mut loaded: std::collections::HashMap<String, Change> =
            std::collections::HashMap::new();
        let mut out = Vec::new();
        let c = self.load_change(from)?;
        heap.push((c.time, from.to_owned()));
        loaded.insert(from.to_owned(), c);
        seen.insert(from.to_owned());
        while let Some((_, id)) = heap.pop() {
            let Some(c) = loaded.remove(&id) else { continue };
            for p in &c.parents {
                if seen.insert(p.clone()) {
                    let pc = self.load_change(p)?;
                    heap.push((pc.time, p.clone()));
                    loaded.insert(p.clone(), pc);
                }
            }
            out.push((id, c));
        }
        Ok(out)
    }

    /// The working copy as a tree: every versioned file hashed, nothing written.
    pub fn scan(&self) -> Result<Vec<Entry>> {
        let ig = Ignore::load(&self.root);
        let mut out = Vec::new();
        self.scan_dir(&self.root, "", &ig, &mut out)?;
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn scan_dir(&self, dir: &Path, rel: &str, ig: &Ignore, out: &mut Vec<Entry>) -> Result<()> {
        let rd = fs::read_dir(dir)
            .map_err(|e| Refusal::io(format!("cannot read {}", dir.display()), &e))?;
        for entry in rd {
            let entry = entry.map_err(|e| Refusal::io("cannot read a directory entry", &e))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(Refusal::new(
                    "path-unsupported",
                    format!("a non-UTF-8 name under `{rel}`"),
                ));
            };
            let path = if rel.is_empty() { name.to_owned() } else { format!("{rel}/{name}") };
            let meta = fs::symlink_metadata(entry.path())
                .map_err(|e| Refusal::io(format!("cannot stat {path}"), &e))?;
            if ig.skips(&path, meta.is_dir()) {
                continue;
            }
            if meta.is_dir() {
                self.scan_dir(&entry.path(), &path, ig, out)?;
            } else if meta.file_type().is_symlink() {
                let target = fs::read_link(entry.path())
                    .map_err(|e| Refusal::io(format!("cannot read link {path}"), &e))?;
                let t = target.to_str().ok_or_else(|| {
                    Refusal::new("path-unsupported", format!("link target of `{path}`"))
                })?;
                check_path(&path)?;
                out.push(Entry {
                    path,
                    mode: Mode::Link,
                    blob: object_id(Kind::Blob, t.as_bytes()),
                });
            } else if meta.is_file() {
                if meta.len() as usize > MAX_BLOB {
                    return Err(Refusal::new(
                        "file-too-large",
                        format!("`{path}` is {} bytes", meta.len()),
                    )
                    .with(json!({"path": path})));
                }
                let bytes = fs::read(entry.path())
                    .map_err(|e| Refusal::io(format!("cannot read {path}"), &e))?;
                check_path(&path)?;
                out.push(Entry {
                    path,
                    mode: file_mode(&meta),
                    blob: object_id(Kind::Blob, &bytes),
                });
            }
        }
        Ok(())
    }

    /// The working copy against HEAD.
    pub fn status(&self) -> Result<Vec<StatusEntry>> {
        let work = Tree::new(self.scan()?)?;
        let head = self.head_tree()?;
        Ok(crate::tree::diff_trees(&head, &work)
            .into_iter()
            .map(|c| StatusEntry { path: c.path, kind: c.kind })
            .collect())
    }

    /// Store the blob of a working-copy entry.
    pub fn store_working_blob(&self, entry: &Entry) -> Result<()> {
        let abs = self.root.join(&entry.path);
        let bytes = if entry.mode == Mode::Link {
            fs::read_link(&abs)
                .map_err(|e| Refusal::io(format!("cannot read link {}", entry.path), &e))?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else {
            fs::read(&abs).map_err(|e| Refusal::io(format!("cannot read {}", entry.path), &e))?
        };
        let id = self.store.put(Kind::Blob, &bytes)?;
        if id != entry.blob {
            return Err(Refusal::new(
                "dirty-working-copy",
                format!("`{}` changed while it was being recorded", entry.path),
            ));
        }
        Ok(())
    }

    /// Write one tree entry into the working copy.
    pub fn materialize(&self, entry: &Entry) -> Result<()> {
        check_path(&entry.path)?;
        let abs = self.root.join(&entry.path);
        let bytes = self.store.get(Kind::Blob, &entry.blob)?;
        self.clear_obstacles(&abs)?;
        if entry.mode == Mode::Link {
            #[cfg(unix)]
            {
                let target = String::from_utf8_lossy(&bytes).into_owned();
                if let Some(p) = abs.parent() {
                    fs::create_dir_all(p)
                        .map_err(|e| Refusal::io("cannot create a directory", &e))?;
                }
                let _ = fs::remove_file(&abs);
                return std::os::unix::fs::symlink(&target, &abs)
                    .map_err(|e| Refusal::io(format!("cannot link {}", entry.path), &e));
            }
            #[cfg(not(unix))]
            return Err(Refusal::new("path-unsupported", "symbolic links need a unix host"));
        }
        fsx::write_atomic(&self.vcs.join("tmp"), &abs, &bytes, entry.mode == Mode::Exec, false)
            .map_err(|e| Refusal::io(format!("cannot write {}", entry.path), &e))
    }

    /// A directory where a file goes (or the reverse) is removed first.
    fn clear_obstacles(&self, abs: &Path) -> Result<()> {
        let mut p = abs.parent();
        while let Some(dir) = p {
            if dir == self.root {
                break;
            }
            if let Ok(m) = fs::symlink_metadata(dir)
                && !m.is_dir()
            {
                fs::remove_file(dir)
                    .map_err(|e| Refusal::io("cannot replace a file by a directory", &e))?;
            }
            p = dir.parent();
        }
        if let Ok(m) = fs::symlink_metadata(abs)
            && m.is_dir()
        {
            fs::remove_dir_all(abs)
                .map_err(|e| Refusal::io("cannot replace a directory by a file", &e))?;
        }
        Ok(())
    }

    /// Delete a file and the directories it leaves empty.
    pub fn remove_path(&self, path: &str) -> Result<()> {
        check_path(path)?;
        let abs = self.root.join(path);
        match fs::remove_file(&abs) {
            Ok(()) => {}
            // A parent that has become a file (a directory replaced by a file
            // earlier in the same switch) leaves nothing to delete.
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(e) => return Err(Refusal::io(format!("cannot delete {path}"), &e)),
        }
        let mut p = abs.parent().map(Path::to_path_buf);
        while let Some(dir) = p {
            if dir == self.root || fs::remove_dir(&dir).is_err() {
                break;
            }
            p = dir.parent().map(Path::to_path_buf);
        }
        Ok(())
    }

    pub fn read_blob_text(&self, id: &str) -> Result<Vec<u8>> {
        self.store.get(Kind::Blob, id)
    }

    pub fn move_branch(&self, branch: &str, new: &str, expect: &Expect, note: &str) -> Result<()> {
        refs::update_ref(&self.vcs, branch, new, expect, note)
    }
}

#[cfg(unix)]
fn file_mode(meta: &fs::Metadata) -> Mode {
    use std::os::unix::fs::PermissionsExt;
    if meta.permissions().mode() & 0o111 != 0 { Mode::Exec } else { Mode::File }
}

#[cfg(not(unix))]
fn file_mode(_: &fs::Metadata) -> Mode {
    Mode::File
}
