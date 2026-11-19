//! `cancho build --compiler <exe>`: the program built by the compiler written in cancho
//! (`examples/selfhost/compiler.cho`, built once) instead of by the Rust one (`docs/self-hosting.md`).
//!
//! The compiler reads the target and the files of the program on standard input, and answers with the
//! module as LLVM IR text, or `ERR <rule> <from> <to>`, the first refusal as a rule and a span of the files
//! laid end to end, one byte between them. `clang` turns the module into an object, and the link is the
//! one every other build uses.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cancho_syntax::lexer::TokenKind;
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

/// The dotted names that follow each `keyword` token (`module a.b;`, `import a.b;`), read from the lexer's tokens, so that the
/// layout of a file does not matter: several statements to a line, spaces round a dot, comments.
fn named_after(text: &str, keyword: TokenKind) -> Vec<String> {
    let Ok(tokens) = cancho_syntax::lexer::tokenize(text) else {
        return Vec::new();
    };
    let word = |at: usize| &text[tokens[at].span.start as usize..tokens[at].span.end as usize];
    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if tokens[at].kind != keyword {
            continue;
        }
        let mut name = String::new();
        let mut i = at + 1;
        while i < tokens.len() && tokens[i].kind == TokenKind::Ident {
            name.push_str(word(i));
            if i + 2 < tokens.len()
                && tokens[i + 1].kind == TokenKind::Dot
                && tokens[i + 2].kind == TokenKind::Ident
            {
                name.push('.');
                i += 2;
            } else {
                break;
            }
        }
        if !name.is_empty() {
            found.push(name);
        }
    }
    found
}

/// The name a file gives itself (`module a.b;`) and the names it imports (`import a.b;`).
fn module_name(text: &str) -> Option<String> {
    named_after(text, TokenKind::Module).into_iter().next()
}

fn imports(text: &str) -> Vec<String> {
    named_after(text, TokenKind::Import)
}

/// The standard library's modules the program imports, and those they import: the compiler in cancho does
/// not take the rest, which it would have to check too.
fn add_imported(files: &mut Vec<(String, String)>) {
    let mut at = 0;
    while at < files.len() {
        for name in imports(&files[at].1) {
            let already =
                files.iter().any(|(_, t)| module_name(t).as_deref() == Some(name.as_str()));
            if already {
                continue;
            }
            if let Some((file, text)) =
                crate::STD.iter().find(|(_, t)| module_name(t).as_deref() == Some(name.as_str()))
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
            "{at}: the compiler in cancho has no code for this construct yet: `{near}`"
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

static FLAG: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The compiler `--compiler` named, for every build of this run.
pub fn set_flag(flag: Option<PathBuf>) {
    if let Some(flag) = flag {
        let _ = FLAG.set(flag);
    }
}

/// Which compiler builds this program: the one named by `--compiler`, else by `CANCHO_COMPILER`, as a path to an executable
/// (the `cancho` that `scripts/build-standalone.sh` writes, or the seed's). The Rust compiler, and the `auto` that built the
/// compiler in cancho with it, went with the Rust code generators (`docs/removing-rust.md`, Phase 5): naming them, or naming
/// nothing, is refused and says what to name.
pub fn choose(_target: Option<&Triple>) -> Result<PathBuf, Failure> {
    let named =
        FLAG.get().cloned().or_else(|| std::env::var_os("CANCHO_COMPILER").map(PathBuf::from));
    match named {
        Some(name) if name == Path::new("rust") || name == Path::new("auto") => {
            Err(usage(format!(
                "`{}` names a compiler the Rust binary no longer has: its code generators are retired (`docs/removing-rust.md` D2, Phase 5); name the path of a compiler written in cancho (`scripts/build-standalone.sh`) with `--compiler <exe>` or `CANCHO_COMPILER`",
                name.display()
            )))
        }
        Some(compiler) => Ok(compiler),
        None => Err(environment(
            "this binary writes no code itself any more (its code generators are retired, `docs/removing-rust.md` D2, Phase 5), and no compiler was named: build one with `scripts/build-standalone.sh` and name it with `--compiler <exe>` or `CANCHO_COMPILER`",
        )),
    }
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
/// What building a program with the compiler in cancho came to: built, or the compiler has no code for something in it
/// yet (what it says is the refusal to give if there is no other compiler to fall back to).
pub enum Built {
    Done,
    NoCode(Failure),
    /// The compiler in cancho stopped without a module, or what it wrote did not build: a bug of its own, not a refusal.
    Broke(Failure),
}

pub fn build(job: Job<'_>) -> Result<Built, Failure> {
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
        let said = refusal(&answer, &files);
        if answer.split_whitespace().nth(1) == Some("SKIP") {
            return Ok(Built::NoCode(said));
        }
        return Err(said);
    }
    if !ended.status.success() || answer.is_empty() {
        return Ok(Built::Broke(environment(format!(
            "the compiler `{}` ended with {} and no module",
            compiler.display(),
            ended.status
        ))));
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
        .args(["-O2", "-Wno-override-module", "-c"])
        .arg(&module)
        .arg("-o")
        .arg(&object)
        .output()
        .map_err(|e| environment(format!("cannot run `{cc}`: {e}")));
    let _ = std::fs::remove_file(&module);
    let made = made?;
    if !made.status.success() {
        return Ok(Built::Broke(environment(format!(
            "`{cc}` could not build the module the compiler in cancho wrote: {}",
            String::from_utf8_lossy(&made.stderr)
        ))));
    }
    if emit == Emit::Exe {
        let linked = crate::link(&object, output, link_libs, link_paths);
        let _ = std::fs::remove_file(&object);
        linked?;
    }
    Ok(Built::Done)
}

#[cfg(test)]
mod tests {
    use super::{imports, module_name};

    #[test]
    fn the_layout_of_a_file_does_not_hide_its_module_or_its_imports() {
        let plain = "edition 5;\nmodule a.b;\nimport std.conns;\nimport std.http;\nfn main() -> int { return 0; }\n";
        let flat = "edition 5 ; module a . b ; import std . conns ; import std . http ; fn main ( ) -> int { return 0 ; }";
        let commented = "// import std.nothing;\nmodule a.b; import std.conns; // import std.nothing;\nimport std . http;\n";
        for text in [plain, flat, commented] {
            assert_eq!(module_name(text).as_deref(), Some("a.b"), "{text}");
            assert_eq!(imports(text), ["std.conns", "std.http"], "{text}");
        }
    }

    #[test]
    fn text_that_looks_like_an_import_is_not_one() {
        let text = "module a;\nfn f() -> [] byte { return \"import std.conns;\"; }\n";
        assert_eq!(module_name(text).as_deref(), Some("a"));
        assert!(imports(text).is_empty());
        assert_eq!(module_name("fn f() -> int { return 0; }"), None);
    }
}
