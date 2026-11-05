use super::*;

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

fn delta(base: &[u8], target: &[u8]) -> Vec<u8> {
    let d = DeltaIndex::new(base).encode(base, target, usize::MAX).expect("no bound");
    assert_eq!(apply(base, &d, target.len()).as_deref(), Ok(target), "round trip");
    d
}

/// `base` with random insertions, deletions and replacements.
fn mutate(rng: &mut Lcg, base: &[u8], edits: usize) -> Vec<u8> {
    let mut t = base.to_vec();
    for _ in 0..edits {
        let at = rng.below(t.len() + 1);
        match rng.below(3) {
            0 => {
                let n = rng.below(40);
                let ins: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                t.splice(at..at, ins);
            }
            1 => {
                let n = rng.below(40).min(t.len() - at);
                t.drain(at..at + n);
            }
            _ => {
                if at < t.len() {
                    t[at] ^= 0x5a;
                }
            }
        }
    }
    t
}

#[test]
fn similar_files_make_small_deltas() {
    let mut rng = Lcg(7);
    for round in 0..60 {
        let len = 100 + rng.below(if round % 6 == 0 { 300_000 } else { 20_000 });
        let base: Vec<u8> = (0..len).map(|_| b"abcdefghij\n"[rng.below(11)]).collect();
        let edits = 1 + rng.below(10);
        let target = mutate(&mut rng, &base, edits);
        let d = delta(&base, &target);
        if len > 5_000 {
            assert!(d.len() < target.len() / 10, "{len} bytes, {edits} edits: delta {}", d.len());
        }
    }
}

#[test]
fn edge_cases_round_trip() {
    let mut rng = Lcg(8);
    let some: Vec<u8> = (0..5000).map(|_| rng.next() as u8).collect();
    let cases: [(&[u8], &[u8]); 6] = [
        (b"", b""),
        (b"", b"new"),
        (&some[..], b""),
        (&some[..], &some[..]),
        (&some[..10], &some[..]),
        (&some[..], &some[100..200]),
    ];
    for (b, t) in cases {
        delta(b, t);
    }
    // Unrelated bytes: a delta is all inserts and still exact.
    let other: Vec<u8> = (0..5000).map(|_| rng.next() as u8).collect();
    delta(&some, &other);
    // A long copy is split into pieces a reader accepts (sizes up to 2^24 - 1).
    let big = vec![9u8; (1 << 24) + 100];
    let d = delta(&big, &big);
    assert!(d.len() < 64);
}

#[test]
fn a_bound_refuses_a_delta_that_would_not_pay() {
    let mut rng = Lcg(9);
    let a: Vec<u8> = (0..4000).map(|_| rng.next() as u8).collect();
    let b: Vec<u8> = (0..4000).map(|_| rng.next() as u8).collect();
    assert!(DeltaIndex::new(&a).encode(&a, &b, 1000).is_none());
}

#[test]
fn malformed_deltas_are_errors_not_panics() {
    let base = b"0123456789".to_vec();
    assert_eq!(apply(&base, &[10, 3, 0x91, 2, 3], 100), Ok(b"234".to_vec()));
    assert!(apply(&base, &[9, 3, 0x91, 2, 3], 100).is_err(), "base size");
    assert!(apply(&base, &[10, 3, 0x91, 9, 3], 100).is_err(), "copy past the base");
    assert!(apply(&base, &[10, 3, 0], 100).is_err(), "opcode 0");
    assert!(apply(&base, &[10, 3, 5, b'a'], 100).is_err(), "truncated insert");
    assert!(apply(&base, &[10, 200, 1], 100).is_err(), "result over the bound");
    let mut rng = Lcg(10);
    for _ in 0..5000 {
        let n = rng.below(30);
        let d: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
        let _ = apply(&base, &d, 1 << 16);
    }
}
