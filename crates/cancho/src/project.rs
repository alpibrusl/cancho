//! `cancho.toml`, `cancho install`, `cancho add` and `cancho build` with no files (`docs/package-system.md` §8).
//!
//! A project file names the compiler its sources were written for, the dependencies it fetches (each a repository, a full
//! commit hash and the store inside it, §7) and the programs it builds. Nothing here is new machinery: a dependency is
//! fetched and checked by the same `fetch_verified` that `vcs fetch` runs, from the same cache of checkouts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cancho_vcs::{Lock, Origin};
use serde::Deserialize;

use crate::vcs_cli::fetch_verified;
use crate::vcs_dir::lock_of_all;
use crate::vcs_origin;
use crate::{Backend, Emit, Failure, environment, refused, usage};

/// The commit this compiler was built from (`build.rs`).
pub const COMPILER_REV: &str = env!("CANCHO_REV_STAMP");

const FILE: &str = "cancho.toml";
const DEPS_DIR: &str = "build/deps";
const DEFAULT_STORE: &str = ".cancho-vcs";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    package: Package,
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
    #[serde(default, rename = "bin")]
    bins: Vec<Bin>,
    #[serde(default, rename = "test")]
    tests: Vec<TestSet>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    name: String,
    /// The commit of the compiler these sources were written for.
    #[serde(default, rename = "cancho")]
    cancho: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dependency {
    git: String,
    rev: String,
    #[serde(default = "default_store")]
    path: String,
}

fn default_store() -> String {
    DEFAULT_STORE.to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bin {
    name: String,
    sources: Vec<String>,
    #[serde(default)]
    std: bool,
    out: Option<String>,
}

/// A set of files `cancho test` runs together: the `test_*` functions of these files, against the project's dependencies.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestSet {
    name: String,
    sources: Vec<String>,
    #[serde(default)]
    std: bool,
}

impl Dependency {
    fn origin(&self) -> Origin {
        Origin { git: self.git.clone(), rev: self.rev.clone(), path: self.path.clone() }
    }
}

/// The directory holding `cancho.toml`, looking from `start` upwards.
fn find_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|dir| dir.join(FILE).is_file()).map(Path::to_path_buf)
}

fn current_root() -> Result<PathBuf, Failure> {
    let cwd = std::env::current_dir()
        .map_err(|e| environment(format!("cannot read the current directory: {e}")))?;
    find_root(&cwd).ok_or_else(|| {
        refused(format!(
            "no `{FILE}` in `{}` or any directory above it (docs/package-system.md §8)",
            cwd.display()
        ))
    })
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

fn load(root: &Path) -> Result<Project, Failure> {
    let path = root.join(FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| environment(format!("cannot read `{}`: {e}", path.display())))?;
    let project: Project = toml::from_str(&text)
        .map_err(|e| refused(format!("`{}` is not a project file: {e}", path.display())))?;
    if !name_ok(&project.package.name) {
        return Err(refused(format!(
            "`{}`: package name `{}` must be letters, digits, `-`, `_` or `.`",
            path.display(),
            project.package.name
        )));
    }
    if let Some(rev) = &project.package.cancho {
        revision_ok(rev)
            .map_err(|why| refused(format!("`{}`: [package] cancho: {why}", path.display())))?;
    }
    for (name, dep) in &project.dependencies {
        if !name_ok(name) {
            return Err(refused(format!(
                "`{}`: dependency name `{name}` must be letters, digits, `-`, `_` or `.`",
                path.display()
            )));
        }
        dep.origin().validate().map_err(|why| {
            refused(format!("`{}`: [dependencies.{name}]: {why}", path.display()))
        })?;
    }
    let mut seen = BTreeSet::new();
    for bin in &project.bins {
        if !name_ok(&bin.name) || !seen.insert(bin.name.clone()) {
            return Err(refused(format!(
                "`{}`: [[bin]] name `{}` must be unique, of letters, digits, `-`, `_` or `.`",
                path.display(),
                bin.name
            )));
        }
        if bin.sources.is_empty() {
            return Err(refused(format!(
                "`{}`: [[bin]] `{}` has no `sources`",
                path.display(),
                bin.name
            )));
        }
    }
    let mut seen = BTreeSet::new();
    for set in &project.tests {
        if !name_ok(&set.name) || !seen.insert(set.name.clone()) {
            return Err(refused(format!(
                "`{}`: [[test]] name `{}` must be unique, of letters, digits, `-`, `_` or `.`",
                path.display(),
                set.name
            )));
        }
        if set.sources.is_empty() {
            return Err(refused(format!(
                "`{}`: [[test]] `{}` has no `sources`",
                path.display(),
                set.name
            )));
        }
    }
    Ok(project)
}

fn revision_ok(rev: &str) -> Result<(), String> {
    let hex = rev.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if (rev.len() == 40 || rev.len() == 64) && hex {
        Ok(())
    } else {
        Err(format!("must be a full commit hash (40 or 64 lowercase hex digits), found `{rev}`"))
    }
}

/// Refuse a compiler other than the one the project names (§8.4). `ignore` is `--ignore-compiler-rev`.
fn check_compiler(project: &Project, ignore: bool) -> Result<(), Failure> {
    let Some(want) = &project.package.cancho else { return Ok(()) };
    if want == COMPILER_REV {
        return Ok(());
    }
    if ignore {
        eprintln!(
            "note: this project was written for cancho {want}; this is {COMPILER_REV} (--ignore-compiler-rev)"
        );
        return Ok(());
    }
    let why = if COMPILER_REV == "unknown" {
        "this compiler does not know which commit it was built from, so it cannot be checked \
         (build it from a git checkout, or with CANCHO_REV set)"
            .to_owned()
    } else {
        format!("this compiler is {COMPILER_REV}")
    };
    Err(refused(format!(
        "this project was written for cancho {want}, and {why}; install that compiler, change \
         `[package] cancho` after checking the sources against this one, or pass \
         --ignore-compiler-rev"
    )))
}

/// Fetch and check every dependency into `<root>/build/deps`, clearing it first (a fetched file is named by its hash, so
/// one left by an older pin would be a second declaration). Answers the files, in a stable order.
fn install_all(root: &Path, project: &Project) -> Result<Vec<PathBuf>, Failure> {
    let deps = root.join(DEPS_DIR);
    if deps.exists() {
        std::fs::remove_dir_all(&deps)
            .map_err(|e| environment(format!("cannot clear `{}`: {e}", deps.display())))?;
    }
    std::fs::create_dir_all(&deps)
        .map_err(|e| environment(format!("cannot create `{}`: {e}", deps.display())))?;
    let mut files = BTreeSet::new();
    for (name, dep) in &project.dependencies {
        let origin = dep.origin();
        let store = vcs_origin::ensure(&origin).map_err(environment)?;
        let mut lock: Lock = lock_of_all(&store)?;
        lock.set_origin(Some(origin)).map_err(refused)?;
        if lock.is_empty() {
            return Err(refused(format!(
                "dependency `{name}`: nothing is published at `{}` in {} ({})",
                dep.path,
                dep.git,
                short(&dep.rev)
            )));
        }
        let fetched = fetch_verified(&lock, &store, &deps).map_err(|f| Failure {
            message: format!("dependency `{name}`: {}", f.message),
            code: f.code,
        })?;
        println!("installed {name} ({}): {} file(s)", short(&dep.rev), fetched.files.len());
        files.extend(fetched.files);
    }
    Ok(files.into_iter().collect())
}

fn short(rev: &str) -> &str {
    &rev[..rev.len().min(12)]
}

/// Whether `cancho build <args>` is a project build: no files, only `--bin <name>` and `--ignore-compiler-rev`, and a
/// project file to build.
pub fn wants_project(args: &[String]) -> bool {
    only_flags(args, "--bin")
}

/// The same for `cancho test`, whose one value flag is `--test <name>`.
pub fn wants_project_test(args: &[String]) -> bool {
    only_flags(args, "--test")
}

fn only_flags(args: &[String], value_flag: &str) -> bool {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ignore-compiler-rev" => {}
            flag if flag == value_flag => {
                it.next();
            }
            _ => return false,
        }
    }
    std::env::current_dir().ok().and_then(|cwd| find_root(&cwd)).is_some()
}

pub fn cmd_install(args: &[String]) -> Result<ExitCode, Failure> {
    let mut ignore = false;
    for arg in args {
        match arg.as_str() {
            "--ignore-compiler-rev" => ignore = true,
            other => {
                return Err(usage(format!(
                    "`install` takes no argument but `--ignore-compiler-rev`, found `{other}`"
                )));
            }
        }
    }
    let root = current_root()?;
    let project = load(&root)?;
    check_compiler(&project, ignore)?;
    install_all(&root, &project)?;
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_build(args: &[String]) -> Result<ExitCode, Failure> {
    let mut ignore = false;
    let mut only: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ignore-compiler-rev" => ignore = true,
            "--bin" => {
                only = Some(it.next().ok_or_else(|| usage("`--bin` needs a name"))?.clone());
            }
            other => return Err(usage(format!("unexpected `{other}`"))),
        }
    }
    let root = current_root()?;
    let project = load(&root)?;
    check_compiler(&project, ignore)?;
    if project.bins.is_empty() {
        return Err(refused(format!("`{FILE}` has no [[bin]] to build")));
    }
    if let Some(name) = &only {
        if !project.bins.iter().any(|b| &b.name == name) {
            return Err(refused(format!("`{FILE}` has no [[bin]] named `{name}`")));
        }
    }
    let deps = install_all(&root, &project)?;
    for bin in project.bins.iter().filter(|b| only.as_ref().is_none_or(|n| n == &b.name)) {
        let mut inputs = sources_of(&root, &bin.name, &bin.sources)?;
        inputs.extend(deps.iter().cloned());
        let out = root.join(bin.out.clone().unwrap_or_else(|| format!("build/{}", bin.name)));
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| environment(format!("cannot create `{}`: {e}", parent.display())))?;
        }
        crate::build(
            &inputs,
            &out,
            Emit::Exe,
            bin.std,
            crate::Codegen { backend: Backend::Llvm, target: None },
            &[],
            &[],
        )?;
        println!("built {}", out.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// The files of a `[[bin]]`: each `sources` entry that is a file, or every `.cho` directly in it if it is a directory.
fn sources_of(root: &Path, name: &str, sources: &[String]) -> Result<Vec<PathBuf>, Failure> {
    let mut files = BTreeSet::new();
    for entry in sources {
        let path = root.join(entry);
        if path.is_dir() {
            let found: Vec<PathBuf> = std::fs::read_dir(&path)
                .map_err(|e| environment(format!("cannot read `{}`: {e}", path.display())))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "cho"))
                .collect();
            if found.is_empty() {
                return Err(refused(format!("`{name}`: `{entry}` holds no `.cho` file")));
            }
            files.extend(found);
        } else if path.is_file() {
            files.insert(path);
        } else {
            return Err(refused(format!("`{name}`: `{entry}` is not a file or directory")));
        }
    }
    Ok(files.into_iter().collect())
}

/// `cancho test` with no files: install, then run every `[[test]]` (`--test <name>` one), each against the project's
/// dependencies. Every set runs even if an earlier one failed; the result is the first failure's exit code.
pub fn cmd_test(args: &[String]) -> Result<ExitCode, Failure> {
    let mut ignore = false;
    let mut only: Option<String> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ignore-compiler-rev" => ignore = true,
            "--test" => {
                only = Some(it.next().ok_or_else(|| usage("`--test` needs a name"))?.clone());
            }
            other => return Err(usage(format!("unexpected `{other}`"))),
        }
    }
    let root = current_root()?;
    let project = load(&root)?;
    check_compiler(&project, ignore)?;
    if project.tests.is_empty() {
        return Err(refused(format!("`{FILE}` has no [[test]] to run")));
    }
    if let Some(name) = &only {
        if !project.tests.iter().any(|t| &t.name == name) {
            return Err(refused(format!("`{FILE}` has no [[test]] named `{name}`")));
        }
    }
    let deps = install_all(&root, &project)?;
    let mut first_failure: Option<ExitCode> = None;
    for set in project.tests.iter().filter(|t| only.as_ref().is_none_or(|n| n == &t.name)) {
        println!("== test {}", set.name);
        let mut files = sources_of(&root, &set.name, &set.sources)?;
        files.extend(deps.iter().cloned());
        let mut forwarded: Vec<String> =
            files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        if set.std {
            forwarded.push("--std".to_owned());
        }
        let code = crate::test_cli::cmd_test(&forwarded)?;
        if code != ExitCode::SUCCESS && first_failure.is_none() {
            first_failure = Some(code);
        }
    }
    Ok(first_failure.unwrap_or(ExitCode::SUCCESS))
}

pub fn cmd_add(args: &[String]) -> Result<ExitCode, Failure> {
    let mut positional = Vec::new();
    let mut rev: Option<String> = None;
    let mut git_ref: Option<String> = None;
    let mut path: Option<String> = None;
    let mut ignore = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |what: &str| -> Result<String, Failure> {
            it.next().cloned().ok_or_else(|| usage(format!("`{what}` needs a value")))
        };
        match arg.as_str() {
            "--rev" => rev = Some(value("--rev")?),
            "--ref" => git_ref = Some(value("--ref")?),
            "--path" => path = Some(value("--path")?),
            "--ignore-compiler-rev" => ignore = true,
            other if other.starts_with('-') => {
                return Err(usage(format!("unknown option `{other}`")));
            }
            other => positional.push(other.to_owned()),
        }
    }
    let [name, url] = positional.as_slice() else {
        return Err(usage("`add` needs a name and a git url: `cancho add <name> <git-url>`"));
    };
    if !name_ok(name) {
        return Err(usage(format!(
            "`{name}` is not a dependency name (letters, digits, `-`, `_`, `.`)"
        )));
    }
    let root = current_root()?;
    let project = load(&root)?;
    check_compiler(&project, ignore)?;
    if project.dependencies.contains_key(name) {
        return Err(refused(format!(
            "`{FILE}` already has a dependency `{name}`; edit its `rev` to move it"
        )));
    }
    let rev = match (rev, git_ref) {
        (Some(_), Some(_)) => return Err(usage("give `--rev` or `--ref`, not both")),
        (Some(rev), None) => rev,
        (None, reference) => {
            let reference = reference.unwrap_or_else(|| "HEAD".to_owned());
            let hash = vcs_origin::resolve_ref(url, &reference).map_err(environment)?;
            println!("{reference} is {hash}");
            hash
        }
    };
    let dep = Dependency { git: url.clone(), rev, path: path.unwrap_or_else(default_store) };
    for (what, text) in [("git url", &dep.git), ("path", &dep.path)] {
        if text.bytes().any(|b| b == b'"' || b == b'\\' || b.is_ascii_control()) {
            return Err(usage(format!(
                "the {what} cannot contain a quote, a backslash or a control character"
            )));
        }
    }
    let origin = dep.origin();
    origin.validate().map_err(refused)?;
    // The store must be there before the file is touched, and if it is not a store, say what in that repository is.
    let store = vcs_origin::ensure(&origin).map_err(environment)?;
    if !store.join("manifest.json").is_file() {
        let mut stores: Vec<String> = std::fs::read_dir(&store)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().join("manifest.json").is_file())
                    .map(|e| {
                        format!(
                            "{}/{}",
                            dep.path.trim_end_matches('/'),
                            e.file_name().to_string_lossy()
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        stores.sort();
        let hint = if stores.is_empty() {
            String::new()
        } else {
            format!("; it holds stores at: {}. Pick one with --path", stores.join(", "))
        };
        return Err(refused(format!("`{}` in {} is not a store{hint}", dep.path, dep.git)));
    }

    // Append, so comments and layout in the file survive; put it back if the install fails.
    let file = root.join(FILE);
    let before = std::fs::read_to_string(&file)
        .map_err(|e| environment(format!("cannot read `{}`: {e}", file.display())))?;
    let mut after = before.clone();
    if !after.ends_with('\n') {
        after.push('\n');
    }
    after.push_str(&format!(
        "\n[dependencies.{name}]\ngit = \"{}\"\nrev = \"{}\"\npath = \"{}\"\n",
        dep.git, dep.rev, dep.path
    ));
    std::fs::write(&file, &after)
        .map_err(|e| environment(format!("cannot write `{}`: {e}", file.display())))?;
    let result = load(&root).and_then(|project| install_all(&root, &project));
    match result {
        Ok(_) => {
            println!("added {name} ({}) to {FILE}", short(&dep.rev));
            Ok(ExitCode::SUCCESS)
        }
        Err(failure) => {
            let _ = std::fs::write(&file, before);
            Err(failure)
        }
    }
}
