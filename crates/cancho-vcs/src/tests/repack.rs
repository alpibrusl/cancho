use super::*;
use crate::object::object_id;
use crate::pack::tests::scratch;
use crate::tree::{Entry, Mode};

/// A store with `n` versions of one file, each in a tree, plus one blob in
/// V1's raw layout.
fn store_with_history(n: usize) -> (Store, Vec<(Kind, String)>) {
    let vcs = scratch("gc");
    let s = Store::new(&vcs);
    let mut ids = Vec::new();
    let mut text: String =
        (0..800).map(|i| format!("pub fn f{i}(n: int) -> int {{ n }}\n")).collect();
    for v in 0..n {
        text.insert_str(text.len() / 2, &format!("// version {v}\n"));
        let b = s.put(Kind::Blob, text.as_bytes()).unwrap();
        let other = s.put(Kind::Blob, format!("notes {v}\n").as_bytes()).unwrap();
        let t = Tree::new(vec![
            Entry { path: "src/a.cho".into(), mode: Mode::File, blob: b.clone() },
            Entry { path: "notes.md".into(), mode: Mode::File, blob: other.clone() },
        ])
        .unwrap()
        .save(&s)
        .unwrap();
        ids.extend([(Kind::Blob, b), (Kind::Blob, other), (Kind::Tree, t)]);
    }
    // V1 wrote objects uncompressed under their bare name; they still count.
    let raw = b"an object written by V1\n";
    let id = object_id(Kind::Blob, raw);
    let p = s.path(Kind::Blob, &id);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, raw).unwrap();
    ids.push((Kind::Blob, id));
    (s, ids)
}

fn all_readable(s: &Store, ids: &[(Kind, String)]) {
    for (k, id) in ids {
        let b = s.get(*k, id).unwrap_or_else(|e| panic!("{} {id}: {e}", k.name()));
        assert_eq!(object_id(*k, &b), *id);
        assert!(s.has(*k, id));
    }
}

#[test]
fn loose_objects_are_compressed_and_v1_ones_still_read() {
    let (s, ids) = store_with_history(3);
    all_readable(&s, &ids);
    let (_, id) = &ids[0];
    assert!(s.zpath(Kind::Blob, id).is_file(), "put writes `<id>.z`");
    let z = fs::read(s.zpath(Kind::Blob, id)).unwrap();
    assert!(z.len() < s.get(Kind::Blob, id).unwrap().len() / 3);
    // `list` gives ids, not file names, for both layouts.
    let blobs = s.list(Kind::Blob).unwrap();
    assert!(blobs.iter().all(|i| is_id(i)), "{blobs:?}");
    assert_eq!(blobs.len(), 7);
    // A damaged compressed file is corrupt, not missing and not trusted.
    fs::write(s.zpath(Kind::Blob, id), b"tampered").unwrap();
    assert_eq!(s.get(Kind::Blob, id).unwrap_err().tag, "object-corrupt");
}

#[test]
fn gc_packs_with_deltas_and_is_idempotent() {
    let (s, ids) = store_with_history(30);
    let dry = gc(&s, true).unwrap();
    assert!(dry.dry_run && dry.pack.is_none());
    assert_eq!(dry.loose_before, ids.len());
    assert!(s.zpath(Kind::Blob, &ids[0].1).is_file(), "a dry run changes nothing");

    let r = gc(&s, false).unwrap();
    assert_eq!(r.objects, ids.len());
    assert_eq!(r.removed_loose, ids.len());
    // The 29 later versions of the file (the trees and notes differ in
    // every line, and are too small to pay for a delta).
    assert!(r.deltas >= 29, "versions become deltas: {r:?}");
    let after = r.bytes_after.unwrap();
    assert!(after * 4 < r.bytes_before, "{r:?}");
    assert!(s.loose(Kind::Blob).is_empty() && s.loose(Kind::Tree).is_empty());
    all_readable(&s, &ids);
    // A fresh store (another process) reads the pack too.
    all_readable(&Store::new(&s.vcs), &ids);

    let again = gc(&s, false).unwrap();
    assert_eq!(again.pack, r.pack, "nothing to do: the same pack");
    assert_eq!((again.removed_loose, again.removed_packs, again.deltas), (0, 0, 0));
    assert_eq!(again.bytes_after, Some(after));

    // New loose objects and the old pack become one pack again.
    let extra = s.put(Kind::Blob, b"written after the first gc\n").unwrap();
    let r3 = gc(&s, false).unwrap();
    assert_eq!((r3.removed_loose, r3.removed_packs), (1, 1));
    assert_eq!(pack::pack_names(&s.vcs).len(), 1);
    let mut ids = ids;
    ids.push((Kind::Blob, extra));
    all_readable(&s, &ids);
    let _ = fs::remove_dir_all(&s.vcs);
}

#[test]
fn a_store_left_between_the_steps_of_gc_is_sound_and_gc_finishes_it() {
    let (s, ids) = store_with_history(5);
    let snapshot = scratch("gc-snap");
    let copy = |from: &Path, to: &Path| {
        fn walk(a: &Path, b: &Path) {
            fs::create_dir_all(b).unwrap();
            for e in fs::read_dir(a).unwrap().flatten() {
                if e.file_type().unwrap().is_dir() {
                    walk(&e.path(), &b.join(e.file_name()));
                } else {
                    fs::copy(e.path(), b.join(e.file_name())).unwrap();
                }
            }
        }
        walk(from, to);
    };
    copy(&s.vcs, &snapshot);
    gc(&s, false).unwrap();
    let name = pack::pack_names(&s.vcs)[0].clone();
    let dir = pack_dir(&s.vcs);

    // (1) Killed after the pack, before its index: the pack does not exist.
    let a = scratch("gc-a");
    copy(&snapshot, &a);
    fs::create_dir_all(pack_dir(&a)).unwrap();
    let pack_file = format!("pack-{name}.pack");
    fs::copy(dir.join(&pack_file), pack_dir(&a).join(&pack_file)).unwrap();
    let sa = Store::new(&a);
    all_readable(&sa, &ids);
    assert!(pack::pack_names(&a).is_empty());
    gc(&sa, false).unwrap();
    all_readable(&Store::new(&a), &ids);

    // (2) Killed after the index, before the loose files went: both copies.
    let b = scratch("gc-b");
    copy(&snapshot, &b);
    copy(&dir, &pack_dir(&b));
    let sb = Store::new(&b);
    all_readable(&sb, &ids);
    assert_eq!(sb.list(Kind::Blob).unwrap().len(), 11, "each object listed once");
    let r = gc(&sb, false).unwrap();
    assert_eq!(r.pack.as_deref(), Some(name.as_str()), "the same objects make the same pack");
    assert_eq!(r.removed_packs, 0, "the pack it re-made is not removed");
    assert!(sb.loose(Kind::Blob).is_empty());
    all_readable(&Store::new(&b), &ids);
    for d in [&snapshot, &a, &b, &s.vcs] {
        let _ = fs::remove_dir_all(d);
    }
}

#[test]
fn gc_refuses_a_corrupt_object_and_removes_nothing() {
    let (s, ids) = store_with_history(2);
    let (_, id) = &ids[0];
    fs::write(s.zpath(Kind::Blob, id), b"tampered").unwrap();
    assert_eq!(gc(&s, false).unwrap_err().tag, "object-corrupt");
    assert!(pack::pack_names(&s.vcs).is_empty());
    assert_eq!(s.loose(Kind::Blob).len(), 5);
    let _ = fs::remove_dir_all(&s.vcs);
}
