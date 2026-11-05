//! The line diff shown to a reader (`docs/vcs-cli.md` section 4): the
//! histogram algorithm (the one `git diff --histogram` and jgit use),
//! written here because the CLI may not run `diff`. Lines keep their
//! terminator, so "no newline at end of file" is a difference like any
//! other. A region with no line that occurs 64 times or fewer is shown
//! as one replacement rather than searched further (git falls back to
//! Myers there), and an input of more than 200,000 lines is one
//! replacement: both are display choices, never a wrong diff.

use std::collections::HashMap;

const MAX_OCCURRENCES: usize = 64;
const MAX_LINES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub a_start: usize,
    pub a_len: usize,
    pub b_start: usize,
    pub b_len: usize,
}

pub fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The differing line ranges between `a` and `b`, in order.
pub fn diff_lines(a: &[&str], b: &[&str]) -> Vec<Hunk> {
    if a.len() > MAX_LINES || b.len() > MAX_LINES {
        return if a == b {
            vec![]
        } else {
            vec![Hunk { a_start: 0, a_len: a.len(), b_start: 0, b_len: b.len() }]
        };
    }
    let mut hunks = Vec::new();
    let mut work = vec![(0, a.len(), 0, b.len())];
    while let Some((mut a0, mut a1, mut b0, mut b1)) = work.pop() {
        while a0 < a1 && b0 < b1 && a[a0] == b[b0] {
            a0 += 1;
            b0 += 1;
        }
        while a0 < a1 && b0 < b1 && a[a1 - 1] == b[b1 - 1] {
            a1 -= 1;
            b1 -= 1;
        }
        if a0 == a1 && b0 == b1 {
            continue;
        }
        if a0 == a1 || b0 == b1 {
            hunks.push(Hunk { a_start: a0, a_len: a1 - a0, b_start: b0, b_len: b1 - b0 });
            continue;
        }
        match best_match(a, b, a0, a1, b0, b1) {
            Some((i, j, len)) => {
                work.push((a0, i, b0, j));
                work.push((i + len, a1, j + len, b1));
            }
            None => hunks.push(Hunk { a_start: a0, a_len: a1 - a0, b_start: b0, b_len: b1 - b0 }),
        }
    }
    hunks.sort_by_key(|h| (h.a_start, h.b_start));
    hunks
}

/// The common block whose first line is rarest in `a`, longest among ties.
fn best_match(
    a: &[&str],
    b: &[&str],
    a0: usize,
    a1: usize,
    b0: usize,
    b1: usize,
) -> Option<(usize, usize, usize)> {
    let mut at: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, line) in a.iter().enumerate().take(a1).skip(a0) {
        let v = at.entry(line).or_default();
        if v.len() <= MAX_OCCURRENCES {
            v.push(i);
        }
    }
    let mut best: Option<(usize, usize, usize, usize)> = None; // count, i, j, len
    for j in b0..b1 {
        let Some(positions) = at.get(b[j]) else { continue };
        let count = positions.len();
        if count > MAX_OCCURRENCES || best.is_some_and(|(c, ..)| count > c) {
            continue;
        }
        for &i in positions {
            let (mut si, mut sj) = (i, j);
            while si > a0 && sj > b0 && a[si - 1] == b[sj - 1] {
                si -= 1;
                sj -= 1;
            }
            let (mut ei, mut ej) = (i + 1, j + 1);
            while ei < a1 && ej < b1 && a[ei] == b[ej] {
                ei += 1;
                ej += 1;
            }
            let len = ei - si;
            if best.is_none_or(|(c, _, _, l)| count < c || (count == c && len > l)) {
                best = Some((count, si, sj, len));
            }
        }
    }
    best.map(|(_, i, j, l)| (i, j, l))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    /// `' '`, `'-'` or `'+'` and the line (terminator kept).
    pub lines: Vec<(char, String)>,
}

/// Unified-diff hunks with `context` lines around each change; hunks whose
/// context touches are joined. Line numbers are 1-based as in `diff -u`.
pub fn unified(a: &str, b: &str, context: usize) -> Vec<UnifiedHunk> {
    let (al, bl) = (split_lines(a), split_lines(b));
    let hunks = diff_lines(&al, &bl);
    let mut out: Vec<UnifiedHunk> = Vec::new();
    let mut group: Vec<Hunk> = Vec::new();
    let flush = |group: &mut Vec<Hunk>, out: &mut Vec<UnifiedHunk>| {
        let (Some(first), Some(last)) = (group.first().copied(), group.last().copied()) else {
            return;
        };
        let a_from = first.a_start.saturating_sub(context);
        let b_from = first.b_start - (first.a_start - a_from);
        let a_to = (last.a_start + last.a_len + context).min(al.len());
        let mut lines = Vec::new();
        let mut ai = a_from;
        for h in group.iter() {
            while ai < h.a_start {
                lines.push((' ', al[ai].to_owned()));
                ai += 1;
            }
            for l in &al[h.a_start..h.a_start + h.a_len] {
                lines.push(('-', (*l).to_owned()));
            }
            for l in &bl[h.b_start..h.b_start + h.b_len] {
                lines.push(('+', (*l).to_owned()));
            }
            ai = h.a_start + h.a_len;
        }
        while ai < a_to {
            lines.push((' ', al[ai].to_owned()));
            ai += 1;
        }
        let old_lines = lines.iter().filter(|(c, _)| *c != '+').count();
        let new_lines = lines.iter().filter(|(c, _)| *c != '-').count();
        out.push(UnifiedHunk {
            old_start: if old_lines == 0 { a_from } else { a_from + 1 },
            old_lines,
            new_start: if new_lines == 0 { b_from } else { b_from + 1 },
            new_lines,
            lines,
        });
        group.clear();
    };
    for h in hunks {
        if let Some(prev) = group.last()
            && h.a_start > prev.a_start + prev.a_len + 2 * context
        {
            flush(&mut group, &mut out);
        }
        group.push(h);
    }
    flush(&mut group, &mut out);
    out
}

/// Text form of `unified`.
pub fn render_unified(old_name: &str, new_name: &str, hunks: &[UnifiedHunk]) -> String {
    if hunks.is_empty() {
        return String::new();
    }
    let mut s = format!("--- {old_name}\n+++ {new_name}\n");
    for h in hunks {
        s.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            h.old_start, h.old_lines, h.new_start, h.new_lines
        ));
        for (c, l) in &h.lines {
            s.push(*c);
            s.push_str(l);
            if !l.ends_with('\n') {
                s.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    s
}

#[cfg(test)]
#[path = "tests/diff_text.rs"]
mod tests;
