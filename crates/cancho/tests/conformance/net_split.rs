//! `docs/net-bounds.md` section 3, edition 11: `split` hands out `Out` and `Ports` where it handed out `Net`.
//!
//! The fixtures under `tests/` cover what compiles and what is refused; this file builds and runs: the report names
//! `net_out` for the outbound half and `net_in` for the inbound one and calls the program bounded, and each half is
//! enforced at run time by its own bound on both backends (a dial to another port traps, and so does a listener on one).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{BIN, authority_json, scratch};

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// A program that narrows `out` and `ports`, dials `dial_port` through the first and listens on `listen_port` through the
/// second. Nothing answers on the dialed port, and a refused connection is an answer, so only the bound can kill it.
fn program(out_bound: &str, ports_bound: &str, dial_port: u32, listen_port: u32) -> String {
    format!(
        r#"edition 11;
fn work[&o, &p](up: &o Out("{out_bound}"), inbound: &p Ports("{ports_bound}")) -> [net_out("{out_bound}"), net_in("{ports_bound}")] int {{
    match tcp_listen(inbound, {listen_port}, 8, 0) {{
        Listening::Ok(l) => {{ listener_close(l); }}
        Listening::Failed(e) => {{ let ignored = e; }}
    }}
    match tcp_connect(up, "127.0.0.1", {dial_port}) {{
        Dialed::Ok(c) => {{ conn_close(c); }}
        Dialed::Failed(e) => {{ let ignored = e; }}
    }}
    return 0;
}}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, out, ports, clock, signals, exec, tty }} = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args); release(clock);
    release(signals); release(exec); release(tty);
    let up = narrow(out, "{out_bound}");
    let inbound = narrow(ports, "{ports_bound}");
    var status = 0;
    borrow up as &a in {{
        borrow inbound as &b in {{ status = work(a, b); }}
    }}
    release(up); release(inbound);
    return status;
}}
"#
    )
}

fn build(dir: &Path, source: &str, backend: &str) -> PathBuf {
    let path = dir.join("split.cho");
    std::fs::write(&path, source).expect("a writable fixture");
    let exe = dir.join(format!("split-{backend}"));
    let out = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        out.status.success(),
        "should compile on {backend}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    exe
}

/// True when the run was killed by a signal (the bound's trap), false when it ran through.
fn trapped(source: &str, dir: &Path) -> bool {
    let mut verdicts = Vec::new();
    for backend in BACKENDS {
        let exe = build(dir, source, backend);
        verdicts
            .push(Command::new(exe).output().expect("the program runs").status.code().is_none());
    }
    assert_eq!(verdicts[0], verdicts[1], "the backends disagree: {verdicts:?}");
    verdicts[0]
}

#[test]
fn the_report_names_a_dial_and_a_listener_and_is_bounded() {
    let json = authority_json(&program("127.0.0.1:53", "5353", 53, 5353), "net-split-report");
    assert!(json.starts_with("{\n  \"bounded\": true,\n  \"unbounded_by\": [],"), "{json}");
    assert!(json.contains("\"name\": \"net_out\""), "{json}");
    assert!(json.contains("\"argument\": \"127.0.0.1:53\""), "{json}");
    assert!(json.contains("\"name\": \"net_in\""), "{json}");
    assert!(json.contains("\"argument\": \"5353\""), "{json}");
}

#[test]
fn each_half_is_enforced_by_its_own_bound_on_both_backends() {
    let dir = scratch("net-split-run");
    assert!(!trapped(&program("127.0.0.1:53", "5353", 53, 5353), &dir), "inside both bounds");
    assert!(trapped(&program("127.0.0.1:53", "5353", 54, 5353), &dir), "a dial to another port");
    assert!(
        trapped(&program("127.0.0.1:53", "5353", 53, 5354), &dir),
        "a listener on another port"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
