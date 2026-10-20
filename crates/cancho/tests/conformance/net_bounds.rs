//! `docs/net-bounds.md` section 2.4: the report does not call an unnarrowed network bounded.
//!
//! `split` hands out the whole network, and a program that dials or listens with it can reach every host and port (or every
//! port). Its label has an empty argument, which names no domain, so `bounded` is `false` and `unbounded_by` says which
//! label made it so, as it does for a foreign symbol. A program that narrows first keeps `bounded: true`.

use std::process::Command;

use super::{BIN, authority_json, scratch};

/// A program that dials with the network as `split` gave it, or narrowed to `BOUND` when that is not empty.
fn dialer(bound: &str) -> String {
    let (ty, take) = if bound.is_empty() {
        ("Net(\"\")".to_owned(), "net".to_owned())
    } else {
        (format!("Net(\"{bound}\")"), format!("narrow(net, \"{bound}\")"))
    };
    format!(
        r#"edition 5;
fn run(bound: {ty}, io: Io) -> [] int {{
    var status = 1;
    borrow bound as &n in {{
        match tcp_connect(n, "127.0.0.1", 9) {{
            Dialed::Ok(c) => {{ conn_close(c); status = 0; }}
            Dialed::Failed(e) => {{ status = 2; }}
        }}
    }}
    release(bound);
    release(io);
    return status;
}}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = {take};
    return run(bound, io);
}}
"#
    )
}

/// A program that listens with the network as `split` gave it, or narrowed to the port `bound`.
fn listener(bound: &str) -> String {
    let (ty, take) = if bound.is_empty() {
        ("Net(\"\")".to_owned(), "net".to_owned())
    } else {
        (format!("Net(\"{bound}\")"), format!("narrow(net, \"{bound}\")"))
    };
    format!(
        r#"edition 5;
fn run(bound: {ty}, io: Io) -> [] int {{
    var status = 1;
    borrow bound as &n in {{
        match tcp_listen(n, 8080, 8, 0) {{
            Listening::Ok(l) => {{ listener_close(l); status = 0; }}
            Listening::Failed(e) => {{ status = 2; }}
        }}
    }}
    release(bound);
    release(io);
    return status;
}}
fn main(world: World) -> [] int {{
    let Split {{ io, ffi, fs, heap, args, net, clock }} = split(world);
    release(ffi); release(fs); release(heap); release(args); release(clock);
    let bound = {take};
    return run(bound, io);
}}
"#
    )
}

#[test]
fn an_unnarrowed_dialer_is_not_bounded_and_says_which_label() {
    let json = authority_json(&dialer(""), "net-bounds-open-out");
    assert!(
        json.starts_with(
            "{\n  \"bounded\": false,\n  \"unbounded_by\": [\n    \"net_out(\\\"\\\")\"\n  ],"
        ),
        "{json}"
    );
    assert!(
        json.contains("{ \"name\": \"net_out\", \"argument\": \"\", \"bounded\": false }"),
        "the label itself is the one that is not bounded:\n{json}"
    );
    // Everything else the program does is still bounded.
    assert!(
        json.contains("{ \"name\": \"heap\", \"argument\": null, \"bounded\": true }")
            || !json.contains("\"heap\""),
        "{json}"
    );
}

#[test]
fn an_unnarrowed_listener_is_not_bounded_and_says_which_label() {
    let json = authority_json(&listener(""), "net-bounds-open-in");
    assert!(
        json.starts_with(
            "{\n  \"bounded\": false,\n  \"unbounded_by\": [\n    \"net_in(\\\"\\\")\"\n  ],"
        ),
        "{json}"
    );
}

#[test]
fn a_narrowed_network_stays_bounded() {
    let out = authority_json(&dialer("127.0.0.1:9"), "net-bounds-narrow-out");
    assert!(out.starts_with("{\n  \"bounded\": true,\n  \"unbounded_by\": [],"), "{out}");
    assert!(
        out.contains("{ \"name\": \"net_out\", \"argument\": \"127.0.0.1:9\", \"bounded\": true }"),
        "{out}"
    );
    let inb = authority_json(&listener("8080"), "net-bounds-narrow-in");
    assert!(inb.starts_with("{\n  \"bounded\": true,\n  \"unbounded_by\": [],"), "{inb}");
}

#[test]
fn the_prose_report_says_so_too() {
    let dir = scratch("net-bounds-prose");
    let path = dir.join("program.cho");
    std::fs::write(&path, dialer("")).expect("a writable fixture");
    let out = Command::new(BIN)
        .args(["authority", "--std"])
        .arg(&path)
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        text.starts_with("UNBOUNDED: this program holds the network without narrowing it."),
        "{text}"
    );
    assert!(text.contains("    net_out(\"\")    <- unbounded"), "{text}");
    assert!(text.contains("unbounded by\n    net_out(\"\")"), "{text}");
    assert!(!text.contains("calls foreign code"), "a network is not foreign code:\n{text}");
}
