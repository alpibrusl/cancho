//! The cancho bootstrap compiler CLI.
//!
//! The v0 compiler is hosted in Rust (an open decision in #1, settled here and
//! recorded in `docs/bootstrap.md`). It compiles one file at a time: M0 has no
//! module system, and a unit is a file.
//!
//! Exit codes are semantic, as they are across the rest of the ecosystem:
//!
//! | code | meaning |
//! |---|---|
//! | 0 | success |
//! | 1 | the program was refused (a located diagnostic was printed) -- including by the compiler's own failure, rule `internal` (`docs/internal-errors.md`) |
//! | 2 | the command line was wrong |
//! | 3 | the environment failed: no linker, unwritable output, unsupported host |
//! | 4 | `test` only: the program built, and at least one `test_*` function failed |
//!
//! `run` is the exception: it replaces its own status with the compiled
//! program's, so `cancho run p.cho` and `cancho build p.cho && ./p` agree.
#![forbid(unsafe_code)]

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use target_lexicon::Triple;

use cancho_syntax::SourceFile;

mod acli;
mod fmt_cli;
mod scratch;
mod selfhost_build;
mod test_cli;
mod vcs_args;
mod vcs_cli;
mod vcs_decl;
mod vcs_git;
mod vcs_record;
mod vcs_repo;

const USAGE: &str = "\
cancho — the bootstrap compiler for the cancho systems dialect

usage:
    cancho build <file.cho>... [-o <output>] [--emit exe|obj] [--std] [--backend llvm] [--target <triple>] [-l <name>]... [-L <path>]... [--compiler <exe>]
    cancho run   <file.cho>... [--std] [--backend llvm] [--target <triple>] [-l <name>]... [-L <path>]...
    cancho test  <file.cho>... [--std] [--backend llvm] [-l <name>]... [-L <path>]...
    cancho fmt   <file.cho|dir>... [--check]
    cancho layout    <file.cho>... [--std]
    cancho print <file.cho> [--output json]
    cancho ingest <file.json> [--output json]
    cancho agent-guidelines
    cancho introspect [--output json]
    cancho skill [--output json] [<out-file>]
    cancho --version

options:
    -o <output>     where to write the result (default: the first input's stem)
    --emit exe|obj  emit a linked executable (default) or a bare object file
    --std           make the standard library's source available
    --compiler <exe>  (build) which compiler builds the program: the path of a `cancho` written in cancho
                      (`scripts/build-standalone.sh`), or `CANCHO_COMPILER`. This binary writes no code itself any more
                      (`docs/removing-rust.md`, Phase 5), so naming none is refused, and so are `rust` and `auto`
    --output json   `print` and `ingest` as data rather than prose
    --backend llvm
                    which backend generates code (default: llvm, since #127).
                    `llvm` is the only one, written by the compiler in cancho
                    (docs/llvm-backend.md); accepted, and ignored here.
    --target <triple>
                    generate code for another target (`--backend llvm` only;
                    docs/wasm.md). Only `wasm32-wasip1` is built so far: it
                    needs `CLANG` (a clang with the wasm32 target), `WASM_LD`,
                    and `WASI_SYSROOT` (a wasi-libc sysroot); `run` uses
                    `WASMTIME` and `WASMTIME_FLAGS` (e.g. `--dir=.`). A builtin
                    with no WASI meaning is refused.
    -l <name>       link `lib<name>` at the final link step (`build`/`run`
                    only); repeatable, passed to `cc` unexamined
    -L <path>       add `<path>` to the linker's search path for the above;
                    repeatable, same treatment

A program is the set of files named on the command line, in any order.
Each file is in a module -- the root, unless it says `module a.b;` -- and
reaches another module's names through `import`. See docs/many-files.md
and docs/modules.md.

`--std` adds the standard library's source, which is compiled into this
binary rather than looked up on disk: no search path, no manifest. It is
not a prelude -- a program still writes `import std.io;` where it uses
one -- and a declaration nothing calls emits nothing. See
docs/standard-library.md.

`test` runs every `fn test_*` in the files named, one process each, and
exits 4 if any failed. A test answers 0 to pass; `std.test`'s `assert`
traps otherwise. The files must not declare `main`: the runner writes
one. See docs/testing.md.

`fmt` rewrites each file (or each `.cho` under a directory) in canonical
layout, keeping its comments, its blank lines and the way it spelled each
literal. `--check` writes nothing and exits 1 if any file would change.
A file it cannot format without moving or losing a comment is left alone
and reported, also exit 1. See docs/formatting.md.

`agent-guidelines` prints AGENTS.md, which is how to write cancho in
one page rather than in 42 documents. Every checked code block in it is
run by the test suite, so a guideline that stops being true is a red
build. See docs/agent-errors.md for what a refusal says as data.

`introspect` prints the full command tree as JSON (ACLI spec §1.2) and
`skill` prints a generated agentskills.io SKILL.md -- both sourced from
one registration in src/acli.rs, so the command list an agent reads
cannot drift from the dispatch table the way a hand-written doc could.
Neither is the exit-code contract: `skill`'s own \"Exit codes\" section
is the ACLI SDK's generic template, not this binary's. `--version`,
`build` and `run` keep the codes at the top of this file.
See docs/agent-cli.md.

`print` renders one parsed file in canonical form. It is the AST-to-text
direction of that same pipeline, not a formatter: comments never reach the
AST, so they are not in the output. `fmt` is the formatter.

`-l`/`-L` are `cc`'s own flags, passed through unexamined: this project
invents no manifest and no dependency resolution, only the ability to
ask the linker for a library beyond libc at all, which nothing here
could do before. `c_ptr` (docs/opaque-pointers.md) lets a foreign
signature name a handle from a library like OpenSSL; this is what
makes that library reachable at link time. See docs/foreign-linking.md.
";

const EXIT_REFUSED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_ENVIRONMENT: u8 = 3;
const EXIT_TEST_FAILED: u8 = 4;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(Failure { message, code }) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

struct Failure {
    message: String,
    code: u8,
}

fn refused(message: impl Into<String>) -> Failure {
    Failure { message: message.into(), code: EXIT_REFUSED }
}

fn usage(message: impl Into<String>) -> Failure {
    Failure { message: format!("{}\n\n{USAGE}", message.into()), code: EXIT_USAGE }
}

fn environment(message: impl Into<String>) -> Failure {
    Failure { message: message.into(), code: EXIT_ENVIRONMENT }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Emit {
    Exe,
    Obj,
}

/// What `build` generates code for: the target triple when it is not the host's (`docs/wasm.md`). The backend is no
/// longer a choice: the Rust code generators are gone (`docs/removing-rust.md`, Phase 5).
#[derive(Clone, Copy)]
struct Codegen<'a> {
    target: Option<&'a Triple>,
}

/// Strip a leading `--output FORMAT` (or `--output=FORMAT`) from `args`,
/// for the two ACLI-native commands (`introspect`, `skill`). Kept
/// separate from `parse_args`'s own `--output json` (a bool flag on
/// `check`/`authority`, unrelated to this): those two commands are not
/// `Invocation`-shaped at all, and this accepts `text`/`json`/`table`
/// rather than just `json`, per the acli crate's own `OutputFormat`.
fn parse_agent_output(args: &[String]) -> Result<(::acli::OutputFormat, Vec<String>), Failure> {
    use std::str::FromStr;
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut format = ::acli::OutputFormat::Text;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--output" {
            let value = it.next().ok_or_else(|| usage("`--output` needs a form"))?;
            format = ::acli::OutputFormat::from_str(value).map_err(usage)?;
        } else if let Some(v) = arg.strip_prefix("--output=") {
            format = ::acli::OutputFormat::from_str(v).map_err(usage)?;
        } else {
            out.push(arg.clone());
        }
    }
    Ok((format, out))
}

fn run(args: &[String]) -> Result<ExitCode, Failure> {
    let Some(command) = args.first() else {
        return Err(usage("no command given"));
    };

    match command.as_str() {
        "--version" | "-V" => {
            println!(
                "cancho {} (rev {}, host {})",
                env!("CARGO_PKG_VERSION"),
                COMPILER_REV,
                Triple::host()
            );
            Ok(ExitCode::SUCCESS)
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        // ACLI built-ins (docs/agent-cli.md): the command tree and a
        // generated skill file, so an agent learns the surface from the
        // binary itself rather than from a doc that can drift.
        "introspect" => {
            let (fmt, _) = parse_agent_output(&args[1..])?;
            acli::build_app().handle_introspect(&fmt);
            Ok(ExitCode::SUCCESS)
        }
        "skill" => {
            let (fmt, rest) = parse_agent_output(&args[1..])?;
            let out_path = rest.first().map(String::as_str);
            acli::build_app().handle_skill(out_path, &fmt);
            Ok(ExitCode::SUCCESS)
        }
        "check" | "authority" | "ids" | "satisfy" => Err(retired(command)),
        // The one command that reads no program: it is a contract with
        // whoever is about to write one.
        "agent-guidelines" => {
            if args.len() > 1 {
                return Err(usage("`agent-guidelines` takes no arguments"));
            }
            let stdout = io::stdout();
            let mut out = stdout.lock();
            match out.write_all(AGENT_GUIDELINES.as_bytes()).and_then(|()| out.flush()) {
                Ok(()) => Ok(ExitCode::SUCCESS),
                // The reader stopped listening, which is their business.
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
                Err(e) => Err(environment(format!("cannot write to stdout: {e}"))),
            }
        }
        // `docs/vcs-publish.md`: a real `.cho` file through
        // `crates/cancho-vcs` for the first time.
        "vcs" => vcs_cli::cmd_vcs(&args[1..]),
        // `docs/many-files.md` §5: printing is about text, and text is
        // what a file is -- so this renders exactly one.
        "print" => {
            let Invocation { inputs, json, .. } = parse_args(&args[1..], false, false)?;
            let [input] = &inputs[..] else {
                return Err(usage("`print` renders one file at a time"));
            };
            let text = std::fs::read_to_string(input)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
            let file = SourceFile::new(input.display().to_string(), text);
            let ast = cancho_syntax::parse(&file.text).map_err(|d| refused(d.render(&file)))?;
            // `--output json` is the data form of the same tree
            // (`docs/structured-ingest.md` §2): the AST as JSON rather than
            // as canonical text, for an agent that reads the one and writes
            // the other.
            if json {
                println!("{}", cancho_syntax::write_json(&cancho_syntax::ast_to_json(&ast)));
                return Ok(ExitCode::SUCCESS);
            }
            print!("{}", cancho_syntax::print(&ast));
            Ok(ExitCode::SUCCESS)
        }
        // `docs/structured-ingest.md`: the other direction of the same
        // pipeline — JSON in, canonical `.cho` text out, with the tree
        // checked against the same vocabulary the encoder writes and a
        // refusal carrying a rule like any other.
        "ingest" => {
            let Invocation { inputs, json: as_data, .. } = parse_args(&args[1..], false, false)?;
            let [input] = &inputs[..] else {
                return Err(usage("`ingest` reads one file at a time"));
            };
            let text = std::fs::read_to_string(input)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
            let file = SourceFile::new(input.display().to_string(), text);
            let build = cancho_syntax::read_json(&file.text)
                .and_then(|json| cancho_syntax::json_to_ast(&json));
            let Ok(ast) = build else {
                // A refusal is data here too (`docs/structured-ingest.md`
                // §3): an agent's first contact with this path is a
                // rejected file, and `--output json` answers the rule, the
                // sentence and the position the way `check` does — the
                // same shape, the same exit code, no new vocabulary.
                let d = build.expect_err("checked above");
                if as_data {
                    let line = 1 + file.text[..d.span.start as usize]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count();
                    let column = d.span.start + 1;
                    let body = format!(
                        "{{\n  \"refused\": [\n    {{\n      \"rule\": \"{}\",\n      \"message\": \"{}\",\n      \"explanation\": \"{}\",\n      \"position\": {{ \"file\": \"{}\", \"line\": {line}, \"column\": {column} }}\n    }}\n  ]\n}}\n",
                        d.rule.tag(),
                        escaped(&d.message),
                        escaped(d.rule.explanation()),
                        escaped(&file.path),
                    );
                    let stdout = io::stdout();
                    let mut out = stdout.lock();
                    let _ = out.write_all(body.as_bytes()).and_then(|()| out.flush());
                    return Ok(ExitCode::from(EXIT_REFUSED));
                }
                return Err(refused(d.render(&file)));
            };
            print!("{}", cancho_syntax::print(&ast));
            Ok(ExitCode::SUCCESS)
        }
        // `docs/formatting.md`.
        "fmt" => fmt_cli::cmd_fmt(&args[1..]),
        // `docs/testing.md` §3.
        "test" => test_cli::cmd_test(&args[1..]),
        // `cancho layout` is the compiler in cancho's (`layoutrep.cho`); the Rust one measured with the
        // Cranelift leaf counts, which went with `cancho-codegen` (`docs/removing-rust.md`, Phase 5).
        "layout" => Err(refused(
            "`cancho layout` is no longer written by the Rust compiler (the Cranelift backend it measured with is retired, `docs/removing-rust.md` D2); use the compiler in cancho",
        )),
        // `docs/package-system.md` §8: the project file.
        "install" | "add" => Err(retired(command)),
        "build" => {
            let Invocation {
                inputs,
                output,
                emit,
                with_std,
                target,
                link_libs,
                link_paths,
                compiler,
                ..
            } = parse_args(&args[1..], true, true)?;
            let output = output.unwrap_or_else(|| default_output(&inputs[0], emit));
            selfhost_build::set_flag(compiler);
            build(
                &inputs,
                &output,
                emit,
                with_std,
                Codegen { target: target.as_ref() },
                &link_libs,
                &link_paths,
            )?;
            Ok(ExitCode::SUCCESS)
        }
        "run" => {
            let Invocation { inputs, with_std, target, link_libs, link_paths, .. } =
                parse_args(&args[1..], false, true)?;
            let dir = scratch::private_temp_dir("cancho-run")
                .map_err(|e| environment(format!("cannot create a temporary directory: {e}")))?;
            let exe =
                dir.join(default_output(&inputs[0], Emit::Exe).file_name().unwrap_or_default());
            let result = build(
                &inputs,
                &exe,
                Emit::Exe,
                with_std,
                Codegen { target: target.as_ref() },
                &link_libs,
                &link_paths,
            )
            .and_then(|()| match target.as_ref() {
                Some(triple) => Err(environment(format!(
                    "`run` cannot execute a `{triple}` program on this host; use `build`"
                ))),
                None => Command::new(&exe)
                    .status()
                    .map_err(|e| environment(format!("cannot run `{}`: {e}", exe.display()))),
            });
            let _ = std::fs::remove_dir_all(&dir);
            let status = result?;
            Ok(ExitCode::from(status.code().unwrap_or(EXIT_ENVIRONMENT as i32) as u8))
        }
        other => Err(usage(format!("unknown command `{other}`"))),
    }
}

/// The standard library's source, compiled into this binary
/// (`docs/standard-library.md` §2).
///
/// Embedded rather than looked up, so `--std` adds **no search path**,
/// no manifest and no build step -- `modules.md` §6 promised none of
/// those, and this keeps the promise by never going near the disk.
///
/// The cost is that the library's version is the compiler's version.
/// For a language at this stage that is the right trade -- one artifact,
/// one thing to install, nothing to resolve -- and §2.1 records it as
/// the first thing to revisit when a package story exists.
/// `AGENTS.md`, compiled into the binary the way the library is.
///
/// `docs/agent-errors.md` §7 named the gap: lex-lang's contract says a
/// downstream repo copies `AGENT_GUIDELINES.md`, and cancho is a
/// different language, so it needs its own. Carried in the binary for
/// the same reason `--std` is — a reader with the compiler needs
/// nothing else, and a file on disk is a file that can be absent.
const AGENT_GUIDELINES: &str = include_str!("../../../AGENTS.md");

// The standard library, one `(name, text)` per file of `std/`, in the order of `std/std.files` (the order `--std` adds them): generated by
// `build.rs`, so a new module of the library is one line of that file and nothing here.
include!(concat!(env!("OUT_DIR"), "/std_table.rs"));

/// What a command line asked for.
///
/// A struct rather than a tuple because the fifth field is where a tuple
/// stops being readable at the call site -- the same reason
/// `resolve_type_at` became a `Resolving`.
struct Invocation {
    inputs: Vec<PathBuf>,
    output: Option<PathBuf>,
    emit: Emit,
    with_std: bool,
    /// `--output json` (`docs/authority.md` §3): the report as data rather
    /// than as prose, for a consumer that checks it against a grant.
    json: bool,
    /// `--target` (`docs/wasm.md`): the triple to generate code for, if not the host's.
    target: Option<Triple>,
    /// `-l <name>` (`docs/foreign-linking.md` §3): libraries to link
    /// beyond libc, in the order given, passed to `cc` unexamined.
    link_libs: Vec<String>,
    /// `-L <path>` (`docs/foreign-linking.md` §3): search paths for the
    /// above, same order, same treatment.
    link_paths: Vec<String>,
    /// `--compiler <exe>` (`docs/self-hosting.md`): build with the compiler written in cancho.
    compiler: Option<PathBuf>,
}

fn parse_args(
    args: &[String],
    allow_output: bool,
    allow_link: bool,
) -> Result<Invocation, Failure> {
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut output = None;
    let mut emit = Emit::Exe;
    let mut with_std = false;
    let mut json = false;
    let mut target: Option<Triple> = None;
    let mut link_libs = Vec::new();
    let mut link_paths = Vec::new();
    let mut compiler: Option<PathBuf> = None;
    let mut it = args.iter();

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" if allow_output => {
                let value = it.next().ok_or_else(|| usage("`-o` needs a path"))?;
                output = Some(PathBuf::from(value));
            }
            "--emit" if allow_output => {
                emit = match it.next().map(String::as_str) {
                    Some("exe") => Emit::Exe,
                    Some("obj") => Emit::Obj,
                    Some(other) => return Err(usage(format!("unknown emit kind `{other}`"))),
                    None => return Err(usage("`--emit` needs a kind")),
                };
            }
            // `docs/standard-library.md` §2. Opt-in, and never an
            // implicit prelude: this decides whether the library's
            // *source* is present, never whether a name is in scope. A
            // program still writes `import std.io;` where it uses it.
            "--std" => with_std = true,
            "--output" => match it.next().map(String::as_str) {
                Some("json") => json = true,
                Some(other) => return Err(usage(format!("unknown output form `{other}`"))),
                None => return Err(usage("`--output` needs a form")),
            },
            "--backend" => match it.next().map(String::as_str) {
                Some("cranelift") => {
                    return Err(usage(
                        "`--backend cranelift` is retired with the Rust compiler's Cranelift backend (`docs/removing-rust.md` D2); the only backend is `llvm`",
                    ));
                }
                Some("llvm") => {}
                Some(other) => return Err(usage(format!("unknown backend `{other}`"))),
                None => return Err(usage("`--backend` needs a name")),
            },
            "--target" => {
                let value = it.next().ok_or_else(|| usage("`--target` needs a triple"))?;
                target = Some(
                    value
                        .parse::<Triple>()
                        .map_err(|e| usage(format!("unknown target `{value}`: {e}")))?,
                );
            }
            "--compiler" if allow_output => {
                let value = it.next().ok_or_else(|| usage("`--compiler` needs a path"))?;
                compiler = Some(PathBuf::from(value));
            }
            "-l" if allow_link => {
                let value = it.next().ok_or_else(|| usage("`-l` needs a library name"))?;
                link_libs.push(value.clone());
            }
            "-L" if allow_link => {
                let value = it.next().ok_or_else(|| usage("`-L` needs a path"))?;
                link_paths.push(value.clone());
            }
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            // `docs/many-files.md` §2: a program is a set of files, named
            // on the command line in any order.
            other => inputs.push(PathBuf::from(other)),
        }
    }

    if inputs.is_empty() {
        return Err(usage("no input file given"));
    }
    Ok(Invocation { inputs, output, emit, with_std, json, target, link_libs, link_paths, compiler })
}

fn default_output(input: &Path, emit: Emit) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default();
    match emit {
        Emit::Exe => PathBuf::from(stem),
        Emit::Obj => PathBuf::from(format!("{}.o", stem.to_string_lossy())),
    }
}

/// Build a program with the compiler this run names (`selfhost_build::choose`): the one written in cancho, as an executable
/// (`--compiler <exe>`, `CANCHO_COMPILER`). The Rust code generators are gone (`docs/removing-rust.md`, Phase 5), so nothing
/// else can write code: a build that names no compiler, or `rust`, or a target the compiler in cancho does not write code
/// for, is refused here with a message that says what to name.
fn build(
    inputs: &[PathBuf],
    output: &Path,
    emit: Emit,
    with_std: bool,
    codegen: Codegen<'_>,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<(), Failure> {
    let compiler = selfhost_build::choose(codegen.target)?;
    let built = selfhost_build::build(selfhost_build::Job {
        compiler: &compiler,
        inputs,
        output,
        emit,
        with_std,
        target: codegen.target,
        link_libs,
        link_paths,
    })?;
    match built {
        selfhost_build::Built::Done => Ok(()),
        // The compiler in cancho says what it cannot do; there is no other compiler to hand the build to.
        selfhost_build::Built::NoCode(failure) | selfhost_build::Built::Broke(failure) => {
            Err(failure)
        }
    }
}

/// Link with the platform C toolchain.
///
/// M0 shells out to `cc` rather than driving a linker itself: the C runtime
/// provides `_start` and `putchar`, and "no C" is a much later goal than "no
/// Rust" (#1).
///
/// `link_libs`/`link_paths` are `-l`/`-L`, passed to `cc` unexamined, in the
/// order given, between the object and `-o` (`docs/foreign-linking.md` §3.1:
/// a library named before the object whose symbols it resolves is a library
/// the linker never looks at, measured against this project's own `ld`).
fn link(
    object: &Path,
    output: &Path,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<(), Failure> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let status = Command::new(&cc)
        .arg(object)
        .args(link_paths.iter().map(|path| format!("-L{path}")))
        .args(link_libs.iter().map(|lib| format!("-l{lib}")))
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| environment(format!("cannot run the linker `{cc}`: {e}")))?;
    if !status.success() {
        return Err(environment(format!("the linker `{cc}` failed with {status}")));
    }
    Ok(())
}

/// The commands that needed the Rust front end's lowering (`cancho-ir`, deleted in `docs/removing-rust.md` Phase 5, step 3). The compiler in cancho
/// answers all of them; this binary says so rather than answer from a front end it no longer has.
fn retired(command: &str) -> Failure {
    Failure {
        message: format!(
            "`cancho {command}` is no longer answered by the Rust binary: its front end (`cancho-ir`) is deleted (`docs/removing-rust.md`, Phase 5); use the compiler in cancho (`scripts/build-standalone.sh`)"
        ),
        code: EXIT_USAGE,
    }
}

/// A JSON string body. Quotes and backslashes are not enough: a control
/// character inside a string is invalid JSON, and a parse error's message
/// once carried the whole rendered excerpt, newlines and all, which no
/// JSON parser would read (`docs/agent-errors.md` §5).
fn escaped(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The commit this compiler was built from (`build.rs`).
const COMPILER_REV: &str = env!("CANCHO_REV_STAMP");
