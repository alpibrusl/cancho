//! What the tests of `vcs.rs` share about package stores and the git repositories that hold them: `git`, `copy_dir`, `stores_in` and `repository`. The four tests that were here (a store
//! fetched, refused, installed and added by the compiler in cancho, as `cancho install` and `cancho add` did) are `tests/commands/store.sh` (`docs/gate.md`, G4 wave 4); this file goes with `vcs.rs`.

use super::*;

pub(super) fn git(dir: &Path, args: &[&str]) -> String {
    let done = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=a@b", "-c", "user.name=n", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
    String::from_utf8_lossy(&done.stdout).trim().to_owned()
}

pub(super) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory is made");
    for entry in std::fs::read_dir(from).expect("a directory is read") {
        let path = entry.expect("an entry").path();
        let target = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("a file is copied");
        }
    }
}

/// Every directory beneath `dir` (and `dir`) that holds a `manifest.json`, as paths relative to `dir`.
pub(super) fn stores_in(dir: &Path, at: &Path, found: &mut Vec<String>) {
    if at.join("manifest.json").is_file() {
        found.push(at.strip_prefix(dir).unwrap().display().to_string());
    }
    let mut kids: Vec<PathBuf> = std::fs::read_dir(at)
        .expect("a directory is read")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.is_dir()
                && p.file_name().is_some_and(|n| n != "sources" && n != "ops" && n != "requires")
        })
        .collect();
    kids.sort();
    for kid in kids {
        stores_in(dir, &kid, found);
    }
}

pub(super) struct World {
    pub(super) url: String,
    pub(super) rev: String,
}

/// A repository whose commit holds the directory `tree` as `packages` (a store that requires another by a relative path finds it there).
pub(super) fn repository(work: &Path, tree: &Path, name: &str) -> World {
    let repo = work.join(name);
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "."]);
    copy_dir(tree, &repo.join("packages"));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "stores"]);
    let rev = git(&repo, &["rev-parse", "HEAD"]);
    World { url: format!("file://{}", repo.display()), rev }
}
