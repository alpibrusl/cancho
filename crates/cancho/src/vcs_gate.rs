//! The built-in gate of `record`, `revert`, `merge` and `replay`
//! (`docs/vcs-cli.md` sections 3 and 8, R2): `cancho check` of every `.cho`
//! file a change adds or modifies, as the program it belongs to. A file is
//! checked together with the files that declare the modules it imports,
//! transitively, and the standard library when it imports `std`. A module is
//! looked for in the tree first, then in the *module roots* the project has
//! outside the tree: `build/deps` (what `cancho install` fetched), the
//! directory in `CANCHO_PACKAGES` or the `packages` next to the compiler
//! (the convention of `cancho-forge`'s scripts), the directories named by
//! `--gate-path`, and the lines of `.cancho/vcs/gate-path`. Nothing is
//! fetched: a root that is not on disk is not used, and a module found
//! nowhere is the checker's `unknown-name`, with the modules listed.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use cancho_vcs::{Entry, Refusal, Repo, Tree, VcsResult};
use serde_json::{Value, json};

use crate::vcs_args::{Parsed, entry_bytes};

/// Blobs a merge has made and not stored yet (blob id to bytes).
pub(crate) type Pending = HashMap<String, Vec<u8>>;

pub(crate) fn bytes_of(repo: &Repo, pending: &Pending, e: &Entry) -> VcsResult<Vec<u8>> {
    match pending.get(&e.blob) {
        Some(b) => Ok(b.clone()),
        None => entry_bytes(repo, e),
    }
}

/// The directories outside the tree where modules may be found, in the
/// order they are tried.
pub(crate) fn gate_roots(repo: &Repo, p: &Parsed) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = p.values("--gate-path").iter().map(PathBuf::from).collect();
    if let Ok(text) = std::fs::read_to_string(repo.vcs.join("gate-path")) {
        roots.extend(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from));
    }
    roots.push(repo.root.join("build/deps"));
    if let Some(dir) = std::env::var_os("CANCHO_PACKAGES") {
        roots.push(PathBuf::from(dir));
    } else if let Ok(exe) = std::env::current_exe() {
        for up in [exe.parent(), exe.parent().and_then(Path::parent)].into_iter().flatten() {
            roots.push(up.join("packages"));
        }
    }
    let mut seen = BTreeSet::new();
    roots.retain(|r| r.is_dir() && seen.insert(r.clone()));
    roots
}

/// Every `.cho` file under the roots, parsed once, as `module -> files`.
struct External {
    roots: Vec<PathBuf>,
    index: Option<BTreeMap<String, Vec<PathBuf>>>,
}

const MAX_EXTERNAL_FILES: usize = 4000;
const MAX_EXTERNAL_BYTES: u64 = 1 << 20;

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if out.len() >= MAX_EXTERNAL_FILES {
            return;
        }
        if meta.is_dir() {
            // A package's `build` directory holds what it built and a dot
            // directory (`.cancho-vcs`, `.git`) what a tool stored: neither is
            // what the package declares.
            let name = e.file_name();
            let name = name.to_string_lossy();
            if depth < 4 && !name.starts_with('.') && !matches!(&*name, "build" | "target") {
                collect(&path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|x| x == "cho") && meta.len() <= MAX_EXTERNAL_BYTES {
            out.push(path);
        }
    }
}

impl External {
    fn find(&mut self, module: &str) -> Vec<PathBuf> {
        let roots = &self.roots;
        let index = self.index.get_or_insert_with(|| {
            let mut files = Vec::new();
            for r in roots {
                collect(r, 0, &mut files);
            }
            let mut ix: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                let Ok(ast) = cancho_syntax::parse(&text) else { continue };
                for m in &ast.modules {
                    let name: Vec<&str> = m.path.iter().map(|s| ast.name_of(*s)).collect();
                    ix.entry(name.join(".")).or_default().push(f.clone());
                }
            }
            ix
        });
        index.get(module).cloned().unwrap_or_default()
    }
}

/// Every module a tree declares, as `module path -> files`, so an import
/// can be followed to the files that declare it (`docs/modules.md`).
fn module_index(
    repo: &Repo,
    tree: &Tree,
    pending: &Pending,
) -> VcsResult<BTreeMap<String, Vec<String>>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in tree.entries().iter().filter(|e| e.path.ends_with(".cho")) {
        let Ok(text) = String::from_utf8(bytes_of(repo, pending, e)?) else { continue };
        let Ok(ast) = cancho_syntax::parse(&text) else { continue };
        for m in &ast.modules {
            let name: Vec<&str> = m.path.iter().map(|s| ast.name_of(*s)).collect();
            index.entry(name.join(".")).or_default().push(e.path.clone());
        }
    }
    Ok(index)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Src {
    Tree(String),
    External(PathBuf),
}

impl Src {
    fn name(&self) -> String {
        match self {
            Src::Tree(p) => p.clone(),
            Src::External(p) => format!("external:{}", p.display()),
        }
    }
}

/// What the gate checked: the touched `.cho` paths, and the files from
/// outside the tree their programs needed.
pub(crate) struct Gated {
    pub checked: Vec<String>,
    pub external: Vec<String>,
}

impl Gated {
    pub fn json(&self, ran: bool) -> Value {
        json!({"ran": ran, "checked": self.checked, "external": self.external})
    }
}

/// Check the `.cho` files a change adds or modifies. Each is checked as the
/// program it belongs to. The first failure is the refusal, tagged with its rule.
pub(crate) fn gate(
    repo: &Repo,
    tree: &Tree,
    paths: &[String],
    pending: &Pending,
    roots: &[PathBuf],
) -> VcsResult<Gated> {
    let mut out = Gated { checked: Vec::new(), external: Vec::new() };
    let mut index: Option<BTreeMap<String, Vec<String>>> = None;
    let mut ext = External { roots: roots.to_vec(), index: None };
    let mut all_external: BTreeSet<String> = BTreeSet::new();
    for path in paths.iter().filter(|p| p.ends_with(".cho")) {
        if tree.get(path).is_none() {
            continue;
        }
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        let mut queue = vec![Src::Tree(path.clone())];
        let mut with_std = false;
        let mut missing: BTreeSet<String> = BTreeSet::new();
        while let Some(src) = queue.pop() {
            let name = src.name();
            if files.contains_key(&name) {
                continue;
            }
            let text = match &src {
                Src::Tree(p) => {
                    let Some(entry) = tree.get(p) else { continue };
                    String::from_utf8(bytes_of(repo, pending, entry)?)
                        .map_err(|_| gate_failed(p, "program-shape", "the file is not UTF-8"))?
                }
                Src::External(f) => std::fs::read_to_string(f)
                    .map_err(|e| Refusal::io(format!("cannot read {}", f.display()), &e))?,
            };
            let ast = cancho_syntax::parse(&text)
                .map_err(|d| gate_failed(&name, d.rule.tag(), &d.message))?;
            for m in &ast.modules {
                for i in &m.imports {
                    let segs: Vec<&str> = i.path.iter().map(|s| ast.name_of(*s)).collect();
                    if segs.first() == Some(&"std") {
                        with_std = true;
                        continue;
                    }
                    if index.is_none() {
                        index = Some(module_index(repo, tree, pending)?);
                    }
                    // The longest prefix of the import that names a module,
                    // in the tree first and then outside it.
                    let mut found = false;
                    for n in (1..=segs.len()).rev() {
                        let key = segs[..n].join(".");
                        if let Some(fs) = index.as_ref().and_then(|ix| ix.get(&key)) {
                            queue.extend(fs.iter().cloned().map(Src::Tree));
                            found = true;
                            break;
                        }
                        let outside = ext.find(&key);
                        if !outside.is_empty() {
                            queue.extend(outside.into_iter().map(Src::External));
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        missing.insert(segs.join("."));
                    }
                }
            }
            files.insert(name, text);
        }
        for f in files.keys().filter(|n| n.starts_with("external:")) {
            all_external.insert(f.trim_start_matches("external:").to_owned());
        }
        let mut program: Vec<(String, String)> = files.into_iter().collect();
        if with_std {
            program.extend(crate::vcs_cli::std_texts());
        }
        let refs: Vec<(&str, &str)> =
            program.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
        if let Err(diags) = cancho_vcs::check_candidate(&refs) {
            let first = diags.first();
            let rule = first.map_or("internal".to_owned(), |d| d.rule.to_string());
            let message = first.map_or(String::new(), |d| d.message.clone());
            let all: Vec<Value> = diags
                .iter()
                .map(|d| json!({"rule": d.rule.to_string(), "message": d.message}))
                .collect();
            return Err(Refusal::new("gate-failed", format!("{path}: {rule}: {message}")).with(
                json!({"path": path, "rule": rule, "diagnostics": all,
                       "unresolved_modules": missing,
                       "searched": roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>()}),
            ));
        }
        out.checked.push(path.clone());
    }
    out.external = all_external.into_iter().collect();
    Ok(out)
}

pub(crate) fn gate_failed(path: &str, rule: &str, message: &str) -> Refusal {
    Refusal::new("gate-failed", format!("{path}: {rule}: {message}")).with(
        json!({"path": path, "rule": rule, "diagnostics": [{"rule": rule, "message": message}]}),
    )
}
