use super::*;
use crate::inflate::zlib_decode;

/// A small deterministic generator (no dependency for tests either).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }
}

fn round_trip(data: &[u8]) -> usize {
    let z = zlib_compress(data);
    let (back, used) = zlib_decode(&z, data.len() + 1).expect("our stream inflates");
    assert_eq!(back.len(), data.len());
    assert!(back == data, "round trip differs ({} bytes)", data.len());
    assert_eq!(used, z.len(), "the stream ends where the adler32 ends");
    // A valid zlib header: deflate, 32 KiB window, check bits.
    assert_eq!(z[0], 0x78);
    assert_eq!((u16::from(z[0]) << 8 | u16::from(z[1])) % 31, 0);
    z.len()
}

fn words(rng: &mut Lcg, n: usize) -> Vec<u8> {
    const W: [&str; 12] = [
        "fn ", "let ", "return ", "effect ", "module ", "pub ", "int", "{ ", "}\n", "(n)", "; ",
        "check ",
    ];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(W[rng.below(W.len())].as_bytes());
    }
    out.truncate(n);
    out
}

#[test]
fn empty_and_tiny_inputs() {
    for d in [&b""[..], b"a", b"ab", b"abc", b"aaaa", b"hello hello hello hello\n"] {
        round_trip(d);
    }
}

#[test]
fn random_bytes_round_trip_and_do_not_grow_much() {
    let mut rng = Lcg(1);
    for len in [1usize, 7, 100, 1000, 65_535, 65_536, 70_000, 300_000] {
        let d: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        let z = round_trip(&d);
        // Incompressible input is stored: five bytes per block, not more.
        assert!(z <= len + 6 + 5 * (len / 32_768 + 1), "{len} -> {z}");
    }
}

#[test]
fn repetitive_inputs_compress_well() {
    let run = vec![1u8; 16 * 1024 * 1024 + 1];
    let z = round_trip(&run);
    assert!(z < run.len() / 500, "a run of one byte: {z}");
    let mut rng = Lcg(2);
    let text = words(&mut rng, 200_000);
    let z = round_trip(&text);
    assert!(z < text.len() / 3, "word soup: {} -> {z}", text.len());
    let period: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
    let z = round_trip(&period);
    assert!(z < 2_000, "a period of 251: {z}");
}

#[test]
fn dynamic_blocks_beat_fixed_ones_on_skewed_text() {
    // Few distinct symbols: a dynamic code is far shorter than the fixed one.
    let mut rng = Lcg(3);
    let d: Vec<u8> = (0..50_000).map(|_| b"ab"[rng.below(2)]).collect();
    let z = round_trip(&d);
    assert!(z < d.len() / 5, "two symbols: {} -> {z}", d.len());
}

#[test]
fn property_mixed_structured_inputs() {
    // Random lengths, random mixtures of runs, copies from far back, words
    // and noise: every one inflates to itself.
    for seed in 0..300u64 {
        let mut rng = Lcg(seed);
        let target = rng.below(if seed % 10 == 0 { 200_000 } else { 5_000 });
        let mut d: Vec<u8> = Vec::with_capacity(target);
        while d.len() < target {
            match rng.below(4) {
                0 => {
                    let b = rng.next() as u8;
                    let n = rng.below(300);
                    d.extend(std::iter::repeat_n(b, n));
                }
                1 if !d.is_empty() => {
                    let from = rng.below(d.len());
                    let n = rng.below(400).min(d.len() - from);
                    let chunk = d[from..from + n].to_vec();
                    d.extend_from_slice(&chunk);
                }
                2 => {
                    let n = rng.below(500);
                    d.extend(words(&mut rng, n));
                }
                _ => {
                    let n = rng.below(50);
                    d.extend((0..n).map(|_| rng.next() as u8));
                }
            }
        }
        round_trip(&d);
    }
}

#[test]
fn many_blocks_and_far_matches() {
    // More symbols than one block holds, and matches at the window's edge.
    let mut rng = Lcg(4);
    let unit: Vec<u8> = (0..32_768).map(|_| rng.next() as u8).collect();
    let mut d = unit.clone();
    d.extend_from_slice(&unit);
    d.extend_from_slice(&unit[..1000]);
    let z = round_trip(&d);
    assert!(z < unit.len() + 2_000, "the second copy is matched at distance 32768: {z}");
    let lits: Vec<u8> = (0..200_000).map(|i| (i * 7 % 13) as u8 ^ rng.next() as u8 & 1).collect();
    round_trip(&lits);
}

#[test]
fn code_lengths_respect_their_limit() {
    // Fibonacci frequencies make the deepest unrestricted tree.
    let mut f = vec![0u32; 30];
    let (mut a, mut b) = (1u32, 1u32);
    for x in f.iter_mut() {
        *x = a;
        let c = a.saturating_add(b);
        a = b;
        b = c;
    }
    let l = code_lengths(&f, 15);
    assert!(l.iter().all(|&x| (1..=15).contains(&x)));
    let kraft: f64 = l.iter().map(|&x| 0.5f64.powi(i32::from(x))).sum();
    assert!(kraft <= 1.0 + 1e-9, "a prefix code: {kraft}");
    let l = code_lengths(&f[..19], 7);
    assert!(l.iter().all(|&x| (1..=7).contains(&x)));
}
