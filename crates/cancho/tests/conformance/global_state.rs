//! `docs/unsafe-and-global-state.md` (#587) §5: state that outlives the thing it described must not be usable by the
//! next one. The other half of this file (§6, a failed allocation leaves nothing half-made, and what a trap leaves) moved
//! to `tests/commands/global_state.cho`; this test stays because it needs a server that accepts two connections and
//! holds them while the program runs, and the harness has no helper for a background server (`docs/gate.md` G4).
//!
//! A generated program is one process, and `cancho test` runs one process a test (`docs/testing.md`), so "a later
//! run" is a later *unit of work* in a long-lived program (a server) or a later compilation in one compiler process.

use super::sockets::{BACKENDS, build, dial_program};
use super::*;
use std::net::TcpListener;

/// A server that keeps every connection open, so the descriptors a program dials stay in use at the other end only.
fn keep_open() -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("an address").port();
    let thread = std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
            if held.len() == 2 {
                break;
            }
        }
        // Hold both until the program has gone.
        std::thread::sleep(std::time::Duration::from_secs(5));
        drop(held);
    });
    (port, thread)
}

const REUSED_DESCRIPTOR: &str = r#"
fn run(bound: Net("BOUND"), io: Io) -> [] int {
    var score = 0;
    borrow bound as &n in {
        match tcp_connect(n, "127.0.0.1", PORT) {
            Dialed::Ok(a) => {
                let first = conn_detach(a);
                match conn_attach(first) {
                    Attached::Ok(back) => {
                        // The connection ends and the kernel may give its number to the next one.
                        conn_close(back);
                        match tcp_connect(n, "127.0.0.1", PORT) {
                            Dialed::Ok(b) => {
                                let second = conn_detach(b);
                                // The same descriptor number, held by another connection: the case the epoch is for.
                                if first & 4294967295 == second & 4294967295 { score = score + 1; }
                                // The old ticket must not reach the new connection...
                                match conn_attach(first) {
                                    Attached::Ok(stolen) => { conn_close(stolen); }
                                    Attached::Failed(e) => { if e == 9 { score = score + 1; } }
                                }
                                // ...which is still there for the ticket that is its own.
                                match conn_attach(second) {
                                    Attached::Ok(own) => { score = score + 1; conn_close(own); }
                                    Attached::Failed(e) => { }
                                }
                            }
                            Dialed::Failed(e) => { }
                        }
                    }
                    Attached::Failed(e) => { }
                }
            }
            Dialed::Failed(e) => { }
        }
    }
    release(bound);
    release(io);
    if score == 3 {
        return 0;
    }
    return 100 + score;
}
"#;

/// §5: `lexs_fd_epoch`. A ticket from `conn_detach` names a descriptor and the epoch it had; the table is the one piece
/// of state a long-lived server carries from one connection to the next. When a connection ends and another takes its
/// descriptor number, the first one's ticket must not reach the second (the use-after-free of a descriptor). The program
/// arranges the reuse and *checks that it happened* (equal numbers) before it asks.
#[test]
fn a_ticket_does_not_reach_the_connection_that_took_its_descriptor_number() {
    for backend in BACKENDS {
        let (port, server) = keep_open();
        let dir = scratch(&format!("global-state-fd-{backend}"));
        let exe = build(
            &dir,
            "reuse",
            &dial_program(port, &format!("127.0.0.1:{port}"), REUSED_DESCRIPTOR),
            backend,
        );
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(0),
            "{backend}: score {:?} (101 means the number was not reused, so nothing was shown)",
            run.status.code().map(|c| c - 100)
        );
        drop(server);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
