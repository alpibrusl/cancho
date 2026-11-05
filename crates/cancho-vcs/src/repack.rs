//! `gc`: move every object into one pack, with deltas (`docs/vcs-cli.md`
//! sections 4, 5 and R4).
//!
//! The objects are ordered as git orders them for delta search: by kind,
//! then (for blobs) by a path a tree gives them, then by size, largest
//! first; each is tried as a delta against the [`WINDOW`] objects before it
//! of the same kind, and the smallest delta under half the object's size is
//! kept, with chains no deeper than [`pack::MAX_DEPTH`].
//!
//! Crash safety: the pack is written and fsynced, then its index (whose
//! rename is the moment the pack exists), then every object is read back
//! from the new pack and re-hashed, and only then are the loose files and
//! the old packs removed. A kill at any point leaves every object readable
//! from somewhere; running `gc` again finishes the job, and on a store that
//! is one pack and no loose files it does nothing.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::delta::DeltaIndex;
use crate::error::{Refusal, Result};
use crate::fsx;
use crate::object::{Kind, Store, id_bytes, is_id};
use crate::pack::{self, NewEntry, Pack, pack_dir};
use crate::tree::Tree;

/// Candidate bases per object (git's default).
pub const WINDOW: usize = 10;
/// Objects smaller than this are stored whole.
const MIN_DELTA: usize = 64;
/// Files in `tmp` older than this (seconds) are a crashed write's leftovers.
const TMP_AGE: u64 = 24 * 3600;

#[derive(Debug, Clone, Default)]
pub struct GcReport {
    pub dry_run: bool,
    pub loose_before: usize,
    pub packs_before: usize,
    pub bytes_before: u64,
    pub objects: usize,
    pub deltas: usize,
    pub pack: Option<String>,
    pub bytes_after: Option<u64>,
    pub removed_loose: usize,
    pub removed_packs: usize,
    pub removed_tmp: usize,
}

impl GcReport {
    pub fn to_json(&self) -> Value {
        json!({
            "dry_run": self.dry_run,
            "loose_before": self.loose_before,
            "packs_before": self.packs_before,
            "bytes_before": self.bytes_before,
            "objects": self.objects,
            "deltas": self.deltas,
            "pack": self.pack,
            "bytes_after": self.bytes_after,
            "removed_loose": self.removed_loose,
            "removed_packs": self.removed_packs,
            "removed_tmp": self.removed_tmp,
        })
    }
}

/// Apparent bytes of every file beneath `dir`.
fn du(dir: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => du(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

struct Item {
    kind: Kind,
    id: String,
    path: String,
    size: usize,
}

/// Pack every object of `store` into one pack and remove what it replaces.
pub fn gc(store: &Store, dry: bool) -> Result<GcReport> {
    let objects_dir = store.vcs.join("objects");
    let mut r = GcReport { dry_run: dry, ..GcReport::default() };
    r.bytes_before = du(&objects_dir);
    let loose: Vec<(Kind, std::path::PathBuf, String)> = Kind::ALL
        .into_iter()
        .flat_map(|k| store.loose(k).into_iter().map(move |(p, id, _)| (k, p, id)))
        .filter(|(_, _, id)| is_id(id))
        .collect();
    let old = pack::pack_names(&store.vcs);
    r.loose_before = loose.len();
    r.packs_before = old.len();
    let packs = store.packs(true);
    let packs = packs.as_ref().as_ref().map_err(Clone::clone)?;
    let mut all: BTreeSet<(Kind, String)> =
        loose.iter().map(|(k, _, id)| (*k, id.clone())).collect();
    for p in packs {
        all.extend(p.entries.iter().map(|e| (e.kind, fsx::hex(&e.id))));
    }
    r.objects = all.len();
    if dry || (loose.is_empty() && old.len() <= 1) {
        r.pack = old.first().cloned().filter(|_| loose.is_empty() && old.len() == 1);
        r.bytes_after = (!dry).then_some(r.bytes_before);
        return Ok(r);
    }

    // Pass 1: sizes, and a path for every blob (the least one naming it).
    let mut items: Vec<Item> = Vec::with_capacity(all.len());
    let mut paths: HashMap<String, String> = HashMap::new();
    for (kind, id) in &all {
        let bytes = store.get(*kind, id)?;
        if *kind == Kind::Tree {
            for e in Tree::decode(&bytes)?.entries() {
                match paths.get_mut(&e.blob) {
                    Some(p) if *p <= e.path => {}
                    Some(p) => *p = e.path.clone(),
                    None => {
                        paths.insert(e.blob.clone(), e.path.clone());
                    }
                }
            }
        }
        items.push(Item { kind: *kind, id: id.clone(), path: String::new(), size: bytes.len() });
    }
    for it in items.iter_mut().filter(|i| i.kind == Kind::Blob) {
        if let Some(p) = paths.get(&it.id) {
            it.path = p.clone();
        }
    }
    items.sort_by(|a, b| {
        (a.kind, &a.path, std::cmp::Reverse(a.size), &a.id).cmp(&(
            b.kind,
            &b.path,
            std::cmp::Reverse(b.size),
            &b.id,
        ))
    });

    // Pass 2: deltas against a sliding window.
    struct Slot {
        kind: Kind,
        n: u32,
        depth: usize,
        bytes: Vec<u8>,
        index: DeltaIndex,
    }
    let mut window: VecDeque<Slot> = VecDeque::with_capacity(WINDOW + 1);
    let mut entries: Vec<NewEntry> = Vec::with_capacity(items.len());
    for (n, it) in items.iter().enumerate() {
        let bytes = store.get(it.kind, &it.id)?;
        let mut best: Option<(u32, usize, Vec<u8>)> = None;
        if bytes.len() >= MIN_DELTA {
            for s in window.iter().rev() {
                if s.kind != it.kind || s.depth >= pack::MAX_DEPTH || s.bytes.len() < MIN_DELTA {
                    continue;
                }
                let limit = best.as_ref().map_or(bytes.len() / 2 - 20, |b| b.2.len() - 1);
                if let Some(d) = s.index.encode(&s.bytes, &bytes, limit) {
                    best = Some((s.n, s.depth + 1, d));
                }
            }
        }
        let id = id_bytes(&it.id).ok_or_else(|| Refusal::new("bad-id", it.id.clone()))?;
        let depth = best.as_ref().map_or(0, |b| b.1);
        let (base, payload) = match best {
            Some((b, _, d)) => {
                r.deltas += 1;
                (Some(b), d)
            }
            None => (None, bytes.clone()),
        };
        entries.push(NewEntry { kind: it.kind, id, size: bytes.len() as u64, base, payload });
        if bytes.len() >= MIN_DELTA {
            let index = DeltaIndex::new(&bytes);
            window.push_back(Slot { kind: it.kind, n: n as u32, depth, bytes, index });
            if window.len() > WINDOW {
                window.pop_front();
            }
        }
    }
    drop(window);

    // Write: the pack, then its index (the commit point).
    let (name, pack_bytes, idx_bytes) = pack::encode(&entries);
    drop(entries);
    let dir = pack_dir(&store.vcs);
    let io = |what: &str, e: std::io::Error| Refusal::io(format!("cannot write {what}"), &e);
    fsx::write_atomic(
        &store.tmp(),
        &dir.join(format!("pack-{name}.pack")),
        &pack_bytes,
        false,
        true,
    )
    .map_err(|e| io("the pack", e))?;
    drop(pack_bytes);
    fsx::write_atomic(&store.tmp(), &dir.join(format!("pack-{name}.idx")), &idx_bytes, false, true)
        .map_err(|e| io("the pack index", e))?;

    // Read every object back from the new pack before anything is removed.
    let fresh = Pack::load(&store.vcs, &name)?;
    if fresh.entries.len() != all.len() {
        return Err(Refusal::new("object-corrupt", format!("pack {name}: wrong entry count")));
    }
    for k in 0..fresh.entries.len() {
        fresh.read(k as u32)?;
    }
    drop(fresh);
    store.packs(true);
    r.pack = Some(name.clone());

    for (_, path, _) in &loose {
        fsx::failpoint();
        if fs::remove_file(path).is_ok() {
            r.removed_loose += 1;
        }
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
    for o in old.iter().filter(|o| **o != name) {
        fsx::failpoint();
        let _ = fs::remove_file(dir.join(format!("pack-{o}.idx")));
        let _ = fs::remove_file(dir.join(format!("pack-{o}.pack")));
        r.removed_packs += 1;
    }
    // Pack files whose index never landed (a write killed in between).
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if let Some(h) = n.strip_prefix("pack-").and_then(|n| n.strip_suffix(".pack"))
                && !dir.join(format!("pack-{h}.idx")).exists()
            {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    r.removed_tmp = remove_old_tmp(&store.tmp());
    store.packs(true);
    r.bytes_after = Some(du(&objects_dir));
    Ok(r)
}

/// Remove files in `tmp` that no running write can still own.
fn remove_old_tmp(tmp: &Path) -> usize {
    let Ok(rd) = fs::read_dir(tmp) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > TMP_AGE);
        if old && fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
#[path = "tests/repack.rs"]
mod tests;
