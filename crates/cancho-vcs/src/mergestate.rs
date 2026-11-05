//! The one operation in progress (`docs/vcs-cli.md` section 6): a merge or a
//! replay that stopped on conflicts. Nothing of the merge is recorded until
//! every path is settled, so the state is only what was decided: the three
//! changes it works from and the resolutions so far. The merge itself is
//! recomputed from them, which is deterministic, so `merge --abort` is
//! deleting this file and the working copy was never touched.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change::Intent;
use crate::error::{Refusal, Result};
use crate::fsx;
use crate::merge::Resolutions;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeState {
    /// `merge` or `replay`.
    pub op: String,
    pub ours: String,
    pub theirs: String,
    /// The merge base (for a replay, the replayed change's parent).
    pub base: String,
    /// The ref that moves when the operation completes, e.g. `heads/main`;
    /// `None` for a detached HEAD.
    pub branch: Option<String>,
    /// Whether that ref is the checked-out one, so the working copy follows.
    pub checkout: bool,
    #[serde(default)]
    pub resolutions: Resolutions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<Intent>,
    pub author: String,
    pub time: i64,
    #[serde(default)]
    pub no_gate: bool,
}

fn path(vcs: &Path) -> PathBuf {
    vcs.join("merge-state.json")
}

impl MergeState {
    pub fn load(vcs: &Path) -> Result<Option<MergeState>> {
        match fs::read(path(vcs)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| Refusal::new("object-corrupt", format!("merge-state.json: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Refusal::io("cannot read the merge state", &e)),
        }
    }

    pub fn save(&self, vcs: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        fsx::write_atomic(&vcs.join("tmp"), &path(vcs), &bytes, false, true)
            .map_err(|e| Refusal::io("cannot write the merge state", &e))
    }

    /// Delete the state; true if there was one.
    pub fn clear(vcs: &Path) -> Result<bool> {
        match fs::remove_file(path(vcs)) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(Refusal::io("cannot remove the merge state", &e)),
        }
    }
}
