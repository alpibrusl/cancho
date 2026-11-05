use super::*;

#[test]
fn derived_edits_reproduce_the_new_text() {
    let base = "a\nb\nc\nb\nd\ne\n";
    for new in ["a\nb\nc\nB\nd\ne\n", "a\nb\nc\nb\nd\ne\nf\n", "x\nb\nc\nb\nd\nY\n", "e\n"] {
        let edits = derive_edits("f.txt", base, new);
        assert!(!edits.is_empty());
        assert_eq!(apply_edits(base, &edits).unwrap(), new);
    }
}

#[test]
fn an_anchor_is_widened_until_it_is_unique() {
    // `b` occurs twice, so the edit of the second one carries its neighbours.
    let edits = derive_edits("f", "a\nb\nc\nb\nd\n", "a\nb\nc\nB\nd\n");
    assert_eq!(edits.len(), 1);
    assert!(edits[0].old.matches('\n').count() >= 2);
    assert_eq!(edits[0].old.matches("c\nb\n").count(), 1);
}

#[test]
fn not_found_and_ambiguous_have_their_tags() {
    let e = |old: &str| Edit { path: "f".into(), old: old.into(), new: "z".into() };
    assert_eq!(apply_edits("a b a", &[e("q")]).unwrap_err().tag, "anchor-not-found");
    assert_eq!(apply_edits("a b a", &[e("a")]).unwrap_err().tag, "anchor-ambiguous");
    assert_eq!(apply_edits("a b a", &[e("b")]).unwrap(), "a z a");
}

#[test]
fn replaying_onto_a_moved_base_still_lands() {
    let base = "one\ntwo\nthree\n";
    let edits = derive_edits("f", base, "one\n2\nthree\n");
    // The other side added lines elsewhere; the anchor is still unique.
    assert_eq!(
        apply_edits("zero\none\ntwo\nthree\nfour\n", &edits).unwrap(),
        "zero\none\n2\nthree\nfour\n"
    );
}
