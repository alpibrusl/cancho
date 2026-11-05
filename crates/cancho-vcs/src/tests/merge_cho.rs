use super::*;

const BASE: &str = "fn a(x: int) -> [] int {\n    return x + 1;\n}\n\nfn b(x: int) -> [] int {\n    return x + 2;\n}\n";

fn none() -> BTreeMap<String, Choice> {
    BTreeMap::new()
}

#[test]
fn declarations_edited_on_each_side_merge_even_when_adjacent() {
    let ours = BASE.replace("x + 1", "x + 10");
    let theirs = BASE.replace("x + 2", "x + 20");
    let m = merge_cho(BASE, &ours, &theirs, &none()).unwrap();
    assert!(m.conflicts.is_empty() && m.frame_conflicts.is_empty());
    assert!(m.text.contains("x + 10") && m.text.contains("x + 20"));
}

#[test]
fn the_same_declaration_edited_both_ways_is_a_conflict_that_names_it() {
    let ours = BASE.replace("x + 1", "x + 10");
    let theirs = BASE.replace("x + 1", "x + 11");
    let m = merge_cho(BASE, &ours, &theirs, &none()).unwrap();
    assert_eq!(m.conflicts.len(), 1);
    assert_eq!(m.conflicts[0].key, "fn a");
    assert!(m.text.contains("x + 10"));
    let mut pick = BTreeMap::new();
    pick.insert("fn a".to_owned(), Choice::Theirs);
    let m = merge_cho(BASE, &ours, &theirs, &pick).unwrap();
    assert!(m.conflicts.is_empty() && m.text.contains("x + 11"));
}

#[test]
fn two_declarations_added_at_the_same_place_are_both_kept() {
    let ours = format!("{BASE}\nfn c() -> [] int {{\n    return 3;\n}}\n");
    let theirs = format!("{BASE}\nfn d() -> [] int {{\n    return 4;\n}}\n");
    let m = merge_cho(BASE, &ours, &theirs, &none()).unwrap();
    assert!(m.conflicts.is_empty());
    assert!(m.text.contains("fn c") && m.text.contains("fn d"));
}

#[test]
fn a_reformat_of_one_side_conflicts_with_nothing() {
    let ours = BASE.replace("x + 1", "x+1");
    let theirs = BASE.replace("x + 2", "x + 20");
    let m = merge_cho(BASE, &ours, &theirs, &none()).unwrap();
    assert!(m.conflicts.is_empty());
}

#[test]
fn an_edit_against_a_delete_conflicts() {
    let ours = BASE.replace("x + 1", "x + 10");
    let theirs = "fn b(x: int) -> [] int {\n    return x + 2;\n}\n";
    let m = merge_cho(BASE, &ours, theirs, &none()).unwrap();
    assert_eq!(m.conflicts.len(), 1);
    assert!(m.conflicts[0].theirs.is_none());
}

#[test]
fn a_file_that_does_not_parse_is_left_to_the_text_merge() {
    assert!(merge_cho(BASE, "fn (((", BASE, &none()).is_none());
}

#[test]
fn a_module_file_with_pub_items_splits_and_merges() {
    let base = "module m;\n\npub fn a(x: int) -> [] int {\n    return x + 1;\n}\n\npub fn b(x: int) -> [] int {\n    return x + 2;\n}\n";
    let ours = format!("{base}\npub fn c(x: int) -> [] int {{\n    return x + 3;\n}}\n");
    let theirs = format!("{base}\npub fn d(x: int) -> [] int {{\n    return x + 4;\n}}\n");
    assert!(split(base).is_some(), "base splits");
    assert!(split(&ours).is_some(), "ours splits");
    let m = merge_cho(base, &ours, &theirs, &none()).expect("merges by declaration");
    assert!(m.conflicts.is_empty() && m.frame_conflicts.is_empty());
}
