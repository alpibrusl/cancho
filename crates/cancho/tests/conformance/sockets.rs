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

/// A program with its own `run(bound, io)`, the capability narrowed to `bound` (a `host:port`), `BOUND` and `PORT` substituted.
pub(super) fn dial_program(port: u16, bound: &str, source: &str) -> String {
    let main = r#"
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = narrow(net, "BOUND");
    return run(bound, io);
}
"#;
    format!("edition 5;\nimport std.io;\n{source}\n{main}")
        .replace("BOUND", bound)
        .replace("PORT", &port.to_string())
}

/// Wait for a line on the child's standard error.
pub(super) fn wait_for(lines: &mut impl std::io::BufRead, wanted: &str) {
    let mut line = String::new();
    loop {
        line.clear();
        let n = lines.read_line(&mut line).expect("the server's standard error");
        assert!(n > 0, "the server ended before saying `{wanted}`");
        if line.trim() == wanted {
            return;
        }
    }
}
