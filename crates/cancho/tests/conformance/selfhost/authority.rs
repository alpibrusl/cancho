//! `cancho authority` in cancho (`examples/selfhost/authority.cho`, `docs/cli-in-cancho.md` section 34), against `cancho authority`.

use super::*;

/// The name of the module a file declares, and the modules it imports.
fn module_and_imports(text: &str) -> (Option<String>, Vec<String>) {
    let mut module = None;
    let mut imports = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("module ") {
            module = Some(rest.trim_end_matches(';').trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("import ") {
            let path = rest.trim_end_matches(';').split_whitespace().next().unwrap_or("");
            imports.push(path.to_owned());
        }
    }
    (module, imports)
}

/// The files of a program: `files`, and, for each module they import that none of them declares and the library does not have, the file of `packages/` that declares it (once, and what it imports in turn).
fn with_packages(root: &Path, files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut index: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
    let mut packages: Vec<PathBuf> = Vec::new();
    for dir in std::fs::read_dir(root.join("packages")).unwrap() {
        let dir = dir.unwrap().path();
        if dir.is_dir() {
            for file in std::fs::read_dir(&dir).unwrap() {
                let file = file.unwrap().path();
                if file.extension().is_some_and(|e| e == "cho") {
                    packages.push(file);
                }
            }
        }
    }
    packages.sort();
    for file in packages {
        if let (Some(module), _) = module_and_imports(&std::fs::read_to_string(&file).unwrap()) {
            index.entry(module).or_insert(file);
        }
    }
    let mut files = files;
    loop {
        let mut declared = Vec::new();
        let mut imported = Vec::new();
        for file in &files {
            let (module, imports) = module_and_imports(&std::fs::read_to_string(file).unwrap());
            declared.extend(module);
            imported.extend(imports);
        }
        let more: Vec<PathBuf> = imported
            .iter()
            .filter(|m| !declared.contains(m) && !m.starts_with("std"))
            .filter_map(|m| index.get(m).cloned())
            .filter(|f| !files.contains(f))
            .collect();
        if more.is_empty() {
            return files;
        }
        for file in more {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
}

/// Rust's report without the two counts the compiler in cancho does not make: `folded_operators` and `folded_calls` of the data form, and the lines of "evaluated at compile time" of the prose.
fn without_folded(report: &str, json: bool) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in report.split_inclusive('\n') {
        if json {
            let t = line.trim_start();
            if t.starts_with("\"folded_operators\":") || t.starts_with("\"folded_calls\":") {
                continue;
            }
        } else if line.starts_with("evaluated at compile time") {
            skipping = true;
            continue;
        } else if skipping && line.starts_with("    ") {
            continue;
        } else {
            skipping = false;
        }
        out.push_str(line);
    }
    out
}

/// Programs the compiler in cancho answers status 3 for, and why: it has no code for them (`skip`), or one of them calls a function through a function value, whose labels it does not follow.
const STATUS_3: [&str; 2] = ["aliasing_in_a_slice_of_references.cho", "function_value.cho"];

/// `cancho authority` and `cli.cho authority` on the programs of the repository -- the programs of `examples/` (the ones of several files with the packages they import), and every program of
/// `tests/accept/` that has a `main` -- in both forms: the output has to be the same byte for byte, but for the two counts of what compile-time evaluation folded, which the compiler in cancho does not make.
#[test]
fn authority_in_cancho_reports_like_cancho_authority() {
    let exe = build("cli-authority", &with_front_end("cli.cho"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let std_dir = root.join("std");
    let mut programs: Vec<Vec<PathBuf>> = Vec::new();
    let mut singles: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    singles.extend(
        std::fs::read_dir(root.join("tests/accept"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "cho")),
    );
    singles.sort();
    for file in singles {
        if std::fs::read_to_string(&file).unwrap().lines().any(|l| l.starts_with("fn main")) {
            programs.push(vec![file]);
        }
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "cho"))
            .collect();
        files.sort();
        let files = with_packages(&root, files);
        if !files.is_empty()
            && files.iter().any(|f| {
                std::fs::read_to_string(f).unwrap().lines().any(|l| l.starts_with("fn main"))
            })
        {
            programs.push(files);
        }
    }
    let mut failures = Vec::new();
    let (mut compared, mut refused_by_rust, mut status_3) = (0, 0, 0);
    for files in &programs {
        let uses_std = files.iter().any(|f| {
            std::fs::read_to_string(f).unwrap().lines().any(|l| l.starts_with("import std"))
        });
        let name = files[0].strip_prefix(&root).unwrap().display().to_string();
        // The two forms for the host, and each form once for a WASI target (`--target`): the imports the row licenses are a function of the labels alone, so one target of each spelling's family is a sample of the table.
        for (json, target) in [
            (false, None),
            (true, None),
            (false, Some("wasm32-wasi")),
            (true, Some("wasm32-wasip1")),
        ] {
            let mut rust = Command::new(BIN);
            rust.arg("authority");
            let mut mine = Command::new(&exe);
            mine.arg("authority");
            if json {
                rust.args(["--output", "json"]);
                mine.args(["--output", "json"]);
            }
            if let Some(triple) = target {
                rust.args(["--target", triple]);
                mine.args(["--target", triple]);
            }
            rust.args(files);
            mine.args(files);
            if uses_std {
                rust.arg("--std");
                mine.arg("--std-dir").arg(&std_dir);
            }
            let want = rust.output().expect("cancho runs");
            if !want.status.success() {
                refused_by_rust += 1;
                continue;
            }
            let got = mine.output().expect("cli runs");
            compared += 1;
            if got.status.code() == Some(3) {
                status_3 += 1;
                if !STATUS_3.iter().any(|n| name.ends_with(n)) {
                    failures.push(format!("{name}: status 3, which is not expected of it"));
                }
                continue;
            }
            let want = without_folded(&String::from_utf8_lossy(&want.stdout), json);
            let got_text = String::from_utf8_lossy(&got.stdout).into_owned();
            if !got.status.success() || got_text != want {
                failures.push(format!(
                    "{name} ({}{}): status {:?}\n--- rust\n{want}--- cho\n{got_text}",
                    if json { "data" } else { "prose" },
                    target.map(|t| format!(", {t}")).unwrap_or_default(),
                    got.status.code()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {compared} reports differ ({status_3} answered status 3, {refused_by_rust} were refused by the Rust command):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(compared >= 300, "only {compared} reports were compared");
}

/// The head of a probe: `main` takes the world apart and gives back what the probe does not use.
fn probe(keep: &str, body: &str) -> String {
    let mut text = String::from(
        "edition 10;\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec, tty } = split(world);\n",
    );
    for name in ["io", "ffi", "fs", "heap", "args", "net", "clock", "signals", "exec", "tty"] {
        if !keep.split(',').any(|k| k == name) {
            text.push_str(&format!("    release({name});\n"));
        }
    }
    text.push_str("    var code = 0;\n");
    text.push_str(body);
    text.push_str("    return code;\n}\n");
    text
}

/// A probe that calls `call` on `/tmp` narrowed from the filesystem, and the shape of its answer.
fn on_tmp(call: &str, answer: &str) -> String {
    let (ok, failed) = match answer {
        "Done" => ("Done::Ok(d) => {\n                code = 0;\n            }", "Done::Failed(e)"),
        "Opened" => (
            "Opened::Ok(f) => {\n                file_close(f);\n            }",
            "Opened::Failed(e)",
        ),
        _ => (
            "DirOpened::Ok(d) => {\n                dir_close(d);\n            }",
            "DirOpened::Failed(e)",
        ),
    };
    probe(
        "fs",
        &format!(
            "    let tmp = narrow(fs, \"/tmp\");\n    borrow tmp as &t in {{\n        match {call} {{\n            {ok}\n            {failed} => {{\n                code = 1;\n            }}\n        }}\n    }}\n    release(tmp);\n"
        ),
    )
}

/// The probes of the families of builtins whose labels carry an argument, each a program of one call: the paths (`fs_read`, `fs_write`: every way to open, remove and rename), a listener and a dial
/// (`net_in`, `net_out`), and a program started (`exec`). The programs of the repository do not call some of them, and a mapping nothing compares is a mapping nothing keeps.
#[test]
fn authority_in_cancho_reports_each_family_of_labels() {
    let exe = build("cli-authority-families", &with_front_end("cli.cho"));
    let work = scratch("selfhost-authority-families");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let mut probes: Vec<(String, String)> = vec![
        ("fs_remove".into(), on_tmp("fs_remove(t, \"/tmp/cancho-probe\")", "Done")),
        (
            "fs_rename".into(),
            on_tmp("fs_rename(t, \"/tmp/cancho-probe\", \"/tmp/cancho-probe2\")", "Done"),
        ),
        ("open_dir".into(), on_tmp("open_dir(t, \"/tmp\")", "DirOpened")),
    ];
    for open in ["open_read", "open_write", "open_append", "open_new", "open_rw"] {
        probes.push((open.into(), on_tmp(&format!("{open}(t, \"/tmp/cancho-probe\")"), "Opened")));
    }
    probes.push((
        "tcp_listen_on".into(),
        probe(
            "net",
            "    let local = narrow(net, \"127.0.0.1\");\n    borrow local as &n in {\n        match tcp_listen_on(n, \"127.0.0.1\", 0, 8, 0) {\n            Listening::Ok(l) => {\n                listener_close(l);\n            }\n            Listening::Failed(e) => {\n                code = 1;\n            }\n        }\n    }\n    release(local);\n",
        ),
    ));
    probes.push((
        "tcp_connect".into(),
        probe(
            "net",
            "    let local = narrow(net, \"127.0.0.1\");\n    borrow local as &n in {\n        match tcp_connect(n, \"127.0.0.1\", 1) {\n            Dialed::Ok(c) => {\n                conn_close(c);\n            }\n            Dialed::Failed(e) => {\n                code = 1;\n            }\n        }\n    }\n    release(local);\n",
        ),
    ));
    probes.push((
        "udp_bind".into(),
        probe(
            "net",
            "    borrow net as &nb in {\n        match udp_bind(nb, 0, 0) {\n            UdpOpened::Ok(ua) => {\n                code = udp_close(ua);\n            }\n            UdpOpened::Failed(reason) => {\n                code = 2;\n            }\n        }\n    }\n    release(net);\n",
        ),
    ));
    probes.push((
        "exec_spawn".into(),
        probe(
            "exec",
            "    let program = narrow(exec, \"/usr/bin/true\");\n    borrow program as &x in {\n        match exec_spawn(x, \"/usr/bin/true\", \"\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n            Spawned::Ok(child) => {\n                match child_wait(child) {\n                    Exited::Code(n) => {\n                        code = n;\n                    }\n                    Exited::Signaled(s) => {\n                        code = 100;\n                    }\n                    Exited::Failed(e) => {\n                        code = 101;\n                    }\n                }\n            }\n            Spawned::Failed(e) => {\n                code = 1;\n            }\n        }\n    }\n    release(program);\n",
        ),
    ));
    // Generic functions instantiated at a tuple and at a `Box`: the names of the instances in the list of the pure functions (`second$(int__int)$int`, `pass$Box_int_`, `pass$Box__int__`).
    let generic = "fn second[A: val, B: val](a: A, b: B) -> [] B {\n    return b;\n}\n\nfn pass[T](x: T) -> [] T {\n    return x;\n}\n\nfn size[&r](b: &r Box[[int]]) -> [] int {\n    return len(contents(b));\n}\n\n";
    for (name, keep, body) in [
        ("generic_tuple", "heap", "    code = second((1, 2), 5);\n    release(heap);\n"),
        (
            "generic_box",
            "heap",
            "    var pool = heap;\n    borrow mut pool as &!h in {\n        let b = pass(box(h, 7));\n        code = unbox(h, b);\n    }\n    release(pool);\n",
        ),
        (
            "generic_run",
            "heap",
            "    var pool = heap;\n    borrow mut pool as &!h in {\n        let b = pass(box_slice(h, 4, 0));\n        borrow b as &r in {\n            code = size(r);\n        }\n        unbox_slice(h, b);\n    }\n    release(pool);\n",
        ),
    ] {
        let text = probe(keep, body);
        let at = text.find("fn main").unwrap();
        probes.push((name.into(), format!("{}{generic}{}", &text[..at], &text[at..])));
    }
    let mut failures = Vec::new();
    for (name, source) in &probes {
        let file = work.join(format!("{name}.cho"));
        std::fs::write(&file, source).unwrap();
        for (json, target) in [
            (false, None),
            (true, None),
            (false, Some("wasm32-wasip1")),
            (true, Some("wasm32-wasip2")),
            (false, Some("wasm32-unknown-wasi")),
            (true, Some("wasm32-wasip1-threads")),
        ] {
            let mut rust = Command::new(BIN);
            rust.arg("authority");
            let mut mine = Command::new(&exe);
            mine.arg("authority");
            if json {
                rust.args(["--output", "json"]);
                mine.args(["--output", "json"]);
            }
            if let Some(triple) = target {
                rust.args(["--target", triple]);
                mine.args(["--target", triple]);
            }
            let want = rust.arg(&file).output().expect("cancho runs");
            let got = mine.arg(&file).output().expect("cli runs");
            let (want_text, got_text) = (
                without_folded(&String::from_utf8_lossy(&want.stdout), json),
                String::from_utf8_lossy(&got.stdout).into_owned(),
            );
            if !want.status.success() || !got.status.success() || want_text != got_text {
                failures.push(format!(
                    "{name} ({}{}): rust {:?}, cho {:?}\n--- rust\n{want_text}{}\n--- cho\n{got_text}",
                    if json { "data" } else { "prose" },
                    target.map(|t| format!(", {t}")).unwrap_or_default(),
                    want.status.code(),
                    got.status.code(),
                    String::from_utf8_lossy(&want.stderr)
                ));
            }
        }
    }
    // A target that is not WASI is a usage error of the command line in cancho (the Rust command reports the host's report unchanged), and so is a triple that is none.
    let hello = work.join("generic_tuple.cho");
    for triple in ["x86_64-unknown-linux-gnu", "no such triple"] {
        let got = Command::new(&exe)
            .args(["authority", "--target", triple])
            .arg(&hello)
            .output()
            .expect("cli runs");
        if got.status.code() != Some(2) {
            failures.push(format!("--target {triple}: status {:?}, not 2", got.status.code()));
        }
    }
    assert!(failures.is_empty(), "{} probes differ:\n{}", failures.len(), failures.join("\n"));
    let _ = std::fs::remove_dir_all(&work);
}
