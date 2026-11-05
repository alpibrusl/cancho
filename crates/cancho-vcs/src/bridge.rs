//! The git bridge (`docs/vcs-cli.md` section 7): `import-git` makes one
//! change per git commit, `export-git` renders every change as a git
//! commit, and the round trip must reproduce every commit id. No program
//! is run; git objects are read and written by `gitread` and this file.
//!
//! What makes the ids reproduce: an imported change keeps the commit's
//! header lines after `parent` and its message byte for byte (`GitCommit`),
//! its parents in order, and its tree flattened to `(path, mode, blob)`,
//! which is enough to rebuild git's nested trees exactly. A change made
//! here (no `GitCommit`) is rendered from its fields.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::rc::Rc;

use serde_json::{Value, json};

use crate::change::{Change, GitCommit, Intent};
use crate::error::{Refusal, Result};
use crate::fsx::{self, hex};
use crate::gitread::{GitId, GitKind, GitRepo, id_of, parse_commit, parse_tree};
use crate::inflate::zlib_store;
use crate::object::{Kind, Store};
use crate::refs::{self, Expect};
use crate::repo::Repo;
use crate::tree::{Entry, Mode, Tree};

type Flat = Rc<Vec<(String, Mode, String)>>;

fn gitmap_path(repo: &Repo) -> std::path::PathBuf {
    repo.vcs.join("gitmap")
}

/// git commit id -> change id, as written by `import-git`.
pub fn load_gitmap(repo: &Repo) -> HashMap<String, String> {
    fs::read_to_string(gitmap_path(repo))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once(' ').map(|(g, c)| (g.to_owned(), c.to_owned())))
        .collect()
}

fn save_gitmap(repo: &Repo, map: &HashMap<String, String>) -> Result<()> {
    let mut lines: Vec<String> = map.iter().map(|(g, c)| format!("{g} {c}\n")).collect();
    lines.sort();
    fsx::write_atomic(&repo.vcs.join("tmp"), &gitmap_path(repo), lines.concat().as_bytes(), false, true)
        .map_err(|e| Refusal::io("cannot write the git map", &e))
}

struct Importer<'a> {
    git: &'a GitRepo,
    store: &'a Store,
    dry: bool,
    blobs: HashMap<GitId, String>,
    trees: HashMap<GitId, Flat>,
    new_blobs: usize,
}

impl Importer<'_> {
    fn blob(&mut self, id: &GitId) -> Result<String> {
        if let Some(b) = self.blobs.get(id) {
            return Ok(b.clone());
        }
        let (kind, body) = self.git.read(id)?;
        if kind != GitKind::Blob {
            return Err(Refusal::new("git-unreadable", format!("{} is not a blob", hex(id))));
        }
        let cid = if self.dry {
            crate::object::object_id(Kind::Blob, &body)
        } else {
            if !self.store.has(Kind::Blob, &crate::object::object_id(Kind::Blob, &body)) {
                self.new_blobs += 1;
            }
            self.store.put(Kind::Blob, &body)?
        };
        self.blobs.insert(*id, cid.clone());
        Ok(cid)
    }

    fn flatten(&mut self, id: &GitId) -> Result<Flat> {
        if let Some(f) = self.trees.get(id) {
            return Ok(f.clone());
        }
        let (kind, body) = self.git.read(id)?;
        if kind != GitKind::Tree {
            return Err(Refusal::new("git-unreadable", format!("{} is not a tree", hex(id))));
        }
        let mut out = Vec::new();
        for (mode, name, eid) in parse_tree(&body)? {
            let name = String::from_utf8(name)
                .map_err(|_| Refusal::new("path-unsupported", "a file name that is not UTF-8"))?;
            match mode {
                0o040000 => {
                    for (p, m, b) in self.flatten(&eid)?.iter() {
                        out.push((format!("{name}/{p}"), *m, b.clone()));
                    }
                }
                0o160000 => {
                    return Err(Refusal::new("submodule-unsupported", format!("`{name}` is a submodule")));
                }
                _ => {
                    let m = match mode {
                        0o100644 => Mode::File,
                        0o100755 => Mode::Exec,
                        0o120000 => Mode::Link,
                        other => {
                            return Err(Refusal::new(
                                "mode-unsupported",
                                format!("`{name}` has mode {other:o}"),
                            ));
                        }
                    };
                    out.push((name, m, self.blob(&eid)?));
                }
            }
        }
        let f = Rc::new(out);
        self.trees.insert(*id, f.clone());
        Ok(f)
    }
}

fn author_fields(headers: &[u8]) -> (String, i64, String) {
    for line in headers.split(|&b| b == b'\n') {
        if let Some(rest) = line.strip_prefix(b"author ") {
            let s = String::from_utf8_lossy(rest).into_owned();
            let mut parts = s.rsplitn(3, ' ');
            let (tz, time, who) = (parts.next(), parts.next(), parts.next());
            if let (Some(tz), Some(time), Some(who)) = (tz, time, who)
                && let Ok(t) = time.parse::<i64>()
            {
                return (who.to_owned(), t, tz.to_owned());
            }
            return (s, 0, "+0000".to_owned());
        }
    }
    (String::new(), 0, "+0000".to_owned())
}

pub struct ImportReport {
    pub commits: usize,
    pub already_imported: usize,
    pub new_blobs: usize,
    pub branches: Vec<(String, String)>,
    pub tags: Vec<(String, String)>,
    pub remotes: Vec<(String, String)>,
    pub mismatches: Vec<(String, String)>,
    pub refs_kept: Vec<String>,
}

impl ImportReport {
    pub fn to_json(&self) -> Value {
        json!({
            "commits": self.commits,
            "already_imported": self.already_imported,
            "new_blobs": self.new_blobs,
            "branches": self.branches.iter().map(|(n, c)| json!({"name": n, "change": c})).collect::<Vec<_>>(),
            "remotes": self.remotes.iter().map(|(n, c)| json!({"name": n, "change": c})).collect::<Vec<_>>(),
            "tags": self.tags.iter().map(|(n, c)| json!({"name": n, "change": c})).collect::<Vec<_>>(),
            "self_check_mismatches": self.mismatches.iter().map(|(g, c)| json!({"git": g, "recomputed": c})).collect::<Vec<_>>(),
            "refs_kept": self.refs_kept,
        })
    }
}

/// Import every commit reachable from the branches, tags and HEAD of `git`.
pub fn import_git(repo: &Repo, git: &GitRepo, dry: bool) -> Result<ImportReport> {
    let mut tips: Vec<(String, GitId)> = git.refs()?;
    if let Some((None, id)) = git.head()? {
        tips.push(("HEAD".to_owned(), id));
    }
    // Peel annotated tags to the commit they name.
    let mut commit_tips = Vec::new();
    for (name, mut id) in tips {
        for _ in 0..8 {
            let (kind, body) = git.read(&id)?;
            match kind {
                GitKind::Commit => {
                    commit_tips.push((name.clone(), id));
                    break;
                }
                GitKind::Tag => {
                    let text = String::from_utf8_lossy(&body);
                    let Some(next) = text.lines().next().and_then(|l| l.strip_prefix("object ")).and_then(crate::gitread::parse_id)
                    else {
                        break;
                    };
                    id = next;
                }
                _ => break,
            }
        }
    }
    // Parents before children, iteratively (history can be deep).
    let mut order: Vec<GitId> = Vec::new();
    let mut done: HashSet<GitId> = HashSet::new();
    let mut commits: HashMap<GitId, crate::gitread::GitCommitData> = HashMap::new();
    for (_, tip) in &commit_tips {
        let mut stack = vec![(*tip, false)];
        while let Some((id, expanded)) = stack.pop() {
            if expanded {
                order.push(id);
                continue;
            }
            if done.contains(&id) {
                continue;
            }
            done.insert(id);
            let (kind, body) = git.read(&id)?;
            if kind != GitKind::Commit {
                return Err(Refusal::new("git-unreadable", format!("{} is not a commit", hex(&id))));
            }
            let c = parse_commit(&body)?;
            stack.push((id, true));
            for p in c.parents.iter().rev() {
                if !done.contains(p) {
                    stack.push((*p, false));
                }
            }
            commits.insert(id, c);
        }
    }

    let mut gitmap = load_gitmap(repo);
    let mut imp = Importer { git, store: &repo.store, dry, blobs: HashMap::new(), trees: HashMap::new(), new_blobs: 0 };
    let mut report = ImportReport {
        commits: 0,
        already_imported: 0,
        new_blobs: 0,
        branches: vec![],
        tags: vec![],
        remotes: vec![],
        mismatches: vec![],
        refs_kept: vec![],
    };
    let mut ids: HashMap<GitId, String> = HashMap::new();
    let mut since_save = 0;
    for gid in &order {
        let gh = hex(gid);
        if let Some(c) = gitmap.get(&gh)
            && (dry || repo.store.has(Kind::Change, c))
        {
            ids.insert(*gid, c.clone());
            report.already_imported += 1;
            continue;
        }
        let c = &commits[gid];
        let flat = imp.flatten(&c.tree)?;
        let tree = Tree::new(flat.iter().map(|(p, m, b)| Entry { path: p.clone(), mode: *m, blob: b.clone() }).collect())?;
        let tree_id = if dry { crate::object::object_id(Kind::Tree, &tree.encode()) } else { tree.save(&repo.store)? };
        let parents: Vec<String> = c
            .parents
            .iter()
            .map(|p| ids.get(p).cloned().ok_or_else(|| Refusal::new("git-unreadable", "a parent was not imported first")))
            .collect::<Result<_>>()?;
        let (author, time, tz) = author_fields(&c.headers);
        let change = Change {
            tree: tree_id,
            parents,
            author,
            time,
            tz,
            intent: Some(Intent {
                prompt: String::from_utf8_lossy(&c.message).trim_end().to_owned(),
                model: None,
                session: None,
                issue: None,
            }),
            decls: vec![],
            edits: vec![],
            attestations: vec![],
            git: Some(GitCommit { headers: crate::change::Bytes(c.headers.clone()), message: crate::change::Bytes(c.message.clone()) }),
        };
        let cid = if dry { crate::object::object_id(Kind::Change, &change.encode()) } else { change.save(&repo.store)? };
        // The self-check: this change must render as the commit it came from.
        let git_ids: Vec<GitId> = c.parents.clone();
        let recomputed = id_of(GitKind::Commit, &commit_bytes(&change, &c.tree, &git_ids));
        // `c.tree` is the original tree id; the tree check is the export's job.
        if recomputed != *gid {
            report.mismatches.push((gh.clone(), hex(&recomputed)));
        }
        gitmap.insert(gh, cid.clone());
        ids.insert(*gid, cid);
        report.commits += 1;
        since_save += 1;
        if since_save >= 200 && !dry {
            save_gitmap(repo, &gitmap)?;
            since_save = 0;
        }
    }
    report.new_blobs = imp.new_blobs;
    if dry {
        return Ok(report);
    }
    save_gitmap(repo, &gitmap)?;
    for (name, id) in git.refs()? {
        let mut cur = id;
        let mut commit = None;
        for _ in 0..8 {
            if let Some(c) = ids.get(&cur) {
                commit = Some(c.clone());
                break;
            }
            let (kind, body) = git.read(&cur)?;
            if kind != GitKind::Tag {
                break;
            }
            match String::from_utf8_lossy(&body).lines().next().and_then(|l| l.strip_prefix("object ")).and_then(crate::gitread::parse_id) {
                Some(n) => cur = n,
                None => break,
            }
        }
        let Some(change) = commit else { continue };
        let (short, bucket) = if let Some(b) = name.strip_prefix("refs/heads/") {
            (format!("heads/{b}"), &mut report.branches)
        } else if let Some(t) = name.strip_prefix("refs/tags/") {
            (format!("tags/{t}"), &mut report.tags)
        } else if let Some(r) = name.strip_prefix("refs/remotes/") {
            (format!("remotes/{r}"), &mut report.remotes)
        } else {
            continue;
        };
        if refs::check_ref_name(&short).is_err() {
            report.refs_kept.push(format!("{name}: not a valid ref name here"));
            continue;
        }
        match refs::read_ref(&repo.vcs, &short)? {
            Some(existing) if existing != change => report.refs_kept.push(format!("{short}: already at {existing}")),
            _ => {
                refs::update_ref(&repo.vcs, &short, &change, &Expect::Any, "import-git")?;
                bucket.push((short, change));
            }
        }
    }
    // HEAD follows git's HEAD when this repository has no change yet.
    if let Some((Some(refname), _)) = git.head()?
        && let Some(b) = refname.strip_prefix("refs/heads/")
        && repo.head_change_id()?.is_none()
        && refs::read_ref(&repo.vcs, &format!("heads/{b}"))?.is_some()
    {
        repo.set_head(&crate::repo::Head::Branch(format!("heads/{b}")))?;
    }
    Ok(report)
}

/// The bytes of the git commit for `change`, given the git ids of its tree
/// and parents.
pub fn commit_bytes(change: &Change, tree: &GitId, parents: &[GitId]) -> Vec<u8> {
    let mut out = format!("tree {}\n", hex(tree)).into_bytes();
    for p in parents {
        out.extend_from_slice(format!("parent {}\n", hex(p)).as_bytes());
    }
    match &change.git {
        Some(g) => {
            out.extend_from_slice(&g.headers.0);
            out.push(b'\n');
            out.extend_from_slice(&g.message.0);
        }
        None => {
            let who = format!("{} {} {}", change.author, change.time, change.tz);
            out.extend_from_slice(format!("author {who}\ncommitter {who}\n\n").as_bytes());
            out.extend_from_slice(native_message(change).as_bytes());
        }
    }
    out
}

/// The commit message of a change made here: the intent prompt, then
/// trailers (`docs/vcs-cli.md` section 7).
fn native_message(change: &Change) -> String {
    let mut m = String::new();
    if let Some(i) = &change.intent {
        m.push_str(i.prompt.trim_end());
        m.push('\n');
    }
    let mut trailers = vec![format!("Cancho-Change: {}", crate::object::object_id(Kind::Change, &change.encode()))];
    if let Some(i) = &change.intent {
        if let Some(s) = &i.session {
            trailers.push(format!("Cancho-Session: {s}"));
        }
        if let Some(s) = &i.model {
            trailers.push(format!("Cancho-Model: {s}"));
        }
        if let Some(s) = &i.issue {
            trailers.push(format!("Cancho-Issue: {s}"));
        }
        trailers.insert(1, "Cancho-Intent: recorded".to_owned());
    }
    m.push('\n');
    m.push_str(&trailers.join("\n"));
    m.push('\n');
    m
}

struct Exporter<'a> {
    repo: &'a Repo,
    git_dir: &'a Path,
    written: HashSet<GitId>,
    blob_ids: HashMap<String, GitId>,
    objects_written: usize,
}

impl Exporter<'_> {
    fn write(&mut self, kind: GitKind, body: &[u8]) -> Result<GitId> {
        let id = id_of(kind, body);
        if self.written.insert(id) {
            let name = hex(&id);
            let path = self.git_dir.join("objects").join(&name[..2]).join(&name[2..]);
            if !path.exists() {
                let mut raw = format!("{} {}\0", kind.name(), body.len()).into_bytes();
                raw.extend_from_slice(body);
                fsx::write_atomic(&self.git_dir.join("objects").join("tmp"), &path, &zlib_store(&raw), false, false)
                    .map_err(|e| Refusal::io(format!("cannot write git object {name}"), &e))?;
                self.objects_written += 1;
            }
        }
        Ok(id)
    }

    fn blob(&mut self, id: &str) -> Result<GitId> {
        if let Some(g) = self.blob_ids.get(id) {
            return Ok(*g);
        }
        let bytes = self.repo.store.get(Kind::Blob, id)?;
        let g = self.write(GitKind::Blob, &bytes)?;
        self.blob_ids.insert(id.to_owned(), g);
        Ok(g)
    }

    /// Git's nested tree for `entries` (all below `depth` bytes of prefix).
    fn tree(&mut self, entries: &[Entry], depth: usize) -> Result<GitId> {
        let mut rows: Vec<(Vec<u8>, String, Vec<u8>, GitId)> = Vec::new(); // sort key, mode, name, id
        let mut i = 0;
        while i < entries.len() {
            let rest = &entries[i].path[depth..];
            match rest.find('/') {
                None => {
                    let id = self.blob(&entries[i].blob)?;
                    rows.push((rest.as_bytes().to_vec(), entries[i].mode.octal().to_owned(), rest.as_bytes().to_vec(), id));
                    i += 1;
                }
                Some(slash) => {
                    let dir = &rest[..slash];
                    let prefix = format!("{}/", &entries[i].path[..depth + slash]);
                    let mut j = i;
                    while j < entries.len() && entries[j].path.starts_with(&prefix) {
                        j += 1;
                    }
                    let id = self.tree(&entries[i..j], depth + slash + 1)?;
                    let mut key = dir.as_bytes().to_vec();
                    key.push(b'/');
                    rows.push((key, "40000".to_owned(), dir.as_bytes().to_vec(), id));
                    i = j;
                }
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let mut body = Vec::new();
        for (_, mode, name, id) in rows {
            body.extend_from_slice(mode.as_bytes());
            body.push(b' ');
            body.extend_from_slice(&name);
            body.push(0);
            body.extend_from_slice(&id);
        }
        self.write(GitKind::Tree, &body)
    }
}

pub struct ExportReport {
    pub commits: usize,
    pub objects_written: usize,
    pub refs: Vec<(String, String)>,
    pub compared: usize,
    pub mismatches: Vec<(String, String, String)>,
}

impl ExportReport {
    pub fn to_json(&self) -> Value {
        json!({
            "commits": self.commits,
            "objects_written": self.objects_written,
            "refs": self.refs.iter().map(|(n, g)| json!({"name": n, "git": g})).collect::<Vec<_>>(),
            "compared_with_import": self.compared,
            "equal": self.compared - self.mismatches.len(),
            "mismatches": self.mismatches.iter().map(|(g, e, c)| json!({"original": g, "exported": e, "change": c})).collect::<Vec<_>>(),
        })
    }
}

/// Render every change reachable from a branch or tag as a git commit in
/// `dir`, a bare git directory (created if absent).
pub fn export_git(repo: &Repo, dir: &Path, dry: bool) -> Result<ExportReport> {
    let mut tips = refs::list_refs(&repo.vcs, "heads")?;
    tips.extend(refs::list_refs(&repo.vcs, "tags")?);
    tips.extend(refs::list_refs(&repo.vcs, "remotes")?);
    if let Some(h) = repo.head_change_id()?
        && !tips.iter().any(|(_, id)| *id == h)
    {
        tips.push(("HEAD".to_owned(), h));
    }
    if !dry {
        for d in ["objects/tmp", "refs/heads", "refs/tags"] {
            fs::create_dir_all(dir.join(d)).map_err(|e| Refusal::io("cannot create the git directory", &e))?;
        }
        let write_new = |name: &str, text: &str| -> Result<()> {
            if dir.join(name).exists() {
                return Ok(());
            }
            fsx::write_atomic(&dir.join("objects").join("tmp"), &dir.join(name), text.as_bytes(), false, true)
                .map_err(|e| Refusal::io(format!("cannot write {name}"), &e))
        };
        write_new("config", "[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = true\n")?;
    }
    let mut ex = Exporter { repo, git_dir: dir, written: HashSet::new(), blob_ids: HashMap::new(), objects_written: 0 };
    let mut git_of: HashMap<String, GitId> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (_, tip) in &tips {
        let mut stack = vec![(tip.clone(), false)];
        while let Some((id, expanded)) = stack.pop() {
            if expanded {
                order.push(id);
                continue;
            }
            if !seen.insert(id.clone()) {
                continue;
            }
            let c = repo.load_change(&id)?;
            stack.push((id, true));
            for p in c.parents.iter().rev() {
                if !seen.contains(p) {
                    stack.push((p.clone(), false));
                }
            }
        }
    }
    let mut trees: HashMap<String, GitId> = HashMap::new();
    for id in &order {
        let c = repo.load_change(id)?;
        let tree_git = match trees.get(&c.tree) {
            Some(t) => *t,
            None => {
                let t = Tree::load(&repo.store, &c.tree)?;
                let g = if dry { [0; 20] } else { ex.tree(t.entries(), 0)? };
                trees.insert(c.tree.clone(), g);
                g
            }
        };
        let parents: Vec<GitId> = c.parents.iter().map(|p| git_of[p]).collect();
        let body = commit_bytes(&c, &tree_git, &parents);
        let g = if dry { id_of(GitKind::Commit, &body) } else { ex.write(GitKind::Commit, &body)? };
        git_of.insert(id.clone(), g);
    }
    let mut report = ExportReport { commits: order.len(), objects_written: ex.objects_written, refs: vec![], compared: 0, mismatches: vec![] };
    if !dry {
        for (name, id) in &tips {
            if name == "HEAD" {
                continue;
            }
            let g = hex(&git_of[id]);
            fsx::write_atomic(&dir.join("objects").join("tmp"), &dir.join("refs").join(name), format!("{g}\n").as_bytes(), false, true)
                .map_err(|e| Refusal::io("cannot write a git ref", &e))?;
            report.refs.push((format!("refs/{name}"), g));
        }
        let head = match repo.head()? {
            crate::repo::Head::Branch(b) => format!("ref: refs/{b}\n"),
            crate::repo::Head::Detached(id) => format!("{}\n", hex(&git_of[&id])),
        };
        fsx::write_atomic(&dir.join("objects").join("tmp"), &dir.join("HEAD"), head.as_bytes(), false, true)
            .map_err(|e| Refusal::io("cannot write git HEAD", &e))?;
    }
    // Every imported commit must come back with its original id.
    for (orig, cid) in load_gitmap(repo) {
        if let Some(g) = git_of.get(&cid) {
            report.compared += 1;
            if hex(g) != orig {
                report.mismatches.push((orig, hex(g), cid));
            }
        }
    }
    report.objects_written = ex.objects_written;
    Ok(report)
}
