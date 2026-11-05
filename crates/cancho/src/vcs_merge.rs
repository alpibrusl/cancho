//! `merge`, `conflicts`, `resolve` and `replay` (`docs/vcs-cli.md` sections 4
//! and 6). One operation at a time: a merge or a replay that stops on
//! conflicts leaves `merge-state.json` (the three changes and the
//! resolutions so far) and records nothing; the working copy is not touched
//! until the change is recorded, so `merge --abort` is deleting that file.
//! Conflicts are data, in the command's JSON, with a rule tag each.

use std::collections::{BTreeMap, HashMap};

use cancho_vcs::{
    Change, Choice, Conflict, Entry, Expect, Head, Inputs, Intent, Kind, MergeState, Mode, Refusal,
    Repo, Resolutions, Settled, Tree, TreeMerge, VcsResult, derive_edits, diff_trees,
    file_directory_clash, is_ancestor, merge_base, merge_trees, now, object_id, read_ref,
    work_lock,
};
use serde_json::{Value, json};

use crate::vcs_args::{Done, Parsed, entry_bytes, open, rel_path, short};
use crate::vcs_gate::{Gated, Pending, gate, gate_roots};
use crate::vcs_record::{author, derive};

const INLINE_LIMIT: usize = 256 * 1024;

fn bad(msg: &str) -> Refusal {
    Refusal::new("bad-argument", msg)
}

/// Where the operation lands: the ref that moves and the change it starts from.
struct Target {
    branch: Option<String>,
    checkout: bool,
    ours: String,
}

fn target(repo: &Repo, p: &Parsed) -> VcsResult<Target> {
    if let Some(spec) = p.value("--onto") {
        let named = [format!("heads/{spec}"), spec.to_owned()]
            .into_iter()
            .find(|n| cancho_vcs::check_ref_name(n).is_ok() && n.starts_with("heads/"))
            .and_then(|n| read_ref(&repo.vcs, &n).ok().flatten().map(|id| (n, id)));
        if let Some((name, ours)) = named {
            let checkout = matches!(repo.head()?, Head::Branch(b) if b == name);
            return Ok(Target { branch: Some(name), checkout, ours });
        }
        if !p.flag("--dry-run") {
            return Err(bad("`--onto` names a branch (or, with `--dry-run`, any change)"));
        }
        let ours = repo.resolve(spec)?;
        return Ok(Target { branch: None, checkout: false, ours });
    }
    let ours = repo
        .head_change_id()?
        .ok_or_else(|| Refusal::new("bad-revision", "HEAD has no change yet"))?;
    Ok(match repo.head()? {
        Head::Branch(b) => Target { branch: Some(b), checkout: true, ours },
        Head::Detached(_) => Target { branch: None, checkout: true, ours },
    })
}

fn entry_json(e: &Option<Entry>) -> Value {
    match e {
        Some(e) => json!({"mode": e.mode.octal(), "blob": e.blob}),
        None => Value::Null,
    }
}

fn text_of(repo: &Repo, e: &Option<Entry>) -> Value {
    let Some(e) = e else { return Value::Null };
    match repo.store.get(Kind::Blob, &e.blob) {
        Ok(b) if b.len() <= INLINE_LIMIT => match String::from_utf8(b) {
            Ok(t) => json!(t),
            Err(_) => Value::Null,
        },
        _ => Value::Null,
    }
}

fn conflict_json(repo: &Repo, c: &Conflict) -> Value {
    let decls: Vec<Value> = c
        .decls
        .iter()
        .map(|d| json!({"decl": d.key, "base": d.base, "ours": d.ours, "theirs": d.theirs}))
        .collect();
    let hunks: Vec<Value> = c
        .hunks
        .iter()
        .map(|h| {
            json!({"base_start": h.base_start, "base_lines": h.base_lines,
                   "ours_lines": h.ours_lines, "theirs_lines": h.theirs_lines})
        })
        .collect();
    json!({
        "path": c.path, "tag": c.tag, "layer": c.layer,
        "base": entry_json(&c.base), "ours": entry_json(&c.ours), "theirs": entry_json(&c.theirs),
        "base_text": text_of(repo, &c.base), "ours_text": text_of(repo, &c.ours),
        "theirs_text": text_of(repo, &c.theirs),
        "hunks": hunks, "decls": decls, "detail": c.detail,
    })
}

struct Evaluated {
    merge: TreeMerge,
    ours_tree: Tree,
    theirs_tree: Tree,
    theirs_change: Change,
}

fn evaluate(repo: &Repo, st: &MergeState) -> VcsResult<Evaluated> {
    let tree_of = |id: &str| repo.tree_of(Some(id));
    let (base_tree, ours_tree, theirs_tree) =
        (tree_of(&st.base)?, tree_of(&st.ours)?, tree_of(&st.theirs)?);
    let theirs_change = repo.load_change(&st.theirs)?;
    let edits = if st.op == "replay" {
        Some(replay_edits(repo, &theirs_change, &base_tree, &theirs_tree)?)
    } else {
        None
    };
    let read = |e: &Entry| entry_bytes(repo, e);
    let merge = merge_trees(&Inputs {
        base: &base_tree,
        ours: &ours_tree,
        theirs: &theirs_tree,
        read: &read,
        resolutions: &st.resolutions,
        edits: edits.as_ref(),
    })?;
    Ok(Evaluated { merge, ours_tree, theirs_tree, theirs_change })
}

/// What a change did to each text file, as anchored edits: the ones recorded
/// with it, and for a path with none (an imported change) the ones its diff
/// derives now.
fn replay_edits(
    repo: &Repo,
    change: &Change,
    parent: &Tree,
    new: &Tree,
) -> VcsResult<HashMap<String, Vec<cancho_vcs::Edit>>> {
    let mut map: HashMap<String, Vec<cancho_vcs::Edit>> = HashMap::new();
    for e in &change.edits {
        map.entry(e.path.clone()).or_default().push(e.clone());
    }
    for c in diff_trees(parent, new) {
        if c.path.ends_with(".cho") || map.contains_key(&c.path) {
            continue;
        }
        let (Some(o), Some(n)) = (&c.old, &c.new) else { continue };
        if o.mode == Mode::Link || n.mode == Mode::Link {
            continue;
        }
        let (Ok(ot), Ok(nt)) =
            (String::from_utf8(entry_bytes(repo, o)?), String::from_utf8(entry_bytes(repo, n)?))
        else {
            continue;
        };
        let edits = derive_edits(&c.path, &ot, &nt);
        if !edits.is_empty() {
            map.insert(c.path.clone(), edits);
        }
    }
    Ok(map)
}

fn state_from(repo: &Repo, p: &Parsed, op: &str, spec: &str) -> VcsResult<(MergeState, Target)> {
    let t = target(repo, p)?;
    let theirs = repo.resolve(spec)?;
    let base = if op == "replay" {
        let c = repo.load_change(&theirs)?;
        match c.parents.as_slice() {
            [one] => one.clone(),
            _ => {
                return Err(Refusal::new(
                    "replay-unsupported",
                    "only a change with exactly one parent can be replayed (a merge has none to replay from)",
                ));
            }
        }
    } else {
        merge_base(repo, &t.ours, &theirs)?
            .ok_or_else(|| {
                Refusal::new("unrelated-histories", "the two changes have no common ancestor")
            })?
            .id
    };
    let time = match p.value("--time") {
        Some(v) => v.parse::<i64>().map_err(|_| bad("`--time` is unix seconds"))?,
        None => now(),
    };
    let st = MergeState {
        op: op.to_owned(),
        ours: t.ours.clone(),
        theirs,
        base,
        branch: t.branch.clone(),
        checkout: t.checkout,
        resolutions: Resolutions::new(),
        intent: crate::vcs_record::intent(p),
        author: author(p),
        time,
        no_gate: p.flag("--no-gate"),
    };
    Ok((st, t))
}

/// The operation in progress, or none: one whose target moved since is stale
/// (it finished, or someone moved the ref) and is dropped.
fn load_state(repo: &Repo) -> VcsResult<Option<MergeState>> {
    let Some(st) = MergeState::load(&repo.vcs)? else { return Ok(None) };
    let tip = match &st.branch {
        Some(b) => read_ref(&repo.vcs, b)?,
        None => repo.head_change_id()?,
    };
    if tip.as_deref() != Some(st.ours.as_str()) {
        MergeState::clear(&repo.vcs)?;
        return Ok(None);
    }
    Ok(Some(st))
}

fn dirty_check(repo: &Repo) -> VcsResult<()> {
    let st = repo.status()?;
    if st.is_empty() {
        return Ok(());
    }
    Err(Refusal::new("dirty-working-copy", "record or restore the working copy first")
        .with(json!({"paths": st.iter().map(|s| s.path.clone()).collect::<Vec<_>>()})))
}

fn expect_of(repo: &Repo, p: &Parsed, ours: &str) -> VcsResult<Expect> {
    Ok(match p.value("--expect") {
        Some(e) => Expect::Id(repo.resolve(e)?),
        None => Expect::Id(ours.to_owned()),
    })
}

fn move_target(
    repo: &Repo,
    st: &MergeState,
    new: &str,
    expect: &Expect,
    note: &str,
) -> VcsResult<()> {
    match &st.branch {
        Some(b) => repo.move_branch(b, new, expect, note),
        None => repo.set_head(&Head::Detached(new.to_owned())),
    }
}

fn apply_to_working_copy(repo: &Repo, from: &Tree, to: &Tree) -> VcsResult<()> {
    for ch in diff_trees(from, to) {
        match ch.new {
            Some(e) => repo.materialize(&e)?,
            None => repo.remove_path(&ch.path)?,
        }
    }
    Ok(())
}

fn short_intent(c: &Change) -> Value {
    json!({"prompt": c.subject(), "intent": c.intent})
}

/// Record the settled operation, or report what is open. `dry` writes nothing.
fn finish(repo: &Repo, p: &Parsed, st: &MergeState, ev: Evaluated, dry: bool) -> VcsResult<Done> {
    let theirs_id = &st.theirs;
    let conflicts: Vec<Value> = ev.merge.conflicts.iter().map(|c| conflict_json(repo, c)).collect();
    let merged: Vec<Value> = ev
        .merge
        .merged_paths
        .iter()
        .map(|(path, layer)| json!({"path": path, "layer": layer}))
        .collect();
    let base_doc = json!({"op": st.op, "ours": st.ours, "theirs": st.theirs, "base": st.base,
                          "dry_run": dry, "merged_paths": merged});
    let mut doc = base_doc.as_object().cloned().unwrap_or_default();
    if !ev.merge.conflicts.is_empty() {
        if !dry {
            st.save(&repo.vcs)?;
        }
        doc.insert("merged".into(), json!(false));
        doc.insert("open_conflicts".into(), json!(conflicts.len()));
        doc.insert("conflicts".into(), json!(conflicts));
        doc.insert("in_progress".into(), json!(!dry));
        return Ok(Value::Object(doc));
    }
    let tree = file_directory_clash(ev.merge.entries.clone())?;
    let pending: Pending = ev.merge.new_blobs.clone();
    let touched: Vec<String> =
        diff_trees(&ev.ours_tree, &tree).into_iter().map(|c| c.path).collect();
    let mut gated = Gated { checked: Vec::new(), external: Vec::new() };
    if !st.no_gate {
        gated = match gate(repo, &tree, &touched, &pending, &gate_roots(repo, p)) {
            Ok(c) => c,
            Err(r) if r.tag == "gate-failed" && st.op == "merge" => {
                let ours = repo.load_change(&st.ours)?;
                let both =
                    json!({"ours": short_intent(&ours), "theirs": short_intent(&ev.theirs_change)});
                let mut detail = r.detail.clone();
                if let Some(m) = detail.as_object_mut() {
                    m.insert("intents".into(), both);
                }
                return Err(Refusal::new("merge-gate-failed", r.message).with(detail));
            }
            Err(r) => return Err(r),
        };
    }
    let (decls, edits) = derive(repo, &pending, &ev.ours_tree, &tree)?;
    let tree_bytes = tree.encode();
    let tree_id = object_id(Kind::Tree, &tree_bytes);
    let unchanged = st.op == "replay" && tree == ev.ours_tree;
    let exact = tree == ev.theirs_tree;
    doc.insert("tree".into(), json!(tree_id));
    doc.insert("gate".into(), gated.json(!st.no_gate));
    if st.op == "replay" {
        doc.insert(
            "result".into(),
            json!(if unchanged {
                "unchanged"
            } else if exact {
                "exact"
            } else {
                "moved"
            }),
        );
    }
    if unchanged {
        MergeState::clear(&repo.vcs).ok();
        doc.insert("merged".into(), json!(false));
        doc.insert("unchanged".into(), json!(true));
        doc.insert("open_conflicts".into(), json!(0));
        return Ok(Value::Object(doc));
    }
    let intent = match (&st.intent, st.op.as_str()) {
        (Some(i), _) => Some(i.clone()),
        (None, "replay") => ev.theirs_change.intent.clone(),
        (None, _) => Some(Intent {
            prompt: format!(
                "Merge {}\n\nMerges change {theirs_id} (\"{}\") into {}.",
                short(theirs_id),
                ev.theirs_change.subject(),
                short(&st.ours)
            ),
            model: None,
            session: None,
            issue: None,
        }),
    };
    let mut attestations = Vec::new();
    if !st.no_gate {
        attestations.push(format!("gate:cancho-check:ok:{}", gated.checked.len()));
    }
    if st.op == "replay" {
        attestations.push(format!("replay:{theirs_id}"));
    }
    let change = Change {
        tree: tree_id.clone(),
        parents: if st.op == "replay" {
            vec![st.ours.clone()]
        } else {
            vec![st.ours.clone(), st.theirs.clone()]
        },
        author: if st.op == "replay" && p.value("--author").is_none() {
            ev.theirs_change.author.clone()
        } else {
            st.author.clone()
        },
        time: st.time,
        tz: "+0000".to_owned(),
        intent,
        decls,
        edits,
        attestations,
        git: None,
    };
    let id = object_id(Kind::Change, &change.encode());
    doc.insert("change".into(), json!(id));
    doc.insert("merged".into(), json!(!dry));
    doc.insert("would_record".into(), json!(dry));
    doc.insert("open_conflicts".into(), json!(0));
    doc.insert("touched".into(), json!(touched));
    if dry {
        return Ok(Value::Object(doc));
    }
    let expect = expect_of(repo, p, &st.ours)?;
    let _lock = work_lock(&repo.vcs)?;
    for (blob, bytes) in &ev.merge.new_blobs {
        let stored = repo.store.put(Kind::Blob, bytes)?;
        debug_assert_eq!(&stored, blob);
    }
    tree.save(&repo.store)?;
    change.save(&repo.store)?;
    move_target(repo, st, &id, &expect, if st.op == "replay" { "replay" } else { "merge" })?;
    if st.checkout {
        apply_to_working_copy(repo, &ev.ours_tree, &tree)?;
    }
    MergeState::clear(&repo.vcs)?;
    doc.insert("in_progress".into(), json!(false));
    Ok(Value::Object(doc))
}

fn fast_forward(repo: &Repo, p: &Parsed, st: &MergeState, dry: bool) -> VcsResult<Done> {
    let (ours_tree, theirs_tree) = (repo.tree_of(Some(&st.ours))?, repo.tree_of(Some(&st.theirs))?);
    let doc = json!({"op": "merge", "ours": st.ours, "theirs": st.theirs, "base": st.base,
                     "fast_forward": true, "merged": !dry, "would_record": dry, "dry_run": dry,
                     "change": st.theirs, "tree": repo.load_change(&st.theirs)?.tree,
                     "open_conflicts": 0, "touched": diff_trees(&ours_tree, &theirs_tree)
                        .into_iter().map(|c| c.path).collect::<Vec<_>>()});
    if dry {
        return Ok(doc);
    }
    let expect = expect_of(repo, p, &st.ours)?;
    let _lock = work_lock(&repo.vcs)?;
    move_target(repo, st, &st.theirs, &expect, "merge (fast-forward)")?;
    if st.checkout {
        apply_to_working_copy(repo, &ours_tree, &theirs_tree)?;
    }
    Ok(doc)
}

pub(crate) fn cmd_merge(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    if p.flag("--abort") {
        let _lock = work_lock(&repo.vcs)?;
        let was = !p.flag("--dry-run") && MergeState::clear(&repo.vcs)?;
        return Ok(json!({"aborted": was, "dry_run": p.flag("--dry-run")}));
    }
    let [spec] = p.pos.as_slice() else {
        return Err(bad("`merge` takes one change, branch or tag (or `--abort`)"));
    };
    let dry = p.flag("--dry-run");
    if let Some(st) = load_state(&repo)? {
        if st.op != "merge" || repo.resolve(spec)? != st.theirs {
            return Err(Refusal::new(
                "merge-in-progress",
                "a merge or replay is open: settle it with `resolve`, or `merge --abort`",
            )
            .with(json!({"op": st.op, "theirs": st.theirs})));
        }
        let ev = evaluate(&repo, &st)?;
        return finish(&repo, p, &st, ev, dry);
    }
    let (st, t) = state_from(&repo, p, "merge", spec)?;
    if t.checkout && !dry {
        dirty_check(&repo)?;
    }
    if is_ancestor(&repo, &st.theirs, &st.ours)? {
        return Ok(json!({"op": "merge", "ours": st.ours, "theirs": st.theirs,
                         "up_to_date": true, "merged": false, "open_conflicts": 0}));
    }
    if st.base == st.ours && !p.flag("--no-ff") {
        return fast_forward(&repo, p, &st, dry);
    }
    let ev = evaluate(&repo, &st)?;
    finish(&repo, p, &st, ev, dry)
}

pub(crate) fn cmd_replay(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [spec] = p.pos.as_slice() else { return Err(bad("`replay` takes one change")) };
    let dry = p.flag("--dry-run");
    if let Some(st) = load_state(&repo)? {
        if st.op != "replay" || repo.resolve(spec)? != st.theirs {
            return Err(Refusal::new(
                "merge-in-progress",
                "a merge or replay is open: settle it with `resolve`, or `merge --abort`",
            )
            .with(json!({"op": st.op, "theirs": st.theirs})));
        }
        let ev = evaluate(&repo, &st)?;
        return finish(&repo, p, &st, ev, dry);
    }
    let (st, t) = state_from(&repo, p, "replay", spec)?;
    if t.checkout && !dry {
        dirty_check(&repo)?;
    }
    let ev = evaluate(&repo, &st)?;
    finish(&repo, p, &st, ev, dry)
}

pub(crate) fn cmd_conflicts(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let Some(st) = load_state(&repo)? else {
        return Ok(json!({"in_progress": false, "open_conflicts": 0, "conflicts": []}));
    };
    let ev = evaluate(&repo, &st)?;
    let conflicts: Vec<Value> =
        ev.merge.conflicts.iter().map(|c| conflict_json(&repo, c)).collect();
    Ok(json!({"in_progress": true, "op": st.op, "ours": st.ours, "theirs": st.theirs,
              "base": st.base, "open_conflicts": conflicts.len(), "conflicts": conflicts}))
}

pub(crate) fn cmd_resolve(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [arg] = p.pos.as_slice() else { return Err(bad("`resolve` takes one path")) };
    let path = rel_path(&repo, arg)?;
    let Some(mut st) = load_state(&repo)? else {
        return Err(Refusal::new("no-merge-in-progress", "there is no merge or replay to resolve"));
    };
    let (take, with) = (p.value("--take"), p.value("--with"));
    if take.is_some() == with.is_some() {
        return Err(bad("name one of `--take ours|theirs|base` or `--with <file>`"));
    }
    let decl = p.value("--decl");
    // The conflict as it stands without this resolution.
    let mut trial = st.clone();
    match decl {
        None => {
            trial.resolutions.remove(&path);
        }
        Some(d) => {
            if let Some(Settled::Decls(m)) = trial.resolutions.get_mut(&path) {
                m.remove(d);
            }
        }
    }
    let ev = evaluate(&repo, &trial)?;
    let Some(conflict) = ev.merge.conflicts.iter().find(|c| c.path == path) else {
        return Err(Refusal::new("not-in-conflict", format!("`{path}` has no open conflict"))
            .with(json!({"open": ev.merge.conflicts.iter().map(|c| c.path.clone()).collect::<Vec<_>>()})));
    };
    let side = |name: &str| -> VcsResult<Option<Entry>> {
        Ok(match name {
            "ours" => conflict.ours.clone(),
            "theirs" => conflict.theirs.clone(),
            "base" => conflict.base.clone(),
            _ => return Err(bad("`--take` is `ours`, `theirs` or `base`")),
        })
    };
    let file_bytes = |f: &str| -> VcsResult<Vec<u8>> {
        std::fs::read(f).map_err(|e| Refusal::io(format!("cannot read `{f}`"), &e))
    };
    let settled = match decl {
        Some(d) => {
            if !conflict.decls.iter().any(|c| c.key == d) {
                return Err(Refusal::new("decl-not-found", format!("`{d}` is not in conflict in `{path}`"))
                    .with(json!({"open": conflict.decls.iter().map(|c| c.key.clone()).collect::<Vec<_>>()})));
            }
            let choice = match (take, with) {
                (Some("ours"), _) => Choice::Ours,
                (Some("theirs"), _) => Choice::Theirs,
                (Some("base"), _) => Choice::Base,
                (Some(_), _) => return Err(bad("`--take` is `ours`, `theirs` or `base`")),
                (None, Some(f)) => {
                    Choice::Text(String::from_utf8(file_bytes(f)?).map_err(|_| {
                        Refusal::new("binary-file", "the declaration text is not UTF-8")
                    })?)
                }
                _ => unreachable!("exactly one of take and with is set"),
            };
            let mut m: BTreeMap<String, Choice> = match st.resolutions.get(&path) {
                Some(Settled::Decls(m)) => m.clone(),
                _ => BTreeMap::new(),
            };
            m.insert(d.to_owned(), choice);
            Settled::Decls(m)
        }
        None => match (take, with) {
            (Some(name), _) => {
                Settled::Entry(side(name)?.map(|e| (e.mode.octal().to_owned(), e.blob)))
            }
            (None, Some(f)) => {
                let bytes = file_bytes(f)?;
                let mode = conflict
                    .ours
                    .as_ref()
                    .or(conflict.theirs.as_ref())
                    .or(conflict.base.as_ref())
                    .map_or(Mode::File, |e| e.mode);
                let blob = if p.flag("--dry-run") {
                    object_id(Kind::Blob, &bytes)
                } else {
                    repo.store.put(Kind::Blob, &bytes)?
                };
                Settled::Entry(Some((mode.octal().to_owned(), blob)))
            }
            _ => unreachable!("exactly one of take and with is set"),
        },
    };
    let dry = p.flag("--dry-run");
    st.resolutions.insert(path.clone(), settled);
    if !dry {
        st.save(&repo.vcs)?;
    }
    let ev = evaluate(&repo, &st)?;
    let mut out = finish(&repo, p, &st, ev, dry)?;
    if let Some(m) = out.as_object_mut() {
        m.insert("resolved".into(), json!(path));
    }
    Ok(out)
}
