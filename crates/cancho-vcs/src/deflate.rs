//! zlib compression (RFC 1950 around RFC 1951), for the object store
//! (`docs/vcs-cli.md` section 3 and R4). Written here for the reason
//! `inflate.rs` is: `cancho-vcs` takes no compression dependency. LZ77
//! over a 32 KiB window with hash chains and one step of lazy matching,
//! then, per block, the smallest of a dynamic Huffman block, a fixed one,
//! or stored bytes. The output is a standard zlib stream: `inflate.rs`
//! reads it, and so does any other inflater.

const WINDOW: usize = 1 << 15;
const WMASK: usize = WINDOW - 1;
const HBITS: u32 = 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// Candidates looked at per position (zlib's level 6 uses 128).
const MAX_CHAIN: usize = 128;
/// A match this long is taken at once, without a lazy look one byte on.
const NICE: usize = 128;
/// A match this long is not deferred for a lazy look.
const LAZY: usize = 32;
/// Symbols per block: the Huffman codes adapt at each block.
const BLOCK_SYMBOLS: usize = 1 << 15;
const NONE: u32 = u32::MAX;

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

#[derive(Clone, Copy)]
enum Sym {
    Lit(u8),
    /// (length 3..=258, distance 1..=32768)
    Match(u16, u16),
}

/// Compress `data` into one zlib stream.
pub(crate) fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut w = BitWriter { out: Vec::with_capacity(data.len() / 3 + 64), acc: 0, n: 0 };
    w.out.extend_from_slice(&[0x78, 0x9c]);
    let mut e = Blocks { data, w, syms: Vec::new(), start: 0 };
    let mut m = Matcher::new(data);
    let mut pos = 0usize;
    // A match found at `pos - 1`, held back to see whether `pos` has a longer one.
    let mut pending: Option<(usize, usize)> = None;
    while pos < data.len() {
        let cur = if pos + MIN_MATCH <= data.len() {
            let c = m.longest(pos);
            m.insert(pos);
            c
        } else {
            (0, 0)
        };
        if let Some((plen, pdist)) = pending.take() {
            if cur.0 <= plen {
                let start = pos - 1;
                e.push(Sym::Match(plen as u16, pdist as u16), start + plen);
                for p in pos + 1..start + plen {
                    m.insert(p);
                }
                pos = start + plen;
                continue;
            }
            e.push(Sym::Lit(data[pos - 1]), pos);
        }
        if cur.0 >= LAZY {
            e.push(Sym::Match(cur.0 as u16, cur.1 as u16), pos + cur.0);
            for p in pos + 1..pos + cur.0 {
                m.insert(p);
            }
            pos += cur.0;
        } else if cur.0 >= MIN_MATCH {
            pending = Some(cur);
            pos += 1;
        } else {
            e.push(Sym::Lit(data[pos]), pos + 1);
            pos += 1;
        }
    }
    // A pending match always leaves at least two bytes to scan, so none is
    // left here; the final block takes whatever symbols remain.
    let Blocks { mut w, syms, start, .. } = e;
    write_block(&mut w, &syms, &data[start..], true);
    w.align();
    w.out.extend_from_slice(&crate::inflate::adler32(data).to_be_bytes());
    w.out
}

/// Symbols collected into blocks; `start` is the first input byte the
/// current block covers (for the stored alternative).
struct Blocks<'a> {
    data: &'a [u8],
    w: BitWriter,
    syms: Vec<Sym>,
    start: usize,
}

impl Blocks<'_> {
    fn push(&mut self, s: Sym, covered_to: usize) {
        self.syms.push(s);
        if self.syms.len() >= BLOCK_SYMBOLS {
            write_block(&mut self.w, &self.syms, &self.data[self.start..covered_to], false);
            self.syms.clear();
            self.start = covered_to;
        }
    }
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}

impl<'a> Matcher<'a> {
    fn new(data: &'a [u8]) -> Matcher<'a> {
        Matcher { data, head: vec![NONE; 1 << HBITS], prev: vec![NONE; WINDOW] }
    }

    fn hash(&self, p: usize) -> usize {
        let d = self.data;
        let v = u32::from(d[p]) | u32::from(d[p + 1]) << 8 | u32::from(d[p + 2]) << 16;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HBITS)) as usize
    }

    fn insert(&mut self, p: usize) {
        if p + MIN_MATCH > self.data.len() {
            return;
        }
        let h = self.hash(p);
        self.prev[p & WMASK] = self.head[h];
        self.head[h] = p as u32;
    }

    /// The longest earlier match for `p` within the window: (length, distance).
    fn longest(&self, p: usize) -> (usize, usize) {
        let d = self.data;
        let max = (d.len() - p).min(MAX_MATCH);
        let (mut best, mut dist) = (0usize, 0usize);
        let mut cand = self.head[self.hash(p)];
        let mut chain = MAX_CHAIN;
        while cand != NONE && chain > 0 {
            let c = cand as usize;
            if c >= p || p - c > WINDOW {
                break;
            }
            if d[c + best.min(max - 1)] == d[p + best.min(max - 1)] {
                let mut l = 0;
                while l < max && d[c + l] == d[p + l] {
                    l += 1;
                }
                if l > best {
                    best = l;
                    dist = p - c;
                    if l >= NICE.min(max) {
                        break;
                    }
                }
            }
            let next = self.prev[c & WMASK];
            if next != NONE && next as usize >= c {
                break;
            }
            cand = next;
            chain -= 1;
        }
        if best >= MIN_MATCH { (best, dist) } else { (0, 0) }
    }
}

struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, bits: u32, n: u32) {
        self.acc |= u64::from(bits) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }
    fn align(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.acc = 0;
        self.n = 0;
    }
}

fn len_code(len: usize) -> usize {
    LEN_BASE.partition_point(|&b| usize::from(b) <= len) - 1
}

fn dist_code(dist: usize) -> usize {
    DIST_BASE.partition_point(|&b| usize::from(b) <= dist) - 1
}

/// Huffman code lengths for `freq`, no longer than `limit`. Frequencies
/// are halved (never to zero) until the tree fits, which always ends: with
/// all of them equal the tree is balanced.
pub(crate) fn code_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freq.iter().map(|&x| u64::from(x)).collect();
    loop {
        let l = huffman(&f);
        if l.iter().all(|&x| x <= limit) {
            return l;
        }
        for x in f.iter_mut().filter(|x| **x > 0) {
            *x = x.div_ceil(2);
        }
    }
}

fn huffman(freq: &[u64]) -> Vec<u8> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut lengths = vec![0u8; freq.len()];
    let used: Vec<usize> = (0..freq.len()).filter(|&i| freq[i] > 0).collect();
    match used.len() {
        0 => return lengths,
        1 => {
            lengths[used[0]] = 1;
            return lengths;
        }
        _ => {}
    }
    // Nodes: leaves 0..n, then internal ones; parent links give depths.
    let mut parent: Vec<usize> = vec![usize::MAX; used.len()];
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> =
        used.iter().enumerate().map(|(k, &i)| Reverse((freq[i], k))).collect();
    while heap.len() > 1 {
        let (Some(Reverse((wa, a))), Some(Reverse((wb, b)))) = (heap.pop(), heap.pop()) else {
            break;
        };
        let node = parent.len();
        parent.push(usize::MAX);
        parent[a] = node;
        parent[b] = node;
        heap.push(Reverse((wa + wb, node)));
    }
    let mut depth = vec![0u8; parent.len()];
    for k in (0..parent.len()).rev() {
        if parent[k] != usize::MAX {
            depth[k] = depth[parent[k]].saturating_add(1);
        }
    }
    for (k, &i) in used.iter().enumerate() {
        lengths[i] = depth[k];
    }
    lengths
}

/// Canonical codes for `lengths`, bit-reversed for an LSB-first writer.
fn canonical(lengths: &[u8]) -> Vec<u32> {
    let mut count = [0u32; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0u32;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            c.reverse_bits() >> (32 - u32::from(l))
        })
        .collect()
}

/// At least two used symbols, so every code is complete for any inflater.
fn at_least_two(freq: &mut [u32]) {
    let mut i = 0;
    while i < freq.len() && freq.iter().filter(|&&f| f > 0).count() < 2 {
        if freq[i] == 0 {
            freq[i] = 1;
        }
        i += 1;
    }
}

/// Run-length encode a code-length list (symbols 0..=18 with extra bits).
fn rle(lengths: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lengths.len() {
        let l = lengths[i];
        let mut run = 1;
        while i + run < lengths.len() && lengths[i + run] == l {
            run += 1;
        }
        let mut left = run;
        if l == 0 {
            while left >= 11 {
                let r = left.min(138);
                out.push((18, (r - 11) as u8));
                left -= r;
            }
            if left >= 3 {
                out.push((17, (left - 3) as u8));
                left = 0;
            }
        } else {
            out.push((l, 0));
            left -= 1;
            while left >= 3 {
                let r = left.min(6);
                out.push((16, (r - 3) as u8));
                left -= r;
            }
        }
        for _ in 0..left {
            out.push((l, 0));
        }
        i += run;
    }
    out
}

fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
    let lit = (0..288)
        .map(|i| match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        })
        .collect();
    (lit, vec![5u8; 30])
}

fn write_block(w: &mut BitWriter, syms: &[Sym], raw: &[u8], last: bool) {
    let mut lf = [0u32; 286];
    let mut df = [0u32; 30];
    let mut extra_bits = 0u64;
    for s in syms {
        match *s {
            Sym::Lit(b) => lf[b as usize] += 1,
            Sym::Match(l, d) => {
                let lc = len_code(l as usize);
                let dc = dist_code(d as usize);
                lf[257 + lc] += 1;
                df[dc] += 1;
                extra_bits += u64::from(LEN_EXTRA[lc]) + u64::from(DIST_EXTRA[dc]);
            }
        }
    }
    lf[256] = 1;
    let (fl, fd) = fixed_lengths();
    let cost = |ll: &[u8], dl: &[u8]| -> u64 {
        lf.iter().zip(ll).map(|(&f, &l)| u64::from(f) * u64::from(l)).sum::<u64>()
            + df.iter().zip(dl).map(|(&f, &l)| u64::from(f) * u64::from(l)).sum::<u64>()
            + extra_bits
    };
    let fixed_cost = 3 + cost(&fl, &fd);

    let mut lf2 = lf;
    let mut df2 = df;
    at_least_two(&mut lf2);
    at_least_two(&mut df2);
    let ll = code_lengths(&lf2, 15);
    let dl = code_lengths(&df2, 15);
    let hlit = (257..=286).rev().find(|&n| ll[n - 1] != 0).unwrap_or(257).max(257);
    let hdist = (1..=30).rev().find(|&n| dl[n - 1] != 0).unwrap_or(1);
    let mut seq = rle(&ll[..hlit]);
    seq.extend(rle(&dl[..hdist]));
    let mut cf = [0u32; 19];
    for &(s, _) in &seq {
        cf[s as usize] += 1;
    }
    at_least_two(&mut cf);
    let cl = code_lengths(&cf, 7);
    let hclen = (4..=19).rev().find(|&n| cl[CL_ORDER[n - 1]] != 0).unwrap_or(4);
    let header: u64 = 3
        + 14
        + 3 * hclen as u64
        + seq
            .iter()
            .map(|&(s, _)| {
                u64::from(cl[s as usize])
                    + match s {
                        16 => 2,
                        17 => 3,
                        18 => 7,
                        _ => 0,
                    }
            })
            .sum::<u64>();
    let dynamic_cost = header + cost(&ll, &dl);
    // Stored: align, then 4 bytes of header per 65,535 bytes.
    let stored_cost =
        3 + 7 + (raw.len() as u64).div_ceil(65_535).max(1) * 40 + raw.len() as u64 * 8;

    if stored_cost < fixed_cost.min(dynamic_cost) {
        let chunks: Vec<&[u8]> =
            if raw.is_empty() { vec![&raw[..0]] } else { raw.chunks(65_535).collect() };
        let n = chunks.len();
        for (i, c) in chunks.into_iter().enumerate() {
            w.put(u32::from(last && i + 1 == n), 1);
            w.put(0, 2);
            w.align();
            w.out.extend_from_slice(&(c.len() as u16).to_le_bytes());
            w.out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
            w.out.extend_from_slice(c);
        }
        return;
    }
    w.put(u32::from(last), 1);
    if fixed_cost <= dynamic_cost {
        w.put(1, 2);
        emit(w, syms, &fl, &fd);
        return;
    }
    w.put(2, 2);
    w.put((hlit - 257) as u32, 5);
    w.put((hdist - 1) as u32, 5);
    w.put((hclen - 4) as u32, 4);
    for &o in CL_ORDER.iter().take(hclen) {
        w.put(u32::from(cl[o]), 3);
    }
    let cc = canonical(&cl);
    for &(s, x) in &seq {
        w.put(cc[s as usize], u32::from(cl[s as usize]));
        match s {
            16 => w.put(u32::from(x), 2),
            17 => w.put(u32::from(x), 3),
            18 => w.put(u32::from(x), 7),
            _ => {}
        }
    }
    emit(w, syms, &ll, &dl);
}

fn emit(w: &mut BitWriter, syms: &[Sym], ll: &[u8], dl: &[u8]) {
    let lc = canonical(ll);
    let dc = canonical(dl);
    for s in syms {
        match *s {
            Sym::Lit(b) => w.put(lc[b as usize], u32::from(ll[b as usize])),
            Sym::Match(l, d) => {
                let (l, d) = (usize::from(l), usize::from(d));
                let c = len_code(l);
                w.put(lc[257 + c], u32::from(ll[257 + c]));
                w.put((l - usize::from(LEN_BASE[c])) as u32, u32::from(LEN_EXTRA[c]));
                let c = dist_code(d);
                w.put(dc[c], u32::from(dl[c]));
                w.put((d - usize::from(DIST_BASE[c])) as u32, u32::from(DIST_EXTRA[c]));
            }
        }
    }
    w.put(lc[256], u32::from(ll[256]));
}

#[cfg(test)]
#[path = "tests/deflate.rs"]
mod tests;
