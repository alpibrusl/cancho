//! `docs/package-system.md` §9: the release scripts, tested with a stand-in compiler.
//!
//! The scripts never look inside the binary but for `--version`, so a shell script that prints a version line is a
//! faithful one, and the tests do not depend on whether the compiler under test was built from a clean checkout.

use super::*;
use std::os::unix::fs::PermissionsExt;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
/// The target name `install.sh` computes for this machine, or `None` where it has no asset to look for.
fn target_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("macos", "aarch64") => Some("darwin-aarch64"),
        _ => None,
    }
}

/// Return from a test on a machine `install.sh` has no asset for, saying so.
macro_rules! supported {
    () => {
        if target_name().is_none() {
            eprintln!("skipped: install.sh has no prebuilt target for this machine");
            return;
        }
    };
}

fn script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts").join(name)
}

fn fake_compiler(dir: &Path, version: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("cancho");
    std::fs::write(&path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn version_line(rev: &str) -> String {
    format!("cancho 0.0.0 (rev {rev}, host fake)")
}

fn package(bin: &Path, out: &Path) -> std::process::Output {
    Command::new(script("package-release.sh"))
        .arg(target_name().unwrap())
        .arg(out)
        .env("CANCHO_BIN", bin)
        .output()
        .unwrap()
}

/// `install.sh <rev> <prefix>` with the assets served from `from`.
fn install(rev: &str, prefix: &Path, from: &Path) -> std::process::Output {
    Command::new(script("install.sh"))
        .arg(rev)
        .arg(prefix)
        .env("CANCHO_RELEASES", format!("file://{}", from.display()))
        .output()
        .unwrap()
}

fn asset(rev: &str) -> String {
    format!("cancho-{rev}-{}.tar.gz", target_name().unwrap())
}

fn sh(dir: &Path, command: &str) {
    let out = Command::new("sh").current_dir(dir).args(["-c", command]).output().unwrap();
    assert!(out.status.success(), "{command}: {}", String::from_utf8_lossy(&out.stderr));
}

fn code(output: &std::process::Output) -> i32 {
    output.status.code().expect("exited, not killed")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Packaged assets for `REV`, in a fresh directory.
fn released(tag: &str) -> (PathBuf, PathBuf) {
    let root = scratch(tag);
    let dist = root.join("dist");
    let bin = fake_compiler(&root.join("build"), &version_line(REV));
    let out = package(&bin, &dist);
    assert!(out.status.success(), "{}", stderr(&out));
    (root, dist)
}

#[test]
fn a_packaged_compiler_installs_and_reports_the_commit_it_was_asked_for() {
    supported!();
    let (root, dist) = released("release-roundtrip");
    let name = asset(REV);
    assert!(
        dist.join(&name).is_file(),
        "{:?}",
        std::fs::read_dir(&dist).unwrap().collect::<Vec<_>>()
    );
    let sum = std::fs::read_to_string(dist.join(format!("{name}.sha256"))).unwrap();
    assert_eq!(sum.lines().count(), 1, "{sum}");
    assert!(sum.trim_end().ends_with(&name), "{sum}");

    let prefix = root.join("prefix");
    let out = install(REV, &prefix, &dist);
    assert!(out.status.success(), "{}", stderr(&out));
    let installed = Command::new(prefix.join("bin/cancho")).arg("--version").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&installed.stdout).trim(), version_line(REV));
}

#[test]
fn a_binary_that_is_not_a_clean_commit_is_not_packaged() {
    supported!();
    for version in [
        format!("cancho 0.0.0 (rev {REV}-dirty, host fake)"),
        "cancho 0.0.0 (rev unknown, host fake)".to_owned(),
        "cancho 0.0.0".to_owned(),
    ] {
        let root = scratch("release-dirty");
        let bin = fake_compiler(&root.join("build"), &version);
        let out = package(&bin, &root.join("dist"));
        assert_eq!(code(&out), 1, "{version}: {}", stderr(&out));
        assert!(stderr(&out).contains("refusing to package"), "{}", stderr(&out));
        assert!(!root.join("dist").exists(), "{version}: wrote something anyway");
    }
}

#[test]
fn packaging_twice_gives_the_same_bytes() {
    supported!();
    let tar = Command::new("tar").arg("--version").output().unwrap();
    if !String::from_utf8_lossy(&tar.stdout).contains("GNU tar") {
        eprintln!("skipped: reproducible archives need GNU tar (docs/package-system.md §9)");
        return;
    }
    let (root, dist) = released("release-twice");
    let bin = root.join("build/cancho");
    let again = root.join("dist2");
    // tar records seconds: a second package made later must still be the same bytes.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert!(package(&bin, &again).status.success());
    let name = asset(REV);
    assert_eq!(std::fs::read(dist.join(&name)).unwrap(), std::fs::read(again.join(&name)).unwrap());
}

#[test]
fn a_tarball_that_does_not_match_its_sha256_is_not_installed() {
    supported!();
    let (root, dist) = released("release-tampered");
    let name = asset(REV);
    let mut bytes = std::fs::read(dist.join(&name)).unwrap();
    bytes.push(b'x');
    std::fs::write(dist.join(&name), bytes).unwrap();
    let prefix = root.join("prefix");
    let out = install(REV, &prefix, &dist);
    assert_eq!(code(&out), 4, "{}", stderr(&out));
    assert!(stderr(&out).contains("sha256 mismatch"), "{}", stderr(&out));
    assert!(!prefix.exists(), "installed something anyway");
}

#[test]
fn a_binary_that_reports_another_commit_is_not_installed() {
    supported!();
    // A genuine asset under the name of a different commit: the hash is right, the compiler is not the one asked for.
    let (root, dist) = released("release-wrongrev");
    let other = "f".repeat(40);
    let (from, to) = (asset(REV), asset(&other));
    sh(
        &dist,
        &format!(
            "mkdir x && tar -xzf {from} -C x && mv x/cancho-{REV}-{t} x/cancho-{other}-{t}",
            t = target_name().unwrap()
        ),
    );
    sh(&dist, &format!("tar -czf {to} -C x cancho-{other}-{}", target_name().unwrap()));
    sh(
        &dist,
        &format!("shasum -a 256 {to} > {to}.sha256 2>/dev/null || sha256sum {to} > {to}.sha256"),
    );
    let prefix = root.join("prefix");
    let out = install(&other, &prefix, &dist);
    assert_eq!(code(&out), 4, "{}", stderr(&out));
    assert!(stderr(&out).contains("reports rev"), "{}", stderr(&out));
    assert!(!prefix.exists(), "installed something anyway");
}

#[test]
fn install_refuses_what_is_not_a_full_commit_and_what_is_not_there() {
    supported!();
    let (root, dist) = released("release-refusals");
    let prefix = root.join("prefix");
    // Forty characters that are not hex digits: the right length is not enough, and `..` must not reach a URL.
    let traversal = format!("{}a", "../".repeat(13));
    for bad in ["abc", "ZZZZ", "main", &"0".repeat(41), &"g".repeat(40), &traversal, ""] {
        let out = install(bad, &prefix, &dist);
        assert_eq!(code(&out), 2, "{bad:?}: {}", stderr(&out));
    }
    let other = "e".repeat(40);
    let out = install(&other, &prefix, &dist);
    assert!(!out.status.success(), "an asset that does not exist installed");
    assert!(!prefix.exists());

    // The sum without the tarball is as missing as the tarball without the sum.
    std::fs::remove_file(dist.join(format!("{}.sha256", asset(REV)))).unwrap();
    let out = install(REV, &prefix, &dist);
    assert!(!out.status.success(), "installed with no checksum");
    assert!(!prefix.exists());
}
