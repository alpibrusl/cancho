//! `cancho test`: run the `test_*` functions of a program, one process each.
//!
//! `docs/testing.md` §3. This language has no macros and no reflection, so
//! nothing inside a program can enumerate its own declarations -- the
//! compiler does it from outside: parse each named file, find the
//! functions that look like tests, and write a `main` that runs whichever
//! one the command line names. One build, then one process per test,
//! because a trap kills the process it happens in and a runner has to
//! survive one test's failure to report the next.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use cancho_syntax::SourceFile;
use cancho_syntax::ast::{Ast, FnDecl, Item, ItemId, TypeExpr, TypeId};

use crate::{EXIT_TEST_FAILED, EXIT_USAGE, Emit, Failure, build, environment, parse_args, refused};

/// A capability a test may ask for, as a unique borrow. Anything else a
/// test wants it has to get from one of these, the way `main` would.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cap {
    Heap,
    Io,
}

pub(crate) struct Test {
    /// The module it is declared in, empty for the root.
    module: Vec<String>,
    name: String,
    caps: Vec<Cap>,
}

impl Test {
    fn label(&self) -> String {
        if self.module.is_empty() {
            self.name.clone()
        } else {
            format!("{}.{}", self.module.join("."), self.name)
        }
    }

    /// How the synthetic `main` names it: unqualified in the root module,
    /// through the module's own import otherwise.
    fn call(&self) -> String {
        let args: Vec<&str> = self
            .caps
            .iter()
            .map(|cap| match cap {
                Cap::Heap => "th",
                Cap::Io => "ti",
            })
            .collect();
        let callee = match self.module.last() {
            None => self.name.clone(),
            Some(alias) => format!("{alias}.{}", self.name),
        };
        format!("{callee}({})", args.join(", "))
    }
}

fn shape_error(file: &str, name: &str, why: &str) -> Failure {
    Failure {
        message: format!(
            "{file}: `{name}` looks like a test but {why}\n\n\
             a test is `fn test_x[regions](caps) -> [row] int` with no type parameters, \
             where each parameter is a unique `Heap` or `Io` reference \
             (`docs/testing.md` §3); it answers 0 to pass"
        ),
        code: EXIT_USAGE,
    }
}

/// A plain, unqualified, unapplied name: `Heap`, `Io`, `int`.
fn bare_name(ast: &Ast, ty: TypeId) -> Option<&str> {
    match &ast.types[ty.index()] {
        TypeExpr::Name { name, qualifier: None, args } if args.is_empty() => {
            Some(ast.name_of(*name))
        }
        _ => None,
    }
}

fn classify(ast: &Ast, file: &str, decl: &FnDecl) -> Result<Vec<Cap>, Failure> {
    let name = ast.name_of(decl.name);
    if !decl.generics.is_empty() {
        return Err(shape_error(file, name, "takes a type parameter"));
    }
    if bare_name(ast, decl.ret) != Some("int") {
        return Err(shape_error(file, name, "does not return `int`"));
    }
    let mut caps = Vec::new();
    for param in &decl.params {
        let cap = match &ast.types[param.ty.index()] {
            TypeExpr::Ref { unique: true, inner, .. } => match bare_name(ast, *inner) {
                Some("Heap") => Some(Cap::Heap),
                Some("Io") => Some(Cap::Io),
                _ => None,
            },
            _ => None,
        };
        let Some(cap) = cap else {
            return Err(shape_error(
                file,
                name,
                "has a parameter that is not a unique `Heap` or `Io` reference",
            ));
        };
        if caps.contains(&cap) {
            return Err(shape_error(file, name, "asks for the same capability twice"));
        }
        caps.push(cap);
    }
    Ok(caps)
}

pub(crate) fn discover(inputs: &[PathBuf]) -> Result<Vec<Test>, Failure> {
    let mut tests = Vec::new();
    for input in inputs {
        let text = std::fs::read_to_string(input)
            .map_err(|e| environment(format!("cannot read `{}`: {e}", input.display())))?;
        let file = SourceFile::new(input.display().to_string(), text);
        let ast = cancho_syntax::parse(&file.text).map_err(|d| refused(d.render(&file)))?;
        for (index, item) in ast.items.iter().enumerate() {
            let Item::Fn(decl) = item else { continue };
            let module = ast.module_of(ItemId(index as u32));
            let path: Vec<String> = ast.modules[module as usize]
                .path
                .iter()
                .map(|s| ast.name_of(*s).to_owned())
                .collect();
            let name = ast.name_of(decl.name);
            if name == "main" && path.is_empty() {
                return Err(Failure {
                    message: format!(
                        "{}: `cancho test` supplies its own `main`; a file it runs must not declare one",
                        input.display()
                    ),
                    code: EXIT_USAGE,
                });
            }
            if !name.starts_with("test_") {
                continue;
            }
            let display = input.display().to_string();
            let caps = classify(&ast, &display, decl)?;
            if !path.is_empty() && !decl.public {
                return Err(shape_error(
                    &display,
                    name,
                    "is not `pub`, and the runner reaches it from outside its module",
                ));
            }
            tests.push(Test { module: path, name: name.to_owned(), caps });
        }
    }
    Ok(tests)
}

/// The program that runs one test, chosen by `argv[1]`.
///
/// Every capability `main` is handed is either released or borrowed and
/// then released, so it obeys the same rules any program does; the tests
/// reach the heap and the console the ordinary way, through a borrow.
fn synthesize_main(tests: &[Test]) -> Result<String, Failure> {
    let mut imports: Vec<(String, String)> = Vec::new();
    for test in tests {
        if let Some(alias) = test.module.last() {
            let path = test.module.join(".");
            match imports.iter().find(|(a, _)| a == alias) {
                Some((_, existing)) if *existing != path => {
                    return Err(Failure {
                        message: format!(
                            "two test modules, `{existing}` and `{path}`, both end in `{alias}`; \
                             the runner imports each by its last segment"
                        ),
                        code: EXIT_USAGE,
                    });
                }
                Some(_) => {}
                None => imports.push((alias.clone(), path)),
            }
        }
    }

    let mut source = String::new();
    for (_, path) in &imports {
        source.push_str(&format!("import {path};\n"));
    }
    source.push_str(
        "\n\
         fn runner_selected_test[&a](args: &a Args) -> [args] int {\n\
         \x20   if arg_count(args) < 2 {\n\
         \x20       return 0 - 1;\n\
         \x20   }\n\
         \x20   let text = arg(args, 1);\n\
         \x20   var n = 0;\n\
         \x20   var i = 0;\n\
         \x20   while i < len(text) {\n\
         \x20       n = n * 10 + (int_of(text[i]) - '0');\n\
         \x20       i = i + 1;\n\
         \x20   }\n\
         \x20   return n;\n\
         }\n\
         \n\
         fn main(world: World) -> [] int {\n\
         \x20   let Split { io, ffi, fs, heap, args } = split(world);\n\
         \x20   release(ffi);\n\
         \x20   release(fs);\n\
         \x20   var which = 0 - 1;\n\
         \x20   borrow args as &g in {\n\
         \x20       which = runner_selected_test(g);\n\
         \x20   }\n\
         \x20   release(args);\n\
         \x20   var status = 255;\n\
         \x20   borrow mut heap as &!th in {\n\
         \x20       borrow mut io as &!ti in {\n",
    );
    for (index, test) in tests.iter().enumerate() {
        source.push_str(&format!(
            "            if which == {index} {{\n                status = {};\n            }}\n",
            test.call()
        ));
    }
    source.push_str(
        "        }\n\
         \x20   }\n\
         \x20   release(heap);\n\
         \x20   release(io);\n\
         \x20   // An exit status is one byte: a multiple of 256 would read as a pass.\n\
         \x20   if status != 0 {\n\
         \x20       if status % 256 == 0 {\n\
         \x20           return 1;\n\
         \x20       }\n\
         \x20   }\n\
         \x20   return status;\n\
         }\n",
    );
    Ok(source)
}

fn describe(status: &std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            let name = match signal {
                4 => " (SIGILL, the trap every checked operation ends with)",
                11 => " (SIGSEGV)",
                6 => " (SIGABRT)",
                _ => "",
            };
            return format!("trapped: killed by signal {signal}{name}");
        }
    }
    match status.code() {
        Some(code) => format!("returned {code}, and a test answers 0 to pass"),
        None => "ended without a status".to_owned(),
    }
}

pub fn cmd_test(args: &[String]) -> Result<ExitCode, Failure> {
    let crate::Invocation { inputs, with_std, backend, target, link_libs, link_paths, .. } =
        parse_args(args, false, true)?;
    if target.is_some() {
        // `test` runs each program on this host; a foreign target is not wired up
        // (`docs/wasm.md`), and ignoring the flag would test the wrong build.
        return Err(crate::usage("`test` does not take `--target` yet"));
    }

    let tests = discover(&inputs)?;
    if tests.is_empty() {
        return Err(Failure {
            message: "no `test_*` functions found in the files given\n\n\
                      `cancho test` runs the functions named `test_*` (`docs/testing.md` §3); \
                      a run that found none is not a pass"
                .to_owned(),
            code: EXIT_USAGE,
        });
    }

    let dir = std::env::temp_dir().join(format!("cancho-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", dir.display())))?;
    let result = run_all(&dir, &inputs, &tests, with_std, backend, &link_libs, &link_paths);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

pub(crate) fn run_all(
    dir: &Path,
    inputs: &[PathBuf],
    tests: &[Test],
    with_std: bool,
    backend: crate::Backend,
    link_libs: &[String],
    link_paths: &[String],
) -> Result<ExitCode, Failure> {
    let main_path = dir.join("runner_main.cho");
    std::fs::write(&main_path, synthesize_main(tests)?)
        .map_err(|e| environment(format!("cannot write `{}`: {e}", main_path.display())))?;
    let mut program: Vec<PathBuf> = inputs.to_vec();
    program.push(main_path);
    let exe = dir.join("runner");
    build(
        &program,
        &exe,
        Emit::Exe,
        with_std,
        crate::Codegen { backend, target: None },
        link_libs,
        link_paths,
    )?;

    println!("running {} test{}", tests.len(), if tests.len() == 1 { "" } else { "s" });
    let mut failures: Vec<(String, String, Vec<u8>, Vec<u8>)> = Vec::new();
    for (index, test) in tests.iter().enumerate() {
        let output = Command::new(&exe)
            .arg(index.to_string())
            .output()
            .map_err(|e| environment(format!("cannot run `{}`: {e}", exe.display())))?;
        let label = test.label();
        if output.status.code() == Some(0) {
            println!("test {label} ... ok");
        } else {
            println!("test {label} ... FAILED");
            failures.push((label, describe(&output.status), output.stdout, output.stderr));
        }
    }

    let passed = tests.len() - failures.len();
    if failures.is_empty() {
        println!("\ntest result: ok. {passed} passed; 0 failed");
        return Ok(ExitCode::SUCCESS);
    }
    println!("\nfailures:");
    for (label, why, stdout, stderr) in &failures {
        println!("\n---- {label} ----\n{why}");
        if !stdout.is_empty() {
            println!("stdout:\n{}", String::from_utf8_lossy(stdout));
        }
        if !stderr.is_empty() {
            println!("stderr:\n{}", String::from_utf8_lossy(stderr));
        }
    }
    println!("\ntest result: FAILED. {passed} passed; {} failed", failures.len());
    Ok(ExitCode::from(EXIT_TEST_FAILED))
}
