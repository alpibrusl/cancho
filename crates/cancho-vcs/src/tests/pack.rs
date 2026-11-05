use super::*;
use crate::delta::DeltaIndex;
use crate::object::{Store, id_bytes};

pub(crate) fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "cancho-vcs-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Three versions of one file (two as deltas, a chain of depth 2) and a tree.
fn sample() -> Vec<(Kind, Vec<u8>)> {
    let v1: Vec<u8> = (0..4000u32).flat_map(|i| format!("line {i}\n").into_bytes()).collect();
    let mut v2 = v1.clone();
    v2.splice(100..100, b"inserted text\n".iter().copied());
    let mut v3 = v2.clone();
    v3.truncate(20_000);
    vec![
        (Kind::Blob, v1),
        (Kind::Blob, v2),
        (Kind::Blob, v3),
        (Kind::Tree, b"100644 x y\n".to_vec()),
        (Kind::Blob, Vec::new()),
    ]
}

fn build(objs: &[(Kind, Vec<u8>)]) -> Vec<NewEntry> {
    let mut out: Vec<NewEntry> = Vec::new();
    for (n, (kind, bytes)) in objs.iter().enumerate() {
        let id = id_bytes(&object_id(*kind, bytes)).unwrap();
        // Blobs 1 and 2 are deltas against the one before them.
        let (base, payload) = if *kind == Kind::Blob && (n == 1 || n == 2) {
            let b = &objs[n - 1].1;
            (Some((n - 1) as u32), DeltaIndex::new(b).encode(b, bytes, usize::MAX).unwrap())
        } else {
            (None, bytes.clone())
        };
        out.push(NewEntry { kind: *kind, id, size: bytes.len() as u64, base, payload });
    }
    out
}

fn install(vcs: &Path, name: &str, pack: &[u8], idx: &[u8]) {
    let d = pack_dir(vcs);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join(format!("pack-{name}.pack")), pack).unwrap();
    fs::write(d.join(format!("pack-{name}.idx")), idx).unwrap();
}

#[test]
fn a_pack_reads_back_every_object_through_its_deltas() {
    let vcs = scratch("pack-read");
    let objs = sample();
    let (name, pack, idx) = encode(&build(&objs));
    assert!(pack.len() < objs[0].1.len(), "three versions in less than one: {}", pack.len());
    install(&vcs, &name, &pack, &idx);
    assert_eq!(pack_names(&vcs), vec![name.clone()]);
    let p = Pack::load(&vcs, &name).unwrap();
    for (kind, bytes) in &objs {
        let k = p.find(*kind, &id_bytes(&object_id(*kind, bytes)).unwrap()).unwrap();
        assert_eq!(p.read(k).unwrap().as_slice(), &bytes[..]);
    }
    // And through the store, which also lists them.
    let s = Store::new(&vcs);
    for (kind, bytes) in &objs {
        let id = object_id(*kind, bytes);
        assert!(s.has(*kind, &id));
        assert_eq!(s.get(*kind, &id).unwrap(), *bytes);
    }
    assert_eq!(s.list(Kind::Blob).unwrap().len(), 4);
    assert_eq!(s.list(Kind::Tree).unwrap().len(), 1);
    let missing = object_id(Kind::Blob, b"not here");
    assert_eq!(s.get(Kind::Blob, &missing).unwrap_err().tag, "object-missing");
    let _ = fs::remove_dir_all(&vcs);
}

#[test]
fn every_damaged_byte_of_a_pack_is_refused_or_harmless() {
    let vcs = scratch("pack-corrupt");
    let objs = sample();
    let (name, pack, idx) = encode(&build(&objs));
    // Flip each byte of the entries (a stride keeps the test quick) and read
    // every object: it is the right bytes or `object-corrupt`, never wrong.
    for at in (12..pack.len() - 32).step_by(7) {
        let mut bad = pack.clone();
        bad[at] ^= 0x41;
        install(&vcs, &name, &bad, &idx);
        let p = Pack::load(&vcs, &name).unwrap();
        for (k, (_, bytes)) in objs.iter().enumerate() {
            match p.read(k as u32) {
                Ok(got) => assert_eq!(got.as_slice(), &bytes[..], "byte {at}"),
                Err(e) => assert_eq!(e.tag, "object-corrupt", "byte {at}: {e}"),
            }
        }
    }
    // A damaged index or a short pack is refused when loaded, with the tag.
    let mut bad_idx = idx.clone();
    bad_idx[20] ^= 1;
    install(&vcs, &name, &pack, &bad_idx);
    assert_eq!(Pack::load(&vcs, &name).unwrap_err().tag, "object-corrupt");
    let s = Store::new(&vcs);
    assert_eq!(s.list(Kind::Blob).unwrap_err().tag, "object-corrupt");
    assert_eq!(
        s.get(Kind::Blob, &object_id(Kind::Blob, &objs[0].1)).unwrap_err().tag,
        "object-corrupt"
    );
    install(&vcs, &name, &pack[..pack.len() - 1], &idx);
    assert_eq!(Pack::load(&vcs, &name).unwrap_err().tag, "object-corrupt");
    // An index whose entry claims a later base: refused, no loop.
    let mut entries = build(&objs);
    entries[1].base = Some(1);
    let (n2, p2, i2) = encode(&entries);
    install(&vcs, &n2, &p2, &i2);
    let p = Pack::load(&vcs, &n2).unwrap();
    assert_eq!(p.read(1).unwrap_err().tag, "object-corrupt");
    let _ = fs::remove_dir_all(&vcs);
}
