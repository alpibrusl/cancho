//! The `.cho` layer of a merge (`docs/vcs-cli.md` section 6, layer 2): a file
//! changed on both sides is merged by declaration. A declaration changed on
//! one side takes that side; changed on both, differently, is a conflict
//! that names it; two sides that each add a declaration at the same place
//! keep both; a pure reformat of a function (same identity, same comments)
//! conflicts with nothing.
//!
//! How: each top-level item (with the `//` comment lines directly above it)
//! becomes one placeholder line `\x01<key>\x02<hash of its text>` in a
//! skeleton of the file; the skeleton is merged by the line diff3 of
//! `merge3`, with a policy that settles any region made only of placeholder
//! lines declaration by declaration; the placeholders are then replaced by
//! the text they stand for. The text between declarations (imports, module
//! lines, loose comments) is merged as lines, so two sides adding an import
//! each is the line merge's business, not this layer's.
//!
//! The layer's own limit, said plainly: layout is kept from the sides, not
//! re-derived by the canonical printer. A file reformatted as a whole
//! conflicts on every declaration both sides edited, unless the edit left a
//! declaration's identity unchanged.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use cancho_syntax::ast::{Item, ItemId};

use crate::merge3::{ConflictHunk, Settle, merge_with};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclConflict {
    pub key: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

/// How a declaration conflict was settled by `resolve`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Choice {
    Ours,
    Theirs,
    Base,
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoMerge {
    /// The merged file; conflicting declarations are "ours" unless chosen.
    pub text: String,
    pub conflicts: Vec<DeclConflict>,
    /// Conflicts of the text between declarations (not of a declaration).
    pub frame_conflicts: Vec<ConflictHunk>,
}

struct Unit {
    key: String,
    text: String,
    id: Option<String>,
}

/// A file as its declarations and a skeleton, or `None` if it does not parse.
fn split(source: &str) -> Option<(Vec<Unit>, String)> {
    let ast = cancho_syntax::parse(source).ok()?;
    let ids = cancho_id::identify(&ast);
    let mut fn_ids: HashMap<(String, String), String> = HashMap::new();
    for f in &ids.functions {
        fn_ids.insert(
            (f.module.clone(), f.name.clone()),
            format!("{}{}", f.sig.to_hex(), f.body.to_hex()),
        );
    }
    let mut spans = Vec::new();
    for n in 0..ast.items.len() as u32 {
        let id = ItemId(n);
        let module = ast.modules.get(ast.module_of(id) as usize);
        let module: String = module
            .map(|m| m.path.iter().map(|s| ast.name_of(*s)).collect::<Vec<_>>().join("."))
            .unwrap_or_default();
        let q = |name: &str| {
            if module.is_empty() { name.to_owned() } else { format!("{module}.{name}") }
        };
        let (key, uid) = match &ast.items[n as usize] {
            Item::Fn(f) => {
                let name = ast.name_of(f.name).to_owned();
                let uid = fn_ids.get(&(module.clone(), name.clone())).cloned();
                (format!("fn {}", q(&name)), uid)
            }
            Item::Extern(e) => (format!("extern {}", q(ast.name_of(e.name))), None),
            Item::Struct(s) => (format!("type {}", q(ast.name_of(s.name))), None),
            Item::Enum(e) => (format!("type {}", q(ast.name_of(e.name))), None),
            Item::Static(s) => (format!("static {}", q(ast.name_of(s.name))), None),
        };
        let sp = ast.item_span(id);
        spans.push((key, uid, sp.start as usize, sp.end as usize));
    }
    // Item spans must be in order and inside the source, or there is no
    // sound way to cut the file along them.
    let mut units = Vec::new();
    let mut skeleton = String::new();
    let mut at = 0usize;
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (key, uid, s, e) in spans {
        if s < at
            || e < s
            || e > source.len()
            || !source.is_char_boundary(s)
            || !source.is_char_boundary(e)
        {
            return None;
        }
        // Back to the start of the line, then over the comment lines above.
        let mut start = source[..s].rfind('\n').map_or(0, |i| i + 1);
        // The span starts after `pub`, which belongs to the item.
        if !matches!(source[start..s].trim(), "" | "pub") {
            return None; // something else shares the item's first line
        }
        while start > at {
            let prev = source[..start - 1].rfind('\n').map_or(0, |i| i + 1);
            if prev >= at && source[prev..start].trim_start().starts_with("//") {
                start = prev;
            } else {
                break;
            }
        }
        let end = match source[e..].find('\n') {
            Some(i) if source[e..e + i].trim().is_empty() => e + i + 1,
            None if source[e..].trim().is_empty() => source.len(),
            _ => return None, // code follows the item on its last line
        };
        let n = seen.entry(key.clone()).or_insert(0);
        *n += 1;
        let key = if *n == 1 { key } else { format!("{key}#{n}") };
        skeleton.push_str(&source[at..start]);
        let text = source[start..end].to_owned();
        skeleton.push_str(&placeholder(&key, &text));
        units.push(Unit { key, text, id: uid });
        at = end;
    }
    skeleton.push_str(&source[at..]);
    Some((units, skeleton))
}

fn unit_of<'a>(us: &'a [Unit], k: &HashMap<String, usize>, key: &str) -> Option<&'a Unit> {
    k.get(key).map(|&i| &us[i])
}

fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex()[..16].to_owned()
}

fn placeholder(key: &str, text: &str) -> String {
    format!("\u{1}{key}\u{2}{}\n", hash(text))
}

fn parse_placeholder(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('\u{1}')?.strip_suffix('\n')?;
    rest.split_once('\u{2}')
}

fn comments(text: &str) -> Vec<&str> {
    text.lines().map(str::trim).filter(|l| l.starts_with("//")).collect()
}

/// Two versions of one declaration are the same if their text is, or if
/// their identity is and nothing a reader wrote in a comment differs.
fn same(a: Option<&Unit>, b: Option<&Unit>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.text == y.text
                || (x.id.is_some() && x.id == y.id && comments(&x.text) == comments(&y.text))
        }
        _ => false,
    }
}

/// `None` when any of the three does not parse (the caller merges as text).
pub fn merge_cho(
    base: &str,
    ours: &str,
    theirs: &str,
    choices: &BTreeMap<String, Choice>,
) -> Option<ChoMerge> {
    let (bu, bs) = split(base)?;
    let (ou, os) = split(ours)?;
    let (tu, ts) = split(theirs)?;
    let by_key = |us: &'_ [Unit]| -> HashMap<String, usize> {
        us.iter().enumerate().map(|(i, u)| (u.key.clone(), i)).collect()
    };
    let (bk, ok, tk) = (by_key(&bu), by_key(&ou), by_key(&tu));
    let texts: HashMap<String, String> =
        bu.iter().chain(&ou).chain(&tu).map(|u| (hash(&u.text), u.text.clone())).collect();
    let conflicts: RefCell<BTreeMap<String, DeclConflict>> = RefCell::new(BTreeMap::new());

    // The declaration-by-declaration policy for a region made of placeholders.
    let settle = |b: &[&str], o: &[&str], t: &[&str]| -> Option<Vec<String>> {
        // A region of placeholders and blank lines is settled here; anything
        // else (an import, a loose comment) is the line merge's conflict.
        let keys = |ls: &[&str]| -> Option<Vec<String>> {
            let mut out = Vec::new();
            for l in ls {
                match parse_placeholder(l) {
                    Some((k, _)) => out.push(k.to_owned()),
                    None if l.trim().is_empty() => {}
                    None => return None,
                }
            }
            Some(out)
        };
        let (bks, oks, tks) = (keys(b)?, keys(o)?, keys(t)?);
        let spaced = [b, o, t].iter().any(|ls| ls.iter().any(|l| l.trim().is_empty()));
        let mut order: Vec<String> = Vec::new();
        for k in oks.iter().chain(&tks).chain(&bks) {
            if !order.contains(k) {
                order.push(k.clone());
            }
        }
        let mut out = Vec::new();
        for key in order {
            let (b1, o1, t1) =
                (unit_of(&bu, &bk, &key), unit_of(&ou, &ok, &key), unit_of(&tu, &tk, &key));
            let chosen: Option<Option<&Unit>> = if same(o1, t1) {
                Some(o1.or(t1))
            } else if same(o1, b1) {
                Some(t1)
            } else if same(t1, b1) {
                Some(o1)
            } else {
                None
            };
            match chosen {
                Some(Some(u)) => {
                    if spaced && !out.is_empty() {
                        out.push("\n".to_owned());
                    }
                    out.push(placeholder(&key, &u.text));
                }
                Some(None) => {}
                None => {
                    conflicts.borrow_mut().insert(
                        key.clone(),
                        DeclConflict {
                            key: key.clone(),
                            base: b1.map(|u| u.text.clone()),
                            ours: o1.map(|u| u.text.clone()),
                            theirs: t1.map(|u| u.text.clone()),
                        },
                    );
                    if spaced && !out.is_empty() {
                        out.push("\n".to_owned());
                    }
                    out.push(format!("\u{1}{key}\u{3}\n"));
                }
            }
        }
        Some(out)
    };
    let settle_ref: Settle<'_> = &settle;
    let merged = merge_with(&bs, &os, &ts, Some(settle_ref));
    let mut text = String::new();
    for line in merged.text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix('\u{1}').and_then(|r| r.strip_suffix('\n')) {
            if let Some((key, _)) = rest.split_once('\u{3}') {
                let c = conflicts.borrow();
                let c = c.get(key)?;
                let chosen = match choices.get(key) {
                    Some(Choice::Ours) | None => c.ours.clone(),
                    Some(Choice::Theirs) => c.theirs.clone(),
                    Some(Choice::Base) => c.base.clone(),
                    Some(Choice::Text(t)) => Some(t.clone()),
                };
                if let Some(t) = chosen {
                    text.push_str(&t);
                }
            } else if let Some((_, h)) = rest.split_once('\u{2}') {
                text.push_str(texts.get(h)?);
            } else {
                return None;
            }
        } else {
            text.push_str(line);
        }
    }
    // A declaration edited on one side and deleted by the other reaches
    // `settle` as a region the two sides changed differently, so it is a
    // declaration conflict like any other.
    let open: Vec<DeclConflict> =
        conflicts.into_inner().into_values().filter(|c| !choices.contains_key(&c.key)).collect();
    if cancho_syntax::parse(&text).is_err() {
        return None;
    }
    Some(ChoMerge { text, conflicts: open, frame_conflicts: merged.conflicts })
}

#[cfg(test)]
#[path = "tests/merge_cho.rs"]
mod tests;
