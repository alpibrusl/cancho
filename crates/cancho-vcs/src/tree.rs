//! A tree: the whole snapshot as a sorted flat list of `(path, mode, blob)`
//! (`docs/vcs-cli.md` section 3). A directory is not an object; the paths
//! carry it. Encoded as text, one entry per line, so the store can be read
//! with the tools an agent has.

use crate::error::{Refusal, Result};
use crate::object::{Kind, Store, is_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    File,
    Exec,
    Link,
}

impl Mode {
    /// The git octal name, which is also the stored one.
    pub fn octal(self) -> &'static str {
        match self {
            Mode::File => "100644",
            Mode::Exec => "100755",
            Mode::Link => "120000",
        }
    }
    pub fn from_octal(s: &str) -> Option<Mode> {
        match s {
            "100644" => Some(Mode::File),
            "100755" => Some(Mode::Exec),
            "120000" => Some(Mode::Link),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub mode: Mode,
    pub blob: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    entries: Vec<Entry>,
}

/// `path-escapes` for anything that could leave the working copy or touch
/// the store; `path-unsupported` for what the text encoding cannot hold.
pub fn check_path(path: &str) -> Result<()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') && cfg!(windows) {
        return Err(Refusal::new("path-escapes", format!("`{path}` is not a relative path")));
    }
    for (i, part) in path.split('/').enumerate() {
        if part.is_empty() || part == "." || part == ".." {
            return Err(Refusal::new("path-escapes", format!("`{path}` has an empty or dot component")));
        }
        if i == 0 && (part == ".cancho" || part == ".git") {
            return Err(Refusal::new("path-escapes", format!("`{path}` is inside `{part}`")));
        }
    }
    if path.chars().any(|c| c == '\n' || c == '\0') {
        return Err(Refusal::new("path-unsupported", "a path with a newline or NUL cannot be stored"));
    }
    Ok(())
}

impl Tree {
    pub fn new(mut entries: Vec<Entry>) -> Result<Tree> {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        for e in &entries {
            check_path(&e.path)?;
        }
        for w in entries.windows(2) {
            if w[0].path == w[1].path {
                return Err(Refusal::new("tree-invalid", format!("`{}` appears twice", w[0].path)));
            }
            // A path that is both a file and a directory prefix.
            if w[1].path.starts_with(&format!("{}/", w[0].path)) {
                return Err(Refusal::new(
                    "tree-invalid",
                    format!("`{}` is both a file and a directory", w[0].path),
                ));
            }
        }
        Ok(Tree { entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.binary_search_by(|e| e.path.as_str().cmp(path)).ok().map(|i| &self.entries[i])
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!("{} {} {}\n", e.mode.octal(), e.blob, e.path));
        }
        out.into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<Tree> {
        let bad = |why: &str| Refusal::new("object-corrupt", format!("tree: {why}"));
        let text = std::str::from_utf8(bytes).map_err(|_| bad("not UTF-8"))?;
        let mut entries = Vec::new();
        for line in text.lines() {
            let mut it = line.splitn(3, ' ');
            let (Some(mode), Some(blob), Some(path)) = (it.next(), it.next(), it.next()) else {
                return Err(bad("short line"));
            };
            let mode = Mode::from_octal(mode).ok_or_else(|| bad("unknown mode"))?;
            if !is_id(blob) {
                return Err(bad("bad blob id"));
            }
            entries.push(Entry { path: path.to_owned(), mode, blob: blob.to_owned() });
        }
        Tree::new(entries).map_err(|e| bad(&e.message))
    }

    pub fn save(&self, store: &Store) -> Result<String> {
        store.put(Kind::Tree, &self.encode())
    }

    pub fn load(store: &Store, id: &str) -> Result<Tree> {
        Tree::decode(&store.get(Kind::Tree, id)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
    /// The content is the same, the mode is not.
    Mode,
}

impl ChangeKind {
    pub fn name(&self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Modified => "modified",
            ChangeKind::Mode => "mode",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathChange {
    pub path: String,
    pub kind: ChangeKind,
    pub old: Option<Entry>,
    pub new: Option<Entry>,
}

/// The paths that differ between two trees, in path order. Equal blobs are
/// recognised by id, so nothing is read.
pub fn diff_trees(a: &Tree, b: &Tree) -> Vec<PathChange> {
    let (ae, be) = (a.entries(), b.entries());
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < ae.len() || j < be.len() {
        let order = match (ae.get(i), be.get(j)) {
            (Some(x), Some(y)) => x.path.cmp(&y.path),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        match order {
            std::cmp::Ordering::Less => {
                out.push(PathChange {
                    path: ae[i].path.clone(),
                    kind: ChangeKind::Deleted,
                    old: Some(ae[i].clone()),
                    new: None,
                });
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(PathChange {
                    path: be[j].path.clone(),
                    kind: ChangeKind::Added,
                    old: None,
                    new: Some(be[j].clone()),
                });
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                let (x, y) = (&ae[i], &be[j]);
                if x.blob != y.blob {
                    out.push(PathChange {
                        path: x.path.clone(),
                        kind: ChangeKind::Modified,
                        old: Some(x.clone()),
                        new: Some(y.clone()),
                    });
                } else if x.mode != y.mode {
                    out.push(PathChange {
                        path: x.path.clone(),
                        kind: ChangeKind::Mode,
                        old: Some(x.clone()),
                        new: Some(y.clone()),
                    });
                }
                i += 1;
                j += 1;
            }
        }
    }
    out
}
