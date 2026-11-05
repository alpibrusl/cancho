//! `cancho vcs gc [--dry-run]` (`docs/vcs-cli.md` sections 4, 5 and R4):
//! pack every object into one pack with deltas, then remove the loose
//! files and older packs it replaces. Takes the working-copy lock, so it
//! never runs beside a `record` or another `gc`; a kill at any point leaves
//! a store `verify` calls sound, and a second run finishes the job.

use std::process::ExitCode;

use cancho_vcs::{VcsResult, gc, work_lock};

use crate::Failure;
use crate::vcs_args::{Done, Parsed, open, parse, run};

pub(crate) fn dispatch(args: &[String]) -> Result<ExitCode, Failure> {
    let p = parse(args, &[], &[])?;
    run("gc", &p, || cmd_gc(&p))
}

fn cmd_gc(p: &Parsed) -> VcsResult<Done> {
    let repo = open(p)?;
    if !p.pos.is_empty() {
        return Err(cancho_vcs::Refusal::new("bad-argument", "`gc` takes no paths"));
    }
    let _lock = work_lock(&repo.vcs)?;
    Ok(gc(&repo.store, p.flag("--dry-run"))?.to_json())
}
