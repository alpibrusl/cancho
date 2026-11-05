//! Reading a git repository's objects without git (`docs/vcs-cli.md`
//! section 7): loose objects, pack files through their `.idx` (version 2),
//! `ofs-delta` and `ref-delta` resolution, `objects/info/alternates`, and
//! refs (loose and `packed-refs`). Everything read is untrusted bytes, so
//! each length is bounded and every failure is a `git-unreadable` refusal.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Refusal, Result};
use crate::fsx::{hex, unhex};
use crate::inflate::zlib_decode;
use crate::sha1::git_id;

/// No single git object is believed past this size.
const MAX_OBJECT: usize = 1 << 30;

pub type GitId = [u8; 20];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitKind {
    Commit,
    Tree,
    Blob,
    Tag,
}

impl GitKind {
    pub fn name(self) -> &'static str {
        match self {
            GitKind::Commit => "commit",
            GitKind::Tree => "tree",
            GitKind::Blob => "blob",
            GitKind::Tag => "tag",
        }
    }
    fn parse(s: &[u8]) -> Option<GitKind> {
        match s {
            b"commit" => Some(GitKind::Commit),
            b"tree" => Some(GitKind::Tree),
            b"blob" => Some(GitKind::Blob),
            b"tag" => Some(GitKind::Tag),
            _ => None,
        }
    }
}

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::new("git-unreadable", msg)
}

struct Pack {
    data: Vec<u8>,
    idx: Vec<u8>,
    count: usize,
}

impl Pack {
    fn fanout(&self, i: usize) -> usize {
        let at = 8 + i * 4;
        u32::from_be_bytes([self.idx[at], self.idx[at + 1], self.idx[at + 2], self.idx[at + 3]]) as usize
    }
    fn sha_at(&self, n: usize) -> &[u8] {
        let at = 8 + 256 * 4 + n * 20;
        &self.idx[at..at + 20]
    }
    fn offset_of(&self, n: usize) -> Option<usize> {
        let base = 8 + 256 * 4 + self.count * 20 + self.count * 4;
        let at = base + n * 4;
        let raw = u32::from_be_bytes(self.idx.get(at..at + 4)?.try_into().ok()?);
        if raw & 0x8000_0000 == 0 {
            return Some(raw as usize);
        }
        let big = base + self.count * 4 + (raw & 0x7fff_ffff) as usize * 8;
        Some(u64::from_be_bytes(self.idx.get(big..big + 8)?.try_into().ok()?) as usize)
    }
    fn find(&self, id: &GitId) -> Option<usize> {
        let first = id[0] as usize;
        let lo0 = if first == 0 { 0 } else { self.fanout(first - 1) };
        let (mut lo, mut hi) = (lo0, self.fanout(first));
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.sha_at(mid).cmp(&id[..]) {
                std::cmp::Ordering::Equal => return self.offset_of(mid),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }
}

pub struct GitRepo {
    git_dir: PathBuf,
    object_dirs: Vec<PathBuf>,
    packs: Vec<Pack>,
    /// Resolved pack objects by (pack, offset), bounded.
    cache: RefCell<(HashMap<(usize, usize), (GitKind, std::rc::Rc<Vec<u8>>)>, usize)>,
}

impl GitRepo {
    /// `dir` is a work tree containing `.git`, or a git directory itself.
    pub fn open(dir: &Path) -> Result<GitRepo> {
        let git_dir = if dir.join(".git").is_dir() { dir.join(".git") } else { dir.to_path_buf() };
        if !git_dir.join("objects").is_dir() || !git_dir.join("HEAD").exists() {
            return Err(bad(format!("`{}` is not a git repository", dir.display())));
        }
        let mut object_dirs = vec![git_dir.join("objects")];
        let mut i = 0;
        while i < object_dirs.len() && i < 8 {
            let alt = object_dirs[i].join("info").join("alternates");
            if let Ok(text) = fs::read_to_string(alt) {
                for line in text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
                    let p = Path::new(line);
                    let p = if p.is_absolute() { p.to_path_buf() } else { object_dirs[i].join(p) };
                    if p.is_dir() && !object_dirs.contains(&p) {
                        object_dirs.push(p);
                    }
                }
            }
            i += 1;
        }
        let mut packs = Vec::new();
        for od in &object_dirs {
            let Ok(rd) = fs::read_dir(od.join("pack")) else { continue };
            let mut names: Vec<PathBuf> =
                rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "idx")).collect();
            names.sort();
            for idx_path in names {
                let pack_path = idx_path.with_extension("pack");
                let idx = fs::read(&idx_path).map_err(|e| Refusal::io(format!("cannot read {}", idx_path.display()), &e))?;
                let data = fs::read(&pack_path).map_err(|e| Refusal::io(format!("cannot read {}", pack_path.display()), &e))?;
                if idx.len() < 8 + 1024 || idx[..8] != [0xff, b't', b'O', b'c', 0, 0, 0, 2] {
                    return Err(bad(format!("{} is not a version 2 pack index", idx_path.display())));
                }
                let count =
                    u32::from_be_bytes([idx[8 + 255 * 4], idx[8 + 255 * 4 + 1], idx[8 + 255 * 4 + 2], idx[8 + 255 * 4 + 3]])
                        as usize;
                if idx.len() < 8 + 1024 + count * 28 + 40 {
                    return Err(bad(format!("{} is truncated", idx_path.display())));
                }
                if data.len() < 12 || &data[..4] != b"PACK" {
                    return Err(bad(format!("{} is not a pack", pack_path.display())));
                }
                packs.push(Pack { data, idx, count });
            }
        }
        Ok(GitRepo { git_dir, object_dirs, packs, cache: RefCell::new((HashMap::new(), 0)) })
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// Read an object's kind and body.
    pub fn read(&self, id: &GitId) -> Result<(GitKind, Vec<u8>)> {
        let (kind, body) = self.read_rc(id)?;
        Ok((kind, body.as_ref().clone()))
    }

    fn read_rc(&self, id: &GitId) -> Result<(GitKind, std::rc::Rc<Vec<u8>>)> {
        let name = hex(id);
        for od in &self.object_dirs {
            let p = od.join(&name[..2]).join(&name[2..]);
            if let Ok(raw) = fs::read(&p) {
                let (data, _) = zlib_decode(&raw, MAX_OBJECT).map_err(|e| bad(format!("{name}: {}", e.0)))?;
                let nul = data.iter().position(|&b| b == 0).ok_or_else(|| bad(format!("{name}: no header")))?;
                let header = &data[..nul];
                let sp = header.iter().position(|&b| b == b' ').ok_or_else(|| bad(format!("{name}: bad header")))?;
                let kind = GitKind::parse(&header[..sp]).ok_or_else(|| bad(format!("{name}: unknown kind")))?;
                let len: usize = std::str::from_utf8(&header[sp + 1..])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| bad(format!("{name}: bad length")))?;
                if data.len() - nul - 1 != len {
                    return Err(bad(format!("{name}: length mismatch")));
                }
                return Ok((kind, std::rc::Rc::new(data[nul + 1..].to_vec())));
            }
        }
        for (pi, pack) in self.packs.iter().enumerate() {
            if let Some(off) = pack.find(id) {
                return self.read_at(pi, off, 0);
            }
        }
        Err(bad(format!("object {name} not found")))
    }

    fn read_at(&self, pi: usize, off: usize, depth: usize) -> Result<(GitKind, std::rc::Rc<Vec<u8>>)> {
        if depth > 5000 {
            return Err(bad("delta chain too deep"));
        }
        if let Some((k, d)) = self.cache.borrow().0.get(&(pi, off)) {
            return Ok((*k, d.clone()));
        }
        let data = &self.packs[pi].data;
        let mut p = off;
        let first = *data.get(p).ok_or_else(|| bad("offset past the pack"))?;
        p += 1;
        let ty = (first >> 4) & 7;
        let mut size = (first & 15) as usize;
        let mut shift = 4;
        let mut b = first;
        while b & 0x80 != 0 {
            b = *data.get(p).ok_or_else(|| bad("truncated object header"))?;
            p += 1;
            if shift > 56 {
                return Err(bad("object size too large"));
            }
            size |= ((b & 0x7f) as usize) << shift;
            shift += 7;
        }
        if size > MAX_OBJECT {
            return Err(bad("object too large"));
        }
        let result = match ty {
            1..=4 => {
                let kind = [GitKind::Commit, GitKind::Tree, GitKind::Blob, GitKind::Tag][ty as usize - 1];
                let (body, _) = zlib_decode(&data[p..], size).map_err(|e| bad(format!("pack object: {}", e.0)))?;
                if body.len() != size {
                    return Err(bad("pack object length mismatch"));
                }
                (kind, std::rc::Rc::new(body))
            }
            6 | 7 => {
                let (base_kind, base) = if ty == 6 {
                    let mut c = *data.get(p).ok_or_else(|| bad("truncated"))?;
                    p += 1;
                    let mut dist = (c & 0x7f) as usize;
                    while c & 0x80 != 0 {
                        c = *data.get(p).ok_or_else(|| bad("truncated"))?;
                        p += 1;
                        dist = dist.checked_add(1).and_then(|d| d.checked_shl(7)).ok_or_else(|| bad("bad offset"))? | (c & 0x7f) as usize;
                    }
                    let base_off = off.checked_sub(dist).ok_or_else(|| bad("delta base before the pack"))?;
                    self.read_at(pi, base_off, depth + 1)?
                } else {
                    let id: GitId = data.get(p..p + 20).ok_or_else(|| bad("truncated"))?.try_into().map_err(|_| bad("truncated"))?;
                    p += 20;
                    self.read_rc(&id)?
                };
                let (delta, _) = zlib_decode(&data[p..], size).map_err(|e| bad(format!("delta: {}", e.0)))?;
                (base_kind, std::rc::Rc::new(apply_delta(&base, &delta)?))
            }
            _ => return Err(bad(format!("unknown pack object type {ty}"))),
        };
        let mut cache = self.cache.borrow_mut();
        if cache.1 > 256 << 20 {
            cache.0.clear();
            cache.1 = 0;
        }
        cache.1 += result.1.len();
        cache.0.insert((pi, off), (result.0, result.1.clone()));
        Ok(result)
    }

    /// Branch, tag and remote-tracking refs as `(full name, id)`, plus HEAD's target if it is a branch.
    pub fn refs(&self) -> Result<Vec<(String, GitId)>> {
        let mut out: HashMap<String, GitId> = HashMap::new();
        if let Ok(text) = fs::read_to_string(self.git_dir.join("packed-refs")) {
            for line in text.lines() {
                if line.starts_with('#') || line.starts_with('^') {
                    continue;
                }
                if let Some((h, n)) = line.split_once(' ')
                    && let Some(id) = parse_id(h)
                    && ["refs/heads/", "refs/tags/", "refs/remotes/"].iter().any(|p| n.starts_with(p))
                {
                    out.insert(n.to_owned(), id);
                }
            }
        }
        fn walk(dir: &Path, prefix: &str, out: &mut HashMap<String, GitId>) {
            let Ok(rd) = fs::read_dir(dir) else { return };
            for e in rd.flatten() {
                let name = format!("{prefix}{}", e.file_name().to_string_lossy());
                if e.path().is_dir() {
                    walk(&e.path(), &format!("{name}/"), out);
                } else if let Ok(t) = fs::read_to_string(e.path())
                    && let Some(id) = parse_id(t.trim())
                {
                    out.insert(name, id);
                }
            }
        }
        walk(&self.git_dir.join("refs").join("heads"), "refs/heads/", &mut out);
        walk(&self.git_dir.join("refs").join("tags"), "refs/tags/", &mut out);
        walk(&self.git_dir.join("refs").join("remotes"), "refs/remotes/", &mut out);
        let mut v: Vec<_> = out.into_iter().collect();
        v.sort();
        Ok(v)
    }

    /// The commit HEAD names, if it is a branch that exists or a detached id.
    pub fn head(&self) -> Result<Option<(Option<String>, GitId)>> {
        let text = fs::read_to_string(self.git_dir.join("HEAD")).map_err(|e| Refusal::io("cannot read git HEAD", &e))?;
        let text = text.trim();
        if let Some(r) = text.strip_prefix("ref: ") {
            let refs = self.refs()?;
            return Ok(refs.into_iter().find(|(n, _)| n == r).map(|(n, id)| (Some(n), id)));
        }
        Ok(parse_id(text).map(|id| (None, id)))
    }
}

pub fn parse_id(s: &str) -> Option<GitId> {
    unhex(s)?.try_into().ok()
}

fn varint(delta: &[u8], p: &mut usize) -> Result<usize> {
    let mut v = 0usize;
    let mut shift = 0;
    loop {
        let b = *delta.get(*p).ok_or_else(|| bad("truncated delta"))?;
        *p += 1;
        if shift > 56 {
            return Err(bad("delta size too large"));
        }
        v |= ((b & 0x7f) as usize) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
}

fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>> {
    let mut p = 0;
    if varint(delta, &mut p)? != base.len() {
        return Err(bad("delta base size mismatch"));
    }
    let target = varint(delta, &mut p)?;
    if target > MAX_OBJECT {
        return Err(bad("delta target too large"));
    }
    let mut out = Vec::with_capacity(target);
    while p < delta.len() {
        let op = delta[p];
        p += 1;
        if op & 0x80 != 0 {
            let (mut off, mut len) = (0usize, 0usize);
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    off |= (*delta.get(p).ok_or_else(|| bad("truncated delta"))? as usize) << (8 * i);
                    p += 1;
                }
            }
            for i in 0..3 {
                if op & (0x10 << i) != 0 {
                    len |= (*delta.get(p).ok_or_else(|| bad("truncated delta"))? as usize) << (8 * i);
                    p += 1;
                }
            }
            if len == 0 {
                len = 0x10000;
            }
            let chunk = base.get(off..off.checked_add(len).ok_or_else(|| bad("bad copy"))?).ok_or_else(|| bad("copy out of range"))?;
            out.extend_from_slice(chunk);
        } else if op != 0 {
            let n = op as usize;
            out.extend_from_slice(delta.get(p..p + n).ok_or_else(|| bad("truncated insert"))?);
            p += n;
        } else {
            return Err(bad("delta opcode 0"));
        }
        if out.len() > target {
            return Err(bad("delta overruns its target"));
        }
    }
    if out.len() != target {
        return Err(bad("delta result size mismatch"));
    }
    Ok(out)
}

/// A parsed git commit: parents, tree, and the raw tail that must survive.
pub struct GitCommitData {
    pub tree: GitId,
    pub parents: Vec<GitId>,
    /// Header lines after the last `parent` line, verbatim, each with its `\n`.
    pub headers: Vec<u8>,
    pub message: Vec<u8>,
}

pub fn parse_commit(body: &[u8]) -> Result<GitCommitData> {
    let split = body.windows(2).position(|w| w == b"\n\n");
    let (head, message) = match split {
        Some(i) => (&body[..i + 1], body[i + 2..].to_vec()),
        None => (body, Vec::new()),
    };
    let mut tree = None;
    let mut parents = Vec::new();
    let mut headers_at = None;
    let mut pos = 0;
    while pos < head.len() {
        let end = head[pos..].iter().position(|&b| b == b'\n').map_or(head.len(), |i| pos + i + 1);
        let line = &head[pos..end];
        if tree.is_none() && line.starts_with(b"tree ") {
            tree = parse_id(std::str::from_utf8(&line[5..]).unwrap_or("").trim());
        } else if headers_at.is_none() && line.starts_with(b"parent ") {
            parents.push(parse_id(std::str::from_utf8(&line[7..]).unwrap_or("").trim()).ok_or_else(|| bad("bad parent line"))?);
        } else if headers_at.is_none() {
            headers_at = Some(pos);
        }
        pos = end;
    }
    let headers = headers_at.map_or_else(Vec::new, |at| head[at..].to_vec());
    Ok(GitCommitData { tree: tree.ok_or_else(|| bad("commit without a tree"))?, parents, headers, message })
}

/// Tree entries: `(mode, name, id)`.
pub fn parse_tree(body: &[u8]) -> Result<Vec<(u32, Vec<u8>, GitId)>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < body.len() {
        let sp = body[p..].iter().position(|&b| b == b' ').ok_or_else(|| bad("bad tree entry"))? + p;
        let mode = std::str::from_utf8(&body[p..sp])
            .ok()
            .and_then(|s| u32::from_str_radix(s, 8).ok())
            .ok_or_else(|| bad("bad tree mode"))?;
        let nul = body[sp..].iter().position(|&b| b == 0).ok_or_else(|| bad("bad tree entry"))? + sp;
        let id: GitId = body.get(nul + 1..nul + 21).ok_or_else(|| bad("truncated tree"))?.try_into().map_err(|_| bad("truncated tree"))?;
        out.push((mode, body[sp + 1..nul].to_vec(), id));
        p = nul + 21;
    }
    Ok(out)
}

/// The id git would give an object.
pub fn id_of(kind: GitKind, body: &[u8]) -> GitId {
    git_id(kind.name(), body)
}
