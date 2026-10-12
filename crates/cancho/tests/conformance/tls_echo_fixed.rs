//! `examples/tls_echo_fixed` (`docs/narrowing-into-several.md` section 11.1): `tls_echo` with the
//! certificate directory fixed at build time, so `main` narrows the filesystem to two literals and
//! the authority report has no `fs_read("")`. It shares `serve`, the loop and the options with
//! `tls_echo` (`examples/tls_echo/echo.cho`, `front.cho`), which `duplication.rs` keeps true.

use super::tls_echo::{Log, Running, build, clients, install, signal};
use super::*;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Condvar, Mutex};

/// The directory the shipped example reads its certificates from.
const FIXED: &str = "/etc/cancho/tls_echo";

fn main_file() -> PathBuf {
    repo_root().join("examples/tls_echo_fixed/tls_echo_fixed.cho")
}

fn example_files(main: &Path) -> Vec<PathBuf> {
    let mut files = vec![main.to_path_buf()];
    files.extend(
        ["echo.cho", "front.cho", "identity.cho"]
            .iter()
            .map(|f| repo_root().join("examples/tls_echo").join(f)),
    );
    files.extend(super::tls_server::package_files());
    files
}

fn labels_of(json: &str) -> Vec<String> {
    json.lines()
        .filter_map(|l| l.trim().strip_prefix("{ \"name\": "))
        .map(|l| l.split(", \"bounded\"").next().unwrap().to_string())
        .collect()
}

/// The report, pinned as `tls_echo`'s is, with the one difference the example exists for: the
/// two `fs_read` labels name `/dev/urandom` and the certificate directory, and none names `""`.
/// Every other label is `tls_echo`'s.
#[test]
fn the_report_names_the_two_paths_and_no_fs_read_of_the_root() {
    let out = Command::new(BIN)
        .arg("authority")
        .args(example_files(&main_file()))
        .args(["--std", "--output", "json"])
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(json.trim_start().starts_with("{\n  \"bounded\": true,"), "{json}");
    assert!(json.contains("\"foreign_symbols\": []"), "{json}");
    assert!(!json.contains("\"fs_read\", \"argument\": \"\""), "{json}");
    let want = [
        "\"args\", \"argument\": null",
        "\"clock\", \"argument\": null",
        "\"conn_accept\", \"argument\": null",
        "\"conn_read\", \"argument\": null",
        "\"conn_write\", \"argument\": null",
        "\"dir_read\", \"argument\": null",
        "\"err_write\", \"argument\": null",
        "\"file_read\", \"argument\": null",
        "\"fs_read\", \"argument\": \"/dev/urandom\"",
        "\"fs_read\", \"argument\": \"/etc/cancho/tls_echo\"",
        "\"heap\", \"argument\": null",
        "\"io_write\", \"argument\": null",
        "\"net_in\", \"argument\": \"\"",
        "\"poll\", \"argument\": null",
        "\"signals\", \"argument\": \"HUP,INT,TERM\"",
        "\"signals_read\", \"argument\": null",
    ];
    assert_eq!(labels_of(&json), want, "{json}");
}

/// The shipped example with `FIXED` replaced by `certs`, built; the literal is the only
/// difference (a test cannot create `/etc/cancho`).
fn build_with_directory(dir: &Path, certs: &Path) -> PathBuf {
    let source = std::fs::read_to_string(main_file()).expect("the example is readable");
    assert!(
        source.contains(&format!("narrow(fs, \"/dev/urandom\", \"{FIXED}\")")),
        "main narrows to the two literals"
    );
    let main = dir.join("tls_echo_fixed.cho");
    std::fs::write(&main, source.replace(FIXED, &certs.to_string_lossy()))
        .expect("a writable scratch directory");
    build(dir, "tls_echo_fixed", &example_files(&main))
}

/// It serves from the fixed directory with no `--dir`, echoes to a client that verifies the
/// chain, refuses `--dir` (a usage error, status 2), exits 5 when the fixed directory is absent,
/// and stops cleanly on `SIGTERM`.
#[test]
fn it_serves_from_the_fixed_directory_and_refuses_dir() {
    let dir = scratch("tls-echo-fixed");
    let certs = dir.join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    install("first", &certs, &["chain.pem", "key.pem", "names"]);
    let echo = build_with_directory(&dir, &certs);
    let mut many_files = vec![repo_root().join("tests/programs/tls_many.cho")];
    many_files.extend(super::tls_server::package_files());
    let many = build(&dir, "tls_many", &many_files);

    // `--dir` is not an option here.
    let refused = Command::new(&echo)
        .args(["--port", "1", "--dir"])
        .arg(&certs)
        .output()
        .expect("the example runs");
    assert_eq!(refused.status.code(), Some(2), "{}", String::from_utf8_lossy(&refused.stderr));

    let port = free_port();
    let mut server = Running(
        Command::new(&echo)
            .args(["--port", &port.to_string(), "--idle", "500", "--handshakes", "4"])
            .args(["--connections", "16"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let log = Arc::new(Log { lines: Mutex::new(Vec::new()), more: Condvar::new() });
    let stdout = server.0.stdout.take().unwrap();
    let writer = Arc::clone(&log);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            writer.lines.lock().unwrap().push(line.unwrap());
            writer.more.notify_all();
        }
    });
    log.wait("listening", 1);
    clients(&many, port, 8);
    log.wait(" established suite=", 8);
    signal(server.0.id(), "TERM");
    let status = server.0.wait().unwrap();
    reader.join().unwrap();
    assert_eq!(status.code(), Some(0));

    // The directory the literal names is missing: status 5, nothing listening.
    let absent = dir.join("absent");
    let other = dir.join("absent-build");
    std::fs::create_dir_all(&other).expect("a writable scratch directory");
    let echo = build_with_directory(&other, &absent);
    let missing = Command::new(&echo)
        .args(["--port", &free_port().to_string()])
        .output()
        .expect("the example runs");
    assert_eq!(missing.status.code(), Some(5), "{}", String::from_utf8_lossy(&missing.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
