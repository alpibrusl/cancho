use super::*;

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

#[test]
fn changes_in_different_places_merge() {
    let base = lines(10);
    let ours = base.replace("line 2\n", "two\n");
    let theirs = base.replace("line 9\n", "nine\n");
    let m = merge_text(&base, &ours, &theirs);
    assert!(m.conflicts.is_empty());
    assert_eq!(m.text, base.replace("line 2\n", "two\n").replace("line 9\n", "nine\n"));
}

#[test]
fn the_same_change_on_both_sides_is_one_change() {
    let base = lines(6);
    let both = base.replace("line 3\n", "three\n");
    let theirs = both.replace("line 6\n", "six\n");
    let m = merge_text(&base, &both, &theirs);
    assert!(m.conflicts.is_empty());
    assert_eq!(m.text, theirs);
}

#[test]
fn different_changes_to_one_line_conflict_and_are_data() {
    let base = lines(5);
    let ours = base.replace("line 3\n", "ours\n");
    let theirs = base.replace("line 3\n", "theirs\n");
    let m = merge_text(&base, &ours, &theirs);
    assert_eq!(m.text, ours);
    assert_eq!(m.conflicts.len(), 1);
    let c = &m.conflicts[0];
    assert_eq!(c.base_start, 2);
    assert_eq!(c.base_lines, vec!["line 3\n"]);
    assert_eq!(c.ours_lines, vec!["ours\n"]);
    assert_eq!(c.theirs_lines, vec!["theirs\n"]);
}

#[test]
fn adjacent_edits_conflict_as_in_git() {
    let base = lines(5);
    let ours = base.replace("line 2\n", "two\n");
    let theirs = base.replace("line 3\n", "three\n");
    assert_eq!(merge_text(&base, &ours, &theirs).conflicts.len(), 1);
}

#[test]
fn a_delete_against_an_edit_conflicts() {
    let base = lines(5);
    let ours = base.replace("line 3\n", "");
    let theirs = base.replace("line 3\n", "three\n");
    let m = merge_text(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1);
    assert!(m.conflicts[0].ours_lines.is_empty());
}

#[test]
fn a_settle_policy_can_keep_both_insertions() {
    let base = "a\nz\n";
    let (ours, theirs) = ("a\nx\nz\n", "a\ny\nz\n");
    let both: Settle = &|_, o, t| Some(o.iter().chain(t).map(|l| (*l).to_owned()).collect());
    let m = merge_with(base, ours, theirs, Some(both));
    assert!(m.conflicts.is_empty());
    assert!(m.text.contains("x\n") && m.text.contains("y\n"));
}

#[test]
fn a_missing_final_newline_is_a_difference_like_any_other() {
    let m = merge_text("a\nb", "a\nb\n", "a\nb");
    assert_eq!(m.text, "a\nb\n");
    assert!(m.conflicts.is_empty());
}

#[test]
fn insertions_into_a_run_of_repeated_lines_are_seen_at_one_place() {
    // Found by `scripts/vcs_r3.py` on cancho-forge's `cancho.toml`: ours appends one
    // `[[test]]` block, theirs appends the same block and two more; the repeated
    // `std = true` lines let a diff write each insertion at a different offset,
    // and the merge was "clean" with the first block twice. git conflicts.
    let block = |n: &str| format!("\n[[test]]\nname = \"{n}\"\nstd = true\n");
    let base = format!("x\n{}", block("a"));
    let ours = format!("{base}{}", block("run"));
    let theirs = format!("{base}{}{}", block("run"), block("more"));
    let m = merge_text(&base, &ours, &theirs);
    assert_eq!(m.conflicts.len(), 1, "{}", m.text);
}
