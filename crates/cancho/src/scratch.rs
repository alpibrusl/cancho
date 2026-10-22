//! The directory a command builds into when it must run what it built (`run`, `test`, `satisfy`): made by the command,
//! private to the user, and never one that was there before (`docs/unsafe-and-global-state.md` §3).
//!
//! These directories used to be `temp_dir()/cancho-<command>-<pid>`, made with `create_dir_all`. A process id is guessable
//! and the temporary directory is shared, so another user could make the directory first, or leave a link at the name:
//! `create_dir_all` accepts both, the build then wrote the executable into a directory the other user controls, and the
//! command ran it after they had had the chance to replace it.

use std::path::{Path, PathBuf};

/// A new directory under `base`, named `<prefix>-<pid>-<n>` for the first `n` that is free, mode `0700`. A name that is
/// taken by anything (a directory, a file, a link) is skipped, not used, and never written through.
pub(crate) fn private_dir_in(base: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    let pid = std::process::id();
    let mut last = None;
    for n in 0..64 {
        let dir = base.join(format!("{prefix}-{pid}-{n}"));
        let mut made = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut made, 0o700);
        match made.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no free name")))
}

/// [`private_dir_in`] under the temporary directory.
pub(crate) fn private_temp_dir(prefix: &str) -> std::io::Result<PathBuf> {
    private_dir_in(&std::env::temp_dir(), prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn scratch(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("cancho-scratch-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("a scratch directory");
        base
    }

    /// Names another user could have made first are skipped: a link to somewhere else, a directory, a file. What a
    /// link points at is not written to, and the directory that comes back is new, empty and `0700`.
    #[test]
    fn a_name_that_is_taken_is_skipped_never_used() {
        let base = scratch("taken");
        let victim = base.join("victim");
        std::fs::create_dir(&victim).unwrap();
        let pid = std::process::id();
        symlink(&victim, base.join(format!("p-{pid}-0"))).unwrap();
        std::fs::create_dir(base.join(format!("p-{pid}-1"))).unwrap();
        std::fs::write(base.join(format!("p-{pid}-2")), "x").unwrap();
        let made = private_dir_in(&base, "p").expect("a free name is found");
        assert_eq!(made, base.join(format!("p-{pid}-3")));
        assert_eq!(std::fs::metadata(&made).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::read_dir(&made).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_dir(&victim).unwrap().count(),
            0,
            "nothing was made through the link"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Two calls get two directories.
    #[test]
    fn two_calls_get_two_directories() {
        let base = scratch("two");
        let (a, b) = (private_dir_in(&base, "q").unwrap(), private_dir_in(&base, "q").unwrap());
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Nothing free is an error, not the last name.
    #[test]
    fn when_every_name_is_taken_it_is_an_error() {
        let base = scratch("full");
        let pid = std::process::id();
        for n in 0..64 {
            std::fs::create_dir(base.join(format!("r-{pid}-{n}"))).unwrap();
        }
        assert!(private_dir_in(&base, "r").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
