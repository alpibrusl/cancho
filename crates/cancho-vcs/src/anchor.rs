//! Anchored edit operations (`docs/vcs-cli.md` section 3): the replay-able
//! form of a text edit, `{path, old, new}`, valid only if `old` occurs
//! exactly once. Derived from a diff against the base and checked by
//! applying them, so a recorded list always reproduces the new text from
//! the old.

use crate::change::Edit;
use crate::diff_text::{diff_lines, split_lines};
use crate::error::{Refusal, Result};

/// Files over this many bytes get no edit list: deriving one is quadratic in
/// the worst case and a megabyte of generated text is not an edit anyone
/// replays. Their change is still recorded, as a tree.
pub const MAX_EDIT_BYTES: usize = 1 << 20;

/// Apply the edits of one path to `text`, in order.
pub fn apply_edits(text: &str, edits: &[Edit]) -> Result<String> {
    let mut text = text.to_owned();
    for e in edits {
        if e.old.is_empty() {
            return Err(Refusal::new("anchor-not-found", format!("an empty anchor in `{}`", e.path)));
        }
        let mut found = text.match_indices(e.old.as_str());
        let Some((at, _)) = found.next() else {
            return Err(Refusal::new("anchor-not-found", format!("the old text of an edit is not in `{}`", e.path)));
        };
        if found.next().is_some() {
            return Err(Refusal::new(
                "anchor-ambiguous",
                format!("the old text of an edit occurs more than once in `{}`", e.path),
            ));
        }
        text.replace_range(at..at + e.old.len(), &e.new);
    }
    Ok(text)
}

/// The edits that turn `base` into `new`: one per diff hunk, each widened
/// with the fewest surrounding lines that make its old text unique in
/// `base`. If applying them in order does not give `new` (overlapping
/// anchors), one edit for the whole file instead.
pub fn derive_edits(path: &str, base: &str, new: &str) -> Vec<Edit> {
    if base == new || base.is_empty() || base.len() > MAX_EDIT_BYTES || new.len() > MAX_EDIT_BYTES {
        return Vec::new();
    }
    let (al, bl) = (split_lines(base), split_lines(new));
    let mut edits = Vec::new();
    for h in diff_lines(&al, &bl) {
        let (lo, hi) = (h.a_start, h.a_start + h.a_len);
        let mut k = 0;
        let (from, to) = loop {
            let from = lo.saturating_sub(k);
            let to = (hi + k).min(al.len());
            let old: String = al[from..to].concat();
            if (to > from && base.matches(old.as_str()).count() == 1) || (from == 0 && to == al.len()) {
                break (from, to);
            }
            k += 1;
        };
        let mut newtext: String = al[from..lo].concat();
        newtext.push_str(&bl[h.b_start..h.b_start + h.b_len].concat());
        newtext.push_str(&al[hi..to].concat());
        edits.push(Edit { path: path.to_owned(), old: al[from..to].concat(), new: newtext });
    }
    match apply_edits(base, &edits) {
        Ok(t) if t == new => edits,
        _ => vec![Edit { path: path.to_owned(), old: base.to_owned(), new: new.to_owned() }],
    }
}

#[cfg(test)]
#[path = "tests/anchor.rs"]
mod tests;
