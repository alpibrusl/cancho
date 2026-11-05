//! Atomic file writes and the crash failpoint (`docs/vcs-cli.md` section 5).
//!
//! A write goes to a temporary name in `<store>/tmp`, is fsynced, and is
//! renamed to its final name, so a reader sees the old file or the new one
//! and never half of one. `CANCHO_VCS_CRASH_AT=<n>` makes the process exit
//! (without running destructors, as a kill would) at the n-th failpoint it
//! reaches; the crash test runs a command once for every n until none
//! fires, and checks the store after each. The variable is read once; with
//! it unset a failpoint is one atomic increment.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

static STEP: AtomicUsize = AtomicUsize::new(0);
static TMP: AtomicU64 = AtomicU64::new(0);

fn crash_at() -> Option<usize> {
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    *AT.get_or_init(|| std::env::var("CANCHO_VCS_CRASH_AT").ok().and_then(|v| v.parse().ok()))
}

/// A point where a kill would leave the store in a new intermediate state.
pub(crate) fn failpoint() {
    let n = STEP.fetch_add(1, Ordering::SeqCst) + 1;
    if crash_at() == Some(n) {
        std::process::exit(137);
    }
}

/// How many failpoints this process has passed (the crash test's progress).
pub fn failpoints_passed() -> usize {
    STEP.load(Ordering::SeqCst)
}

/// Write `bytes` to `dest` atomically. `tmp_dir` must be on the same
/// filesystem as `dest`. `sync` is off only for derived data (the git
/// bridge's output), which `export-git` can rebuild.
pub(crate) fn write_atomic(
    tmp_dir: &Path,
    dest: &Path,
    bytes: &[u8],
    executable: bool,
    sync: bool,
) -> std::io::Result<()> {
    failpoint();
    fs::create_dir_all(tmp_dir)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp: PathBuf = tmp_dir.join(format!(
        "{}-{}",
        std::process::id(),
        TMP.fetch_add(1, Ordering::SeqCst)
    ));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        failpoint();
        if sync {
            f.sync_all()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }))?;
        }
        #[cfg(not(unix))]
        let _ = executable;
    }
    failpoint();
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    failpoint();
    Ok(())
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}
