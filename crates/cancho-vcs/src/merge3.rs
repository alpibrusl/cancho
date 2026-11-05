//! The three-way text merge of `docs/vcs-cli.md` section 6, layer 3: a
//! line-based diff3 over the histogram diff of `diff_text`, written here
//! because the CLI may not run `diff3` or `git merge-file`.
//!
//! A change from the base is a hunk of base lines replaced by other lines.
//! Hunks of the two sides that overlap or touch (share a boundary, which is
//! also git's rule: adjacent edits conflict) form a cluster; a cluster
//! touched by one side takes that side, a cluster both sides changed the
//! same way takes either, and any other is a conflict. A conflict is data,
//! not markers: it carries the three versions of the
//! whole region, and the merged text has "ours" in its place.

use crate::diff_text::{Hunk, diff_lines, split_lines};

/// One region both sides changed differently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictHunk {
    /// Index of the first base line of the region (0-based).
    pub base_start: usize,
    pub base_lines: Vec<String>,
    pub ours_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merged {
    /// The merged text; where there is a conflict, "ours" stands in.
    pub text: String,
    pub conflicts: Vec<ConflictHunk>,
}

/// A policy for a cluster that would be a conflict: return the lines to
/// use instead (the `.cho` layer uses it to keep two added declarations).
pub type Settle<'a> = &'a dyn Fn(&[&str], &[&str], &[&str]) -> Option<Vec<String>>;

pub fn merge_text(base: &str, ours: &str, theirs: &str) -> Merged {
    merge_with(base, ours, theirs, None)
}

pub fn merge_with(base: &str, ours: &str, theirs: &str, settle: Option<Settle<'_>>) -> Merged {
    if ours == theirs || theirs == base {
        return Merged { text: ours.to_owned(), conflicts: vec![] };
    }
    if ours == base {
        return Merged { text: theirs.to_owned(), conflicts: vec![] };
    }
    let (a, o, t) = (split_lines(base), split_lines(ours), split_lines(theirs));
    let ho = diff_lines(&a, &o);
    let ht = diff_lines(&a, &t);
    let mut out = String::new();
    let mut conflicts = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut at = 0; // next base line not yet emitted
    while i < ho.len() || j < ht.len() {
        let start = match (ho.get(i), ht.get(j)) {
            (Some(x), Some(y)) => x.a_start.min(y.a_start),
            (Some(x), None) => x.a_start,
            (None, Some(y)) => y.a_start,
            (None, None) => break,
        };
        // Grow the cluster over every hunk that overlaps or touches it.
        let mut end = start;
        let (i0, j0) = (i, j);
        loop {
            let mut grew = false;
            if let Some(h) = ho.get(i)
                && h.a_start <= end
            {
                end = end.max(h.a_start + h.a_len);
                i += 1;
                grew = true;
            }
            if let Some(h) = ht.get(j)
                && h.a_start <= end
            {
                end = end.max(h.a_start + h.a_len);
                j += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        for l in &a[at..start] {
            out.push_str(l);
        }
        at = end;
        let (co, ct) = (&ho[i0..i], &ht[j0..j]);
        let take_ours = apply(&a, &o, co, start, end);
        let take_theirs = apply(&a, &t, ct, start, end);
        if ct.is_empty() || take_ours == take_theirs {
            out.extend(take_ours.iter().copied());
        } else if co.is_empty() {
            out.extend(take_theirs.iter().copied());
        } else {
            match settle.and_then(|s| s(&a[start..end], &take_ours, &take_theirs)) {
                Some(lines) => lines.iter().for_each(|l| out.push_str(l)),
                None => {
                    let owned = |v: &[&str]| v.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>();
                    conflicts.push(ConflictHunk {
                        base_start: start,
                        base_lines: owned(&a[start..end]),
                        ours_lines: owned(&take_ours),
                        theirs_lines: owned(&take_theirs),
                    });
                    out.extend(take_ours.iter().copied());
                }
            }
        }
    }
    for l in &a[at..] {
        out.push_str(l);
    }
    Merged { text: out, conflicts }
}

/// The side's text for base lines `start..end`, given the side's hunks that
/// fall inside it (all of them start at or after `start` and end at or
/// before `end`, by construction).
fn apply<'a>(
    a: &[&'a str],
    side: &[&'a str],
    hunks: &[Hunk],
    start: usize,
    end: usize,
) -> Vec<&'a str> {
    let mut v = Vec::new();
    let mut at = start;
    for h in hunks {
        v.extend_from_slice(&a[at..h.a_start]);
        v.extend_from_slice(&side[h.b_start..h.b_start + h.b_len]);
        at = h.a_start + h.a_len;
    }
    v.extend_from_slice(&a[at..end]);
    v
}

#[cfg(test)]
#[path = "tests/merge3.rs"]
mod tests;
