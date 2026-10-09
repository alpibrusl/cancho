//! `cancho satisfy <contract.cho> <candidate.cho>` — the spec-to-implementation
//! loop the manifesto is about (`docs/satisfy.md`, #406): a contract is a
//! `.cho` file whose tests call the candidate, and satisfaction is the
//! ordinary checker and the ordinary test runner composed, with the
//! verdict named. No new trust machinery — the checker is the gate it
//! always was, and the candidate's recomputed identity is answered so a
//! caller can see what it now depends on.

use std::path::PathBuf;
use std::process::ExitCode;

use cancho_id::identify;

use crate::vcs_cli::parse_texts;
use crate::{Failure, environment, usage};

/// `cancho satisfy <contract.cho> <candidate.cho> [--std]`.
///
/// Exit codes are the ones the composed commands already answer: 0
/// satisfied, 1 refused by the checker, 4 a test failed — because
/// satisfy *is* `check` plus `test` against one program, and the
/// verdict does not change what happened.
pub fn cmd_satisfy(args: &[String]) -> Result<ExitCode, Failure> {
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut with_std = false;
    for arg in args {
        match arg.as_str() {
            "--std" => with_std = true,
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown `satisfy` option `{other}`")));
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }
    if inputs.len() != 2 {
        return Err(usage(
            "`cancho satisfy <contract.cho> <candidate.cho> [--std]` — exactly two files",
        ));
    }
    let (contract, candidate) = (inputs[0].clone(), inputs[1].clone());

    // 1. Check the pair as one program: the contract's tests call the
    //    candidate, so a missing declaration or a mismatched signature is
    //    the ordinary refusal vocabulary, located, exactly as `check`
    //    answers it. A candidate that does not check does not run.
    let files: Vec<(String, String)> = [&contract, &candidate]
        .iter()
        .map(|p| {
            let text = std::fs::read_to_string(p)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", p.display())))?;
            Ok((p.display().to_string(), text))
        })
        .collect::<Result<_, Failure>>()?;
    let std_texts: Vec<(String, String)> = if with_std {
        crate::STD.iter().map(|(n, t)| ((*n).to_owned(), (*t).to_owned())).collect()
    } else {
        Vec::new()
    };
    let mut named = files.clone();
    named.extend(std_texts.clone());
    let (ast, map) = parse_texts(&named).map_err(crate::refused)?;
    cancho_ir::lower_all(&ast).map_err(|diagnostics| {
        let text: Vec<String> = diagnostics.iter().map(|d| d.render_in(&map)).collect();
        crate::refused(text.join("\n\n"))
    })?;

    // 2. What the candidate provides, by identity: every declaration the
    //    contract's calls resolve to, recomputed from the pair — the
    //    `SigId` a caller will depend on, answered in the verdict.
    let identities = identify(&ast);
    let candidate_names: std::collections::BTreeSet<(String, String)> = {
        let (own_ast, _) = parse_texts(&[files[1].clone()]).map_err(crate::refused)?;
        identify(&own_ast).functions.iter().map(|f| (f.module.clone(), f.name.clone())).collect()
    };

    // 3. Run the contract's tests against the candidate, exactly as
    //    `cancho test` does. The tests are discovered from the pair;
    //    a contract with none is a contract that checks nothing, and
    //    that is a usage error rather than a pass.
    let tests = crate::test_cli::discover(&[contract.clone(), candidate.clone()])?;
    if tests.is_empty() {
        return Err(usage(
            "the contract declares no `test_*` functions; a contract that \
             tests nothing cannot be satisfied",
        ));
    }
    let dir = std::env::temp_dir().join(format!("cancho-satisfy-{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", dir.display())))?;
    let run = crate::test_cli::run_all(
        &dir,
        &[contract, candidate],
        &tests,
        with_std,
        crate::Backend::Cranelift,
        &[],
        &[],
    );
    let _ = std::fs::remove_dir_all(&dir);
    let code = run?;

    // 4. The verdict, named. `run_all` answers 0 when every test
    //    passed and 4 when one failed, which are satisfaction and its
    //    negation; the identities beside the verdict are what a caller
    //    reading the output as data takes away.
    if code == ExitCode::SUCCESS {
        println!("satisfied");
        for f in &identities.functions {
            if candidate_names.contains(&(f.module.clone(), f.name.clone())) {
                println!(
                    "  {}{} sig {}",
                    f.module,
                    if f.module.is_empty() { "" } else { "::" },
                    f.name
                );
                println!("    sig_id {}", f.sig.to_hex());
            }
        }
        Ok(ExitCode::SUCCESS)
    } else {
        println!("not satisfied: see the test failures above");
        Ok(code)
    }
}
