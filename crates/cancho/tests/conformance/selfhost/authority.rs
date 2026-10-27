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
        for json in [false, true] {
            let mut rust = Command::new(BIN);
            rust.arg("authority");
            let mut mine = Command::new(&exe);
            mine.arg("authority");
            if json {
                rust.args(["--output", "json"]);
                mine.args(["--output", "json"]);
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
                    "{name} ({}): status {:?}\n--- rust\n{want}--- cho\n{got_text}",
                    if json { "data" } else { "prose" },
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
