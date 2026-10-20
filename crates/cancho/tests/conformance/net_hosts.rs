//! `docs/net-bounds.md` section 2.3, edition 10: the host half of a `Net` bound is matched exactly, or as `*.suffix` for
//! one label. Before edition 10 it is a prefix (`127.0.0.1` admits `127.0.0.10`), and that is unchanged.
//!
//! The names are IP literals, so no resolver is asked: a name the bound admits is dialed (a refused connection is an
//! answer, `Dialed::Failed`), and a name it does not admit traps. `*.0.0.1` stands for a one-label suffix because
//! `127.0.0.1` is `127` ahead of `.0.0.1`.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{BIN, scratch};

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// A program at `edition` that narrows the network to `bound` and dials `name` on port 9 through it.
fn dialer(edition: u32, bound: &str, name: &str) -> String {
    format!(
        r#"edition {edition};
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals, exec, tty }} = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(clock);
    release(signals); release(exec); release(tty);
    let bound = narrow(net, "{bound}");
    var status = 0;
    borrow bound as &n in {{
        match tcp_connect(n, "{name}", 9) {{
            Dialed::Ok(c) => {{ conn_close(c); status = 0; }}
            Dialed::Failed(e) => {{ status = 0; }}
        }}
    }}
    release(bound);
    return status;
}}
"#
    )
}

fn build(dir: &Path, name: &str, source: &str, backend: &str) -> PathBuf {
    let path = dir.join(format!("{name}.cho"));
    std::fs::write(&path, source).expect("a writable fixture");
    let exe = dir.join(format!("{name}-{backend}"));
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`{name}` should compile on {backend}:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

/// `true` when the dial was refused by the bound (the program is killed by a signal), `false` when it ran through.
fn trapped(edition: u32, bound: &str, name: &str, dir: &Path) -> bool {
    let mut verdicts = Vec::new();
    for backend in BACKENDS {
        let exe = build(dir, "dial", &dialer(edition, bound, name), backend);
        let run = Command::new(&exe).output().expect("the compiled program runs");
        verdicts.push(run.status.code().is_none());
    }
    assert_eq!(
        verdicts[0], verdicts[1],
        "the backends disagree on `{name}` through `{bound}`: {verdicts:?}"
    );
    verdicts[0]
}

#[test]
fn before_edition_ten_a_host_bound_is_a_prefix() {
    let dir = scratch("net-hosts-prefix");
    // `127.0.0.1` admits `127.0.0.10`, and that is the defect edition 10 closes.
    assert!(!trapped(9, "127.0.0.1:9", "127.0.0.10", &dir));
    assert!(!trapped(9, "127.0.0.1:9", "127.0.0.1", &dir));
    assert!(trapped(9, "127.0.0.1:9", "127.0.0.2", &dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn from_edition_ten_a_host_bound_is_exact() {
    let dir = scratch("net-hosts-exact");
    assert!(!trapped(10, "127.0.0.1:9", "127.0.0.1", &dir));
    assert!(trapped(10, "127.0.0.1:9", "127.0.0.10", &dir), "a longer name is a different host");
    assert!(trapped(10, "127.0.0.1:9", "127.0.0.", &dir), "a shorter one is too");
    assert!(trapped(10, "127.0.0.1:9", "127.0.0.2", &dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wildcard_is_exactly_one_label_ahead_of_its_suffix() {
    let dir = scratch("net-hosts-wildcard");
    assert!(!trapped(10, "*.0.0.1:9", "127.0.0.1", &dir), "one label ahead of the suffix");
    assert!(!trapped(10, "*.0.0.1:9", "7.0.0.1", &dir));
    assert!(trapped(10, "*.0.0.1:9", "5.127.0.0.1", &dir), "two labels");
    assert!(trapped(10, "*.0.0.1:9", ".0.0.1", &dir), "an empty label");
    assert!(trapped(10, "*.0.0.1:9", "0.0.1", &dir), "no label at all");
    assert!(trapped(10, "*.0.0.1:9", "127.0.0.2", &dir), "another suffix");
    assert!(trapped(10, "*.0.0.1:9", "127.0.0.10", &dir), "a longer suffix");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_host_half_still_admits_every_name_in_every_edition() {
    let dir = scratch("net-hosts-empty");
    for edition in [9, 10] {
        assert!(!trapped(edition, ":9", "127.0.0.1", &dir), "edition {edition}");
        assert!(!trapped(edition, ":9", "127.0.0.10", &dir), "edition {edition}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
