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
    //    `cancho test` does. The tests are discovered from the contract alone: a `test_*`
    //    function in the candidate is not the contract's, and counting it
    //    would let an empty contract pass on the candidate's own tests.
    //    A contract with none is a contract that checks nothing, and
    //    that is a usage error rather than a pass.
    let tests = crate::test_cli::discover(std::slice::from_ref(&contract))?;
    if tests.is_empty() {
        return Err(usage(
            "the contract declares no `test_*` functions; a contract that \
             tests nothing cannot be satisfied",
        ));
    }
    // A contract whose text never calls anything the candidate declares
    //    constrains nothing about it: every test would pass against any
    //    candidate. Refused the way an empty contract is.
    let contract_text = &files[0].1;
    let provided: Vec<&str> = candidate_names
        .iter()
        .map(|(_, name)| name.as_str())
        .filter(|name| !name.starts_with("test_"))
        .collect();
    if !provided.is_empty() && !calls_any(contract_text, &provided) {
        return Err(usage(format!(
            "the contract never calls anything the candidate declares ({}); a contract \
             that does not exercise the candidate is satisfied by every candidate",
            provided.join(", ")
        )));
    }
    let dir = crate::scratch::private_temp_dir("cancho-satisfy")
        .map_err(|e| environment(format!("cannot create a temporary directory: {e}")))?;
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

/// Whether `text` contains a call, `name(`, to any of `names`: a
/// syntactic check on the contract's own source, comments and string
/// literals set aside. A declaration `fn name(` is not a call. It is
/// deliberately by name, not by resolution: the pair has already
/// checked, so a name that is called is the one the pair resolved.
fn calls_any(text: &str, names: &[&str]) -> bool {
    let mut code = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        code.push('\n');
                        break;
                    }
                }
            }
            '"' => {
                while let Some(d) = chars.next() {
                    if d == '\\' {
                        chars.next();
                    } else if d == '"' {
                        break;
                    }
                }
                code.push(' ');
            }
            other => code.push(other),
        }
    }
    let bytes = code.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut previous = "";
    let mut i = 0;
    while i < bytes.len() {
        if is_ident(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_ident(bytes[i]) {
                i += 1;
            }
            let word = &code[start..i];
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if previous != "fn" && bytes.get(j) == Some(&b'(') && names.contains(&word) {
                return true;
            }
            previous = word;
        } else {
            i += 1;
        }
    }
    false
}
