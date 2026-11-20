//! The declaration view of a `.cho` file for `diff`, `record` and `blame`
//! (`docs/vcs-cli.md` section 4): what `cancho ids` reports per declaration,
//! compared across two versions of a file. There is none now (see `decls_of`).

use std::collections::BTreeMap;

use cancho_vcs::DeclChange;

pub(crate) type Decls = BTreeMap<String, (String, String)>;

/// No declaration view: the Rust binary has no parser or identity encoder any more (`cancho-syntax` and `cancho-id` are
/// deleted, `docs/removing-rust.md` Phase 5), so every file is a text file to `diff`, `record` and `blame`.
pub(crate) fn decls_of(_source: &str) -> Option<Decls> {
    None
}

/// Declaration changes from `old` to `new` (a missing side is an empty file).
pub(crate) fn diff_decls(path: &str, old: Option<&Decls>, new: Option<&Decls>) -> Vec<DeclChange> {
    let empty = Decls::new();
    let (old, new) = (old.unwrap_or(&empty), new.unwrap_or(&empty));
    let mut out = Vec::new();
    let mut push = |name: &str, kind: &str| {
        out.push(DeclChange {
            path: path.to_owned(),
            name: name.to_owned(),
            kind: kind.to_owned(),
        });
    };
    for (name, (sig, body)) in new {
        match old.get(name) {
            None => push(name, "added"),
            Some((osig, _)) if osig != sig => push(name, "signature-changed"),
            Some((_, obody)) if obody != body => push(name, "body-changed"),
            Some(_) => {}
        }
    }
    for name in old.keys() {
        if !new.contains_key(name) {
            push(name, "removed");
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
