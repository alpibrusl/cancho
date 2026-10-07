//! `packages/x509` (`docs/x509.md` §5): the corpus `scripts/x509_corpus.py`
//! made and pyca/cryptography described, field for field, on both
//! backends; every damaged certificate refused with its own tag; PEM
//! blocks that are not well-formed; and the fuzz harness for a short run.
//! All through `tests/programs/x509_driver.cho` and `x509_fuzz.cho`.

use super::json::feed;
use super::*;

fn build_x509(program: &str, test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("x509-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join(program))
        .arg(repo_root().join("packages/x509/x509.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn run(exe: &Path, input: &[u8]) -> Vec<String> {
    let out = feed(exe, input);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(|l| l.trim_end().to_string()).collect()
}

fn vectors(name: &str) -> String {
    std::fs::read_to_string(repo_root().join("tests/vectors/x509").join(name)).unwrap()
}

#[test]
fn every_corpus_certificate_reads_as_pyca_reads_it_on_both_backends() {
    let pem = vectors("corpus.pem");
    let want: Vec<String> = vectors("corpus.txt").lines().map(str::to_string).collect();
    assert_eq!(
        want.len(),
        10,
        "RSA, P-256, P-384, P-521, Ed25519, PSS, v1, and the two leniencies"
    );
    for backend in ["cranelift", "llvm"] {
        let (dir, exe) = build_x509("tests/programs/x509_driver.cho", "corpus", backend);
        let got = run(&exe, pem.as_bytes());
        assert_eq!(got.len(), want.len(), "one line per certificate on {backend}");
        for (n, (g, w)) in got.iter().zip(&want).enumerate() {
            assert_eq!(g, w, "certificate {n} on {backend}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn every_damaged_certificate_is_refused_with_its_own_tag() {
    let text = vectors("negative.txt");
    let rows: Vec<Vec<&str>> =
        text.lines().filter(|l| !l.starts_with('#')).map(|l| l.split(" | ").collect()).collect();
    assert_eq!(rows.len(), 36);
    let mut input = rows.iter().map(|r| r[2]).collect::<Vec<_>>().join("\n");
    input.push('\n');
    let (dir, exe) = build_x509("tests/programs/x509_driver.cho", "negative", "cranelift");
    let got = run(&exe, input.as_bytes());
    assert_eq!(got.len(), rows.len());
    for (row, line) in rows.iter().zip(&got) {
        let tag = line.split(' ').nth(1).unwrap_or("");
        assert_eq!(tag, row[1], "{}: {line}", row[0]);
        assert!(line.starts_with('-'), "{}: a refusal has a negative code: {line}", row[0]);
    }
    // Every refusal the parser has, except `pem`, which DER cannot reach
    // (it is the next test's).
    let tags: std::collections::BTreeSet<&str> = rows.iter().map(|r| r[1]).collect();
    assert_eq!(tags.len(), 18, "{tags:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_pem_block_is_refused_and_the_bundle_goes_on() {
    let pem = vectors("corpus.pem");
    let first = &pem[..pem.find("-----END CERTIFICATE-----").unwrap() + 26];
    let body = &first["-----BEGIN CERTIFICATE-----\n".len()..first.find("-----END").unwrap()];
    let block = |b: &str| format!("-----BEGIN CERTIFICATE-----\n{b}-----END CERTIFICATE-----\n");
    let big = "QUFB".repeat(6000);
    let input = [
        first.to_string(),
        block(&body.replacen('M', "!", 1)),
        block("QUJD=\n"),
        block("QUI\n"),
        block(&format!("{big}\n")),
        first.to_string(),
        "-----BEGIN CERTIFICATE-----\nMIIB\n".to_string(),
    ]
    .concat();
    let (dir, exe) = build_x509("tests/programs/x509_driver.cho", "pem", "cranelift");
    let got = run(&exe, input.as_bytes());
    let tags: Vec<&str> = got.iter().map(|l| l.split(' ').nth(1).unwrap_or("")).collect();
    assert_eq!(
        tags,
        ["ok", "pem", "pem", "pem", "x509-too-large", "ok", "pem"],
        "a bad character, padding after a whole group, a missing pad, 18,000 bytes, and no END line"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `docs/x509.md` §5.4 reports the 1,000,000-round run; this is 20,000
/// rounds of the same harness, so a regression traps here first.
#[test]
fn the_fuzz_harness_runs_clean() {
    let mut input = b"20000\n".to_vec();
    input.extend_from_slice(vectors("corpus.pem").as_bytes());
    let (dir, exe) = build_x509("tests/programs/x509_fuzz.cho", "fuzz", "cranelift");
    let got = run(&exe, &input);
    assert_eq!(got.last().map(String::as_str), Some("x509 fuzz ok rounds=20000"), "{got:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
