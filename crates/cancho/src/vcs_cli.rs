//! `cancho vcs`: the dispatch table.
//!
//! The package-store subcommands (`publish`, `sync`, `log --store`, `resolve`, `lock`, `fetch`) re-type-checked every source under the
//! Rust front end (`cancho-ir`), which is deleted (`docs/removing-rust.md`, Phase 5, step 3); the compiler in cancho answers them
//! (`examples/selfhost/clivcs*.cho`), and this binary says so. The repository subcommands (`init`, `record`, `status`, ...) need no
//! type-check and stay (`vcs_repo`).

use std::process::ExitCode;

use crate::{Failure, retired, usage};

pub fn cmd_vcs(args: &[String]) -> Result<ExitCode, Failure> {
    match args.first().map(String::as_str) {
        Some(cmd @ ("publish" | "sync" | "resolve" | "lock" | "fetch")) => {
            Err(retired(&format!("vcs {cmd}")))
        }
        Some("log") if args.iter().any(|a| a == "--store" || a.starts_with("--store=")) => {
            Err(retired("vcs log --store"))
        }
        Some(
            cmd @ ("init" | "log" | "status" | "diff" | "record" | "show" | "blame" | "branch"
            | "switch" | "revert" | "restore" | "verify" | "unlock" | "import-git"
            | "export-git" | "apply"),
        ) => crate::vcs_repo::dispatch(cmd, &args[1..]),
        Some(other) => Err(usage(format!("unknown `vcs` subcommand `{other}`"))),
        None => Err(usage("`vcs` needs a subcommand: `init`, `record`, `status`, `log`, ...")),
    }
}
