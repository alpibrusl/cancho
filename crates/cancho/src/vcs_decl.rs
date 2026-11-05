//! The declaration view of a `.cho` file for `diff`, `record` and `blame`
//! (`docs/vcs-cli.md` section 4): what `cancho ids` reports per declaration,
//! compared across two versions of a file. A file that does not parse has
//! no declaration view; it is a text file to every command, and the gate
//! (not this module) is what refuses it.

use std::collections::BTreeMap;

use cancho_vcs::DeclChange;

pub(crate) type Decls = BTreeMap<String, (String, String)>;

pub(crate) fn decls_of(source: &str) -> Option<Decls> {
    let ast = cancho_syntax::parse(source).ok()?;
    let ids = cancho_id::identify(&ast);
    let mut out = Decls::new();
    for f in &ids.functions {
        let key = if f.module.is_empty() { format!("fn {}", f.name) } else { format!("fn {}.{}", f.module, f.name) };
        out.insert(key, (f.sig.to_hex(), f.body.to_hex()));
    }
    for t in &ids.types {
        out.insert(format!("type {}", t.name), (t.id.to_hex(), t.id.to_hex()));
    }
    Some(out)
}

/// Declaration changes from `old` to `new` (a missing side is an empty file).
pub(crate) fn diff_decls(path: &str, old: Option<&Decls>, new: Option<&Decls>) -> Vec<DeclChange> {
    let empty = Decls::new();
    let (old, new) = (old.unwrap_or(&empty), new.unwrap_or(&empty));
    let mut out = Vec::new();
    let mut push = |name: &str, kind: &str| {
        out.push(DeclChange { path: path.to_owned(), name: name.to_owned(), kind: kind.to_owned() });
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
