//! The engine's work areas (`docs/tls-memory.md` section 8), with no network: a client engine and a server engine in one
//! process, their bytes carried between them by `tests/programs/tls_pool.cho`, with as many areas as slots, with fewer, and
//! with one. It checks, on both backends, that every connection is established and holding no area once it is idle, that
//! messages of any size in any pieces come back byte for byte, that a start with no area free is refused `tls-pool` and
//! starts nothing, that a connection that needs an area and finds none waits rather than fails, that the areas all come back,
//! that a connection resting in its area (`docs/record-in-place.md` §2) holds its core there and nothing else and goes on
//! from its own core once the area has passed to another,
//! and that no free area or free slot keeps a key, a secret or plaintext (`tls.residue`), with server tickets and
//! resumption, with a client identity, and on a server that requires client certificates.

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

/// `input()` and, for `clientcert` (`tests/vectors/tls/client_auth`): the clients' CAs (`pool_ca.pem` and
/// `pool_ca2.pem`, one bundle), the client's chain and its key, and two more chains for that key: a longer leaf and one
/// larger than the engine keeps outside a work area (`docs/tls-memory.md` §11).
fn input_for(words: &[&str]) -> Vec<u8> {
    let mut bytes = input();
    if words.contains(&"clientcert") {
        let vectors = repo_root().join("tests/vectors/tls/client_auth");
        let read = |name: &str| std::fs::read(vectors.join(name)).unwrap();
        let mut cas = read("pool_ca.pem");
        cas.extend(read("pool_ca2.pem"));
        for file in [
            cas,
            read("pool_chain.pem"),
            read("pool_key.pem"),
            read("pool_chain2.pem"),
            read("pool_big_chain.pem"),
        ] {
            bytes.push(0);
            bytes.extend(file);
        }
    }
    bytes
}

fn run(exe: &Path, slots: u32, areas: u32, steps: u32, seed: u32) -> Vec<String> {
    run_with(exe, slots, areas, steps, seed, &[])
}

fn run_with(
    exe: &Path,
    slots: u32,
    areas: u32,
    steps: u32,
    seed: u32,
    words: &[&str],
) -> Vec<String> {
    let mut child = Command::new(exe)
        .args([slots, areas, steps, seed].map(|n| n.to_string()))
        .args(words)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the driver runs");
    let mut stdin = child.stdin.take().unwrap();
    let bytes = input_for(words);
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
        // One area: a record half fed holds it; a second connection that needs it waits (it is fed nothing, `recv`
        // needs no area) and is served once the area is back; a socket's end with none free fails cleanly.
        let one = run(&exe, 8, 1, 40, 3);
        for want in [
            "a record half fed holds the area",
            "a connection fed with no area free takes nothing and stays established",
            "the first completes and the area comes back",
            "the same bytes offered again once an area is free are taken and echoed",
            "the socket's end with no area free fails tls-peer-closed with its keys cleared",
            "after every step no free area or free core holds a byte, residue 0",
            "after the drops no area or core holds a byte, residue 0",
        ] {
            assert!(one.iter().any(|l| l.contains(want)), "{want}: {one:#?}");
        }
        // Resting (`docs/record-in-place.md` §2): an idle connection keeps its area until another needs one, at most as
        // many as allowed, its core there and nothing else; with every slot an area and with one area for all.
        for got in [&all, &one] {
            for want in [
                "rest: three connections in turn, at most two resting, each echoed from its area or its own core, residue 0",
                "rest: fewer allowed, the longest resting give their areas back cleared",
                "rest: a handshake takes a resting connection's area",
                "rest: a resting connection dropped gives its area back cleared",
                "rest: a resting connection whose socket ends fails tls-peer-closed and gives its area back",
                "rest: none allowed, the areas all come back",
            ] {
                assert!(got.iter().any(|l| l.contains(want)), "{want}: {got:#?}");
            }
        }
        // One slot, one area; and other seeds, so the sizes and the pieces differ.
        run(&exe, 1, 1, 30, 4);
        run(&exe, 5, 2, 80, 5);
        run(&exe, 16, 4, 80, 6);
        // Server tickets and resumption, and a client engine with a client identity: the ticket randomness, the
        // saved tickets and the identity's hedge are cleared with the rest.
        for (slots, areas, seed, words) in [
            (8, 3, 7, &["tickets"][..]),
            (8, 1, 8, &["tickets", "mutual"][..]),
            (5, 2, 9, &["mutual"][..]),
        ] {
            let got = run_with(&exe, slots, areas, 30, seed, words);
            if words.contains(&"tickets") {
                assert!(
                    got.iter()
                        .any(|l| l.starts_with("ok tickets: every slot saved one and resumed")),
                    "{words:?}: {got:#?}"
                );
            }
        }
        // A server that requires client certificates (`docs/tls-server.md` §13, `docs/tls-memory.md` §11): a verified
        // connection gives its area back after its handshake, its leaf kept in the engine's room for the slot, and the
        // `peer_` calls answer what was verified from it, idle and while an area is held again for a record; a failure,
        // a socket's end (with and without an area free), a drop and the slot's next connection leave nothing of it; a
        // leaf larger than the room keeps its area, and with fewer areas than slots such leaves can hold them all; the
        // store replaced before the client's Finished fails the handshake cleanly; `tls.residue` 0 after every step.
        for (slots, areas, seed) in [(4, 4, 10), (5, 2, 11), (1, 1, 12)] {
            let got = run_with(&exe, slots, areas, 30, seed, &["clientcert"]);
            for want in [
                "clientcert: every client verified, its connection gives its area back and keeps its identity, residue 0",
                "clientcert: traffic on verified connections, an area taken and given back each time, the identity the same",
                "clientcert: a verified connection that fails gives no identity, before and after its alert is taken",
                "clientcert: dropped, every area back and every leaf cleared, residue 0",
                "clientcert: a slot's next connection has its own leaf and nothing of the last one",
                "clientcert: a leaf larger than the room keeps its area and is read from it",
                "clientcert: the large leaves' areas come back cleared when dropped, residue 0",
                "clientcert: the store replaced before the client's Finished fails the handshake, its area cleared",
                "clientcert: after the drops no area or core holds a byte, residue 0",
            ] {
                assert!(got.iter().any(|l| l.contains(want)), "{want}: {got:#?}");
            }
            if areas < slots {
                assert!(
                    got.iter().any(|l| l.contains(
                        "with every area held by a connection with a large leaf a serve is tls-pool"
                    )),
                    "{got:#?}"
                );
                assert!(
                    got.iter().any(|l| l.contains(
                        "whose socket ends gives its leaf up, residue 0, with no area free 1"
                    )),
                    "{got:#?}"
                );
            } else if slots > 1 {
                assert!(
                    got.iter().any(|l| l.contains(
                        "whose socket ends gives its leaf up, residue 0, with no area free 0"
                    )),
                    "{got:#?}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
