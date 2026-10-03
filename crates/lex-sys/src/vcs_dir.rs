//! `lex-sys vcs publish --dir <dir> --store <root>`: publish a library of
//! several files (`docs/package-system.md` §7.4).
//!
//! A store holds exactly one file (§4.1), so a library of four modules is
//! four stores, each requiring the ones it imports. Publishing them one at a
//! time means typing, for each, a `--requires` that names a lock of the one
//! before; this derives all of it. Each file's module name and imports come
//! from its parse, the files are published in dependency order, and a file's
//! requirement on another file of the directory is a lock of everything that
//! file's store holds.
//!
//! Stores are **regenerated**, not appended to: publishing a changed
//! declaration into a store that has it is refused (`vcs-publish.md` §5), which
//! would make a directory unpublishable after its first edit. Nothing a
//! consumer holds is lost -- its lock pins a commit of the repository, and the
//! store at that commit is what it fetches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lex_sys_vcs::{Lock, LockEntry, Manifest};

use crate::vcs_cli::publish_one;
use crate::{Failure, environment, refused};

/// One file of the directory: where it is, what it is called, and which
/// other modules of the directory it imports.
struct Unit {
    file: PathBuf,
    module: String,
    imports: Vec<String>,
}

fn dotted(ast: &lex_sys_syntax::Ast, path: &[lex_sys_syntax::ast::Symbol]) -> String {
    path.iter().map(|s| ast.name_of(*s)).collect::<Vec<_>>().join(".")
}

/// `None` for a file with no `module` declaration: a program, not a library module.
fn read_unit(file: &Path) -> Result<Option<(Unit, bool)>, Failure> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| environment(format!("cannot read `{}`: {e}", file.display())))?;
    let mut map = lex_sys_syntax::SourceMap::new();
    let mut ast = lex_sys_syntax::Ast::new();
    let base = map.add(file.to_string_lossy().into_owned(), text.clone());
    lex_sys_syntax::parse_into(&mut ast, &text, base).map_err(|d| refused(d.render_in(&map)))?;
    // Module 0 is the root every parse starts with; a file's own `module` is the one after it.
    let declared: Vec<&lex_sys_syntax::ast::Module> =
        ast.modules.iter().filter(|m| !m.is_root()).collect();
    let module = match declared.as_slice() {
        [] => return Ok(None),
        [module] => module,
        _ => {
            return Err(refused(format!(
                "`{}` declares more than one `module`; a directory is published one module per file",
                file.display()
            )));
        }
    };
    let name = dotted(&ast, &module.path);
    let mut imports = Vec::new();
    let mut uses_std = false;
    for import in &module.imports {
        let path = dotted(&ast, &import.path);
        if path == "std" || path.starts_with("std.") {
            uses_std = true;
        } else {
            imports.push(path);
        }
    }
    Ok(Some((Unit { file: file.to_path_buf(), module: name, imports }, uses_std)))
}

/// Every declaration a store holds, as a lock: what a file that imports the
/// store's module requires.
pub(crate) fn lock_of_all(store: &Path) -> Result<Lock, Failure> {
    let manifest = Manifest::load(store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;
    let mut lock = Lock::default();
    for (sig_id, entry) in manifest.entries() {
        lock.insert(
            entry.name.clone(),
            LockEntry {
                sig_id: sig_id.clone(),
                stage_id: entry.stage_id.clone(),
                source_hash: entry.source_hash.clone(),
            },
        );
    }
    Ok(lock)
}

pub fn publish_dir(dir: &Path, root: &Path, with_std: bool) -> Result<ExitCode, Failure> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| environment(format!("cannot read directory `{}`: {e}", dir.display())))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "ls"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(refused(format!("no `.ls` files in `{}`", dir.display())));
    }

    let mut units: BTreeMap<String, Unit> = BTreeMap::new();
    for file in &files {
        let Some((unit, _uses_std)) = read_unit(file)? else {
            println!(
                "skipped {} (no `module` declaration: a program, not a library module)",
                file.display()
            );
            continue;
        };
        if let Some(other) = units.get(&unit.module) {
            return Err(refused(format!(
                "module `{}` is declared by both `{}` and `{}`",
                unit.module,
                other.file.display(),
                unit.file.display()
            )));
        }
        units.insert(unit.module.clone(), unit);
    }
    if units.is_empty() {
        return Err(refused(format!("no file of `{}` declares a `module`", dir.display())));
    }
    for unit in units.values() {
        for import in &unit.imports {
            if !units.contains_key(import) {
                return Err(refused(format!(
                    "`{}` imports `{import}`, which is neither `std` nor a module of `{}`; \
                     a dependency in another repository is published as its own store and \
                     required with `vcs publish --requires`",
                    unit.file.display(),
                    dir.display()
                )));
            }
        }
    }

    // Dependency order: a module after everything it imports; a cycle is refused.
    let mut order: Vec<&str> = Vec::new();
    let mut state: BTreeMap<&str, bool> = BTreeMap::new(); // false: on the path, true: done
    fn visit<'a>(
        module: &'a str,
        units: &'a BTreeMap<String, Unit>,
        state: &mut BTreeMap<&'a str, bool>,
        path: &mut Vec<&'a str>,
        order: &mut Vec<&'a str>,
    ) -> Result<(), Failure> {
        match state.get(module) {
            Some(true) => return Ok(()),
            Some(false) => {
                path.push(module);
                return Err(refused(format!("modules import each other: {}", path.join(" -> "))));
            }
            None => {}
        }
        state.insert(module, false);
        path.push(module);
        for import in &units[module].imports {
            visit(import, units, state, path, order)?;
        }
        path.pop();
        state.insert(module, true);
        order.push(module);
        Ok(())
    }
    for module in units.keys() {
        visit(module, &units, &mut state, &mut Vec::new(), &mut order)?;
    }

    for module in order {
        let unit = &units[module];
        let store = root.join(module);
        if store.exists() {
            std::fs::remove_dir_all(&store).map_err(|e| {
                environment(format!("cannot clear the store `{}`: {e}", store.display()))
            })?;
        }
        let mut deps = Vec::new();
        for import in &unit.imports {
            let dep_store = root.join(import);
            deps.push((lock_of_all(&dep_store)?, dep_store));
        }
        println!("== {module} ({})", store.display());
        publish_one(&store, &unit.file, deps, with_std)?;
    }
    Ok(ExitCode::SUCCESS)
}
