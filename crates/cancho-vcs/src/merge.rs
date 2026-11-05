//! A merge of three trees (`docs/vcs-cli.md` section 6): the tree layer,
//! which settles each path from base, ours and theirs, and calls the `.cho`
//! layer (`merge_cho`) and the text layer (`merge3`) for a path both sides
//! changed. Nothing here writes: the result is entries, the bytes of the
//! blobs the merge made, and the conflicts as data. The caller records.
//!
//! The same function does `replay` (`docs/vcs-cli.md` section 4): there the
//! base is the change's parent, "theirs" is the change, and a text file the
//! change edited is first tried by re-applying the change's anchored edits
//! to "ours" (what was done, not the text of a patch), then by the line merge.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::anchor::apply_edits;
use crate::change::Edit;
use crate::error::{Refusal, Result};
use crate::merge_cho::{Choice, DeclConflict, merge_cho};
use crate::merge3::{ConflictHunk, merge_text};
use crate::object::{Kind, object_id};
use crate::repo::Repo;
use crate::tree::{Entry, Mode, Tree};

/// How one conflicting path was settled by `resolve`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Settled {
    /// The path's final entry: `None` deletes it. The blob is in the store.
    Entry(Option<(String, String)>),
    /// Declaration by declaration, for a `.cho` file.
    Decls(BTreeMap<String, Choice>),
}

pub type Resolutions = BTreeMap<String, Settled>;

#[derive(Clone, Debug)]
pub struct Conflict {
    pub path: String,
    /// `content-conflict`, `decl-conflict`, `delete-modify`, `rename-edit`,
    /// `add-add`, `binary-conflict`, `mode-conflict`, `anchor-not-found`,
    /// `anchor-ambiguous`.
    pub tag: &'static str,
    /// `tree`, `cho` or `text`.
    pub layer: &'static str,
    pub base: Option<Entry>,
    pub ours: Option<Entry>,
    pub theirs: Option<Entry>,
    pub hunks: Vec<ConflictHunk>,
    pub decls: Vec<DeclConflict>,
    /// For a replay, the anchored edit that did not apply.
    pub detail: Option<String>,
    /// The merged text with "ours" standing in for what is open (a text
    /// or `.cho` conflict only).
    pub partial: Option<String>,
}

#[derive(Debug, Default)]
pub struct TreeMerge {
    /// The merged tree's entries; a conflicting path has "ours" until settled.
    pub entries: Vec<Entry>,
    /// The blobs the merge made (id to bytes); not yet in any store.
    pub new_blobs: HashMap<String, Vec<u8>>,
    pub conflicts: Vec<Conflict>,
    /// Paths settled by a layer finer than "one side changed it".
    pub merged_paths: Vec<(String, &'static str)>,
}

pub struct Inputs<'a> {
    pub base: &'a Tree,
    pub ours: &'a Tree,
    pub theirs: &'a Tree,
    pub read: &'a dyn Fn(&Entry) -> Result<Vec<u8>>,
    pub resolutions: &'a Resolutions,
    /// A replay: the edits of the change, by path.
    pub edits: Option<&'a HashMap<String, Vec<Edit>>>,
}

fn same(a: &Option<&Entry>, b: &Option<&Entry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.blob == y.blob && x.mode == y.mode,
        _ => false,
    }
}

fn text_of(bytes: &[u8]) -> Option<&str> {
    if bytes.iter().take(8000).any(|&b| b == 0) { None } else { std::str::from_utf8(bytes).ok() }
}

/// What a path merge settled to.
enum Path {
    Clean(Option<Entry>),
    Made(Entry, Vec<u8>, &'static str),
    Conflict(Box<Conflict>),
}

pub fn merge_trees(inp: &Inputs<'_>) -> Result<TreeMerge> {
    let mut paths: BTreeSet<&str> = BTreeSet::new();
    for t in [inp.base, inp.ours, inp.theirs] {
        paths.extend(t.entries().iter().map(|e| e.path.as_str()));
    }
    // Blobs each side added (not at the same path in the base): a deleted
    // path whose blob shows up here was renamed.
    let added = |side: &Tree| -> HashSet<String> {
        side.entries()
            .iter()
            .filter(|e| inp.base.get(&e.path).is_none())
            .map(|e| e.blob.clone())
            .collect()
    };
    let (added_ours, added_theirs) = (added(inp.ours), added(inp.theirs));
    let mut out = TreeMerge::default();
    let mut entries = Vec::new();
    for path in paths {
        let (b, o, t) = (inp.base.get(path), inp.ours.get(path), inp.theirs.get(path));
        let res = match inp.resolutions.get(path) {
            Some(Settled::Entry(e)) => {
                let entry = e.as_ref().and_then(|(mode, blob)| {
                    Mode::from_octal(mode).map(|mode| Entry {
                        path: path.to_owned(),
                        mode,
                        blob: blob.clone(),
                    })
                });
                Path::Clean(entry)
            }
            other => {
                let choices = match other {
                    Some(Settled::Decls(c)) => Some(c),
                    _ => None,
                };
                settle_path(inp, path, b, o, t, (&added_ours, &added_theirs), choices)?
            }
        };
        match res {
            Path::Clean(e) => entries.extend(e),
            Path::Made(e, bytes, layer) => {
                out.new_blobs.insert(e.blob.clone(), bytes);
                out.merged_paths.push((path.to_owned(), layer));
                entries.push(e);
            }
            Path::Conflict(c) => {
                entries.extend(c.ours.clone());
                out.conflicts.push(*c);
            }
        }
    }
    out.entries = entries;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn settle_path(
    inp: &Inputs<'_>,
    path: &str,
    b: Option<&Entry>,
    o: Option<&Entry>,
    t: Option<&Entry>,
    added: (&HashSet<String>, &HashSet<String>),
    choices: Option<&BTreeMap<String, Choice>>,
) -> Result<Path> {
    let conflict = |tag: &'static str, layer: &'static str| Conflict {
        path: path.to_owned(),
        tag,
        layer,
        base: b.cloned(),
        ours: o.cloned(),
        theirs: t.cloned(),
        hunks: vec![],
        decls: vec![],
        detail: None,
        partial: None,
    };
    let keep = |e: Option<&Entry>| Path::Clean(e.cloned());
    if same(&o, &t) {
        return Ok(keep(o));
    }
    if same(&o, &b) {
        return Ok(keep(t));
    }
    if same(&t, &b) && inp.edits.is_none_or(|e| !e.contains_key(path)) {
        return Ok(keep(o));
    }
    // From here the two sides differ from each other and neither is the base
    // (or this is a replay of an edit onto a side that moved).
    let (Some(oe), Some(te)) = (o, t) else {
        // One side deleted the path (or, with no base, only one side has it:
        // not reachable, since that side would equal the merge of nothing).
        let added_by_deleter = if o.is_none() { added.0 } else { added.1 };
        let tag = match b {
            None => "add-add",
            Some(be) if added_by_deleter.contains(&be.blob) => "rename-edit",
            Some(_) => "delete-modify",
        };
        return Ok(Path::Conflict(Box::new(conflict(tag, "tree"))));
    };
    // Mode.
    let mode = if oe.mode == te.mode {
        oe.mode
    } else {
        match b {
            Some(be) if be.mode == oe.mode => te.mode,
            Some(be) if be.mode == te.mode => oe.mode,
            _ => {
                return Ok(Path::Conflict(Box::new(conflict("mode-conflict", "tree"))));
            }
        }
    };
    let with_mode = |blob: &str| Entry { path: path.to_owned(), mode, blob: blob.to_owned() };
    if oe.blob == te.blob {
        return Ok(Path::Clean(Some(with_mode(&oe.blob))));
    }
    if let Some(be) = b {
        if be.blob == oe.blob && inp.edits.is_none_or(|e| !e.contains_key(path)) {
            return Ok(Path::Clean(Some(with_mode(&te.blob))));
        }
        if be.blob == te.blob {
            return Ok(Path::Clean(Some(with_mode(&oe.blob))));
        }
    }
    // Content, from here on.
    if oe.mode == Mode::Link || te.mode == Mode::Link {
        return Ok(Path::Conflict(Box::new(conflict("content-conflict", "tree"))));
    }
    let (ob, tb) = ((inp.read)(oe)?, (inp.read)(te)?);
    let bb = match b {
        Some(be) if be.mode != Mode::Link => (inp.read)(be)?,
        _ => Vec::new(),
    };
    let (Some(ot), Some(tt), Some(bt)) = (text_of(&ob), text_of(&tb), text_of(&bb)) else {
        return Ok(Path::Conflict(Box::new(conflict(
            if b.is_none() { "add-add" } else { "binary-conflict" },
            "tree",
        ))));
    };
    // A replay re-applies what the change did, where it can.
    let mut anchor_failure: Option<(&'static str, String)> = None;
    if let Some(edits) = inp.edits.and_then(|e| e.get(path)) {
        match apply_edits(ot, edits) {
            Ok(text) => return Ok(made(path, mode, text.into_bytes(), "edits")),
            Err(r) => anchor_failure = Some((r.tag, r.message)),
        }
    }
    if path.ends_with(".cho")
        && let Some(cm) = merge_cho(bt, ot, tt, choices.unwrap_or(&BTreeMap::new()))
        && cm.frame_conflicts.is_empty()
    {
        if cm.conflicts.is_empty() {
            return Ok(made(path, mode, cm.text.into_bytes(), "cho"));
        }
        let mut c = conflict("decl-conflict", "cho");
        c.decls = cm.conflicts;
        c.partial = Some(cm.text);
        return Ok(Path::Conflict(Box::new(c)));
    }
    let m = merge_text(bt, ot, tt);
    if m.conflicts.is_empty() {
        return Ok(made(path, mode, m.text.into_bytes(), "text"));
    }
    let (tag, detail) = match anchor_failure {
        Some((tag, msg)) => (tag, Some(msg)),
        None if b.is_none() => ("add-add", None),
        None => ("content-conflict", None),
    };
    let mut c = conflict(tag, "text");
    c.hunks = m.conflicts;
    c.detail = detail;
    c.partial = Some(m.text);
    Ok(Path::Conflict(Box::new(c)))
}

fn made(path: &str, mode: Mode, bytes: Vec<u8>, layer: &'static str) -> Path {
    let blob = object_id(Kind::Blob, &bytes);
    Path::Made(Entry { path: path.to_owned(), mode, blob }, bytes, layer)
}

/// The merge base of two changes: the common ancestors that are not
/// ancestors of another common ancestor; of several (a criss-cross), the
/// latest by time, then by id, and the count is reported.
pub struct Base {
    pub id: String,
    pub candidates: usize,
}

pub fn merge_base(repo: &Repo, a: &str, b: &str) -> Result<Option<Base>> {
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    let mut times: HashMap<String, i64> = HashMap::new();
    let mut stack = vec![a.to_owned(), b.to_owned()];
    while let Some(id) = stack.pop() {
        if parents.contains_key(&id) {
            continue;
        }
        let c = repo.load_change(&id)?;
        stack.extend(c.parents.iter().cloned());
        times.insert(id.clone(), c.time);
        parents.insert(id, c.parents);
    }
    let reach = |from: &str| -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut st = vec![from.to_owned()];
        while let Some(id) = st.pop() {
            if seen.insert(id.clone())
                && let Some(ps) = parents.get(&id)
            {
                st.extend(ps.iter().cloned());
            }
        }
        seen
    };
    let (ra, rb) = (reach(a), reach(b));
    let common: Vec<&String> = ra.intersection(&rb).collect();
    let mut covered: HashSet<String> = HashSet::new();
    for c in &common {
        for p in parents.get(*c).map(Vec::as_slice).unwrap_or_default() {
            let mut st = vec![p.clone()];
            while let Some(id) = st.pop() {
                if covered.insert(id.clone())
                    && let Some(ps) = parents.get(&id)
                {
                    st.extend(ps.iter().cloned());
                }
            }
        }
    }
    let mut best: Vec<&String> = common.into_iter().filter(|c| !covered.contains(*c)).collect();
    best.sort_by(|x, y| times[*y].cmp(&times[*x]).then(x.cmp(y)));
    Ok(best.first().map(|id| Base { id: (*id).clone(), candidates: best.len() }))
}

/// Whether `ancestor` is reachable from `from` (a change is its own ancestor).
pub fn is_ancestor(repo: &Repo, ancestor: &str, from: &str) -> Result<bool> {
    let mut seen = HashSet::new();
    let mut st = vec![from.to_owned()];
    while let Some(id) = st.pop() {
        if id == ancestor {
            return Ok(true);
        }
        if seen.insert(id.clone()) {
            st.extend(repo.load_change(&id)?.parents);
        }
    }
    Ok(false)
}

pub fn file_directory_clash(entries: Vec<Entry>) -> Result<Tree> {
    Tree::new(entries)
        .map_err(|e| Refusal::new("file-directory-conflict", e.message).with(serde_json::json!({})))
}

#[cfg(test)]
#[path = "tests/merge.rs"]
mod tests;
