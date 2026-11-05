//! `import-git` and `export-git` (`docs/vcs-cli.md` section 7): the bridge
//! to git, in cancho. Nothing here starts another program.

use std::path::Path;

use cancho_vcs::{GitRepo, Refusal, Repo, Tree, VcsResult, diff_trees, export_git, import_git};
use serde_json::json;

use crate::vcs_args::{Done, Parsed, open};

pub(crate) fn cmd_import(p: &Parsed) -> VcsResult<Done> {
    let [dir] = p.pos.as_slice() else {
        return Err(Refusal::new("bad-argument", "`import-git` takes a git repository directory"));
    };
    let git = GitRepo::open(Path::new(dir))?;
    let dry = p.flag("--dry-run");
    let repo = match open(p) {
        Ok(r) => r,
        Err(e) if e.tag == "not-a-repository" && !dry => {
            let root = std::fs::canonicalize(p.value("--repo").unwrap_or("."))
                .map_err(|e| Refusal::io("cannot resolve the directory", &e))?;
            Repo::init(&root, "main")?.0
        }
        Err(e) => return Err(e),
    };
    let report = import_git(&repo, &git, dry)?;
    let mut checked_out = 0;
    if !dry && !p.flag("--no-checkout") && repo.head_change_id()?.is_some() && repo.scan()?.is_empty() {
        let want = repo.head_tree()?;
        for c in diff_trees(&Tree::default(), &want) {
            if let Some(e) = &c.new {
                repo.materialize(e)?;
                checked_out += 1;
            }
        }
    }
    let mut doc = report.to_json();
    doc["dry_run"] = json!(dry);
    doc["checked_out"] = json!(checked_out);
    let text = format!(
        "imported {} commit(s) ({} already there), {} self-check mismatch(es)\n",
        report.commits,
        report.already_imported,
        report.mismatches.len()
    );
    Ok((doc, text))
}

pub(crate) fn cmd_export(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    let [dir] = p.pos.as_slice() else {
        return Err(Refusal::new("bad-argument", "`export-git` takes a directory for the git repository"));
    };
    let report = export_git(&repo, Path::new(dir), p.flag("--dry-run"))?;
    let mut doc = report.to_json();
    doc["dry_run"] = json!(p.flag("--dry-run"));
    let text = format!(
        "exported {} commit(s); {} of {} imported commit(s) kept their id\n",
        report.commits,
        report.compared - report.mismatches.len(),
        report.compared
    );
    Ok((doc, text))
}
