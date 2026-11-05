//! The repository commands of `cancho vcs` other than `record` and the git
//! bridge: `init`, `status`, `diff`, `log`, `show`, `blame`, `branch`,
//! `switch`, `restore`, `verify`, `unlock` and `apply`
//! (`docs/vcs-cli.md` section 4).

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use cancho_vcs::{
    Change, ChangeKind, Edit, Entry, Expect, Head, Kind, Refusal, Repo, Tree, VcsResult,
    apply_edits, check_ref_name, delete_ref, diff_lines, diff_trees, is_id, list_refs, locks,
    read_ref, render_unified, split_lines, unified, update_ref, work_lock,
};
use serde_json::{Value, json};

use crate::vcs_args::{Done, Parsed, entry_bytes, open, parse, rel_path, run, short};
use crate::vcs_decl::{decls_of, diff_decls};
use crate::{Failure, vcs_git, vcs_record};

const INTENT_FLAGS: [&str; 7] = [
    "--intent-prompt",
    "--intent-model",
    "--intent-session",
    "--intent-issue",
    "--author",
    "--time",
    "--expect",
];

pub(crate) fn dispatch(cmd: &str, args: &[String]) -> Result<ExitCode, Failure> {
    match cmd {
        "init" => {
            let p = parse(args, &["--branch"], &[])?;
            run(cmd, &p, || cmd_init(&p))
        }
        "status" => {
            let p = parse(args, &[], &[])?;
            run(cmd, &p, || cmd_status(&p))
        }
        "diff" => {
            let p = parse(args, &["--context"], &[])?;
            run(cmd, &p, || cmd_diff(&p))
        }
        "record" => {
            let p = parse(args, &INTENT_FLAGS, &["--all", "--no-gate"])?;
            run(cmd, &p, || vcs_record::cmd_record(&p))
        }
        "revert" => {
            let p = parse(args, &INTENT_FLAGS, &["--no-gate"])?;
            run(cmd, &p, || vcs_record::cmd_revert(&p))
        }
        "log" => {
            let p = parse(
                args,
                &["--limit", "--intent", "--session", "--issue", "--path", "--author"],
                &[],
            )?;
            run(cmd, &p, || cmd_log(&p))
        }
        "show" => {
            let p = parse(args, &[], &["--patch"])?;
            run(cmd, &p, || cmd_show(&p))
        }
        "blame" => {
            let p = parse(args, &["--rev", "--decl"], &[])?;
            run(cmd, &p, || cmd_blame(&p))
        }
        "branch" => {
            let p = parse(args, &["--from", "--expect"], &[])?;
            run(cmd, &p, || cmd_branch(&p))
        }
        "switch" => {
            let p = parse(args, &[], &["--keep", "--discard", "--create", "--detach"])?;
            run(cmd, &p, || cmd_switch(&p))
        }
        "restore" => {
            let p = parse(args, &["--from"], &[])?;
            run(cmd, &p, || cmd_restore(&p))
        }
        "verify" => {
            let p = parse(args, &[], &[])?;
            run(cmd, &p, || cmd_verify(&p))
        }
        "unlock" => {
            let p = parse(args, &[], &[])?;
            run(cmd, &p, || cmd_unlock(&p))
        }
        "apply" => {
            let p = parse(args, &[], &[])?;
            run(cmd, &p, || cmd_apply(&p))
        }
        "import-git" => {
            let p = parse(args, &[], &["--no-checkout"])?;
            run(cmd, &p, || vcs_git::cmd_import(&p))
        }
        "export-git" => {
            let p = parse(args, &[], &[])?;
            run(cmd, &p, || vcs_git::cmd_export(&p))
        }
        other => Err(crate::usage(format!("unknown `vcs` subcommand `{other}`"))),
    }
}

fn bad(msg: &str) -> Refusal {
    Refusal::new("bad-argument", msg)
}

fn cmd_init(p: &Parsed) -> VcsResult<Done> {
    let dir = p.pos.first().map_or(".", String::as_str);
    let branch = p.value("--branch").unwrap_or("main");
    if p.flag("--dry-run") {
        let exists = Repo::open(Path::new(dir)).is_ok();
        return Ok((
            json!({"created": false, "would_create": !exists, "branch": branch}),
            String::new(),
        ));
    }
    fs::create_dir_all(dir).map_err(|e| Refusal::io("cannot create the directory", &e))?;
    let root =
        fs::canonicalize(dir).map_err(|e| Refusal::io("cannot resolve the directory", &e))?;
    let (repo, created) = Repo::init(&root, branch)?;
    Ok((
        json!({"created": created, "root": repo.root.display().to_string(), "branch": branch}),
        format!(
            "{} {}\n",
            if created { "initialised" } else { "already a repository:" },
            repo.root.display()
        ),
    ))
}

fn head_json(repo: &Repo) -> VcsResult<(Value, Value)> {
    Ok(match repo.head()? {
        Head::Branch(b) => {
            (json!(b.strip_prefix("heads/").unwrap_or(&b)), json!(repo.head_change_id()?))
        }
        Head::Detached(id) => (Value::Null, json!(id)),
    })
}

fn cmd_status(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let st = repo.status()?;
    let (branch, head) = head_json(&repo)?;
    let mut text = format!("on {}\n", branch.as_str().unwrap_or("(detached)"));
    for s in &st {
        text.push_str(&format!("  {:<9} {}\n", s.kind.name(), s.path));
    }
    if st.is_empty() {
        text.push_str("nothing to record\n");
    }
    Ok((
        json!({"branch": branch, "head": head, "clean": st.is_empty(),
               "changes": st.iter().map(|s| json!({"path": s.path, "state": s.kind.name()})).collect::<Vec<_>>()}),
        text,
    ))
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes.iter().take(8000).any(|&b| b == 0) && std::str::from_utf8(bytes).is_ok()
}

fn blob_of(repo: &Repo, e: &Option<Entry>) -> VcsResult<Option<Vec<u8>>> {
    match e {
        None => Ok(None),
        Some(e) => entry_bytes(repo, e).map(Some),
    }
}

/// The diff document for the paths that differ between two trees.
fn diff_files(
    repo: &Repo,
    changes: Vec<cancho_vcs::PathChange>,
    filter: &[String],
    context: usize,
    text_out: &mut String,
) -> VcsResult<Vec<Value>> {
    let mut files = Vec::new();
    for c in changes {
        if !filter.is_empty()
            && !filter
                .iter()
                .any(|f| f.is_empty() || c.path == *f || c.path.starts_with(&format!("{f}/")))
        {
            continue;
        }
        let (old, new) = (blob_of(repo, &c.old)?, blob_of(repo, &c.new)?);
        let mut file = json!({"path": c.path, "status": c.kind.name()});
        let binary = old.iter().chain(new.iter()).any(|b| !is_text(b));
        file["binary"] = json!(binary);
        if let (Some(o), Some(n)) = (&c.old, &c.new)
            && o.mode != n.mode
        {
            file["mode"] = json!({"old": o.mode.octal(), "new": n.mode.octal()});
        }
        if !binary && c.kind != ChangeKind::Mode {
            let (ot, nt) = (
                old.as_deref().map_or("", |b| std::str::from_utf8(b).unwrap_or("")),
                new.as_deref().map_or("", |b| std::str::from_utf8(b).unwrap_or("")),
            );
            if c.path.ends_with(".cho") {
                let (od, nd) = (decls_of(ot), decls_of(nt));
                if (old.is_none() || od.is_some()) && (new.is_none() || nd.is_some()) {
                    file["decls"] = json!(diff_decls(&c.path, od.as_ref(), nd.as_ref()));
                }
            }
            let hunks = unified(ot, nt, context);
            text_out.push_str(&render_unified(
                &format!("a/{}", c.path),
                &format!("b/{}", c.path),
                &hunks,
            ));
            file["hunks"] = json!(
                hunks
                    .iter()
                    .map(|h| json!({
                        "old_start": h.old_start, "old_lines": h.old_lines,
                        "new_start": h.new_start, "new_lines": h.new_lines,
                        "lines": h.lines.iter().map(|(o, t)| json!({"op": o.to_string(), "text": t})).collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>()
            );
        } else {
            text_out.push_str(&format!(
                "{}: {}\n",
                c.path,
                if binary { "binary file differs" } else { "mode changed" }
            ));
        }
        files.push(file);
    }
    Ok(files)
}

fn cmd_diff(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let context: usize = p
        .value("--context")
        .map_or(Ok(3), |c| c.parse().map_err(|_| bad("`--context` is a count")))?;
    // Arguments that name a revision are revisions; the rest are paths.
    let mut revs = Vec::new();
    let mut paths = Vec::new();
    for a in &p.pos {
        if revs.len() < 2 && repo.resolve(a).is_ok() && !Path::new(a).exists() {
            revs.push(repo.resolve(a)?);
        } else {
            paths.push(rel_path(&repo, a)?);
        }
    }
    let (from_name, to_name, a, b) = match revs.as_slice() {
        [] => (
            "HEAD".to_owned(),
            "working copy".to_owned(),
            repo.head_tree()?,
            Tree::new(repo.scan()?)?,
        ),
        [x] => {
            (x.clone(), "working copy".to_owned(), repo.tree_of(Some(x))?, Tree::new(repo.scan()?)?)
        }
        [x, y] => (x.clone(), y.clone(), repo.tree_of(Some(x))?, repo.tree_of(Some(y))?),
        _ => unreachable!("at most two revisions are taken"),
    };
    let mut text = String::new();
    let files = diff_files(&repo, diff_trees(&a, &b), &paths, context, &mut text)?;
    Ok((json!({"from": from_name, "to": to_name, "files": files}), text))
}

fn change_json(id: &str, c: &Change) -> Value {
    json!({"change": id, "parents": c.parents, "author": c.author, "time": c.time, "tz": c.tz,
           "subject": c.subject(), "intent": c.intent})
}

fn touches(repo: &Repo, c: &Change, path: &str) -> VcsResult<bool> {
    let tree = Tree::load(&repo.store, &c.tree)?;
    let new = tree.get(path).map(|e| (e.blob.clone(), e.mode));
    let old = match c.parents.first() {
        None => None,
        Some(par) => repo.tree_of(Some(par))?.get(path).map(|e| (e.blob.clone(), e.mode)),
    };
    Ok(new != old)
}

fn cmd_log(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let start = match p.pos.first() {
        Some(r) => repo.resolve(r)?,
        None => match repo.head_change_id()? {
            Some(h) => h,
            None => return Ok((json!({"changes": []}), "no changes yet\n".to_owned())),
        },
    };
    let limit: usize = p
        .value("--limit")
        .map_or(Ok(usize::MAX), |l| l.parse().map_err(|_| bad("`--limit` is a count")))?;
    let path = p.value("--path").map(|a| rel_path(&repo, a)).transpose()?;
    let mut rows = Vec::new();
    let mut text = String::new();
    for (id, c) in repo.history(&start)? {
        let i = c.intent.as_ref();
        let ok = p.value("--intent").is_none_or(|q| i.is_some_and(|i| i.prompt.contains(q)))
            && p.value("--session").is_none_or(|q| i.and_then(|i| i.session.as_deref()) == Some(q))
            && p.value("--issue").is_none_or(|q| i.and_then(|i| i.issue.as_deref()) == Some(q))
            && p.value("--author").is_none_or(|q| c.author.contains(q));
        if !ok {
            continue;
        }
        if let Some(path) = &path
            && !touches(&repo, &c, path)?
        {
            continue;
        }
        text.push_str(&format!("{} {} {}\n", short(&id), c.time, c.subject()));
        rows.push(change_json(&id, &c));
        if rows.len() >= limit {
            break;
        }
    }
    Ok((json!({"changes": rows}), text))
}

fn cmd_show(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let id = repo.resolve(p.pos.first().map_or("HEAD", String::as_str))?;
    let c = repo.load_change(&id)?;
    let new = Tree::load(&repo.store, &c.tree)?;
    let old = repo.tree_of(c.parents.first().map(String::as_str))?;
    let changes = diff_trees(&old, &new);
    let summary: Vec<Value> =
        changes.iter().map(|x| json!({"path": x.path, "state": x.kind.name()})).collect();
    let mut text = format!(
        "change {id}\nauthor {}\ntime {}\n\n{}\n\n",
        c.author,
        c.time,
        c.intent.as_ref().map_or("", |i| i.prompt.as_str())
    );
    for s in &changes {
        text.push_str(&format!("  {:<9} {}\n", s.kind.name(), s.path));
    }
    let mut doc = change_json(&id, &c);
    doc["tree"] = json!(c.tree);
    doc["files"] = json!(summary);
    doc["decls"] = json!(c.decls);
    doc["edits"] = json!(c.edits);
    doc["attestations"] = json!(c.attestations);
    if p.flag("--patch") {
        let mut patch = String::new();
        doc["patch"] = json!(diff_files(&repo, changes, &[], 3, &mut patch)?);
        text.push_str(&patch);
    }
    Ok((doc, text))
}

fn cmd_blame(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [arg] = p.pos.as_slice() else { return Err(bad("`blame` takes one path")) };
    let path = rel_path(&repo, arg)?;
    let start = match p.value("--rev") {
        Some(r) => repo.resolve(r)?,
        None => repo.resolve("HEAD")?,
    };
    let content = |c: &Change| -> VcsResult<Option<String>> {
        match Tree::load(&repo.store, &c.tree)?.get(&path) {
            None => Ok(None),
            Some(e) => String::from_utf8(repo.store.get(Kind::Blob, &e.blob)?)
                .map(Some)
                .map_err(|_| Refusal::new("binary-file", format!("`{path}` is not text"))),
        }
    };
    let top = repo.load_change(&start)?;
    let final_text = content(&top)?.ok_or_else(|| {
        Refusal::new("path-not-found", format!("`{path}` is not in {}", short(&start)))
    })?;
    if let Some(decl) = p.value("--decl") {
        return blame_decl(&repo, &start, &path, decl, &final_text);
    }
    let final_lines = split_lines(&final_text);
    let mut owner: Vec<Option<String>> = vec![None; final_lines.len()];
    // For the version being compared: the final line each of its lines is,
    // or None once that line has been attributed to a change.
    let mut map: Vec<Option<usize>> = (0..final_lines.len()).map(Some).collect();
    let (mut id, mut change, mut text) = (start.clone(), top, final_text.clone());
    loop {
        if map.iter().all(Option::is_none) {
            break;
        }
        let parent = match change.parents.first() {
            Some(pid) => Some((pid.clone(), repo.load_change(pid)?)),
            None => None,
        };
        let ptext = match &parent {
            Some((_, pc)) => content(pc)?,
            None => None,
        };
        let (Some((parent_id, parent)), Some(ptext)) = (parent, ptext) else {
            for f in map.iter().flatten() {
                owner[*f] = Some(id.clone());
            }
            break;
        };
        if ptext != text {
            let (pl, cl) = (split_lines(&ptext), split_lines(&text));
            let mut new_map: Vec<Option<usize>> = vec![None; pl.len()];
            let (mut pi, mut ci) = (0, 0);
            for h in diff_lines(&pl, &cl) {
                while ci < h.b_start {
                    new_map[pi] = map[ci];
                    pi += 1;
                    ci += 1;
                }
                for f in map[ci..ci + h.b_len].iter().flatten() {
                    owner[*f] = Some(id.clone());
                }
                ci += h.b_len;
                pi += h.a_len;
            }
            while ci < cl.len() {
                new_map[pi] = map[ci];
                pi += 1;
                ci += 1;
            }
            map = new_map;
        }
        id = parent_id;
        change = parent;
        text = ptext;
    }
    let mut rows = Vec::new();
    let mut out = String::new();
    let mut cache: std::collections::HashMap<String, Change> = std::collections::HashMap::new();
    for (i, line) in final_lines.iter().enumerate() {
        let cid = owner[i].clone().unwrap_or_else(|| start.clone());
        if !cache.contains_key(&cid) {
            cache.insert(cid.clone(), repo.load_change(&cid)?);
        }
        let c = &cache[&cid];
        out.push_str(&format!("{} {:>5} {}", short(&cid), i + 1, line));
        if !line.ends_with('\n') {
            out.push('\n');
        }
        rows.push(json!({"line": i + 1, "change": cid, "author": c.author, "time": c.time,
                         "subject": c.subject(), "intent": c.intent, "text": line.trim_end_matches('\n')}));
    }
    Ok((json!({"path": path, "rev": start, "lines": rows}), out))
}

/// The change that last touched a declaration: walk the first parents while
/// the declaration is the same as at `start`.
fn blame_decl(repo: &Repo, start: &str, path: &str, decl: &str, text: &str) -> VcsResult<Done> {
    let key = |ds: &crate::vcs_decl::Decls| ds.get(decl).cloned();
    let current = decls_of(text)
        .ok_or_else(|| Refusal::new("binary-file", format!("`{path}` does not parse as cancho")))?;
    let want = key(&current).ok_or_else(|| {
        Refusal::new("decl-not-found", format!("no declaration `{decl}` in `{path}`"))
            .with(json!({"declarations": current.keys().collect::<Vec<_>>()}))
    })?;
    let mut id = start.to_owned();
    let mut change = repo.load_change(&id)?;
    while let Some(pid) = change.parents.first().cloned() {
        let parent = repo.load_change(&pid)?;
        let ptext = Tree::load(&repo.store, &parent.tree)?
            .get(path)
            .and_then(|e| repo.store.get(Kind::Blob, &e.blob).ok())
            .and_then(|b| String::from_utf8(b).ok());
        let same =
            ptext.as_deref().and_then(decls_of).and_then(|d| key(&d)).is_some_and(|d| d == want);
        if !same {
            break;
        }
        id = pid;
        change = parent;
    }
    let mut doc = change_json(&id, &change);
    doc["path"] = json!(path);
    doc["decl"] = json!(decl);
    Ok((doc, format!("{} {}\n", short(&id), change.subject())))
}

fn cmd_branch(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let name = |n: Option<&String>| -> VcsResult<String> {
        let n = n.ok_or_else(|| bad("this needs a branch name"))?;
        check_ref_name(&format!("heads/{n}"))?;
        Ok(n.clone())
    };
    let sub = p.pos.first().map_or("list", String::as_str);
    match sub {
        "list" => {
            let current = match repo.head()? {
                Head::Branch(b) => Some(b),
                Head::Detached(_) => None,
            };
            let branches = list_refs(&repo.vcs, "heads")?;
            let mut text = String::new();
            for (n, id) in &branches {
                text.push_str(&format!(
                    "{} {} {}\n",
                    if current.as_deref() == Some(n) { "*" } else { " " },
                    n.trim_start_matches("heads/"),
                    short(id)
                ));
            }
            Ok((
                json!({"current": current.as_deref().map(|c| c.trim_start_matches("heads/")),
                       "branches": branches.iter().map(|(n, id)| json!({"name": n.trim_start_matches("heads/"), "change": id})).collect::<Vec<_>>()}),
                text,
            ))
        }
        "create" => {
            let n = name(p.pos.get(1))?;
            let target = repo.resolve(p.value("--from").unwrap_or("HEAD"))?;
            let existing = read_ref(&repo.vcs, &format!("heads/{n}"))?;
            if existing.as_deref() == Some(target.as_str()) {
                return Ok((
                    json!({"branch": n, "change": target, "created": false}),
                    format!("{n} already at {}\n", short(&target)),
                ));
            }
            if p.flag("--dry-run") {
                return Ok((
                    json!({"branch": n, "change": target, "created": false, "dry_run": true}),
                    String::new(),
                ));
            }
            update_ref(
                &repo.vcs,
                &format!("heads/{n}"),
                &target,
                &Expect::Absent,
                "branch create",
            )?;
            Ok((
                json!({"branch": n, "change": target, "created": true}),
                format!("created {n} at {}\n", short(&target)),
            ))
        }
        "delete" => {
            let n = name(p.pos.get(1))?;
            if repo.head()? == Head::Branch(format!("heads/{n}")) {
                return Err(Refusal::new(
                    "ref-protected",
                    "the checked-out branch cannot be deleted",
                ));
            }
            let Some(cur) = read_ref(&repo.vcs, &format!("heads/{n}"))? else {
                return Ok((json!({"branch": n, "deleted": false}), format!("no branch {n}\n")));
            };
            let expect = match p.value("--expect") {
                Some(e) => Expect::Id(repo.resolve(e)?),
                None => {
                    return Err(bad(
                        "deleting a branch needs --expect <change> (the change you believe it is at)",
                    ));
                }
            };
            if p.flag("--dry-run") {
                return Ok((
                    json!({"branch": n, "deleted": false, "dry_run": true, "change": cur}),
                    String::new(),
                ));
            }
            delete_ref(&repo.vcs, &format!("heads/{n}"), &expect)?;
            Ok((
                json!({"branch": n, "deleted": true, "change": cur}),
                format!("deleted {n} (was {})\n", short(&cur)),
            ))
        }
        other => Err(bad(&format!("`branch {other}`: expected list, create or delete"))),
    }
}

/// Bring the working copy from `old` to `new`; a path that is dirty and also
/// differs between them blocks unless `discard`.
fn move_working_copy(
    repo: &Repo,
    old: &Tree,
    new: &Tree,
    p: &Parsed,
    dry: bool,
) -> VcsResult<Vec<String>> {
    let dirty: BTreeSet<String> = repo.status()?.into_iter().map(|s| s.path).collect();
    let moving = diff_trees(old, new);
    let in_the_way: Vec<String> =
        moving.iter().filter(|c| dirty.contains(&c.path)).map(|c| c.path.clone()).collect();
    let blocked = if p.flag("--discard") {
        vec![]
    } else if p.flag("--keep") {
        in_the_way.clone()
    } else {
        dirty.iter().cloned().collect()
    };
    if !blocked.is_empty() {
        return Err(Refusal::new("dirty-working-copy", "uncommitted changes would be lost or carried; record them, or pass --keep or --discard")
            .with(json!({"paths": blocked})));
    }
    if !dry {
        for c in &moving {
            match &c.new {
                Some(e) => repo.materialize(e)?,
                None => repo.remove_path(&c.path)?,
            }
        }
    }
    Ok(in_the_way)
}

fn cmd_switch(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [target] = p.pos.as_slice() else { return Err(bad("`switch` takes one branch or change")) };
    let dry = p.flag("--dry-run");
    let _lock = if dry { None } else { Some(work_lock(&repo.vcs)?) };
    let old_tree = repo.head_tree()?;
    let branch_ref = format!("heads/{target}");
    let (new_head, change) = if p.flag("--create") {
        check_ref_name(&branch_ref)?;
        let at = repo
            .head_change_id()?
            .ok_or_else(|| Refusal::new("bad-revision", "there is no change to branch from yet"))?;
        if !dry {
            update_ref(&repo.vcs, &branch_ref, &at, &Expect::Absent, "switch --create")?;
        }
        (Head::Branch(branch_ref.clone()), at)
    } else if !p.flag("--detach")
        && check_ref_name(&branch_ref).is_ok()
        && read_ref(&repo.vcs, &branch_ref)?.is_some()
    {
        let id = read_ref(&repo.vcs, &branch_ref)?.unwrap_or_default();
        (Head::Branch(branch_ref.clone()), id)
    } else {
        let id = repo.resolve(target)?;
        (Head::Detached(id.clone()), id)
    };
    if repo.head()? == new_head {
        return Ok((json!({"switched": false, "head": change}), format!("already on {target}\n")));
    }
    let new_tree = repo.tree_of(Some(&change))?;
    let carried = move_working_copy(&repo, &old_tree, &new_tree, p, dry)?;
    if !dry {
        repo.set_head(&new_head)?;
    }
    Ok((
        json!({"switched": !dry, "dry_run": dry, "head": change, "branch": match &new_head { Head::Branch(b) => json!(b.trim_start_matches("heads/")), _ => Value::Null },
               "discarded": if p.flag("--discard") { carried } else { vec![] }}),
        format!("switched to {target}\n"),
    ))
}

fn cmd_restore(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    if p.pos.is_empty() {
        return Err(Refusal::new("nothing-selected", "name the paths to restore (`.` for all)"));
    }
    let from = match p.value("--from") {
        Some(r) => repo.resolve(r)?,
        None => repo.resolve("HEAD")?,
    };
    let want = repo.tree_of(Some(&from))?;
    let dry = p.flag("--dry-run");
    let _lock = if dry { None } else { Some(work_lock(&repo.vcs)?) };
    let work = Tree::new(repo.scan()?)?;
    let selected: Vec<String> =
        p.pos.iter().map(|a| rel_path(&repo, a)).collect::<VcsResult<_>>()?;
    let covers = |path: &str| {
        selected.iter().any(|s| s.is_empty() || path == s || path.starts_with(&format!("{s}/")))
    };
    let mut restored = Vec::new();
    for c in diff_trees(&work, &want) {
        if !covers(&c.path) {
            continue;
        }
        restored.push(json!({"path": c.path, "was": c.kind.name()}));
        if !dry {
            match &c.new {
                Some(e) => repo.materialize(e)?,
                None => repo.remove_path(&c.path)?,
            }
        }
    }
    let text = format!("restored {} path(s) from {}\n", restored.len(), short(&from));
    Ok((json!({"from": from, "restored": restored, "dry_run": dry}), text))
}

fn cmd_unlock(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let found = locks(&repo.vcs);
    let mut removed = Vec::new();
    for (path, pid, age) in &found {
        removed.push(json!({"lock": path.display().to_string(), "pid": pid, "age_seconds": age}));
        if !p.flag("--dry-run") {
            fs::remove_file(path).map_err(|e| Refusal::io("cannot remove a lock", &e))?;
        }
    }
    Ok((
        json!({"removed": removed, "dry_run": p.flag("--dry-run")}),
        format!("{} lock(s)\n", found.len()),
    ))
}

fn cmd_apply(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [file] = p.pos.as_slice() else { return Err(bad("`apply` takes a JSON file of edits")) };
    let text =
        fs::read_to_string(file).map_err(|e| Refusal::io(format!("cannot read {file}"), &e))?;
    let edits: Vec<Edit> = serde_json::from_str(&text).map_err(|e| bad(&format!("{file}: {e}")))?;
    let mut by_path: std::collections::BTreeMap<String, Vec<Edit>> =
        std::collections::BTreeMap::new();
    for e in edits {
        by_path.entry(rel_path(&repo, &e.path)?).or_default().push(e);
    }
    let _lock = work_lock(&repo.vcs)?;
    let mut results = Vec::new();
    for (path, es) in &by_path {
        let old = fs::read_to_string(repo.root.join(path))
            .map_err(|e| Refusal::io(format!("cannot read {path}"), &e))?;
        let new = apply_edits(&old, es)?;
        results.push((path.clone(), new));
    }
    // Every file was checked before any is written, so a refusal changes nothing.
    if !p.flag("--dry-run") {
        for (path, new) in &results {
            let mode = fs::metadata(repo.root.join(path)).map(|m| m.permissions()).ok();
            fs::write(repo.root.join(path), new)
                .map_err(|e| Refusal::io(format!("cannot write {path}"), &e))?;
            if let Some(m) = mode {
                let _ = fs::set_permissions(repo.root.join(path), m);
            }
        }
    }
    Ok((
        json!({"applied": results.iter().map(|(p, _)| p).collect::<Vec<_>>(), "dry_run": p.flag("--dry-run")}),
        format!("applied edits to {} file(s)\n", results.len()),
    ))
}

fn cmd_verify(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let mut problems: Vec<Value> = Vec::new();
    let mut counts = serde_json::Map::new();
    let mut seen_blobs: HashSet<String> = HashSet::new();
    let mut problem = |tag: &str, what: String| problems.push(json!({"tag": tag, "object": what}));
    for kind in Kind::ALL {
        let ids = repo.store.list(kind)?;
        counts.insert(kind.name().to_owned(), json!(ids.len()));
        for id in ids {
            if !is_id(&id) {
                problem("object-corrupt", format!("{} {id}: not an object name", kind.name()));
                continue;
            }
            match repo.store.get(kind, &id) {
                Err(e) => problem(e.tag, format!("{} {id}", kind.name())),
                Ok(bytes) => match kind {
                    Kind::Blob => {
                        seen_blobs.insert(id);
                    }
                    Kind::Tree => match Tree::decode(&bytes) {
                        Err(_) => problem("object-corrupt", format!("tree {id}")),
                        Ok(t) => {
                            for e in t.entries() {
                                if !repo.store.has(Kind::Blob, &e.blob) {
                                    problem(
                                        "object-missing",
                                        format!("blob {} (in tree {id}, {})", e.blob, e.path),
                                    );
                                }
                            }
                        }
                    },
                    Kind::Change => match Change::load(&repo.store, &id) {
                        Err(_) => problem("object-corrupt", format!("change {id}")),
                        Ok(c) => {
                            if !repo.store.has(Kind::Tree, &c.tree) {
                                problem(
                                    "object-missing",
                                    format!("tree {} (of change {id})", c.tree),
                                );
                            }
                            for par in &c.parents {
                                if !repo.store.has(Kind::Change, par) {
                                    problem(
                                        "object-missing",
                                        format!("change {par} (parent of {id})"),
                                    );
                                }
                            }
                        }
                    },
                },
            }
        }
    }
    let mut refs_checked = 0;
    for prefix in ["heads", "tags", "remotes"] {
        for (name, id) in list_refs(&repo.vcs, prefix)? {
            refs_checked += 1;
            if !repo.store.has(Kind::Change, &id) {
                problem("object-missing", format!("change {id} (ref {name})"));
            }
        }
    }
    if let Some(h) = repo.head_change_id()?
        && !repo.store.has(Kind::Change, &h)
    {
        problem("object-missing", format!("change {h} (HEAD)"));
    }
    let stale: Vec<Value> = locks(&repo.vcs)
        .iter()
        .map(|(p, pid, age)| json!({"lock": p.display().to_string(), "pid": pid, "age_seconds": age}))
        .collect();
    let garbage = fs::read_dir(repo.vcs.join("tmp")).map(|r| r.count()).unwrap_or(0);
    if !problems.is_empty() {
        let first = problems[0]["tag"].as_str().unwrap_or("object-corrupt").to_owned();
        let tag: &'static str = match first.as_str() {
            "object-missing" => "object-missing",
            _ => "object-corrupt",
        };
        return Err(Refusal::new(
            tag,
            format!("{} problem(s); first: {}", problems.len(), problems[0]["object"]),
        )
        .with(json!({"problems": problems, "objects": counts})));
    }
    Ok((
        json!({"sound": true, "objects": counts, "refs": refs_checked, "locks": stale, "tmp_garbage": garbage}),
        format!(
            "sound: {} refs, {} lock(s), {garbage} temporary file(s)\n",
            refs_checked,
            stale.len()
        ),
    ))
}
