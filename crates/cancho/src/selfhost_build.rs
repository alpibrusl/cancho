//! `cancho build --compiler <exe>`: the program built by the compiler written in cancho
//! (`examples/selfhost/compiler.cho`, built once) instead of by the Rust one (`docs/self-hosting.md`).
//!
//! The compiler reads the target and the files of the program on standard input, and answers with the
//! module as LLVM IR text, or `ERR <rule> <from> <to>`, the first refusal as a rule and a span of the files
//! laid end to end, one byte between them. `clang` turns the module into an object, and the link is the
//! one every other build uses.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
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
        .filter_map(|names| names.split_whitespace().next())
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

/// The text of a span of the laid-out files, shortened to its first line.
fn quote(files: &[(String, String)], from: usize, to: usize) -> String {
    let mut base = 0;
    for (_, text) in files {
        if from >= base && from <= base + text.len() {
            let start = from - base;
            let end = (to - base).min(text.len()).max(start);
            let first = text[start..end].lines().next().unwrap_or("");
            let mut shown: String = first.chars().take(80).collect();
            if shown.len() < first.len() || text[start..end].lines().count() > 1 {
                shown.push_str(" ...");
            }
            return shown;
        }
        base += text.len() + 1;
    }
    String::new()
}

/// Where a span of the laid-out files is: the file, the line and column, and the text of the line.
fn located(files: &[(String, String)], offset: usize) -> Option<(String, usize, usize, String)> {
    let mut base = 0;
    for (name, text) in files {
        if offset <= base + text.len() {
            let at = (offset - base).min(text.len());
            let line = 1 + text[..at].matches('\n').count();
            let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let column = 1 + text[start..at].chars().count();
            let shown = text[start..].lines().next().unwrap_or("").to_owned();
            return Some((name.clone(), line, column, shown));
        }
        base += text.len() + 1;
    }
    None
}

/// A refusal as the compiler in cancho gives it: `ERR <rule> <from> <to>`, and on the next line the sentence that says
/// why (`Rule::explanation`, the same for every program the rule refuses), as a person reads it: where, why, the line,
/// and what of it.
fn refusal(answer: &str, files: &[(String, String)]) -> Failure {
    let mut lines = answer.lines();
    let first = lines.next().unwrap_or("");
    let sentence = lines.next().unwrap_or("").trim();
    let mut words = first.split_whitespace().skip(1);
    let rule = words.next().unwrap_or("unknown");
    let from: Option<usize> = words.next().and_then(|w| w.parse().ok());
    let to: usize = words.next().and_then(|w| w.parse().ok()).unwrap_or(0);
    let at = from.map(|offset| place(files, offset)).unwrap_or_else(|| "the program".to_owned());
    let near = from.map(|offset| quote(files, offset, to)).unwrap_or_default();
    if rule == "SKIP" {
        return refused(format!(
            "{at}: the compiler in cancho has no code for this construct yet: `{near}` (`cancho build` without \
             `--compiler` builds the program)"
        ));
    }
    let why = if sentence.is_empty() { "refused" } else { sentence };
    let Some((file, line, column, shown)) = from.and_then(|offset| located(files, offset)) else {
        return refused(format!("the program: error: {why} (rule `{rule}`)"));
    };
    // The caret runs under the span on its first line.
    let width = to
        .saturating_sub(from.unwrap_or(0))
        .min(shown.chars().count().saturating_sub(column - 1))
        .max(1);
    refused(format!(
        "{file}:{line}:{column}: error: {why} (rule `{rule}`)\n    {shown}\n    {}{}",
        " ".repeat(column - 1),
        "^".repeat(width)
    ))
}

/// The files of `compiler.cho` and the modules of `examples/selfhost` it imports, in the order they are built.
fn compiler_sources(dir: &Path) -> Result<Vec<PathBuf>, Failure> {
    fn walk(
        dir: &Path,
        file: &Path,
        seen: &mut BTreeSet<PathBuf>,
        order: &mut Vec<PathBuf>,
    ) -> Result<(), Failure> {
        let text = std::fs::read_to_string(file)
            .map_err(|e| environment(format!("cannot read `{}`: {e}", file.display())))?;
        for names in text
            .lines()
            .filter_map(|l| l.trim().strip_prefix("import selfhost.")?.strip_suffix(';'))
        {
            let Some(name) = names.split_whitespace().next() else { continue };
            let path = dir.join(format!("{name}.cho"));
            if path.exists() && seen.insert(path.clone()) {
                walk(dir, &path, seen, order)?;
                order.push(path);
            }
        }
        Ok(())
    }
    let root = dir.join("compiler.cho");
    let (mut seen, mut order) = (BTreeSet::new(), Vec::new());
    walk(dir, &root, &mut seen, &mut order)?;
    order.push(root);
    Ok(order)
}

/// `--compiler auto`: the compiler written in cancho, built from this repository's `examples/selfhost` by the Rust
/// compiler the first time and kept in the cache by a hash of its sources (`$CANCHO_CACHE`, or `~/.cache/cancho`). A
/// compiler installed without the repository next to it cannot build it: it says so.
pub fn ensure_compiler() -> Result<PathBuf, Failure> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/selfhost");
    if !dir.join("compiler.cho").exists() {
        return Err(environment(
            "`--compiler auto` builds the compiler in cancho from `examples/selfhost`, which is not next to this \
             binary; build it once and name it with `--compiler <exe>`"
                .to_owned(),
        ));
    }
    let files = compiler_sources(&dir)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);
    for file in &files {
        std::fs::read(file)
            .map_err(|e| environment(format!("cannot read `{}`: {e}", file.display())))?
            .hash(&mut hasher);
    }
    let base = std::env::var_os("CANCHO_CACHE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache/cancho")))
        .ok_or_else(|| environment("no cache directory: set CANCHO_CACHE or HOME".to_owned()))?;
    let cache = base.join(format!("selfhost-{:016x}", hasher.finish()));
    let exe = cache.join("compiler");
    if exe.exists() {
        return Ok(exe);
    }
    // Several builds can start together with nothing cached: each builds in a directory of its own (the object file and
    // the IR are named after the executable) and moves the finished compiler into place, which is atomic, so no build
    // sees another's half-written files and whichever lands last leaves the same bytes.
    let scratch = cache.join(format!(
        "building-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&scratch)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", scratch.display())))?;
    eprintln!("building the compiler in cancho (once) into {}", exe.display());
    let built = scratch.join("compiler");
    let made = crate::build(
        &files,
        &built,
        Emit::Exe,
        true,
        crate::Codegen { backend: crate::Backend::Llvm, target: None },
        &[],
        &[],
    );
    let moved = made.and_then(|()| {
        std::fs::rename(&built, &exe).map_err(|e| {
            environment(format!("cannot place the compiler at `{}`: {e}", exe.display()))
        })
    });
    let _ = std::fs::remove_dir_all(&scratch);
    moved?;
    Ok(exe)
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
    let built;
    let compiler = if compiler == Path::new("auto") {
        built = ensure_compiler()?;
        built.as_path()
    } else {
        compiler
    };
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
    if let Some(path) = std::env::var_os("CANCHO_COMPILER_DUMP") {
        // For finding out why the compiler stops: the stream it is sent, to be given to the census build of it.
        let _ = std::fs::write(path, &bytes);
    }
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
