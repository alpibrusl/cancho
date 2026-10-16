//! What the Rust crates have that the compiler in cancho (`examples/selfhost`) does not know about yet, so that
//! a rule or a builtin added in Rust cannot go unnoticed: it fails here until the compiler in cancho has it, or the
//! name is listed (`unported_rules.txt`, `unported_builtins.txt`) with the reason, and a listed name that has been
//! ported fails until the line is taken out, so the lists only shrink.
//!
//! The generated tables (`runtime.cho`, `tables.cho`, `builtins.cho`) are held equal to the Rust ones by
//! `cancho-ir`'s `selfhost_tables`; the corpus of refused programs by `refusals_agree_with_the_rust_front_end`; the
//! lexer and the parser by the differential tests of this module. This is what they cannot see: a name nobody wrote
//! code for.

use std::collections::BTreeSet;
use std::path::Path;

fn selfhost_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/selfhost")
}

/// The names a list holds: a line is a name, then perhaps a `#` and why.
fn listed(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every source of the compiler in cancho but the files that only hold the Rust crates' tables, as one text.
fn sources(skip: &[&str]) -> String {
    let mut all = String::new();
    let mut files: Vec<_> = std::fs::read_dir(selfhost_dir())
        .expect("examples/selfhost")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    files.sort();
    for file in files {
        let name = file.file_name().and_then(|n| n.to_str()).expect("a name");
        if skip.contains(&name) {
            continue;
        }
        all.push_str(&std::fs::read_to_string(&file).expect("a source"));
        all.push('\n');
    }
    all
}

fn check(kind: &str, known: &BTreeSet<String>, wanted: &BTreeSet<String>, list: &str, file: &str) {
    let missing: Vec<&String> =
        wanted.iter().filter(|n| !known.contains(*n) && !listed(list).contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "{} of the Rust crates' {kind}s are not known to the compiler in cancho (examples/selfhost) and are not listed in \
         tests/conformance/selfhost/{file}: {missing:?}. Port them, or list them there with the issue that will.",
        missing.len()
    );
    let stale: Vec<String> =
        listed(list).into_iter().filter(|n| known.contains(n) || !wanted.contains(n)).collect();
    assert!(
        stale.is_empty(),
        "{file} lists {kind}s that are ported now, or that Rust no longer has; take them out: {stale:?}"
    );
}

#[test]
fn every_rule_of_the_rust_front_end_is_known_to_the_compiler_in_cancho() {
    let text = sources(&[]);
    let known: BTreeSet<String> = cancho_syntax::Rule::ALL
        .iter()
        .map(|r| r.tag().to_owned())
        .filter(|tag| text.contains(&format!("\"{tag}\"")))
        .collect();
    let wanted: BTreeSet<String> =
        cancho_syntax::Rule::ALL.iter().map(|r| r.tag().to_owned()).collect();
    check("rule", &known, &wanted, include_str!("unported_rules.txt"), "unported_rules.txt");
    // And the other way: a name in rules.cho that Rust does not have is a stale one.
    let rules = std::fs::read_to_string(selfhost_dir().join("rules.cho")).expect("rules.cho");
    for line in rules.lines().filter(|l| l.trim_start().starts_with("return \"")) {
        let name = line.trim().trim_start_matches("return \"").trim_end_matches("\";");
        assert!(
            name == "SKIP" || wanted.contains(name),
            "rules.cho names `{name}`, which the Rust crates do not have"
        );
    }
}

#[test]
fn every_builtin_of_the_rust_front_end_is_named_by_the_compiler_in_cancho() {
    // The tables are generated from the Rust ones, so naming a builtin there says nothing; the code is elsewhere.
    let text = sources(&["tables.cho", "builtins.cho", "runtime.cho"]);
    let wanted: BTreeSet<String> =
        cancho_ir::Builtin::ALL.iter().map(|b| b.name().to_owned()).collect();
    let known: BTreeSet<String> =
        wanted.iter().filter(|name| text.contains(&format!("\"{name}\""))).cloned().collect();
    check(
        "builtin",
        &known,
        &wanted,
        include_str!("unported_builtins.txt"),
        "unported_builtins.txt",
    );
}

/// The modules the compiler in cancho imports, from its root, in the order they must be compiled in.
fn closure() -> Vec<String> {
    fn walk(dir: &Path, name: &str, seen: &mut Vec<String>) {
        let text = std::fs::read_to_string(dir.join(format!("{name}.cho"))).expect("a module");
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("import selfhost.") else { continue };
            let module = rest.split([' ', ';']).next().unwrap_or("");
            if !module.is_empty()
                && !seen.contains(&module.to_owned())
                && dir.join(format!("{module}.cho")).exists()
            {
                walk(dir, module, seen);
                seen.push(module.to_owned());
            }
        }
    }
    let mut seen = Vec::new();
    walk(&selfhost_dir(), "compiler", &mut seen);
    seen
}

/// A module the compiler imports but a test's own list of the compiler's files leaves out makes it refuse its own
/// source with `unknown-name`: this says which list, before a long test does.
#[test]
fn every_module_of_the_compiler_is_in_the_lists_the_tests_keep_of_them() {
    let lists = [
        ("selfhost.rs `with_front_end`", include_str!("../selfhost.rs")),
        ("selfhost/bootstrap.rs `own_source`", include_str!("bootstrap.rs")),
    ];
    for module in closure() {
        for (which, text) in lists {
            let named = text.contains(&format!("\"{module}.cho\""))
                || text.contains(&format!("\"{module}\""));
            assert!(
                named,
                "the module `{module}` of examples/selfhost is not in {which}; add it, before the modules that import it"
            );
        }
    }
}
