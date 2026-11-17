//! What `devices`, `global_state` and `conn_peer` still take from the socket tests: build a program on a backend, the two backends, a program that dials, and waiting for a
//! line on a child's standard error. The socket tests themselves are `tests/commands/sockets.sh` and `udp.sh` (docs/gate.md, G4 wave 4); this goes with the last of those
//! three files.

use super::*;

/// Build `source` with `backend`, returning the executable's path.
pub(super) fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let file = dir.join(format!("{name}.cho"));
    std::fs::write(&file, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            file.as_os_str(),
            "--std".as_ref(),
            "--backend".as_ref(),
            backend.as_ref(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`--backend {backend}` should build `{name}`, but said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

pub(super) const BACKENDS: [&str; 2] = ["cranelift", "llvm"];
