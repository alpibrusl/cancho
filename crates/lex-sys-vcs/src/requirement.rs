//! What a *store* itself depends on, distinct from what a *program*
//! depends on ([`crate::Lock`]).
//!
//! `docs/package-system.md` §4.6: `net.sockets`/`net.connect`/`agent.wire`
//! each publish with no `import` of anything but `std` (never available to
//! `vcs publish` either, so none of them use it) — a package that needs
//! another package is new. `Requirement` is what a publisher records once,
//! at publish time, so a later `vcs resolve`/`vcs fetch` can walk the
//! closure without the *consumer* ever having to already know that this
//! package has one: the same "chosen once, resolved by hash forever after"
//! discipline §4.5 already gives a program's own dependencies, now applied
//! to a package's dependency on another package.
//!
//! One file per distinct dependency store, the same shape a program with
//! two direct dependencies already uses (`examples/fetch/net.lock` +
//! `connect.lock`) — a package with two dependency stores just has two
//! `Requirement`s.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lock::Lock;

/// One dependency store a package's own declarations need, and the exact
/// pin the publisher chose into it.
///
/// `store` is recorded exactly as the publisher typed it after `--store`
/// for that dependency — interpreted relative to whoever runs
/// `vcs resolve`/`vcs fetch` next's own working directory, never rewritten,
/// the same meaning a bare `--store <dir>` argument already has everywhere
/// else in this document. A repo-relative path recorded at publish time
/// inside one monorepo stays valid for every consumer, the same way every
/// existing example's own command already assumes running from the repo
/// root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub store: String,
    pub lock: Lock,
}

/// Why loading or saving a store's own `requires/` failed.
#[derive(Debug)]
pub enum RequirementError {
    Io(io::Error),
    /// A file under `requires/` parsed as JSON but not as a `Requirement`
    /// -- on-disk corruption or a hand-edited file, the same failure mode
    /// [`crate::LockError::Malformed`]/[`crate::ManifestError::Malformed`]
    /// name for their own formats.
    Malformed(PathBuf, serde_json::Error),
}

impl From<io::Error> for RequirementError {
    fn from(e: io::Error) -> Self {
        RequirementError::Io(e)
    }
}

impl std::fmt::Display for RequirementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequirementError::Io(e) => write!(f, "{e}"),
            RequirementError::Malformed(path, e) => {
                write!(f, "malformed requirement at `{}`: {e}", path.display())
            }
        }
    }
}

impl std::error::Error for RequirementError {}

fn requires_dir(store_root: &Path) -> PathBuf {
    store_root.join("requires")
}

/// Every requirement a store carries, read back in a stable order
/// (sorted by file name) so two callers walking the same store agree on
/// the order without needing to care what it is.
///
/// An absent `requires/` directory is a store with no dependency of its
/// own — the ordinary case for every package built before this, and not
/// an error — so this answers an empty list rather than refusing.
pub fn load_all(store_root: &Path) -> Result<Vec<Requirement>, RequirementError> {
    let dir = requires_dir(store_root);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();

    let mut requirements = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)?;
        let requirement: Requirement = serde_json::from_slice(&bytes)
            .map_err(|e| RequirementError::Malformed(path.clone(), e))?;
        requirements.push(requirement);
    }
    Ok(requirements)
}

/// Replace a store's own `requires/` wholesale with exactly `requirements`
/// -- last-publish-wins, the same whole-store semantics every other
/// per-store fact `vcs publish` records has, since incremental publish
/// (`docs/vcs-publish.md` §5) is not built for declarations either. An
/// empty list removes the directory rather than leaving a stale, empty
/// one behind.
pub fn save_all(store_root: &Path, requirements: &[Requirement]) -> io::Result<()> {
    let dir = requires_dir(store_root);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    if requirements.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(&dir)?;
    for (i, requirement) in requirements.iter().enumerate() {
        let bytes = serde_json::to_vec_pretty(requirement)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let path = dir.join(format!("{i}.json"));
        let tmp = dir.join(format!("{i}.json.tmp"));
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &path)?;
    }
    Ok(())
}
