//! Close-on-exec (`docs/processes.md` §4.5, slice 0): no descriptor a
//! builtin opens crosses an `exec`.
//!
//! The program holds one of everything a builtin can open -- a file read
//! and one opened through `fopen`, a `Dir`, one entered beneath it, a file
//! opened beneath it, a listing, a listener, both ends of a connection, the
//! edition-2 `bind`, `connect` and `accept` descriptors, a `Poller` and a
//! signal claim -- and then asks a child what it inherited: `system("ls
//! /dev/fd")` forks and execs `/bin/sh`, which runs `ls`. The child must see
//! its three standard streams and the one descriptor `ls` opens to read the
//! directory, and nothing of the parent's. Measured before the change: the
//! child saw twelve of the parent's descriptors. Both backends.
//!
//! `system` is reached through `Ffi("libc")`, which is the point: until
//! `Exec` exists, a foreign `exec` is the only way a lex-sys program starts
//! another, and it is exactly the path that used to leak.

use super::*;

const BACKENDS: [&str; 2] = ["cranelift", "llvm"];

/// The program's exit status is how many handles it held when the child ran:
/// eleven handles and three edition-2 descriptors.
const HELD: i32 = 14;

fn program(p1: u16, p2: u16) -> String {
    format!(
        r#"edition 6;

extern fn system[&f, &c](ffi: &f Ffi("libc"), command: &c [byte]) -> [ffi("libc")] c_int;

// Everything a builtin can open, held at once, and then a shell asked what it
// inherited: `ls /dev/fd` from `system`, which forks and execs `/bin/sh`.
fn show[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {{
    region a {{
        let text = "ls /dev/fd";
        let command = alloc_slice[a](len(text) + 1, byte_of(0));
        var i = 0;
        while i < len(text) {{
            command[i] = text[i];
            i = i + 1;
        }}
        system(ffi, command);
    }}
    return 0;
}}

fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock, signals }} = split(world);
    release(io); release(heap); release(args); release(clock);
    let usr1 = narrow(signals, "USR1");
    let libc = narrow(ffi, "libc");
    var held = 0;
    borrow fs as &f in {{
        borrow net as &n in {{
            borrow usr1 as &u in {{
                borrow libc as &x in {{
                    match open_read(f, "tree/file") {{
                        Opened::Ok(read) => {{
                            held = held + 1;
                            match open_append(f, "tree/log") {{
                                Opened::Ok(log) => {{
                                    held = held + 1;
                                    match open_dir(f, "tree") {{
                                        DirOpened::Ok(d) => {{
                                            var dir = d;
                                            held = held + 1;
                                            borrow dir as &dh in {{
                                                match dir_enter(dh, "sub") {{
                                                    DirOpened::Ok(sub) => {{
                                                        held = held + 1;
                                                        match dir_open_read(dh, "file") {{
                                                            Opened::Ok(inner) => {{
                                                                held = held + 1;
                                                                match dir_list(dh) {{
                                                                    Listing::Ok(list) => {{
                                                                        held = held + 1;
                                                                        match tcp_listen(n, {p1}, 4, 0) {{
                                                                            Listening::Ok(l) => {{
                                                                                var listener = l;
                                                                                held = held + 1;
                                                                                match tcp_connect(n, "127.0.0.1", {p1}) {{
                                                                                    Dialed::Ok(out) => {{
                                                                                        held = held + 1;
                                                                                        borrow mut listener as &!lh in {{
                                                                                            match tcp_accept(lh) {{
                                                                                                Accepted::Ok(inn) => {{
                                                                                                    held = held + 1;
                                                                                                    match poller_new() {{
                                                                                                        Polling::Ok(p) => {{
                                                                                                            held = held + 1;
                                                                                                            match signals_watch(u) {{
                                                                                                                Watching::Ok(w) => {{
                                                                                                                    held = held + 1;
                                                                                                                    let old = bind(n, {p2});
                                                                                                                    listen(old, 4);
                                                                                                                    let dialed = connect(n, "127.0.0.1", {p2});
                                                                                                                    let taken = accept(old);
                                                                                                                    if old >= 0 && dialed >= 0 && taken >= 0 {{
                                                                                                                        held = held + 3;
                                                                                                                    }}
                                                                                                                    show(x);
                                                                                                                    signals_close(w);
                                                                                                                }}
                                                                                                                Watching::Failed(e) => {{ }}
                                                                                                            }}
                                                                                                            poller_close(p);
                                                                                                        }}
                                                                                                        Polling::Failed(e) => {{ }}
                                                                                                    }}
                                                                                                    conn_close(inn);
                                                                                                }}
                                                                                                Accepted::Again => {{ }}
                                                                                                Accepted::Failed(e) => {{ }}
                                                                                            }}
                                                                                        }}
                                                                                        conn_close(out);
                                                                                    }}
                                                                                    Dialed::Failed(e) => {{ }}
                                                                                }}
                                                                                listener_close(listener);
                                                                            }}
                                                                            Listening::Failed(e) => {{ }}
                                                                        }}
                                                                        dir_list_close(list);
                                                                    }}
                                                                    Listing::Failed(e) => {{ }}
                                                                }}
                                                                file_close(inner);
                                                            }}
                                                            Opened::Failed(e) => {{ }}
                                                        }}
                                                        dir_close(sub);
                                                    }}
                                                    DirOpened::Failed(e) => {{ }}
                                                }}
                                            }}
                                            dir_close(dir);
                                        }}
                                        DirOpened::Failed(e) => {{ }}
                                    }}
                                    file_close(log);
                                }}
                                Opened::Failed(e) => {{ }}
                            }}
                            file_close(read);
                        }}
                        Opened::Failed(e) => {{ }}
                    }}
                }}
            }}
        }}
    }}
    release(fs); release(net); release(usr1); release(libc);
    return held;
}}
"#
    )
}

#[test]
fn no_descriptor_a_builtin_opens_crosses_an_exec() {
    let dir = scratch("close-on-exec");
    std::fs::create_dir_all(dir.join("tree").join("sub")).expect("a fixture tree");
    std::fs::write(dir.join("tree").join("file"), b"hi\n").expect("a fixture file");
    for backend in BACKENDS {
        let source = dir.join(format!("held-{backend}.ls"));
        std::fs::write(&source, program(free_port(), free_port())).expect("the program is written");
        let exe = dir.join(format!("held-{backend}"));
        let build = Command::new(BIN)
            .arg("build")
            .arg(&source)
            .args(["--std", "--backend", backend, "-o"])
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(
            build.status.success(),
            "`--backend {backend}` should build the program:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let run = Command::new(&exe).current_dir(&dir).output().expect("the program runs");
        assert_eq!(
            run.status.code(),
            Some(HELD),
            "`{backend}`: the program should hold {HELD} descriptors when the child runs"
        );
        let listed = String::from_utf8_lossy(&run.stdout);
        let mut seen: Vec<u32> =
            listed.split_whitespace().filter_map(|word| word.parse().ok()).collect();
        seen.sort_unstable();
        assert!(
            seen.starts_with(&[0, 1, 2]),
            "`{backend}`: the child should see its standard streams, and saw {seen:?}"
        );
        assert!(
            seen.len() <= 4,
            "`{backend}`: the child inherited the parent's descriptors: it saw {seen:?}, where its \
             own are 0, 1, 2 and the one `ls` reads the directory with"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
