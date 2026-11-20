//! Stamp the compiler with the commit it was built from (`docs/package-system.md` §8.4), so `cancho --version` can say it
//! and a project that names the compiler it was written for can be checked against it.
//!
//! The commit comes from `CANCHO_REV` if that is set (a build outside a git checkout, such as from a tarball), else from
//! `git rev-parse HEAD` in this repository, suffixed `-dirty` if tracked files have uncommitted changes; else it is
//! `unknown`.

use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out =
        Command::new("git").args(args).current_dir(env!("CARGO_MANIFEST_DIR")).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-env-changed=CANCHO_REV");
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        // A new commit moves HEAD, or the branch it points at, or packed refs.
        let git_dir = Path::new(&git_dir);
        println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
        println!("cargo:rerun-if-changed={}", git_dir.join("packed-refs").display());
        if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) {
            if let Some(reference) = head.trim().strip_prefix("ref: ") {
                println!("cargo:rerun-if-changed={}", git_dir.join(reference).display());
            }
        }
    }
    let rev = match std::env::var("CANCHO_REV").ok().filter(|r| !r.is_empty()) {
        Some(rev) => rev,
        None => match git(&["rev-parse", "HEAD"]) {
            Some(head) => {
                let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
                    .is_some_and(|s| !s.is_empty());
                if dirty { format!("{head}-dirty") } else { head }
            }
            None => "unknown".to_owned(),
        },
    };
    println!("cargo:rustc-env=CANCHO_REV_STAMP={rev}");
}
