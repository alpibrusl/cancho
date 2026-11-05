//! Copy/insert deltas between two objects, for packs (`docs/vcs-cli.md`
//! R4). The encoding is git's (a pack's delta format): the base size and
//! the result size as little-endian base-128 varints, then operations: a
//! byte `1..=127` inserts that many literal bytes that follow it; a byte
//! with the top bit set copies from the base, its low four bits saying
//! which offset bytes follow and the next three which size bytes follow
//! (a size of zero means 65,536). Opcode 0 is reserved and refused.
//!
//! The encoder indexes the base at every 16th byte by a rolling hash and
//! scans the result at every byte, extending each hit forwards and back.

const BLOCK: usize = 16;
const PRIME: u32 = 0x0100_0193;
/// Candidates looked at per hash bucket.
const MAX_CANDIDATES: usize = 16;
const NONE: u32 = u32::MAX;
const MAX_COPY: usize = 0x00ff_ffff;

/// A base, indexed once and used against many targets.
pub(crate) struct DeltaIndex {
    head: Vec<u32>,
    next: Vec<u32>,
    shift: u32,
}

fn block_hash(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |h, &x| h.wrapping_mul(PRIME).wrapping_add(u32::from(x)))
}

/// PRIME to the 15th: the weight of the byte that leaves a 16-byte window.
fn out_weight() -> u32 {
    (0..BLOCK - 1).fold(1u32, |a, _| a.wrapping_mul(PRIME))
}

impl DeltaIndex {
    pub(crate) fn new(base: &[u8]) -> DeltaIndex {
        let blocks = base.len() / BLOCK;
        let bits = (usize::BITS - blocks.max(1).leading_zeros()).clamp(4, 24);
        let mut head = vec![NONE; 1 << bits];
        let mut next = vec![NONE; blocks];
        let shift = 32 - bits;
        // Later blocks first, so a chain prefers the earliest occurrence.
        for k in (0..blocks).rev() {
            let h = (block_hash(&base[k * BLOCK..(k + 1) * BLOCK]).wrapping_mul(0x9E37_79B1)
                >> shift) as usize;
            next[k] = head[h];
            head[h] = k as u32;
        }
        DeltaIndex { head, next, shift }
    }

    /// The delta from `base` (the one indexed) to `target`, or `None` if it
    /// would be longer than `max` bytes.
    pub(crate) fn encode(&self, base: &[u8], target: &[u8], max: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(target.len().min(max) / 4 + 16);
        varint(&mut out, base.len());
        varint(&mut out, target.len());
        let mut lit = 0usize; // start of the literal bytes not yet written
        let mut i = 0usize;
        let w = out_weight();
        let mut h = if target.len() >= BLOCK { block_hash(&target[..BLOCK]) } else { 0 };
        while i + BLOCK <= target.len() && !self.next.is_empty() {
            let (off, len) = self.best(base, target, i, h);
            if len >= BLOCK {
                let (mut bo, mut ti, mut len) = (off, i, len);
                while ti > lit && bo > 0 && base[bo - 1] == target[ti - 1] {
                    bo -= 1;
                    ti -= 1;
                    len += 1;
                }
                insert(&mut out, &target[lit..ti]);
                copy(&mut out, bo, len);
                if out.len() > max {
                    return None;
                }
                i = ti + len;
                lit = i;
                if i + BLOCK <= target.len() {
                    h = block_hash(&target[i..i + BLOCK]);
                }
                continue;
            }
            if i + BLOCK < target.len() {
                h = h
                    .wrapping_sub(u32::from(target[i]).wrapping_mul(w))
                    .wrapping_mul(PRIME)
                    .wrapping_add(u32::from(target[i + BLOCK]));
            }
            i += 1;
            if i - lit > max {
                return None;
            }
        }
        insert(&mut out, &target[lit..]);
        (out.len() <= max).then_some(out)
    }

    /// The longest base run that matches `target` from `i`: (offset, length).
    fn best(&self, base: &[u8], target: &[u8], i: usize, h: u32) -> (usize, usize) {
        let mut k = self.head[(h.wrapping_mul(0x9E37_79B1) >> self.shift) as usize];
        let (mut off, mut best) = (0, 0);
        let mut left = MAX_CANDIDATES;
        while k != NONE && left > 0 {
            let c = k as usize * BLOCK;
            let n = base[c..].iter().zip(&target[i..]).take_while(|(a, b)| a == b).count();
            if n > best {
                best = n;
                off = c;
            }
            k = self.next[k as usize];
            left -= 1;
        }
        (off, best)
    }
}

fn varint(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn insert(out: &mut Vec<u8>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = bytes.len().min(127);
        out.push(n as u8);
        out.extend_from_slice(&bytes[..n]);
        bytes = &bytes[n..];
    }
}

fn copy(out: &mut Vec<u8>, mut off: usize, mut len: usize) {
    while len > 0 {
        let n = len.min(MAX_COPY);
        let at = out.len();
        out.push(0x80);
        for i in 0..4 {
            let b = (off >> (8 * i)) as u8;
            if b != 0 {
                out[at] |= 1 << i;
                out.push(b);
            }
        }
        for i in 0..3 {
            let b = (n >> (8 * i)) as u8;
            if b != 0 {
                out[at] |= 0x10 << i;
                out.push(b);
            }
        }
        off += n;
        len -= n;
    }
}

fn read_varint(d: &[u8], p: &mut usize) -> Option<usize> {
    let mut n = 0usize;
    for shift in (0..64).step_by(7) {
        let b = *d.get(*p)?;
        *p += 1;
        n |= usize::from(b & 0x7f).checked_shl(shift)?;
        if b & 0x80 == 0 {
            return Some(n);
        }
    }
    None
}

/// Apply `delta` to `base`. `max` bounds the result; any malformed delta is
/// an error naming what is wrong, never a panic.
pub(crate) fn apply(base: &[u8], delta: &[u8], max: usize) -> Result<Vec<u8>, &'static str> {
    let mut p = 0;
    if read_varint(delta, &mut p).ok_or("truncated delta header")? != base.len() {
        return Err("delta base size mismatch");
    }
    let target = read_varint(delta, &mut p).ok_or("truncated delta header")?;
    if target > max {
        return Err("delta result too large");
    }
    // A damaged header must not reserve memory the delta cannot fill.
    let mut out = Vec::with_capacity(target.min(base.len() + delta.len()));
    while p < delta.len() {
        let op = delta[p];
        p += 1;
        if op & 0x80 != 0 {
            let (mut off, mut len) = (0usize, 0usize);
            for i in 0..4 {
                if op & (1 << i) != 0 {
                    off |= usize::from(*delta.get(p).ok_or("truncated copy")?) << (8 * i);
                    p += 1;
                }
            }
            for i in 0..3 {
                if op & (0x10 << i) != 0 {
                    len |= usize::from(*delta.get(p).ok_or("truncated copy")?) << (8 * i);
                    p += 1;
                }
            }
            if len == 0 {
                len = 0x10000;
            }
            let end = off.checked_add(len).ok_or("copy out of range")?;
            out.extend_from_slice(base.get(off..end).ok_or("copy out of range")?);
        } else if op != 0 {
            let n = usize::from(op);
            out.extend_from_slice(delta.get(p..p + n).ok_or("truncated insert")?);
            p += n;
        } else {
            return Err("delta opcode 0");
        }
        if out.len() > target {
            return Err("delta overruns its result");
        }
    }
    if out.len() != target {
        return Err("delta result size mismatch");
    }
    Ok(out)
}

#[cfg(test)]
#[path = "tests/delta.rs"]
mod tests;
