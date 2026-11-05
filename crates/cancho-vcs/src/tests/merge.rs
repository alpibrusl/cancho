use super::*;
use crate::change::Change;
use crate::object::Store;
use crate::tree::Tree;

fn change(store: &Store, parents: &[&str], time: i64) -> String {
    let tree = Tree::default().save(store).unwrap();
    Change {
        tree,
        parents: parents.iter().map(|p| (*p).to_owned()).collect(),
        author: "a <a@x>".into(),
        time,
        tz: "+0000".into(),
        intent: None,
        decls: vec![],
        edits: vec![],
        attestations: vec![],
        git: None,
    }
    .save(store)
    .unwrap()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cancho-vcs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_merge_base_is_the_latest_best_common_ancestor_and_unrelated_roots_have_none() {
    let dir = scratch("base");
    let (repo, _) = Repo::init(&dir, "main").unwrap();
    let s = &repo.store;
    let root = change(s, &[], 1);
    let a = change(s, &[&root], 2);
    let b = change(s, &[&root], 3);
    let base = merge_base(&repo, &a, &b).unwrap().unwrap();
    assert_eq!((base.id.as_str(), base.candidates), (root.as_str(), 1));
    // A criss-cross: two merges of a and b have two best bases.
    let m1 = change(s, &[&a, &b], 4);
    let m2 = change(s, &[&b, &a], 5);
    let base = merge_base(&repo, &m1, &m2).unwrap().unwrap();
    assert_eq!(base.candidates, 2);
    assert_eq!(base.id, b, "the later of the two");
    assert!(is_ancestor(&repo, &root, &m1).unwrap());
    assert!(!is_ancestor(&repo, &m1, &root).unwrap());
    // Two histories with no change in common.
    let other = change(s, &[], 9);
    assert!(merge_base(&repo, &a, &other).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
