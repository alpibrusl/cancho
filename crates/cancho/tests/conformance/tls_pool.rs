//! The engine's work areas (`docs/tls-memory.md` section 8), with no network: a client engine and a server engine in one
//! process, their bytes carried between them by `tests/programs/tls_pool.cho`, with as many areas as slots, with fewer, and
//! with one. It checks, on both backends, that every connection is established and holding no area once it is idle, that
//! messages of any size in any pieces come back byte for byte, that a start with no area free is refused `tls-pool` and
//! starts nothing, that a connection fed with no area free fails with its keys cleared and the others go on, and that the
//! areas all come back.

use super::*;
use std::io::Write;
use std::process::Stdio;

fn build(backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("tls-pool-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/tls_pool.cho"))
        .args(super::tls_server::package_files())
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// The root, the chain and the key of the test identity, a NUL byte between them.
fn input() -> Vec<u8> {
    let vectors = repo_root().join("tests/vectors/tls/echo");
    let mut bytes = std::fs::read(vectors.join("ca.pem")).unwrap();
    bytes.push(0);
    bytes.extend(std::fs::read(vectors.join("first/chain.pem")).unwrap());
    bytes.push(0);
    bytes.extend(std::fs::read(vectors.join("first/key.pem")).unwrap());
    bytes
}

fn run(exe: &Path, slots: u32, areas: u32, steps: u32, seed: u32) -> Vec<String> {
    let mut child = Command::new(exe)
        .args([slots, areas, steps, seed].map(|n| n.to_string()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the driver runs");
    let mut stdin = child.stdin.take().unwrap();
    let bytes = input();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let out = child.wait_with_output().expect("the driver finishes");
    let _ = writer.join();
    let lines: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert!(
        out.status.success() && lines.iter().all(|l| l.starts_with("ok ")),
        "{slots} slots, {areas} areas, seed {seed}: {lines:#?} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    lines
}

#[test]
fn an_engine_with_fewer_work_areas_than_slots_serves_every_connection_on_both_backends() {
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build(backend);
        // Every slot has an area: nothing can be refused, and nothing is.
        let all = run(&exe, 8, 8, 40, 1);
        for want in [
            "handshakes, every slot established and holding no area",
            "steps echoed byte for byte, wrong 0",
            "a connection with data waiting keeps its area while another is served",
            "all slots dropped, all areas free",
        ] {
            assert!(all.iter().any(|l| l.contains(want)), "{want}: {all:#?}");
        }
        assert!(all.iter().all(|l| !l.contains("tls-pool")), "{all:#?}");
        // Fewer areas than slots: the pool bounds the handshakes in progress.
        let some = run(&exe, 8, 3, 40, 2);
        assert!(
            some.iter()
                .any(|l| l.contains("serve and start with no area free are tls-pool, code -72")),
            "{some:#?}"
        );
        // One area: a record half fed holds it, and a second connection that needs it fails.
        let one = run(&exe, 8, 1, 40, 3);
        for want in [
            "a record half fed holds the area",
            "a connection fed with no area free fails tls-pool",
            "the first completes and the area comes back",
            "another connection is served",
        ] {
            assert!(one.iter().any(|l| l.contains(want)), "{want}: {one:#?}");
        }
        // One slot, one area; and other seeds, so the sizes and the pieces differ.
        run(&exe, 1, 1, 30, 4);
        run(&exe, 5, 2, 80, 5);
        run(&exe, 16, 4, 80, 6);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
