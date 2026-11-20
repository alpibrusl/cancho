//! `cancho vcs`: refused.
//!
//! Every `vcs` subcommand of this binary was implemented on `cancho-vcs`, which is deleted (`docs/removing-rust.md`, Phase 5, step 6);
//! the compiler in cancho answers all of them (`examples/selfhost/clivcs*.cho`), on the same store. This binary says so.

use std::process::ExitCode;

use crate::{Failure, retired, usage};

pub fn cmd_vcs(args: &[String]) -> Result<ExitCode, Failure> {
    match args.first().map(String::as_str) {
        Some(cmd) => Err(retired(&format!("vcs {cmd}"))),
        None => Err(usage("`vcs` needs a subcommand: `init`, `record`, `status`, `log`, ...")),
    }
}
