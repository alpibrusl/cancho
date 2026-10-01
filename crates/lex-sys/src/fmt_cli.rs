//! `lex-sys fmt`: the canonical layout, comments and blank lines kept.
//!
//! `docs/formatting.md`. The work is `lex_sys_syntax::format`; this is the
//! file handling around it: directories are walked, a file is rewritten
//! only if its text changes, and nothing is written at all under
//! `--check`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lex_sys_syntax::{FormatError, SourceFile};

use crate::{EXIT_REFUSED, Failure, environment, parse_args};

/// Every `.ls` file under `path`, or `path` itself if it is a file.
/// Sorted, so the order of the report does not depend on the filesystem,
/// and without descending into a version-control store (`.lex-sys-vcs`
/// holds blobs of source, which are not the working copy).
fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), Failure> {
    if !path.is_dir() {
        out.push(path.to_owned());
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| environment(format!("cannot read directory `{}`: {e}", path.display())))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let skip = entry.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if skip {
            continue;
        }
        if entry.is_dir() || entry.extension().is_some_and(|e| e == "ls") {
            collect(&entry, out)?;
        }
    }
    Ok(())
}

pub fn cmd_fmt(args: &[String]) -> Result<ExitCode, Failure> {
    let check = args.iter().any(|a| a == "--check");
    let rest: Vec<String> = args.iter().filter(|a| *a != "--check").cloned().collect();
    let crate::Invocation { inputs, .. } = parse_args(&rest, false, false)?;

    let mut files = Vec::new();
    for input in &inputs {
        collect(input, &mut files)?;
    }

    let mut changed = 0usize;
    let mut failed = 0usize;
    for path in &files {
        let text = std::fs::read_to_string(path)
            .map_err(|e| environment(format!("cannot read `{}`: {e}", path.display())))?;
        match lex_sys_syntax::format(&text) {
            Ok(formatted) if formatted == text => {}
            Ok(formatted) => {
                changed += 1;
                if check {
                    println!("would reformat {}", path.display());
                } else {
                    std::fs::write(path, formatted).map_err(|e| {
                        environment(format!("cannot write `{}`: {e}", path.display()))
                    })?;
                    println!("formatted {}", path.display());
                }
            }
            Err(FormatError::Parse(diagnostic)) => {
                failed += 1;
                let file = SourceFile::new(path.display().to_string(), text);
                eprintln!("{}", diagnostic.render(&file));
            }
            Err(FormatError::Cannot(why)) => {
                failed += 1;
                eprintln!("{}: not formatted: {why}", path.display());
            }
        }
    }

    if failed > 0 || (check && changed > 0) {
        return Ok(ExitCode::from(EXIT_REFUSED));
    }
    Ok(ExitCode::SUCCESS)
}
