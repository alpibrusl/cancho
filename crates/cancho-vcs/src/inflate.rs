//! zlib decoding and a store-only zlib encoder, for `import-git` and
//! `export-git` (`docs/vcs-cli.md` section 7). Git objects are zlib
//! streams; reading them is the work the design names, and the workspace
//! has no decoder in `cancho-vcs`'s own dependencies (`flate2` is only
//! present beneath the code generator). Input is untrusted bytes from a
//! directory the user pointed at, so every path returns an error and none
//! can panic or allocate without bound: output is capped by the caller.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InflateError(pub(crate) &'static str);

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, n: u32) -> Result<(), InflateError> {
        while self.nbits < n {
            let b = *self.data.get(self.pos).ok_or(InflateError("truncated"))?;
            self.pos += 1;
            self.bit |= u32::from(b) << self.nbits;
            self.nbits += 8;
        }
        Ok(())
    }
    fn take(&mut self, n: u32) -> Result<u32, InflateError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Ok(v)
    }
}

/// A canonical Huffman code, decoded bit by bit (count/symbol tables).
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman, InflateError> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left <<= 1;
            left -= i32::from(c);
            if left < 0 {
                return Err(InflateError("over-subscribed code"));
            }
        }
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + count[i];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16, InflateError> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.count[len]);
            if code - count < first {
                return self
                    .symbol
                    .get((index + (code - first)) as usize)
                    .copied()
                    .ok_or(InflateError("bad symbol"));
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError("bad code"))
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
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

fn codes(
    bits: &mut Bits,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    cap: usize,
) -> Result<(), InflateError> {
    loop {
        let sym = lit.decode(bits)? as usize;
        match sym {
            0..=255 => {
                if out.len() >= cap {
                    return Err(InflateError("output too large"));
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let i = sym - 257;
                let len = LEN_BASE[i] as usize + bits.take(u32::from(LEN_EXTRA[i]))? as usize;
                let d = dist.decode(bits)? as usize;
                if d >= 30 {
                    return Err(InflateError("bad distance symbol"));
                }
                let distance = DIST_BASE[d] as usize + bits.take(u32::from(DIST_EXTRA[d]))? as usize;
                if distance > out.len() {
                    return Err(InflateError("distance too far"));
                }
                if out.len() + len > cap {
                    return Err(InflateError("output too large"));
                }
                for _ in 0..len {
                    out.push(out[out.len() - distance]);
                }
            }
            _ => return Err(InflateError("bad length symbol")),
        }
    }
}

/// Inflate a raw deflate stream; returns the bytes and how many input
/// bytes the stream used.
fn inflate_raw(data: &[u8], cap: usize) -> Result<(Vec<u8>, usize), InflateError> {
    let mut bits = Bits { data, pos: 0, bit: 0, nbits: 0 };
    let mut out = Vec::new();
    loop {
        let last = bits.take(1)?;
        match bits.take(2)? {
            0 => {
                bits.bit = 0;
                bits.nbits = 0;
                let p = bits.pos;
                let hdr = data.get(p..p + 4).ok_or(InflateError("truncated"))?;
                let len = u16::from_le_bytes([hdr[0], hdr[1]]);
                let nlen = u16::from_le_bytes([hdr[2], hdr[3]]);
                if len != !nlen {
                    return Err(InflateError("stored length mismatch"));
                }
                let body = data
                    .get(p + 4..p + 4 + len as usize)
                    .ok_or(InflateError("truncated"))?;
                if out.len() + body.len() > cap {
                    return Err(InflateError("output too large"));
                }
                out.extend_from_slice(body);
                bits.pos = p + 4 + len as usize;
            }
            1 => {
                let mut l = [0u8; 288];
                for (i, v) in l.iter_mut().enumerate() {
                    *v = match i {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    };
                }
                let lit = Huffman::new(&l)?;
                let dist = Huffman::new(&[5u8; 30])?;
                codes(&mut bits, &mut out, &lit, &dist, cap)?;
            }
            2 => {
                let nlen = bits.take(5)? as usize + 257;
                let ndist = bits.take(5)? as usize + 1;
                let ncode = bits.take(4)? as usize + 4;
                if nlen > 286 || ndist > 30 {
                    return Err(InflateError("too many codes"));
                }
                const ORDER: [usize; 19] =
                    [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                let mut lengths = [0u8; 19];
                for &o in ORDER.iter().take(ncode) {
                    lengths[o] = bits.take(3)? as u8;
                }
                let clen = Huffman::new(&lengths)?;
                let mut all = vec![0u8; nlen + ndist];
                let mut i = 0;
                while i < nlen + ndist {
                    let sym = clen.decode(&mut bits)?;
                    if sym < 16 {
                        all[i] = sym as u8;
                        i += 1;
                    } else {
                        let (value, rep) = match sym {
                            16 => {
                                if i == 0 {
                                    return Err(InflateError("repeat with no previous"));
                                }
                                (all[i - 1], 3 + bits.take(2)? as usize)
                            }
                            17 => (0, 3 + bits.take(3)? as usize),
                            _ => (0, 11 + bits.take(7)? as usize),
                        };
                        if i + rep > nlen + ndist {
                            return Err(InflateError("repeat overflows"));
                        }
                        for slot in &mut all[i..i + rep] {
                            *slot = value;
                        }
                        i += rep;
                    }
                }
                if all[256] == 0 {
                    return Err(InflateError("no end-of-block code"));
                }
                let lit = Huffman::new(&all[..nlen])?;
                let dist = Huffman::new(&all[nlen..])?;
                codes(&mut bits, &mut out, &lit, &dist, cap)?;
            }
            _ => return Err(InflateError("bad block type")),
        }
        if last == 1 {
            break;
        }
    }
    Ok((out, bits.pos))
}

/// Decode a zlib stream at the start of `data` (header, deflate, adler32).
/// Returns the bytes and the number of input bytes the stream occupied, so
/// a pack reader knows where the next object starts. `cap` bounds the
/// output.
pub(crate) fn zlib_decode(data: &[u8], cap: usize) -> Result<(Vec<u8>, usize), InflateError> {
    if data.len() < 2 {
        return Err(InflateError("truncated"));
    }
    if data[0] & 0x0f != 8 || (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 != 0 {
        return Err(InflateError("not a zlib stream"));
    }
    if data[1] & 0x20 != 0 {
        return Err(InflateError("preset dictionary"));
    }
    let (out, used) = inflate_raw(&data[2..], cap)?;
    let end = 2 + used;
    let sum = data.get(end..end + 4).ok_or(InflateError("truncated"))?;
    if u32::from_be_bytes([sum[0], sum[1], sum[2], sum[3]]) != adler32(&out) {
        return Err(InflateError("adler32 mismatch"));
    }
    Ok((out, end + 4))
}

pub(crate) fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

/// A valid zlib stream that does not compress (stored blocks). Git
/// accepts it for any loose object; the exported repository is derived
/// data, and the object name depends on the content, not the encoding.
pub(crate) fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    let mut chunks = data.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        out.push(u8::from(chunks.peek().is_none()));
        out.extend_from_slice(&(c.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_round_trip() {
        for data in [&b""[..], b"hello", &vec![7u8; 70_000]] {
            let z = zlib_store(data);
            let (back, used) = zlib_decode(&z, 1 << 20).unwrap();
            assert_eq!(back, data);
            assert_eq!(used, z.len());
        }
    }

    #[test]
    fn fixed_and_dynamic_streams() {
        // `printf 'hello hello hello hello\n' | zlib` (fixed Huffman), made with Python's zlib.
        let fixed = [0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0xb9, 0x00, 0x70, 0xbe, 0x08, 0xbb];
        let (out, _) = zlib_decode(&fixed, 1000).unwrap();
        assert_eq!(out, b"hello hello hello hello\n");
    }

    #[test]
    fn dynamic_huffman_stream() {
        let z = [0x78, 0xda, 0x75, 0x96, 0x51, 0x72, 0x83, 0x30, 0x0c, 0x44, 0xaf, 0xc2, 0xd5, 0xa8, 0x6b, 0x9a, 0x99, 0x12, 0xa6, 0x43, 0xc8, 0xfd, 0x6b, 0x5b, 0x02, 0xbd, 0xc5, 0xce, 0x4f, 0x30, 0xb6, 0xb4, 0x5a, 0x4b, 0x2b, 0x91, 0xf4, 0xc8, 0xe9, 0x77, 0x5a, 0xb6, 0x69, 0xcd, 0xc7, 0x94, 0x97, 0x25, 0xa7, 0x63, 0x9a, 0xd7, 0xbf, 0xc7, 0x3c, 0xbd, 0x8e, 0xfd, 0x5d, 0x5e, 0xbe, 0xf3, 0x7a, 0xcc, 0xbe, 0xf5, 0x33, 0x3f, 0x9f, 0xf3, 0xf4, 0x95, 0xcb, 0x46, 0x35, 0x17, 0x8b, 0x3d, 0x1f, 0xef, 0x7d, 0x3b, 0x21, 0x9a, 0x4d, 0x6a, 0xd8, 0x04, 0x10, 0xd3, 0x12, 0xd4, 0x00, 0xfd, 0x1d, 0xe8, 0xb6, 0x34, 0x7f, 0xfb, 0xf5, 0x58, 0x76, 0x60, 0xbf, 0x06, 0x49, 0x60, 0xfb, 0xa5, 0x8d, 0x5f, 0xcc, 0x0e, 0x9c, 0x1b, 0xad, 0xee, 0x84, 0x0c, 0xad, 0xba, 0x08, 0xa9, 0x0b, 0xec, 0xba, 0x7b, 0x59, 0x93, 0x9e, 0xf9, 0xd9, 0xba, 0x1e, 0x37, 0x3b, 0x0f, 0x5e, 0x1e, 0xce, 0xbe, 0xbe, 0x19, 0x94, 0x6f, 0xc8, 0xad, 0x0c, 0x63, 0x44, 0xc2, 0x36, 0x9c, 0xbe, 0xef, 0xe1, 0x98, 0x14, 0x1c, 0x91, 0xf5, 0x62, 0x19, 0x1a, 0x2f, 0x5b, 0x82, 0x93, 0x23, 0x63, 0x19, 0x9c, 0x51, 0x49, 0xbf, 0x8c, 0x61, 0x7b, 0x6c, 0x00, 0xc2, 0x3d, 0xec, 0x8d, 0x82, 0x7b, 0x52, 0x1c, 0x91, 0x9c, 0xae, 0xee, 0x11, 0x9c, 0x89, 0x01, 0x11, 0xcd, 0x04, 0x15, 0x20, 0x44, 0x1d, 0x44, 0x2a, 0x49, 0xd1, 0x20, 0x03, 0xd4, 0x84, 0x61, 0x88, 0x34, 0x41, 0x0e, 0xe9, 0x27, 0xe2, 0x5d, 0xd3, 0x8e, 0x4d, 0x2c, 0x67, 0xad, 0xa5, 0x47, 0x1f, 0x2d, 0xdb, 0x27, 0xd2, 0x2d, 0xbc, 0x36, 0x98, 0x44, 0x09, 0x32, 0xcd, 0x32, 0xf0, 0xdc, 0x58, 0x1f, 0x97, 0x3e, 0xeb, 0x82, 0xda, 0xd1, 0x29, 0x10, 0x5c, 0x8c, 0x58, 0x04, 0x19, 0xe8, 0x2d, 0x7c, 0x34, 0xf7, 0xc1, 0xa1, 0xee, 0xfb, 0x19, 0xa4, 0xdb, 0x75, 0xc4, 0x69, 0x26, 0xd7, 0x94, 0x97, 0x10, 0x5b, 0xc4, 0x60, 0x82, 0xfa, 0xd9, 0xa1, 0x72, 0xb9, 0x3a, 0x37, 0x38, 0x53, 0x41, 0x74, 0x1f, 0x14, 0x91, 0x46, 0x83, 0x39, 0xc9, 0x83, 0xdb, 0xb4, 0x94, 0x6b, 0xca, 0x40, 0x12, 0x05, 0x21, 0x0f, 0xd2, 0xde, 0xdd, 0x44, 0x8d, 0x3c, 0x74, 0xf3, 0xa2, 0x5c, 0x89, 0xc2, 0x45, 0x05, 0x64, 0x92, 0xa4, 0x73, 0xf6, 0x9f, 0x0d, 0xd9, 0xeb, 0x24, 0xf1, 0xf3, 0x20, 0x0d, 0x20, 0xe4, 0xe0, 0x31, 0x18, 0xd4, 0x66, 0x23, 0x0e, 0x83, 0xa4, 0xe1, 0x9a, 0xc1, 0x94, 0xd7, 0x08, 0x3e, 0x12, 0x34, 0x26, 0x8c, 0xc4, 0xd1, 0x87, 0x8c, 0xbc, 0xae, 0xcb, 0x3e, 0x2a, 0x47, 0x5a, 0x32, 0xb2, 0x1a, 0x09, 0x97, 0x53, 0xa9, 0x70, 0x37, 0xb4, 0xe4, 0xca, 0x97, 0xe8, 0x24, 0xa7, 0xa3, 0x96, 0x0a, 0x95, 0xca, 0x1d, 0x28, 0xd9, 0x3b, 0x03, 0x61, 0x2d, 0xc1, 0xa5, 0xf0, 0x8d, 0x1b, 0x59, 0x98, 0xdf, 0xa8, 0xfa, 0xa3, 0x3f, 0x06, 0x67, 0xa7, 0xb2, 0x3b, 0x38, 0x5d, 0xc3, 0x8d, 0x5f, 0x12, 0xf9, 0x06, 0x4b, 0x8d, 0x45, 0x1f, 0xc2, 0x5a, 0x3b, 0x43, 0x2a, 0xba, 0x6c, 0xff, 0x97, 0x6a, 0x2e, 0x9b];
        let words = ["alpha", "beta", "gamma", "delta", "fn", "let", "return", "struct", "effect", "check"];
        let (out, used) = zlib_decode(&z, 1 << 16).unwrap();
        assert_eq!(used, z.len());
        let seq = "check fn let effect alpha struct delta alpha gamma beta let struct delta return effect beta check delta alpha delta return fn gamma return gamma beta gamma check check struct gamma gamma alpha alpha delta delta gamma gamma fn let delta effect delta gamma delta return fn alpha let return gamma gamma fn beta let fn check check alpha check let beta fn let fn struct let gamma struct struct gamma alpha fn alpha let return alpha effect return let return check alpha struct alpha gamma check delta beta delta struct let effect let effect fn struct beta check let fn alpha return beta delta let effect check let gamma let fn effect beta fn let fn gamma beta gamma fn struct gamma alpha beta check effect return alpha delta check let fn struct return gamma alpha alpha struct let delta gamma check gamma return beta gamma return let gamma alpha return fn gamma struct check gamma effect struct struct let struct fn fn struct return gamma beta return effect gamma struct let gamma beta struct fn effect effect effect let beta let check alpha fn let effect fn struct fn fn let gamma check alpha struct effect fn let fn struct fn effect let let fn let return let gamma struct let let effect gamma effect gamma delta let struct fn beta return gamma check check effect return fn check effect fn alpha delta gamma check struct check gamma delta gamma alpha struct delta gamma alpha gamma beta let gamma struct delta effect alpha return struct let return check beta check delta delta let alpha let return fn return beta effect let alpha effect check fn beta fn effect effect let check fn let gamma return return check effect let struct gamma gamma check return check struct delta gamma check beta let alpha return beta let check check effect gamma let check return return return delta struct fn struct return return gamma check check fn fn struct fn return alpha let fn struct fn gamma struct alpha beta check struct delta fn alpha gamma return alpha struct effect effect fn delta struct alpha delta struct fn gamma fn fn struct check struct effect check beta alpha gamma fn fn effect let check fn effect alpha struct let let check gamma alpha alpha fn effect struct beta effect delta alpha return return check check struct return struct return delta fn";
        let want: Vec<&str> = seq.split(' ').collect();
        assert!(want.iter().all(|w| words.contains(w)));
        assert_eq!(out, seq.as_bytes());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for seed in 0u32..2000 {
            let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
            let mut v = vec![0x78, 0x9c];
            for _ in 0..(seed % 40) {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                v.push((x >> 16) as u8);
            }
            let _ = zlib_decode(&v, 1 << 16);
        }
        assert!(zlib_decode(&[0x78, 0x9c, 0xff], 10).is_err());
    }
}
