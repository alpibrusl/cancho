//! `docs/next-phase.md` §4: no function body is duplicated, verbatim, in a
//! different file -- checked the same way `identity.rs` checks printing,
//! not by rerunning the one-off script that found `print_nat`/`write_all`
//! (#160) by hand.
//!
//! Each file is parsed and identified on its own, exactly as
//! `identity.rs::printing_preserves_every_identity_and_is_idempotent` does:
//! no `--std`, no cross-file import resolution. That is not a limitation
//! here -- it is what makes the check correct. `docs/canonical-ast.md` §4.2
//! and `lex-sys-id`'s own `qualified_name` encode a call to a name declared
//! in the *same* file as the hash of that declaration, and a call to
//! anything else (a builtin, an import) as the literal name. Two files
//! whose functions call the same builtins the same way still collide, which
//! is what catches a real copy; two files whose functions happen to read
//! alike but resolve an unqualified name against two *different* local
//! declarations (`examples/rational.ls`'s own single-parameter `Result[T]`
//! against `std/result.ls`'s two-parameter `Result[T, E]`, both with an
//! `is_ok` whose match arms print identically) hash apart, because the
//! thing each one's `Result::Ok` actually names is not the same thing. A
//! text diff cannot tell those two cases apart; a content hash always can,
//! which is the reason this is built on `lex-sys-id` and not on
//! `lex-sys print`'s output.

use super::*;
use std::collections::HashMap;
use std::path::Path;

use lex_sys_syntax::ast::{Ast, Block, Item, Stmt};

/// A `(file, function)` pair known to duplicate another file's, and kept
/// anyway. Every entry here must still name a real cross-file match, or
/// the assertion at the end of the test below fails and says which entry
/// is stale.
const ALLOWED: &[(&str, &str)] = &[
    // `docs/next-phase.md` §3.1: the one example three unrelated tests
    // (`corpus::run_builds_and_executes_in_one_step`,
    // `corpus::emitting_a_bare_object_file_works`,
    // `refusals::a_clean_program_answers_an_empty_list`) build without
    // `--std`, on purpose -- `import std.io;` would refuse to resolve.
    ("examples/hello.ls", "write_all"),
    // `docs/next-phase.md` §3.1: `docs/modules.md`'s own worked example
    // for "moving a function costs no hash". Migrating this file onto
    // `std.io` would delete the functions its own header comment uses
    // as that example.
    ("examples/modular/text.ls", "write_all"),
    ("examples/modular/text.ls", "print_nat"),
    ("std/io.ls", "print_nat"),
    // `docs/next-phase.md` §3/§6: `nat_of`/`port_of`/`port_of_listen`,
    // four files, already under different names -- a shape question
    // (three names for one parse), not a mechanical fix. Open, not
    // acted on yet.
    ("examples/serve/serve.ls", "port_of"),
    ("examples/collect/collect.ls", "nat_of"),
    ("examples/agent_supervisor/agent_supervisor.ls", "nat_of"),
    ("examples/results_stub/results_stub.ls", "nat_of"),
    // Found building this check: `examples/sort/sort.ls` and
    // `examples/seek/seek.ls` share `read_stdin` verbatim, and
    // `docs/file-handles.md` §1's fix to `read_file` (measured: six
    // reads and 2.75x the file's bytes, on the doubling-retry version)
    // was applied to both files identically -- `sort.ls`'s own comment
    // narrates the difference that already collapsed to zero.
    ("examples/sort/sort.ls", "read_stdin"),
    ("examples/sort/sort.ls", "read_file"),
    ("examples/seek/seek.ls", "read_stdin"),
    ("examples/seek/seek.ls", "read_file"),
    // Found building this check: `examples/buffer/buffer.ls` predates
    // `std/buffer.ls` and was never migrated onto it -- same `res`
    // struct, same four operations, renamed rather than removed
    // (`new_buffer`/`empty` and its own `push`/`push_byte` fall below
    // this check's statement floor and never surface as a cluster, so
    // they need no entry here).
    ("examples/buffer/buffer.ls", "release_buffer"),
    ("examples/buffer/buffer.ls", "reserve"),
    ("examples/buffer/buffer.ls", "push_byte"),
    ("std/buffer.ls", "drop"),
    ("std/buffer.ls", "reserve"),
    ("std/buffer.ls", "push"),
    // Found building this check: alpha-equivalent under `lex-sys-id`
    // (`docs/canonical-ast.md`: "bodies hash up to alpha-equivalence")
    // but textually different enough -- the same byte-blit loop under
    // two names, `append` in `examples/lines.ls` and `put` in
    // `packages/net-sockets/sockets.ls` -- that the raw-text hunt
    // behind #160 never found. `examples/rational.ls`'s own `abs` and
    // `examples/tree.ls`'s own `larger` were the same shape and are
    // migrated onto `std.math` now; `append`/`put` is not yet, for the
    // same reason `nat_of`/`port_of` above is not.
    ("examples/lines.ls", "append"),
    ("packages/net-sockets/sockets.ls", "put"),
];

/// Below this many statements, counted recursively through nested blocks,
/// a match is not worth reporting -- `docs/next-phase.md` §4's own floor,
/// to exclude one-line coincidences like two `fn main`s or two
/// one-statement constructors.
const FLOOR: usize = 2;

fn statement_count(ast: &Ast, block: &Block) -> usize {
    let mut count = 0;
    for stmt_id in &block.stmts {
        count += 1;
        match ast.stmt(*stmt_id) {
            Stmt::Borrow { body, .. } | Stmt::Region { body, .. } => {
                count += statement_count(ast, body);
            }
            Stmt::If { then_block, else_block, .. } => {
                count += statement_count(ast, then_block);
                if let Some(else_block) = else_block {
                    count += statement_count(ast, else_block);
                }
            }
            Stmt::While { body, .. } => {
                count += statement_count(ast, body);
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    count += statement_count(ast, &arm.body);
                }
            }
            _ => {}
        }
    }
    count
}

/// Every `.ls` file under `dir`, recursively, skipping
/// `.lex-sys-vcs` -- content-addressed package-store blobs, copies by
/// design and not a meaningful duplication target.
fn walk_ls_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries {
        let path = entry.expect("a readable directory entry").path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(".lex-sys-vcs") {
                continue;
            }
            walk_ls_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "ls") {
            out.push(path);
        }
    }
}

/// `docs/next-phase.md` §4's proposed check, over `examples/`, `std/`
/// and `packages/`. `benches/`'s `*_checked.ls`/`*_wrapping.ls` pairs are
/// deliberately excluded: `benchmarks.rs::every_benchmark_pair_agrees`
/// already asserts each pair agrees, which is the point of them being
/// near-duplicates, and this check would otherwise need to allowlist
/// every one of those pairs for no added safety.
#[test]
fn no_function_body_is_duplicated_across_files() {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["examples", "std", "packages"] {
        walk_ls_files(&root.join(dir), &mut files);
    }
    files.sort();
    assert!(files.len() > 50, "the walk should have found the whole suite, found {}", files.len());

    // `BodyId` -> every `(file, function name)` whose body hashes there.
    let mut clusters: HashMap<lex_sys_id::Hash, Vec<(String, String)>> = HashMap::new();

    for path in &files {
        let source = std::fs::read_to_string(path).expect("a readable fixture");
        let Ok(ast) = lex_sys_syntax::parse(&source) else { continue };
        let ids = lex_sys_id::identify(&ast);
        let relative = path
            .strip_prefix(&root)
            .expect("every walked file is under the repo root")
            .to_string_lossy()
            .replace('\\', "/");

        // `identify` gives every `Item::Fn`, `Item::Extern` and
        // `Item::Static` a `FunctionId`, in `ast.items` order -- walked
        // here the same way, so the two lists advance together without a
        // name lookup. An `extern` has no body (`ExternDecl`'s own doc:
        // "the implementation is somebody else's"), so its slot is
        // consumed to keep the two lists aligned but never clustered: two
        // files declaring the same foreign symbol the same way is not the
        // duplication this check is about.
        let mut fn_index = 0;
        for item in &ast.items {
            let (body, name) = match item {
                Item::Fn(decl) => (Some(&decl.body), ast.name_of(decl.name)),
                Item::Static(decl) => (Some(&decl.body), ast.name_of(decl.name)),
                Item::Extern(decl) => (None, ast.name_of(decl.name)),
                _ => continue,
            };
            let function_id = &ids.functions[fn_index];
            debug_assert_eq!(name, function_id.name, "{relative}: function order drifted");
            fn_index += 1;

            let Some(body) = body else { continue };
            if statement_count(&ast, body) < FLOOR {
                continue;
            }
            clusters
                .entry(function_id.body)
                .or_default()
                .push((relative.clone(), function_id.name.clone()));
        }
        assert_eq!(
            fn_index,
            ids.functions.len(),
            "{relative}: `identify` and the item list disagree on how many functions there are"
        );
    }

    let is_allowed = |file: &str, name: &str| ALLOWED.contains(&(file, name));
    let mut allowed_entries_seen = vec![false; ALLOWED.len()];
    let mut unexpected = Vec::new();

    for entries in clusters.values() {
        let mut files_involved: Vec<&str> = entries.iter().map(|(f, _)| f.as_str()).collect();
        files_involved.sort();
        files_involved.dedup();
        if files_involved.len() < 2 {
            continue;
        }
        for (file, name) in entries {
            if is_allowed(file, name) {
                if let Some(index) =
                    ALLOWED.iter().position(|(f, n)| *f == file.as_str() && *n == name.as_str())
                {
                    allowed_entries_seen[index] = true;
                }
            } else {
                unexpected.push(format!(
                    "{file}: `{name}` duplicates a function body also found in {:?}",
                    entries
                        .iter()
                        .filter(|(f, n)| f != file || n != name)
                        .map(|(f, n)| format!("{f}:{n}"))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "found {} function body duplicated across files, not in the `ALLOWED` list \
         (`docs/next-phase.md` §4) -- either migrate the copy onto the shared \
         declaration, or add it to `ALLOWED` with a reason:\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );

    let stale: Vec<&(&str, &str)> = ALLOWED
        .iter()
        .zip(allowed_entries_seen.iter())
        .filter(|(_, seen)| !**seen)
        .map(|(entry, _)| entry)
        .collect();
    assert!(
        stale.is_empty(),
        "these `ALLOWED` entries no longer match a real cross-file duplicate -- \
         remove them:\n{stale:#?}"
    );
}
