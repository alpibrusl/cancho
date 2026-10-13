//! The key tables of `packages/tls` (`docs/p256-fast.md` §11), with no network, through
//! `tests/programs/tls_key_tables.cho`: a server identity's table is its point's when the identity is
//! loaded, the new key's when it is replaced, unchanged by a refused replacement, and zeros when the
//! configuration is wiped (`docs/tls-server.md` §3.5); a client's trust store gives each P-256 root a
//! table, and a store replaced leaves nothing of the old one's tables (`tls.residue`,
//! `docs/x509-verify.md` §11). On both backends.

use super::*;
use std::io::Write;
use std::process::Stdio;

#[test]
fn identities_and_trust_anchors_keep_their_tables_and_leave_none_behind() {
    let vectors = repo_root().join("tests/vectors/tls/echo");
    let mut input = Vec::new();
    for (k, file) in
        ["ca.pem", "first/chain.pem", "first/key.pem", "renewed/chain.pem", "renewed/key.pem"]
            .iter()
            .enumerate()
    {
        if k > 0 {
            input.push(0);
        }
        input.extend(std::fs::read(vectors.join(file)).unwrap());
    }
    for backend in ["cranelift", "llvm"] {
        let dir = scratch(&format!("key-tables-{backend}"));
        let exe = dir.join("driver");
        let build = Command::new(BIN)
            .args(["build", "--std", "--backend", backend])
            .arg(repo_root().join("tests/programs/tls_key_tables.cho"))
            .args(super::tls_server::package_files())
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the driver runs");
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let out = child.wait_with_output().expect("the driver finishes");
        let lines: Vec<String> =
            String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
        assert!(
            out.status.success() && lines.len() == 11 && lines.iter().all(|l| l.starts_with("ok ")),
            "{backend}: {lines:#?} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
