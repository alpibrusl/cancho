//! `record` and `revert` (`docs/vcs-cli.md` sections 3 and 4): one change
//! from the working copy, behind the built-in gate. The gate is `cancho
//! check` (the library's `check_candidate`) of every `.cho` file the change
//! adds or modifies, each with the files of the tree that declare the
//! modules it imports and the standard library when it imports that. (The
//! first version checked each file alone and refused half of `cancho-forge`'s
//! files with `unknown-name`: R2 in `docs/vcs-cli.md`.) The first failure's
//! rule tag is the refusal.

use std::collections::BTreeMap;

use cancho_vcs::{
    Change, DeclChange, Edit, Entry, Expect, Head, Intent, Kind, Refusal, Repo, Tree, VcsResult,
    derive_edits, diff_trees, now, object_id, work_lock,
};
use serde_json::{Value, json};

use crate::vcs_args::{Done, Parsed, entry_bytes, rel_path};
use crate::vcs_decl::{decls_of, diff_decls};

pub(crate) fn author(p: &Parsed) -> String {
    p.value("--author")
        .map(str::to_owned)
        .or_else(|| std::env::var("CANCHO_AUTHOR").ok())
        .unwrap_or_else(|| "cancho <cancho@localhost>".to_owned())
}

pub(crate) fn intent(p: &Parsed) -> Option<Intent> {
    let prompt = p.value("--intent-prompt")?;
    Some(Intent {
        prompt: prompt.to_owned(),
        model: p.value("--intent-model").map(str::to_owned),
        session: p.value("--intent-session").map(str::to_owned),
        issue: p.value("--intent-issue").map(str::to_owned),
    })
}

/// Every module a tree declares, as `module path -> files`, so an import
/// can be followed to the files that declare it (`docs/modules.md`).
fn module_index(repo: &Repo, tree: &Tree) -> VcsResult<BTreeMap<String, Vec<String>>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in tree.entries().iter().filter(|e| e.path.ends_with(".cho")) {
        let Ok(text) = String::from_utf8(entry_bytes(repo, e)?) else { continue };
        let Ok(ast) = cancho_syntax::parse(&text) else { continue };
        for m in &ast.modules {
            let name: Vec<&str> = m.path.iter().map(|s| ast.name_of(*s)).collect();
            index.entry(name.join(".")).or_default().push(e.path.clone());
        }
    }
    Ok(index)
}

/// Check the `.cho` files a change adds or modifies. Each is checked as the
/// program it belongs to: itself, the files of the tree that declare the
/// modules it imports (transitively), and the standard library when it
/// imports that. The first failure is the refusal, tagged with its rule.
fn gate(repo: &Repo, tree: &Tree, paths: &[String]) -> VcsResult<Vec<String>> {
    let mut checked = Vec::new();
    let mut index: Option<BTreeMap<String, Vec<String>>> = None;
    for path in paths.iter().filter(|p| p.ends_with(".cho")) {
        if tree.get(path).is_none() {
            continue;
        }
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        let mut queue = vec![path.clone()];
        let mut with_std = false;
        while let Some(p) = queue.pop() {
            if files.contains_key(&p) {
                continue;
            }
            let Some(entry) = tree.get(&p) else { continue };
            let text = String::from_utf8(entry_bytes(repo, entry)?)
                .map_err(|_| gate_failed(&p, "program-shape", "the file is not UTF-8"))?;
            let ast = cancho_syntax::parse(&text)
                .map_err(|d| gate_failed(&p, d.rule.tag(), &d.message))?;
            for m in &ast.modules {
                for i in &m.imports {
                    let segs: Vec<&str> = i.path.iter().map(|s| ast.name_of(*s)).collect();
                    if segs.first() == Some(&"std") {
                        with_std = true;
                        continue;
                    }
                    if index.is_none() {
                        index = Some(module_index(repo, tree)?);
                    }
                    // The longest prefix of the import that names a module.
                    for n in (1..=segs.len()).rev() {
                        if let Some(found) =
                            index.as_ref().and_then(|ix| ix.get(&segs[..n].join(".")))
                        {
                            queue.extend(found.iter().cloned());
                            break;
                        }
                    }
                }
            }
            files.insert(p, text);
        }
        let mut program: Vec<(String, String)> = files.into_iter().collect();
        if with_std {
            program.extend(crate::vcs_cli::std_texts());
        }
        let refs: Vec<(&str, &str)> =
            program.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        if let Err(diags) = cancho_vcs::check_candidate(&refs) {
            let first = diags.first();
            let rule = first.map_or("internal".to_owned(), |d| d.rule.to_string());
            let message = first.map_or(String::new(), |d| d.message.clone());
            let all: Vec<Value> = diags
                .iter()
                .map(|d| json!({"rule": d.rule.to_string(), "message": d.message}))
                .collect();
            return Err(Refusal::new("gate-failed", format!("{path}: {rule}: {message}"))
                .with(json!({"path": path, "rule": rule, "diagnostics": all})));
        }
        checked.push(path.clone());
    }
    Ok(checked)
}

fn gate_failed(path: &str, rule: &str, message: &str) -> Refusal {
    Refusal::new("gate-failed", format!("{path}: {rule}: {message}")).with(
        json!({"path": path, "rule": rule, "diagnostics": [{"rule": rule, "message": message}]}),
    )
}

/// Declarations and edit operations of the move from `old` to `new`.
fn derive(repo: &Repo, old: &Tree, new: &Tree) -> VcsResult<(Vec<DeclChange>, Vec<Edit>)> {
    let (mut decls, mut edits) = (Vec::new(), Vec::new());
    for c in diff_trees(old, new) {
        let load = |e: &Option<Entry>| -> VcsResult<Option<String>> {
            match e {
                Some(e) if e.mode != cancho_vcs::Mode::Link => {
                    Ok(String::from_utf8(entry_bytes(repo, e)?).ok())
                }
                _ => Ok(None),
            }
        };
        let (o, n) = (load(&c.old)?, load(&c.new)?);
        if c.path.ends_with(".cho") {
            let (od, nd) = (o.as_deref().and_then(decls_of), n.as_deref().and_then(decls_of));
            if (c.old.is_none() || od.is_some()) && (c.new.is_none() || nd.is_some()) {
                decls.extend(diff_decls(&c.path, od.as_ref(), nd.as_ref()));
            }
        } else if let (Some(o), Some(n)) = (&o, &n) {
            edits.extend(derive_edits(&c.path, o, n));
        }
    }
    Ok((decls, edits))
}

pub(crate) struct Recorded {
    pub id: String,
    pub change: Change,
    pub fresh: bool,
    pub checked: Vec<String>,
}

/// Shared by `record` and `revert`: store `tree` as a change on HEAD's
/// branch (or HEAD itself when detached), behind the gate and the lock.
pub(crate) fn commit_tree(
    repo: &Repo,
    tree: Tree,
    touched: &[String],
    intent: Option<Intent>,
    p: &Parsed,
    note: &str,
) -> VcsResult<Recorded> {
    let head = repo.head_change_id()?;
    let head_tree = repo.tree_of(head.as_deref())?;
    if tree == head_tree {
        let id = head.ok_or_else(|| {
            Refusal::new("nothing-to-record", "the repository is empty and so is the working copy")
        })?;
        let change = repo.load_change(&id)?;
        return Ok(Recorded { id, change, fresh: false, checked: vec![] });
    }
    let dry = p.flag("--dry-run");
    let checked = if p.flag("--no-gate") { vec![] } else { gate(repo, &tree, touched)? };
    let (decls, edits) = derive(repo, &head_tree, &tree)?;
    let time = match p.value("--time") {
        Some(t) => t
            .parse::<i64>()
            .map_err(|_| Refusal::new("bad-argument", "`--time` is unix seconds"))?,
        None => now(),
    };
    let tree_bytes = tree.encode();
    let change = Change {
        tree: object_id(Kind::Tree, &tree_bytes),
        parents: head.iter().cloned().collect(),
        author: author(p),
        time,
        tz: "+0000".to_owned(),
        intent,
        decls,
        edits,
        attestations: if p.flag("--no-gate") {
            vec![]
        } else {
            vec![format!("gate:cancho-check:ok:{}", checked.len())]
        },
        git: None,
    };
    let id = object_id(Kind::Change, &change.encode());
    if dry {
        return Ok(Recorded { id, change, fresh: true, checked });
    }
    let _lock = work_lock(&repo.vcs)?;
    // Re-read HEAD under the lock: the check above was before it.
    let now_head = repo.head_change_id()?;
    let expect = match p.value("--expect") {
        Some(e) => Expect::Id(repo.resolve(e)?),
        None => match &now_head {
            Some(h) => Expect::Id(h.clone()),
            None => Expect::Absent,
        },
    };
    if now_head != head {
        return Err(Refusal::new("ref-moved", "HEAD moved while the change was being prepared")
            .with(json!({"current": now_head})));
    }
    for e in tree.entries() {
        if !repo.store.has(Kind::Blob, &e.blob) {
            repo.store_working_blob(e)?;
        }
    }
    tree.save(&repo.store)?;
    change.save(&repo.store)?;
    match repo.head()? {
        Head::Branch(b) => repo.move_branch(&b, &id, &expect, note)?,
        Head::Detached(_) => repo.set_head(&Head::Detached(id.clone()))?,
    }
    Ok(Recorded { id, change, fresh: true, checked })
}

pub(crate) fn cmd_record(p: &Parsed) -> VcsResult<Done> {
    let repo = crate::vcs_args::open(p)?;
    let all = p.flag("--all");
    if p.pos.is_empty() && !all {
        return Err(Refusal::new("nothing-selected", "name the paths to record, or pass --all"));
    }
    let work = Tree::new(repo.scan()?)?;
    let head_tree = repo.head_tree()?;
    let mut selected: Vec<String> = Vec::new();
    if all {
        selected.push(String::new());
    }
    for a in &p.pos {
        selected.push(rel_path(&repo, a)?);
    }
    let covers = |path: &str| {
        selected.iter().any(|s| s.is_empty() || path == s || path.starts_with(&format!("{s}/")))
    };
    let mut entries: BTreeMap<String, Entry> =
        head_tree.entries().iter().map(|e| (e.path.clone(), e.clone())).collect();
    let mut touched = Vec::new();
    for c in diff_trees(&head_tree, &work) {
        if !covers(&c.path) {
            continue;
        }
        touched.push(c.path.clone());
        match c.new {
            Some(e) => {
                entries.insert(c.path, e);
            }
            None => {
                entries.remove(&c.path);
            }
        }
    }
    let tree = Tree::new(entries.into_values().collect())?;
    let rec = commit_tree(&repo, tree, &touched, intent(p), p, "record")?;
    let c = &rec.change;
    let branch = match repo.head()? {
        Head::Branch(b) => json!(b.strip_prefix("heads/").unwrap_or(&b)),
        Head::Detached(_) => Value::Null,
    };
    Ok(json!({
        "recorded": rec.fresh && !p.flag("--dry-run"),
        "dry_run": p.flag("--dry-run"),
        "unchanged": !rec.fresh,
        "change": rec.id,
        "tree": c.tree,
        "parents": c.parents,
        "branch": branch,
        "touched": touched,
        "decls": c.decls,
        "edits": c.edits.len(),
        "gate": {"ran": !p.flag("--no-gate"), "checked": rec.checked},
        "intent": c.intent,
    }))
}

pub(crate) fn cmd_revert(p: &Parsed) -> VcsResult<Done> {
    let repo = crate::vcs_args::open(p)?;
    let [spec] = p.pos.as_slice() else {
        return Err(Refusal::new("bad-argument", "`revert` takes one change"));
    };
    let target = repo.resolve(spec)?;
    let tc = repo.load_change(&target)?;
    if tc.parents.len() != 1 {
        return Err(Refusal::new(
            "revert-unsupported",
            "only a change with exactly one parent can be reverted in V1 (merges come with V2)",
        ));
    }
    if !repo.status()?.is_empty() {
        return Err(Refusal::new(
            "dirty-working-copy",
            "record or restore the working copy before a revert",
        )
        .with(
            json!({"paths": repo.status()?.iter().map(|s| s.path.clone()).collect::<Vec<_>>()}),
        ));
    }
    let parent_tree = repo.tree_of(Some(&tc.parents[0]))?;
    let target_tree = Tree::load(&repo.store, &tc.tree)?;
    let head_tree = repo.head_tree()?;
    let mut entries: BTreeMap<String, Entry> =
        head_tree.entries().iter().map(|e| (e.path.clone(), e.clone())).collect();
    let (mut touched, mut conflicts) = (Vec::new(), Vec::new());
    for c in diff_trees(&parent_tree, &target_tree) {
        let now = head_tree.get(&c.path);
        let already_undone = now == c.old.as_ref();
        if already_undone {
            continue;
        }
        if now != c.new.as_ref() {
            conflicts.push(c.path.clone());
            continue;
        }
        touched.push(c.path.clone());
        match c.old {
            Some(e) => {
                entries.insert(c.path, e);
            }
            None => {
                entries.remove(&c.path);
            }
        }
    }
    if !conflicts.is_empty() {
        return Err(Refusal::new("revert-conflict", "later changes touched the same paths")
            .with(json!({"paths": conflicts})));
    }
    let tree = Tree::new(entries.into_values().collect())?;
    let prompt = intent(p).unwrap_or_else(|| Intent {
        prompt: format!("Revert \"{}\"\n\nThis reverts change {target}.", tc.subject()),
        model: None,
        session: None,
        issue: None,
    });
    let rec = commit_tree(&repo, tree.clone(), &touched, Some(prompt), p, "revert")?;
    // The change exists (and passed the gate) before the working copy moves.
    if rec.fresh && !p.flag("--dry-run") {
        let _lock = work_lock(&repo.vcs)?;
        for ch in diff_trees(&head_tree, &tree) {
            match ch.new {
                Some(e) => repo.materialize(&e)?,
                None => repo.remove_path(&ch.path)?,
            }
        }
    }
    Ok(json!({"reverted": target, "change": rec.id, "unchanged": !rec.fresh, "touched": touched,
               "dry_run": p.flag("--dry-run")}))
}
