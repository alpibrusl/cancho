//! `lex-sys vcs publish` / `lex-sys vcs log`.
//!
//! `docs/vcs-publish.md` §6: the smallest real slice that turns
//! `crates/lex-sys-vcs` — a library nothing outside its own tests had ever
//! called — into something a real `.ls` file can go through. A first
//! publish against an empty store needs no diffing (§3): every declaration
//! is new by definition, so every one becomes an `AddFunction`, reusing
//! `lex-sys-id`'s own per-declaration hashing (`lex-sys ids`'s own source)
//! and the declared effect row `lex-sys authority` already reports as
//! strings. §4: a manifest (`SigId -> StageId`) is what lets a *second*
//! publish tell "already logged, unchanged" apart from "already logged,
//! and this document does not yet know how to publish a change to it" —
//! refused rather than silently logged as a second, disconnected
//! `AddFunction`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lex_sys_id::identify;
use lex_sys_ir::{Effects, Func};
use lex_sys_vcs::{
    Blobs, Lock, LockEntry, Manifest, ManifestEntry, OpLog, Operation, OperationKind,
    OperationRecord, Requirement, SigId, load_requirements, save_requirements,
};

use crate::{Failure, environment, refused, usage};

/// Parse several already-in-memory named texts as one program, the same
/// way [`crate::parse_program`] merges real files. `vcs publish`'s own
/// dependency gate and [`verify_selected`]'s closure check both hold
/// their dependency source as trusted strings, never as paths on disk,
/// so they need this rather than writing a temp file just to hand
/// `parse_program` something it can re-read.
fn parse_texts(
    named: &[(String, String)],
) -> Result<(lex_sys_syntax::Ast, lex_sys_syntax::SourceMap), String> {
    let mut map = lex_sys_syntax::SourceMap::new();
    let mut ast = lex_sys_syntax::Ast::new();
    for (name, text) in named {
        let base = map.add(name.clone(), text.clone());
        if let Err(d) = lex_sys_syntax::parse_into(&mut ast, text, base) {
            return Err(d.render_in(&map));
        }
    }
    Ok((ast, map))
}

/// `docs/editions.md`: only edition 1 exists today, so this is not a
/// simplification pending a real one — it is the one plateau
/// `docs/vcs.md` §7 already measured. Revisit when a second edition does.
const EDITION: u32 = 1;

const DEFAULT_STORE: &str = ".lex-sys-vcs";

pub fn cmd_vcs(args: &[String]) -> Result<ExitCode, Failure> {
    match args.first().map(String::as_str) {
        Some("publish") => cmd_publish(&args[1..]),
        Some("log") => cmd_log(&args[1..]),
        Some("resolve") => cmd_resolve(&args[1..]),
        Some("lock") => cmd_lock(&args[1..]),
        Some("fetch") => cmd_fetch(&args[1..]),
        Some(other) => Err(usage(format!("unknown `vcs` subcommand `{other}`"))),
        None => {
            Err(usage("`vcs` needs a subcommand: `publish`, `log`, `resolve`, `lock` or `fetch`"))
        }
    }
}

struct VcsInvocation {
    store: PathBuf,
    inputs: Vec<PathBuf>,
    /// `--requires <lock-file>:<dep-store>`, raw and unsplit -- only
    /// `cmd_publish` reads this; every other caller of
    /// [`parse_vcs_args`] just carries an empty one.
    requires: Vec<String>,
}

fn parse_vcs_args(args: &[String]) -> Result<VcsInvocation, Failure> {
    let mut store = PathBuf::from(DEFAULT_STORE);
    let mut inputs = Vec::new();
    let mut requires = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--store" => {
                let value = it.next().ok_or_else(|| usage("`--store` needs a path"))?;
                store = PathBuf::from(value);
            }
            "--requires" => {
                let value = it
                    .next()
                    .ok_or_else(|| usage("`--requires` needs `<lock-file>:<dep-store>`"))?;
                requires.push(value.clone());
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }
    Ok(VcsInvocation { store, inputs, requires })
}

/// The declared row, formatted exactly the way `lex-sys authority` already
/// prints one (`main.rs`'s own `print_authority`) — a consumer that reads
/// one already reads the other.
fn effect_strings(effects: &Effects) -> BTreeSet<String> {
    effects
        .labels()
        .iter()
        .map(|label| match &label.argument {
            Some(value) => format!("{}(\"{}\")", label.name, value),
            None => label.name.clone(),
        })
        .collect()
}

fn find_func<'a>(funcs: &'a [Func], name: &str) -> Option<&'a Func> {
    funcs.iter().find(|f| f.name == name)
}

/// `--requires <lock-file>:<dep-store>`, split on the first `:` -- a
/// path never contains one on the two targets this project builds for
/// (`docs/reach.md`'s own platform list), so there is no ambiguity to
/// guard against here the way a Windows drive letter would force.
fn split_requires(pair: &str) -> Result<(&str, &str), Failure> {
    pair.split_once(':').ok_or_else(|| {
        usage(format!("`--requires {pair}` needs `<lock-file>:<dep-store>`, separated by `:`"))
    })
}

fn cmd_publish(args: &[String]) -> Result<ExitCode, Failure> {
    let VcsInvocation { store, inputs, requires } = parse_vcs_args(args)?;
    if inputs.is_empty() {
        return Err(usage("no input file given"));
    }
    if inputs.len() > 1 {
        // `many-files.md`'s `ModuleRef` attribution -- which input file a
        // declaration came from -- is real, separate plumbing this slice
        // does not build (`docs/vcs-publish.md` §3's own pseudocode names
        // it and stops there). Refusing is safer than guessing a wrong
        // `in_file` silently.
        return Err(usage(
            "`vcs publish` takes exactly one file today; \
             multi-file attribution (many-files.md) is not built yet",
        ));
    }

    let source = std::fs::read_to_string(&inputs[0])
        .map_err(|e| environment(format!("cannot read `{}`: {e}", inputs[0].display())))?;
    let file_name = inputs[0].to_string_lossy().into_owned();

    // Not purely structural after all -- found here, not assumed:
    // `lex-sys-id`'s own `qualified_name` contributes a *resolved*
    // callee's signature hash into the caller's body hash
    // (`tag::FREE`) when the callee can be resolved, and only falls
    // back to the bare written name (`tag::NONE`) when it cannot. So a
    // function that calls into a `--requires` dependency hashes
    // *differently* depending on whether that dependency is in scope
    // when `identify()` runs -- computing it from this file alone would
    // record a `StageId` that `verify_selected`'s later, dependency-aware
    // recheck could never match, a guaranteed false "body moved"
    // refusal on the very first `vcs resolve`. So identity is computed
    // from the same merged, dependency-resolved AST the soundness gate
    // below uses, and only this file's own names -- known regardless of
    // resolution, from a first pass over the primary file alone -- are
    // kept, so a dependency's own declarations are never republished
    // under this store.
    let (own_ast, _) = parse_texts(&[(file_name.clone(), source.clone())]).map_err(refused)?;
    let own_names: BTreeSet<String> =
        identify(&own_ast).functions.iter().map(|f| f.name.clone()).collect();

    let mut requirements = Vec::with_capacity(requires.len());
    let mut dependency_context: BTreeMap<String, String> = BTreeMap::new();
    let mut visiting = Vec::new();
    let mut pinned = BTreeMap::new();
    for pair in &requires {
        let (lock_path, dep_store) = split_requires(pair)?;
        let lock = Lock::load(Path::new(lock_path))
            .map_err(|e| environment(format!("cannot read lock file at `{lock_path}`: {e}")))?;
        let closure = resolve_closure(&lock, Path::new(dep_store), &mut visiting, &mut pinned)
            .map_err(|problems| refused(problems.join("\n\n")))?;
        dependency_context.extend(closure);
        requirements.push(Requirement { store: dep_store.to_owned(), lock });
    }

    let mut named = vec![(file_name.clone(), source.clone())];
    for (hash, text) in &dependency_context {
        named.push((format!("{hash}.ls"), text.clone()));
    }
    let (merged_ast, merged_map) = parse_texts(&named).map_err(refused)?;
    let program = lex_sys_ir::lower_all(&merged_ast).map_err(|diagnostics| {
        let text: Vec<String> = diagnostics.iter().map(|d| d.render_in(&merged_map)).collect();
        refused(text.join("\n\n"))
    })?;
    let identities = identify(&merged_ast);

    let op_log = OpLog::open(&store)
        .map_err(|e| environment(format!("cannot open store at `{}`: {e}", store.display())))?;
    let blobs = Blobs::open(&store)
        .map_err(|e| environment(format!("cannot open store at `{}`: {e}", store.display())))?;
    let mut manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;

    // Stored once per publish, addressed by its own content — `vcs
    // resolve` (`docs/package-system.md` §4.2) is the reader; a hash alone
    // cannot be re-typechecked, only the source behind it can.
    let source_hash =
        blobs.put(&source).map_err(|e| environment(format!("cannot write to store: {e}")))?;

    let mut published = 0usize;
    let mut unchanged = 0usize;

    for func in &identities.functions {
        // A dependency's own declarations are in `identities` too, now
        // that it is computed from the merged AST -- never republished
        // here; they already live in their own store.
        if !own_names.contains(&func.name) {
            continue;
        }
        let sig_id = func.sig.to_hex();
        let stage_id = func.body.to_hex();

        if let Some(existing) = manifest.get(&sig_id) {
            if existing.stage_id == stage_id {
                unchanged += 1;
                continue;
            }
            // A real change to an already-published declaration is
            // incremental diffing's job (`vcs.md` §4's deferred
            // `compute_diff`/`diff_to_ops` rewrite), not this slice's.
            // Refusing here, located at the declaration, is the honest
            // answer -- logging it as an unrelated second `AddFunction`
            // would silently lose the fact that these two are the same
            // function.
            return Err(refused(format!(
                "`{}` already published at a different body (docs/vcs-publish.md §5: \
                 incremental publish is not built yet)",
                func.name
            )));
        }

        // An ordinary function's effects come from its lowered body; a
        // foreign declaration has no body to lower (`lex-sys-id`'s own
        // `identify()` already gives it the same hash for both its
        // identities, for the same reason), so its effects are read from
        // `program.externs` instead -- the row it declared, not one a
        // lowering pass computed.
        let effects = match find_func(&program.funcs, &func.name) {
            Some(ir_func) => effect_strings(&ir_func.effects),
            None => program
                .externs
                .iter()
                .find(|e| e.name == func.name)
                .map(|e| effect_strings(&e.effects))
                .ok_or_else(|| {
                    refused(format!(
                        "internal: `{}` has an identity but no lowered function or extern",
                        func.name
                    ))
                })?,
        };

        // `check_candidate` re-parses and re-lowers independently of the
        // gate just above -- it needs the same dependency context in
        // scope, for the same reason.
        let candidate_files: Vec<(&str, &str)> =
            named.iter().map(|(name, text)| (name.as_str(), text.as_str())).collect();
        lex_sys_vcs::check_candidate(&candidate_files).map_err(|diagnostics| {
            let text: Vec<String> =
                diagnostics.into_iter().map(|d| format!("{}: {}", d.rule, d.message)).collect();
            refused(text.join("\n\n"))
        })?;

        let op = Operation::new(
            OperationKind::AddFunction {
                sig_id: sig_id.clone(),
                stage_id: stage_id.clone(),
                effects,
                in_file: None,
            },
            EDITION,
            [],
        );
        op_log
            .put(&OperationRecord::new(op))
            .map_err(|e| environment(format!("cannot write to store: {e}")))?;
        manifest.insert(
            sig_id,
            ManifestEntry { name: func.name.clone(), stage_id, source_hash: source_hash.clone() },
        );
        published += 1;
        println!("published {}", func.name);
    }

    manifest
        .save(&store)
        .map_err(|e| environment(format!("cannot write manifest at `{}`: {e}", store.display())))?;

    // Whole-store, last-publish-wins metadata (§4.6's own note: not
    // diffed or versioned per declaration, consistent with every
    // package published here so far having exactly one publish, ever).
    save_requirements(&store, &requirements).map_err(|e| {
        environment(format!("cannot write requirements at `{}`: {e}", store.display()))
    })?;

    if published == 0 {
        println!("nothing new ({unchanged} declaration(s) unchanged)");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_log(args: &[String]) -> Result<ExitCode, Failure> {
    let VcsInvocation { store, inputs, .. } = parse_vcs_args(args)?;
    if !inputs.is_empty() {
        return Err(usage("`vcs log` takes no input files, only `--store`"));
    }

    let manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;

    if manifest.is_empty() {
        println!("nothing published yet at `{}`", store.display());
        return Ok(ExitCode::SUCCESS);
    }

    let mut rows: Vec<(&String, &ManifestEntry)> = manifest.entries().collect();
    rows.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    for (sig_id, entry) in rows {
        println!("{:<24} sig {}  body {}", entry.name, sig_id, entry.stage_id);
    }
    Ok(ExitCode::SUCCESS)
}

/// A lock's pins, read against a store's *current* manifest — never the
/// lock's own stale copy of a `stage_id`/`source_hash`, because a lock
/// only ever chooses *which* declaration a name means (`docs/
/// package-system.md` §4.5), not what is true about it today. Refuses,
/// naming every one, if a locked `sig_id` is no longer published at all.
fn select_locked(
    lock: &Lock,
    manifest: &Manifest,
    store: &Path,
) -> Result<Vec<(SigId, ManifestEntry)>, Vec<String>> {
    let mut selected = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    for (name, lock_entry) in lock.entries() {
        match manifest.get(&lock_entry.sig_id) {
            Some(entry) => selected.push((lock_entry.sig_id.clone(), entry.clone())),
            None => missing.push(format!(
                "{name}: sig_id {} is no longer published at `{}`",
                lock_entry.sig_id,
                store.display()
            )),
        }
    }
    if missing.is_empty() { Ok(selected) } else { Err(missing) }
}

/// `docs/package-system.md` §4.2: never trust a pin without re-checking
/// the source behind it. Groups `selected` by `source_hash` first, so a
/// file several declarations came from is re-parsed and re-typechecked
/// once rather than once per declaration, then re-derives each
/// declaration's identity via `lex-sys-id` and compares it against what
/// the manifest claims. `hash-stability.md` measured that 71% of this
/// repository's own history stops type-checking under today's build;
/// this is that measurement, run against someone else's store instead of
/// assumed away.
///
/// On success, every problem is reported at once rather than the first
/// -- the same discipline `check --output json` already applies. On
/// success, answers the verified source text keyed by `source_hash`,
/// for a caller (`cmd_fetch`) that needs the bytes, not only the verdict.
/// `extra_context` is every distinct source text a transitive dependency
/// closure has already gathered (`resolve_closure`, §4.6) — merged into
/// every group's own `lower_all` call so an `import` in that group's
/// blob resolves, the same way `net.sockets`+`net.connect` already
/// compose at `build` time. Empty for every store with no dependency of
/// its own, which is every check this function made before §4.6 and is
/// unaffected by any of it: an empty `extra_context` merged with one
/// text is that one text, parsed exactly as `lex_sys_syntax::parse`
/// alone already did.
fn verify_selected(
    blobs: &Blobs,
    selected: &[(SigId, ManifestEntry)],
    extra_context: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, Vec<String>> {
    let mut by_source: BTreeMap<String, Vec<(&SigId, &ManifestEntry)>> = BTreeMap::new();
    for (sig_id, entry) in selected {
        by_source.entry(entry.source_hash.clone()).or_default().push((sig_id, entry));
    }

    let mut problems: Vec<String> = Vec::new();
    let mut verified: BTreeMap<String, String> = BTreeMap::new();

    for (source_hash, entries) in &by_source {
        let text = match blobs.get(source_hash) {
            Ok(Some(text)) => text,
            Ok(None) => {
                problems.push(format!(
                    "{source_hash}: no source blob recorded for it ({} declaration(s))",
                    entries.len()
                ));
                continue;
            }
            Err(e) => {
                problems.push(format!("cannot read source blob `{source_hash}`: {e}"));
                continue;
            }
        };

        let mut named = vec![(format!("{source_hash}.ls"), text.clone())];
        for (hash, dep_text) in extra_context {
            named.push((format!("{hash}.ls"), dep_text.clone()));
        }
        let ast = match parse_texts(&named) {
            Ok((ast, _map)) => ast,
            Err(message) => {
                problems.push(format!("{source_hash} no longer parses: {message}"));
                continue;
            }
        };
        if let Err(diagnostics) = lex_sys_ir::lower_all(&ast) {
            let text: Vec<String> =
                diagnostics.iter().map(|d| format!("{}: {}", d.rule.tag(), d.message)).collect();
            problems.push(format!("{source_hash} no longer type-checks:\n{}", text.join("\n\n")));
            continue;
        }

        let identities = identify(&ast);
        let mut sound = true;
        for (sig_id, entry) in entries {
            match identities.functions.iter().find(|f| &f.sig.to_hex() == *sig_id) {
                None => {
                    sound = false;
                    problems.push(format!(
                        "{}: signature {sig_id} is no longer produced by its own source",
                        entry.name
                    ));
                }
                Some(func) if func.body.to_hex() != entry.stage_id => {
                    sound = false;
                    problems.push(format!(
                        "{}: body moved from {} to {} — the manifest's pin no longer matches",
                        entry.name,
                        entry.stage_id,
                        func.body.to_hex()
                    ));
                }
                Some(_) => {}
            }
        }
        if sound {
            verified.insert(source_hash.clone(), text);
        }
    }

    if problems.is_empty() { Ok(verified) } else { Err(problems) }
}

/// Recursively resolve everything one `(lock, store)` pin needs to lower
/// cleanly (`docs/package-system.md` §4.6): this pin's own selected
/// entries, verified, plus every distinct source text its own store's
/// `requires/` says it depends on — walked the same way, however deep.
///
/// `visiting` guards against a cycle (the same store's canonicalized
/// path already on the current path); `pinned` guards against a diamond
/// (two different paths pinning the same store's same name to two
/// different heads). Both are threaded through the whole walk rather
/// than reset per level, because either violation can appear between
/// siblings just as easily as between a level and its own ancestor.
///
/// Returns every distinct verified source text this pin's own blobs
/// need in scope to lower — this level's own, merged with every level
/// beneath it — or every problem found anywhere in the walk.
fn resolve_closure(
    lock: &Lock,
    store: &Path,
    visiting: &mut Vec<PathBuf>,
    pinned: &mut BTreeMap<(PathBuf, String), SigId>,
) -> Result<BTreeMap<String, String>, Vec<String>> {
    let canonical = store.canonicalize().unwrap_or_else(|_| store.to_path_buf());
    if visiting.contains(&canonical) {
        return Err(vec![format!(
            "dependency cycle: `{}` is already on the path {}",
            store.display(),
            visiting.iter().map(|p| format!("`{}`", p.display())).collect::<Vec<_>>().join(" -> ")
        )]);
    }

    for (name, entry) in lock.entries() {
        let key = (canonical.clone(), name.clone());
        match pinned.get(&key) {
            Some(prior) if prior != &entry.sig_id => {
                return Err(vec![format!(
                    "diamond dependency: `{name}` from `{}` is pinned to two different heads \
                     ({prior} and {}) by different paths through the dependency graph",
                    store.display(),
                    entry.sig_id
                )]);
            }
            Some(_) => {}
            None => {
                pinned.insert(key, entry.sig_id.clone());
            }
        }
    }

    visiting.push(canonical);
    let result = (|| -> Result<BTreeMap<String, String>, Vec<String>> {
        let manifest = Manifest::load(store)
            .map_err(|e| vec![format!("cannot read manifest at `{}`: {e}", store.display())])?;
        let blobs = Blobs::open(store)
            .map_err(|e| vec![format!("cannot open store at `{}`: {e}", store.display())])?;
        let selected = select_locked(lock, &manifest, store)?;

        let mut context = resolve_own_requirements(store, visiting, pinned)?;
        let verified = verify_selected(&blobs, &selected, &context)?;
        context.extend(verified);
        Ok(context)
    })();
    visiting.pop();
    result
}

/// The whole-store half of the same walk (`vcs resolve`/`vcs fetch` with
/// no `--lock`, and every level's own entry point): no name to
/// diamond-check against at *this* level — nothing pins a name to it,
/// it is the thing being resolved, not an `import`ed dependency of
/// something else — but its own `requires/`, if it has any, still gets
/// walked exactly the way a deeper level's would.
fn resolve_own_requirements(
    store: &Path,
    visiting: &mut Vec<PathBuf>,
    pinned: &mut BTreeMap<(PathBuf, String), SigId>,
) -> Result<BTreeMap<String, String>, Vec<String>> {
    let requirements = load_requirements(store)
        .map_err(|e| vec![format!("cannot read requirements at `{}`: {e}", store.display())])?;
    let mut context = BTreeMap::new();
    for req in &requirements {
        let dep_store = PathBuf::from(&req.store);
        let deeper = resolve_closure(&req.lock, &dep_store, visiting, pinned)?;
        context.extend(deeper);
    }
    Ok(context)
}

/// `docs/package-system.md` §4.2 and §6: the smallest resolver, against a
/// real second store rather than `std`.
///
/// `--lock <file>` (§4.5) scopes this to just the names a consumer's own
/// `vcs lock` pinned, rather than everything the store has ever
/// published — the difference between "does this store still hold
/// together" and "do *my* dependencies."
///
/// §4.6: a store's own `requires/`, if it has any, is walked
/// recursively too, so this answers "does the whole closure still hold
/// together," not just this one store.
fn cmd_resolve(args: &[String]) -> Result<ExitCode, Failure> {
    let mut lock_path: Option<PathBuf> = None;
    let mut store: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--lock" => {
                let value = it.next().ok_or_else(|| usage("`--lock` needs a path"))?;
                lock_path = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other if store.is_none() => store = Some(PathBuf::from(other)),
            other => {
                return Err(usage(format!(
                    "`vcs resolve` takes exactly one store directory, found a second \
                     argument `{other}`"
                )));
            }
        }
    }
    let store = store.ok_or_else(|| usage("`vcs resolve` needs a store directory"))?;

    let manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;
    if manifest.is_empty() {
        println!("nothing published at `{}`; nothing to resolve", store.display());
        return Ok(ExitCode::SUCCESS);
    }
    let blobs = Blobs::open(&store)
        .map_err(|e| environment(format!("cannot open store at `{}`: {e}", store.display())))?;

    // Everything the store has, or -- with `--lock` -- only what a
    // consumer actually pinned.
    let selected: Vec<(SigId, ManifestEntry)> = match &lock_path {
        None => manifest.entries().map(|(sig_id, entry)| (sig_id.clone(), entry.clone())).collect(),
        Some(lock_path) => {
            let lock = Lock::load(lock_path).map_err(|e| {
                environment(format!("cannot read lock file at `{}`: {e}", lock_path.display()))
            })?;
            if lock.is_empty() {
                println!("nothing locked at `{}`; nothing to resolve", lock_path.display());
                return Ok(ExitCode::SUCCESS);
            }
            select_locked(&lock, &manifest, &store)
                .map_err(|missing| refused(missing.join("\n\n")))?
        }
    };

    let canonical = store.canonicalize().unwrap_or_else(|_| store.clone());
    let mut visiting = vec![canonical];
    let mut pinned = BTreeMap::new();
    let closure_context = resolve_own_requirements(&store, &mut visiting, &mut pinned)
        .map_err(|problems| refused(problems.join("\n\n")))?;

    match verify_selected(&blobs, &selected, &closure_context) {
        Ok(verified) => {
            if closure_context.is_empty() {
                println!(
                    "{} declaration(s) across {} file(s) at `{}` still resolve exactly as \
                     published",
                    selected.len(),
                    verified.len(),
                    store.display()
                );
            } else {
                println!(
                    "{} declaration(s) across {} file(s) at `{}` still resolve exactly as \
                     published, plus {} file(s) verified transitively through its own \
                     `requires/`",
                    selected.len(),
                    verified.len(),
                    store.display(),
                    closure_context.len()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(problems) => Err(refused(problems.join("\n\n"))),
    }
}

/// `docs/package-system.md` §6's own next slice: not compiler surgery,
/// but the one thing `many-files.md`/`modules.md` already need to make a
/// locked dependency buildable -- real bytes on disk. `modules.md` §4.2
/// is explicit that `import` is "a rule for resolving a name" against
/// whatever files are on the command line, never an instruction to go
/// and read something; nothing in the compiler's own module resolution
/// needs to change once a dependency's verified source exists as an
/// ordinary file next to a consumer's own.
///
/// `lex-sys vcs fetch --lock <file> --store <dep-store> -o <dir>`
/// verifies every locked pin the same way `resolve --lock` does (never
/// writing an unverified or broken file to disk) and materialises each
/// distinct source file at `<dir>/<source_hash>.ls`, ready to be named
/// on a `lex-sys build`/`check` command line alongside the consumer's
/// own files, the module `import` chain doing the rest unmodified.
fn cmd_fetch(args: &[String]) -> Result<ExitCode, Failure> {
    let mut lock_path: Option<PathBuf> = None;
    let mut store: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--lock" => {
                let value = it.next().ok_or_else(|| usage("`--lock` needs a path"))?;
                lock_path = Some(PathBuf::from(value));
            }
            "--store" => {
                let value = it.next().ok_or_else(|| usage("`--store` needs a path"))?;
                store = Some(PathBuf::from(value));
            }
            "-o" => {
                let value = it.next().ok_or_else(|| usage("`-o` needs a path"))?;
                out = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other => {
                return Err(usage(format!(
                    "`vcs fetch` takes no positional arguments, found `{other}`"
                )));
            }
        }
    }
    let lock_path = lock_path.ok_or_else(|| usage("`vcs fetch` needs `--lock <file>`"))?;
    let store = store.ok_or_else(|| usage("`vcs fetch` needs `--store <dependency-store>`"))?;
    let out = out.ok_or_else(|| usage("`vcs fetch` needs `-o <dir>`"))?;

    let lock = Lock::load(&lock_path).map_err(|e| {
        environment(format!("cannot read lock file at `{}`: {e}", lock_path.display()))
    })?;
    if lock.is_empty() {
        println!("nothing locked at `{}`; nothing to fetch", lock_path.display());
        return Ok(ExitCode::SUCCESS);
    }
    let manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;
    let blobs = Blobs::open(&store)
        .map_err(|e| environment(format!("cannot open store at `{}`: {e}", store.display())))?;

    let selected =
        select_locked(&lock, &manifest, &store).map_err(|missing| refused(missing.join("\n\n")))?;

    let canonical = store.canonicalize().unwrap_or_else(|_| store.clone());
    let mut visiting = vec![canonical];
    let mut pinned = BTreeMap::new();
    let closure_context = resolve_own_requirements(&store, &mut visiting, &mut pinned)
        .map_err(|problems| refused(problems.join("\n\n")))?;

    let verified = verify_selected(&blobs, &selected, &closure_context)
        .map_err(|problems| refused(problems.join("\n\n")))?;

    std::fs::create_dir_all(&out)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", out.display())))?;
    // The whole closure, not just this store's own -- a consumer that
    // fetches `http.request` (`docs/package-system.md` §4.6) gets
    // `net.sockets`' source alongside it in the same directory, and
    // never has to know it needed fetching at all.
    for (source_hash, text) in verified.iter().chain(closure_context.iter()) {
        let path = out.join(format!("{source_hash}.ls"));
        std::fs::write(&path, text)
            .map_err(|e| environment(format!("cannot write `{}`: {e}", path.display())))?;
        println!("fetched {}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// `docs/package-system.md` §4.5: a name is chosen once, when a
/// dependency is first added, and resolved by hash forever after -- this
/// is that choice. `lex-sys vcs lock --store <dep-store> -o <lockfile>
/// <name>...` looks each name up in the dependency's own manifest (by
/// `ManifestEntry::name`, never by a hash the caller does not have yet)
/// and pins it, refusing rather than guessing if the name is missing or
/// ambiguous. `vcs resolve --lock <lockfile> <dep-store>` is the reader.
fn cmd_lock(args: &[String]) -> Result<ExitCode, Failure> {
    let mut store: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut names: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--store" => {
                let value = it.next().ok_or_else(|| usage("`--store` needs a path"))?;
                store = Some(PathBuf::from(value));
            }
            "-o" => {
                let value = it.next().ok_or_else(|| usage("`-o` needs a path"))?;
                out = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other => names.push(other.to_owned()),
        }
    }
    let store = store.ok_or_else(|| usage("`vcs lock` needs `--store <dependency-store>`"))?;
    let out = out.ok_or_else(|| usage("`vcs lock` needs `-o <lockfile>`"))?;
    if names.is_empty() {
        return Err(usage("`vcs lock` needs at least one declaration name"));
    }

    let manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;
    let mut lock = Lock::load(&out)
        .map_err(|e| environment(format!("cannot read lock file at `{}`: {e}", out.display())))?;

    for name in &names {
        let matches: Vec<(&SigId, &ManifestEntry)> =
            manifest.entries().filter(|(_, entry)| &entry.name == name).collect();
        match matches.as_slice() {
            [] => {
                return Err(refused(format!("`{name}` is not published at `{}`", store.display())));
            }
            [(sig_id, entry)] => {
                lock.insert(
                    name.clone(),
                    LockEntry {
                        sig_id: (*sig_id).clone(),
                        stage_id: entry.stage_id.clone(),
                        source_hash: entry.source_hash.clone(),
                    },
                );
                println!("locked {name}");
            }
            _ => {
                return Err(refused(format!(
                    "`{name}` is ambiguous at `{}`: {} declarations share that name",
                    store.display(),
                    matches.len()
                )));
            }
        }
    }

    lock.save(&out)
        .map_err(|e| environment(format!("cannot write lock file at `{}`: {e}", out.display())))?;
    Ok(ExitCode::SUCCESS)
}
