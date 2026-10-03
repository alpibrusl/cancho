//! Making a [`Origin`] available as a directory (`docs/package-system.md`
//! §7.3): a content-addressed cache of git checkouts, filled by shelling out
//! to `git`.
//!
//! The cache key is the **full commit hash**, so a checkout, once it exists,
//! is complete and never stale: a different commit is a different directory. Nothing here asks the network a question
//! when the directory is present, and nothing is trusted for content -- the
//! caller re-parses, re-typechecks and re-hashes what it finds, exactly as it
//! does for a store in a local directory.
//!
//! `git` is used to *fetch bytes*, never to run anything from them: hooks are
//! pointed at nothing, `ext::` transports are off, prompts are off, and no
//! submodule is initialised. The `.git` directory is deleted once the commit
//! is verified, so the cache holds trees, not repositories.

use std::path::{Path, PathBuf};
use std::process::Command;

use lex_sys_vcs::Origin;

/// The cache root: `$LEX_SYS_CACHE`, else `$XDG_CACHE_HOME/lex-sys`, else
/// `$HOME/.cache/lex-sys`.
fn cache_root() -> Result<PathBuf, String> {
    let from_env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(root) = from_env("LEX_SYS_CACHE") {
        return Ok(root);
    }
    if let Some(xdg) = from_env("XDG_CACHE_HOME") {
        return Ok(xdg.join("lex-sys"));
    }
    if let Some(home) = from_env("HOME") {
        return Ok(home.join(".cache").join("lex-sys"));
    }
    Err("no cache directory: set LEX_SYS_CACHE (or HOME)".to_owned())
}

/// `git` with the settings every call here wants. `-C <dir>` is added by the
/// callers that have a repository.
fn git() -> Command {
    let mut cmd = Command::new("git");
    // Hooks off (a template directory can supply them to `git init`), and only
    // the transports a repository location can honestly use: `ext::` and the
    // other remote helpers run programs, and `http` is cleartext.
    cmd.args(["-c", "core.hooksPath=/dev/null", "-c", "protocol.allow=never"])
        .args(["-c", "protocol.https.allow=always", "-c", "protocol.ssh.allow=always"])
        .args(["-c", "protocol.git.allow=always", "-c", "protocol.file.allow=always"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    cmd
}

fn run(cmd: &mut Command, what: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("cannot run `git` to {what}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("`git` failed to {what}: {}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The directory a checkout of `origin` lives in (it may not exist yet).
/// Keyed by the commit alone: a commit hash *is* its content, so two mirrors
/// of one commit share a directory, and where it came from decides nothing.
fn checkout_dir(origin: &Origin) -> Result<PathBuf, String> {
    Ok(cache_root()?.join("git").join(&origin.rev))
}

/// The store directory `origin` names, fetching the commit first if the
/// cache does not hold it.
pub fn ensure(origin: &Origin) -> Result<PathBuf, String> {
    origin.validate()?;
    let dest = checkout_dir(origin)?;
    if !dest.is_dir() {
        fill(origin, &dest)?;
    }
    let store = dest.join(&origin.path);
    if !store.is_dir() {
        return Err(format!(
            "commit {} of `{}` has no directory `{}`",
            origin.rev, origin.git, origin.path
        ));
    }
    Ok(store)
}

fn fill(origin: &Origin, dest: &Path) -> Result<(), String> {
    let parent = dest.parent().ok_or("the cache path has no parent")?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("cannot create `{}`: {e}", parent.display()))?;
    let tmp = parent.join(format!("{}.tmp.{}", origin.rev, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("cannot create `{}`: {e}", tmp.display()))?;

    let result = fetch_into(origin, &tmp).and_then(|()| {
        match std::fs::rename(&tmp, dest) {
            Ok(()) => Ok(()),
            // Another process finished first. Its directory is as good as ours.
            Err(_) if dest.is_dir() => Ok(()),
            Err(e) => Err(format!("cannot move the checkout into `{}`: {e}", dest.display())),
        }
    });
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn fetch_into(origin: &Origin, dir: &Path) -> Result<(), String> {
    run(git().arg("init").arg("--quiet").arg(dir), "initialise a checkout")?;
    // A commit by hash, one commit deep: what GitHub and any server with
    // protocol v2 will serve. If the server refuses, take its branches and
    // tags and look for the commit among them.
    let shallow = run(
        git()
            .arg("-C")
            .arg(dir)
            .args(["fetch", "--quiet", "--depth", "1", "--no-tags", "--"])
            .arg(&origin.git)
            .arg(&origin.rev),
        "fetch the commit",
    );
    if shallow.is_err() {
        run(
            git()
                .arg("-C")
                .arg(dir)
                .args(["fetch", "--quiet", "--no-tags", "--"])
                .arg(&origin.git)
                .args(["+refs/heads/*:refs/remotes/origin/*", "+refs/tags/*:refs/tags/*"]),
            "fetch the repository",
        )?;
    }
    run(
        git().arg("-C").arg(dir).args(["checkout", "--quiet", "--detach", &origin.rev]),
        "check out the commit",
    )?;
    let head = run(git().arg("-C").arg(dir).args(["rev-parse", "HEAD"]), "read the commit")?;
    if head != origin.rev {
        return Err(format!("asked `{}` for commit {} and got {head}", origin.git, origin.rev));
    }
    std::fs::remove_dir_all(dir.join(".git"))
        .map_err(|e| format!("cannot remove the checkout's `.git`: {e}"))?;
    Ok(())
}

/// Resolve a branch or tag name to the commit it names right now, once, for
/// `vcs lock --ref`. The name is never written anywhere; the hash is.
pub fn resolve_ref(url: &str, name: &str) -> Result<String, String> {
    if url.is_empty() || url.starts_with('-') || url.starts_with("ext::") {
        return Err(format!("not a repository location: `{url}`"));
    }
    if name.is_empty() || name.starts_with('-') {
        return Err(format!("not a ref name: `{name}`"));
    }
    let listing = run(git().args(["ls-remote", "--"]).arg(url).arg(name), "resolve the ref")?;
    let hash = listing.split_whitespace().next().unwrap_or("");
    let ok = (hash.len() == 40 || hash.len() == 64)
        && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if ok { Ok(hash.to_owned()) } else { Err(format!("`{url}` has no ref `{name}`")) }
}
