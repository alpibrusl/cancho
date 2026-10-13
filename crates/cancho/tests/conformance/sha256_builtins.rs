//! The SHA-256 builtins (`docs/sha256-builtin.md` §7), through
//! `tests/programs/sha256_builtins_driver.cho`: FIPS 180-4's one- and
//! two-block examples, and a differential against the compression function
//! written below from FIPS 180-4 §6.2.2, over random states (words with bits
//! above the 32nd too, which the builtins ignore) and every block count to
//! 20. On LLVM, `hw_sha256()` must answer what the CPU says (true on an
//! aarch64 host, and on an x86-64 Linux host whose `/proc/cpuinfo` lists
//! `sha_ni`), so a silent fall back to software cannot hide; on Cranelift it
//! answers false and the builtins trap.

use super::json::feed;
use super::*;

fn build_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("sha256-builtins-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/sha256_builtins_driver.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

/// A deterministic generator, so a failure names a case that reruns.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u64; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// FIPS 180-4 §6.2.2, as written: one block of sixteen words into `h`.
fn compress(h: &mut [u32; 8], m: &[u32; 16]) {
    let mut w = [0u32; 64];
    w[..16].copy_from_slice(m);
    for t in 16..64 {
        let s0 = w[t - 15].rotate_right(7) ^ w[t - 15].rotate_right(18) ^ (w[t - 15] >> 3);
        let s1 = w[t - 2].rotate_right(17) ^ w[t - 2].rotate_right(19) ^ (w[t - 2] >> 10);
        w[t] = w[t - 16].wrapping_add(s0).wrapping_add(w[t - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for t in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[t]).wrapping_add(w[t]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(y);
    }
}

fn words(w: &[u64]) -> String {
    w.iter().map(|x| format!("{x:016x}")).collect()
}

fn bytes(b: &[u8]) -> String {
    if b.is_empty() { "-".to_owned() } else { b.iter().map(|x| format!("{x:02x}")).collect() }
}

/// What the builtins answer for a state of 64-bit `ints` and these blocks:
/// each word's low 32 bits compressed, the result zero-extended.
fn expect(state: &[u64; 8], blocks: &[[u32; 16]]) -> String {
    let mut h: [u32; 8] = state.map(|x| x as u32);
    for m in blocks {
        compress(&mut h, m);
    }
    words(&h.map(u64::from))
}

fn run(exe: &Path, lines: &[String]) -> Vec<String> {
    let mut input = lines.join("\n");
    input.push('\n');
    let out = feed(exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect()
}

/// What this machine's CPU says it has, read the operating system's way.
fn cpu_has_sha() -> Option<bool> {
    if cfg!(target_arch = "aarch64") {
        // Every Apple CPU, and every aarch64 runner this suite has, has FEAT_SHA256.
        return Some(true);
    }
    let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let flags = info.lines().find(|l| l.starts_with("flags"))?;
    Some(flags.split_whitespace().any(|f| f == "sha_ni"))
}

#[test]
fn the_sha256_builtins_agree_with_fips_180_4_and_the_reference_on_llvm() {
    let (dir, exe) = build_driver("agree", "llvm");
    let hw = run(&exe, &["H".to_owned()]);
    if let Some(want) = cpu_has_sha() {
        assert_eq!(hw, [if want { "1" } else { "0" }], "hw_sha256() against the CPU's own answer");
    }
    if hw != ["1"] {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    let mut lines = Vec::new();
    let mut want = Vec::new();
    // FIPS 180-4's examples (`abc`, one block; the 56-byte message, two),
    // padded by hand, through both builtins.
    let mut abc = [0u8; 64];
    abc[..3].copy_from_slice(b"abc");
    abc[3] = 0x80;
    abc[63] = 24;
    lines.push(format!("B {} {}", words(&H0), bytes(&abc)));
    want.push(words(&[
        0xba7816bf, 0x8f01cfea, 0x414140de, 0x5dae2223, 0xb00361a3, 0x96177a9c, 0xb410ff61,
        0xf20015ad,
    ]));
    let mut m = [0u32; 16];
    m[0] = 0x6162_6380;
    m[15] = 24;
    lines.push(format!("W {} {}", words(&H0), words(&m.map(u64::from))));
    want.push(want[0].clone());
    let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    let mut two = [0u8; 128];
    two[..56].copy_from_slice(msg);
    two[56] = 0x80;
    two[126] = 0x01;
    two[127] = 0xc0;
    lines.push(format!("B {} {}", words(&H0), bytes(&two)));
    want.push(words(&[
        0x248d6a61, 0xd20638b8, 0xe5c02693, 0x0c3e6039, 0xa33ce459, 0x64ff2167, 0xf6ecedd4,
        0x19db06c1,
    ]));
    // No blocks: the state comes back as it was, cut to 32 bits.
    let odd: [u64; 8] = [u64::MAX, 1 << 32, 0, 0xffff_ffff, 7, 1 << 63, 0x1234_5678_9abc_def0, 3];
    lines.push(format!("B {} -", words(&odd)));
    want.push(words(&odd.map(|x| x & 0xffff_ffff)));
    // Random states (words with high bits set as well) and 0 to 20 blocks,
    // and single blocks as words.
    let mut rng = Rng(0x5a17_2026_1013_0445);
    for k in 0..400usize {
        let state: [u64; 8] = std::array::from_fn(|_| {
            let w = rng.next();
            if k % 3 == 0 { w } else { w & 0xffff_ffff }
        });
        let n = k % 21;
        let data: Vec<u8> = (0..64 * n).map(|_| rng.next() as u8).collect();
        let blocks: Vec<[u32; 16]> = data
            .chunks(64)
            .map(|c| {
                std::array::from_fn(|i| u32::from_be_bytes(c[4 * i..4 * i + 4].try_into().unwrap()))
            })
            .collect();
        lines.push(format!("B {} {}", words(&state), bytes(&data)));
        want.push(expect(&state, &blocks));
        let block: [u64; 16] = std::array::from_fn(|i| {
            let w = rng.next();
            if (k + i) % 4 == 0 { w } else { w & 0xffff_ffff }
        });
        lines.push(format!("W {} {}", words(&state), words(&block)));
        want.push(expect(&state, &[block.map(|x| x as u32)]));
    }
    // One long run: 16 KiB, a run the size of a TLS record.
    let state = H0;
    let data: Vec<u8> = (0..16384).map(|_| rng.next() as u8).collect();
    let blocks: Vec<[u32; 16]> = data
        .chunks(64)
        .map(|c| {
            std::array::from_fn(|i| u32::from_be_bytes(c[4 * i..4 * i + 4].try_into().unwrap()))
        })
        .collect();
    lines.push(format!("B {} {}", words(&state), bytes(&data)));
    want.push(expect(&state, &blocks));
    let got = run(&exe, &lines);
    assert_eq!(got.len(), lines.len(), "one answer per case");
    for ((line, w), g) in lines.iter().zip(&want).zip(&got) {
        assert_eq!(g, w, "{}", &line[..line.len().min(80)]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A state that is not 8 words, data that is not whole blocks, or a block
/// that is not 16 words traps, as an index out of bounds does, before any
/// instruction runs; the right sizes do not.
#[test]
fn a_wrong_length_traps_on_llvm() {
    let (dir, exe) = build_driver("traps", "llvm");
    if run(&exe, &["H".to_owned()]) != ["1"] {
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    for ok in ["b 8 0", "b 8 64", "b 8 640", "w 8 16"] {
        assert_eq!(run(&exe, &[ok.to_owned()]), ["0"], "{ok}");
    }
    for case in [
        "b 7 64", "b 9 64", "b 8 63", "b 8 65", "b 8 32", "b 8 96", "b 0 0", "w 7 16", "w 8 15",
        "w 8 17", "w 24 16",
    ] {
        let out = feed(&exe, format!("{case}\n").as_bytes());
        assert_ne!(out.status.code(), Some(0), "{case}: traps");
        assert!(out.stdout.is_empty(), "{case}: nothing was answered");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Cranelift has none of the instructions: `hw_sha256()` is false and a
/// builtin reached anyway traps (`docs/sha256-builtin.md` §5).
#[test]
fn on_cranelift_the_instructions_are_absent_and_the_builtins_trap() {
    let (dir, exe) = build_driver("absent", "cranelift");
    assert_eq!(run(&exe, &["H".to_owned()]), ["0"]);
    for case in [format!("B {} -", words(&H0)), "b 8 64".to_owned(), "w 8 16".to_owned()] {
        let out = feed(&exe, format!("{case}\n").as_bytes());
        assert_ne!(out.status.code(), Some(0), "{case}: a builtin on Cranelift traps");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
