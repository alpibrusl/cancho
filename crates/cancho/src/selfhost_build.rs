//! `cancho build --compiler <exe>`: the program built by the compiler written in cancho
//! (`examples/selfhost/compiler.cho`, built once) instead of by the Rust one (`docs/self-hosting.md`).
//!
//! The compiler reads the target and the files of the program on standard input, and answers with the
//! module as LLVM IR text, or `ERR <rule> <from> <to>`, the first refusal as a rule and a span of the files
//! laid end to end, one byte between them. `clang` turns the module into an object, and the link is the
//! one every other build uses.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use target_lexicon::{Architecture, OperatingSystem, Triple};

use crate::{Emit, Failure, environment, refused, usage};

/// The line of the stream that names the target: the host's, or the triple given.
fn target_line(target: Option<&Triple>) -> Result<&'static str, Failure> {
    let host = Triple::host();
    let triple = target.unwrap_or(&host);
    match (triple.architecture, triple.operating_system) {
        (Architecture::Aarch64(_), OperatingSystem::Darwin(_) | OperatingSystem::MacOSX(_)) => {
            Ok("TARGET darwin\n")
        }
        (Architecture::X86_64, OperatingSystem::Linux) => Ok("TARGET linux\n"),
        (Architecture::Aarch64(_), OperatingSystem::Linux) => Ok("TARGET linux aarch64\n"),
        _ => Err(usage(format!(
            "the compiler in cancho writes code for aarch64 macOS, x86-64 Linux and aarch64 Linux, not `{triple}`"
        ))),
    }
}

/// The stream the compiler reads: the target, then each file as `FILE <length>` and its bytes.
fn stream(target: &str, files: &[(String, String)]) -> Vec<u8> {
    let mut out = target.as_bytes().to_vec();
    for (_, text) in files {
        out.extend_from_slice(format!("FILE {}\n", text.len()).as_bytes());
        out.extend_from_slice(text.as_bytes());
    }
    out
}

/// Where in the files a span of the laid-out stream is, as `file:line`.
fn place(files: &[(String, String)], offset: usize) -> String {
    let mut base = 0;
    for (name, text) in files {
        if offset <= base + text.len() {
            let line = 1 + text[..(offset - base).min(text.len())].matches('\n').count();
            return format!("{name}:{line}");
        }
        base += text.len() + 1;
    }
    "the program".to_owned()
}

/// The name a file gives itself (`module a.b;`) and the names it imports (`import a.b;`).
fn module_name(text: &str) -> Option<&str> {
    text.lines().find_map(|l| l.trim().strip_prefix("module ")?.strip_suffix(';')).map(str::trim)
}

fn imports(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("import ")?.strip_suffix(';'))
        .map(str::trim)
        .collect()
}

/// The standard library's modules the program imports, and those they import: the compiler in cancho does
/// not take the rest, which it would have to check too.
fn add_imported(files: &mut Vec<(String, String)>) {
    let mut at = 0;
    while at < files.len() {
        let wanted: Vec<String> = imports(&files[at].1).into_iter().map(str::to_owned).collect();
        for name in wanted {
            let already = files.iter().any(|(_, t)| module_name(t) == Some(name.as_str()));
            if already {
                continue;
            }
            if let Some((file, text)) =
                crate::STD.iter().find(|(_, t)| module_name(t) == Some(name.as_str()))
            {
                files.push(((*file).to_owned(), (*text).to_owned()));
            }
        }
        at += 1;
    }
}

/// A refusal as the compiler in cancho gives it: `ERR <rule> <from> <to>`.
fn refusal(answer: &str, files: &[(String, String)]) -> Failure {
    let mut words = answer.split_whitespace().skip(1);
    let rule = words.next().unwrap_or("unknown");
    let from: Option<usize> = words.next().and_then(|w| w.parse().ok());
    let at = from.map(|offset| place(files, offset)).unwrap_or_else(|| "the program".to_owned());
    if rule == "SKIP" {
        return refused(format!(
            "{at}: the compiler in cancho has no code for this construct yet (`cancho build` without \
             `--compiler` builds the program)"
        ));
    }
    refused(format!(
        "{at}: refused by the rule `{rule}` (the compiler in cancho gives the rule and the place, not the sentence; \
         `cancho check` gives the sentence)"
    ))
}

/// What to build and how: the compiler, the program's files, and what `cancho build` was given.
pub struct Job<'a> {
    pub compiler: &'a Path,
    pub inputs: &'a [std::path::PathBuf],
    pub output: &'a Path,
    pub emit: Emit,
    pub with_std: bool,
    pub target: Option<&'a Triple>,
    pub link_libs: &'a [String],
    pub link_paths: &'a [String],
}

/// Build the program of `job` (and the standard library, with `with_std`) with its compiler.
pub fn build(job: Job<'_>) -> Result<(), Failure> {
    let Job { compiler, inputs, output, emit, with_std, target, link_libs, link_paths } = job;
    let line = target_line(target)?;
    let mut files: Vec<(String, String)> = Vec::new();
    for input in inputs {
        let text = std::fs::read_to_string(input)
            .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
        files.push((input.display().to_string(), text));
    }
    if with_std {
        add_imported(&mut files);
    }
    let mut child = Command::new(compiler)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            environment(format!("cannot run the compiler `{}`: {e}", compiler.display()))
        })?;
    let mut input = child.stdin.take().expect("a piped stdin");
    let bytes = stream(line, &files);
    // The compiler reads all of its input before it answers, so writing it all first cannot deadlock.
    let writer = std::thread::spawn(move || input.write_all(&bytes));
    let ended = child
        .wait_with_output()
        .map_err(|e| environment(format!("the compiler `{}` failed: {e}", compiler.display())))?;
    let _ = writer.join();
    let answer = String::from_utf8_lossy(&ended.stdout).into_owned();
    if answer.starts_with("ERR") {
        return Err(refusal(&answer, &files));
    }
    if !ended.status.success() || answer.is_empty() {
        return Err(environment(format!(
            "the compiler `{}` ended with {} and no module",
            compiler.display(),
            ended.status
        )));
    }
    let module = output.with_extension("ll");
    std::fs::write(&module, &answer)
        .map_err(|e| environment(format!("cannot write `{}`: {e}", module.display())))?;
    let object = match emit {
        Emit::Obj => output.to_path_buf(),
        Emit::Exe => output.with_extension("o"),
    };
    let cc = std::env::var("CLANG").unwrap_or_else(|_| "clang".to_owned());
    let made = Command::new(&cc)
        .args(["-O1", "-Wno-override-module", "-c"])
        .arg(&module)
        .arg("-o")
        .arg(&object)
        .output()
        .map_err(|e| environment(format!("cannot run `{cc}`: {e}")));
    let _ = std::fs::remove_file(&module);
    let made = made?;
    if !made.status.success() {
        return Err(environment(format!(
            "`{cc}` could not build the module the compiler in cancho wrote: {}",
            String::from_utf8_lossy(&made.stderr)
        )));
    }
    if emit == Emit::Exe {
        let linked = crate::link(&object, output, link_libs, link_paths);
        let _ = std::fs::remove_file(&object);
        linked?;
    }
    Ok(())
}
