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

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use lex_sys_id::identify;
use lex_sys_ir::{Effects, Func};
use lex_sys_vcs::{
    Blobs, Manifest, ManifestEntry, OpLog, Operation, OperationKind, OperationRecord,
};

use crate::{Failure, environment, parse_program, refused, usage};

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
        Some(other) => Err(usage(format!("unknown `vcs` subcommand `{other}`"))),
        None => Err(usage("`vcs` needs a subcommand: `publish`, `log` or `resolve`")),
    }
}

struct VcsInvocation {
    store: PathBuf,
    inputs: Vec<PathBuf>,
}

fn parse_vcs_args(args: &[String]) -> Result<VcsInvocation, Failure> {
    let mut store = PathBuf::from(DEFAULT_STORE);
    let mut inputs = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--store" => {
                let value = it.next().ok_or_else(|| usage("`--store` needs a path"))?;
                store = PathBuf::from(value);
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }
    Ok(VcsInvocation { store, inputs })
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

fn cmd_publish(args: &[String]) -> Result<ExitCode, Failure> {
    let VcsInvocation { store, inputs } = parse_vcs_args(args)?;
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

    let (ast, map) = parse_program(&inputs, false)?;
    let identities = identify(&ast);
    let program = lex_sys_ir::lower_all(&ast).map_err(|diagnostics| {
        let text: Vec<String> = diagnostics.iter().map(|d| d.render_in(&map)).collect();
        refused(text.join("\n\n"))
    })?;

    let source = std::fs::read_to_string(&inputs[0])
        .map_err(|e| environment(format!("cannot read `{}`: {e}", inputs[0].display())))?;
    let file_name = inputs[0].to_string_lossy().into_owned();

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

        let ir_func = find_func(&program.funcs, &func.name).ok_or_else(|| {
            refused(format!("internal: `{}` has an identity but no lowered function", func.name))
        })?;

        lex_sys_vcs::check_candidate(&[(file_name.as_str(), source.as_str())]).map_err(
            |diagnostics| {
                let text: Vec<String> =
                    diagnostics.into_iter().map(|d| format!("{}: {}", d.rule, d.message)).collect();
                refused(text.join("\n\n"))
            },
        )?;

        let op = Operation::new(
            OperationKind::AddFunction {
                sig_id: sig_id.clone(),
                stage_id: stage_id.clone(),
                effects: effect_strings(&ir_func.effects),
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

    if published == 0 {
        println!("nothing new ({unchanged} declaration(s) unchanged)");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_log(args: &[String]) -> Result<ExitCode, Failure> {
    let VcsInvocation { store, inputs } = parse_vcs_args(args)?;
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

/// `docs/package-system.md` §4.2 and §6: the smallest resolver, against a
/// real second store rather than `std`. A dependency's lock (its
/// manifest) is never trusted on its own — this re-parses and
/// re-typechecks the *source* behind every pin under today's compiler,
/// then re-derives each declaration's identity and compares it against
/// what the manifest claims. `hash-stability.md` measured that 71% of
/// this repository's own history stops type-checking under today's
/// build; this is that measurement, run against someone else's store
/// instead of assumed away.
///
/// Groups by `source_hash` first, so a file several declarations came
/// from is re-checked once rather than once per declaration.
fn cmd_resolve(args: &[String]) -> Result<ExitCode, Failure> {
    let mut it = args.iter();
    let Some(store) = it.next() else {
        return Err(usage("`vcs resolve` needs a store directory"));
    };
    if it.next().is_some() {
        return Err(usage("`vcs resolve` takes exactly one store directory"));
    }
    let store = PathBuf::from(store);

    let manifest = Manifest::load(&store)
        .map_err(|e| environment(format!("cannot read manifest at `{}`: {e}", store.display())))?;
    if manifest.is_empty() {
        println!("nothing published at `{}`; nothing to resolve", store.display());
        return Ok(ExitCode::SUCCESS);
    }
    let blobs = Blobs::open(&store)
        .map_err(|e| environment(format!("cannot open store at `{}`: {e}", store.display())))?;

    let mut by_source: std::collections::BTreeMap<String, Vec<(&String, &ManifestEntry)>> =
        std::collections::BTreeMap::new();
    for (sig_id, entry) in manifest.entries() {
        by_source.entry(entry.source_hash.clone()).or_default().push((sig_id, entry));
    }

    let mut problems: Vec<String> = Vec::new();
    let mut checked_files = 0usize;

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
                return Err(environment(format!("cannot read source blob `{source_hash}`: {e}")));
            }
        };

        let ast = match lex_sys_syntax::parse(&text) {
            Ok(ast) => ast,
            Err(d) => {
                problems.push(format!(
                    "{source_hash} no longer parses ({}): {}",
                    d.rule.tag(),
                    d.message
                ));
                continue;
            }
        };
        if let Err(diagnostics) = lex_sys_ir::lower_all(&ast) {
            let text: Vec<String> =
                diagnostics.iter().map(|d| format!("{}: {}", d.rule.tag(), d.message)).collect();
            problems.push(format!("{source_hash} no longer type-checks:\n{}", text.join("\n\n")));
            continue;
        }
        checked_files += 1;

        let identities = identify(&ast);
        for (sig_id, entry) in entries {
            match identities.functions.iter().find(|f| &f.sig.to_hex() == *sig_id) {
                None => problems.push(format!(
                    "{}: signature {sig_id} is no longer produced by its own source",
                    entry.name
                )),
                Some(func) if func.body.to_hex() != entry.stage_id => problems.push(format!(
                    "{}: body moved from {} to {} — the manifest's pin no longer matches",
                    entry.name,
                    entry.stage_id,
                    func.body.to_hex()
                )),
                Some(_) => {}
            }
        }
    }

    if !problems.is_empty() {
        return Err(refused(problems.join("\n\n")));
    }
    println!(
        "{} declaration(s) across {} file(s) at `{}` still resolve exactly as published",
        manifest.len(),
        checked_files,
        store.display()
    );
    Ok(ExitCode::SUCCESS)
}
