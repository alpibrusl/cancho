use super::*;

fn apply(a: &[&str], b: &[&str], hunks: &[Hunk]) -> String {
    let mut out = String::new();
    let mut ai = 0;
    for h in hunks {
        for l in &a[ai..h.a_start] {
            out.push_str(l);
        }
        for l in &b[h.b_start..h.b_start + h.b_len] {
            out.push_str(l);
        }
        ai = h.a_start + h.a_len;
    }
    for l in &a[ai..] {
        out.push_str(l);
    }
    out
}

#[test]
fn hunks_rebuild_the_new_text() {
    let cases = [
        ("", ""),
        ("a\nb\nc\n", "a\nb\nc\n"),
        ("a\nb\nc\n", "a\nx\nc\n"),
        ("a\nb\nc\n", "b\nc\n"),
        ("a\nb\nc", "a\nb\nc\n"),
        ("{\n}\n{\n}\n", "{\n  x\n}\n{\n}\n{\n}\n"),
        ("x\ny\n", "p\nq\nr\n"),
    ];
    for (a, b) in cases {
        let (al, bl) = (split_lines(a), split_lines(b));
        assert_eq!(apply(&al, &bl, &diff_lines(&al, &bl)), b, "{a:?} -> {b:?}");
    }
}

#[test]
fn histogram_prefers_the_rare_line() {
    // A common `}` must not anchor the match; the unique `fn two` does.
    let a = "fn one() {\n}\nfn two() {\n}\n";
    let b = "fn one() {\n}\nfn new() {\n}\nfn two() {\n}\n";
    let (al, bl) = (split_lines(a), split_lines(b));
    let h = diff_lines(&al, &bl);
    assert_eq!(h, vec![Hunk { a_start: 2, a_len: 0, b_start: 2, b_len: 2 }]);
}

#[test]
fn unified_has_context_and_counts() {
    let a = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
    let b = "1\n2\n3\n4\nFIVE\n6\n7\n8\n9\n";
    let u = unified(a, b, 2);
    assert_eq!(u.len(), 1);
    assert_eq!((u[0].old_start, u[0].old_lines, u[0].new_start, u[0].new_lines), (3, 5, 3, 5));
    let ops: String = u[0].lines.iter().map(|(c, _)| *c).collect();
    assert_eq!(ops, "  -+  ");
}

#[test]
fn a_missing_final_newline_is_a_difference() {
    let u = unified("a\nb", "a\nb\n", 1);
    assert_eq!(u.len(), 1);
    assert!(u[0].lines.iter().any(|(c, l)| *c == '-' && l == "b"));
}
