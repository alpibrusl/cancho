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

    embed_selfhost();
}

/// The compiler written in cancho (`examples/selfhost/*.cho`), compiled into this binary the way the standard library is,
/// so that `--compiler auto` and the default build need no checkout next to it: the sources are written out and built
/// once, by this compiler, into the cache (`docs/self-hosting.md`, `selfhost_build.rs`).
fn embed_selfhost() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/selfhost");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".cho"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut out = String::from("pub const SELFHOST_SOURCES: &[(&str, &str)] = &[\n");
    for name in &names {
        let path = std::fs::canonicalize(dir.join(name)).unwrap_or_else(|_| dir.join(name));
        // Each file by itself: cargo does not notice an edit to a file inside a directory it was told to watch.
        println!("cargo:rerun-if-changed={}", path.display());
        out.push_str(&format!("    ({name:?}, include_str!({:?})),\n", path.display().to_string()));
    }
    out.push_str("];\n");
    let target = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"))
        .join("selfhost_sources.rs");
    std::fs::write(target, out).expect("OUT_DIR is writable");
}
