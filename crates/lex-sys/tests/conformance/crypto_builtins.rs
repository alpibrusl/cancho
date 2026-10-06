//! The hardware AES and GHASH builtins (`docs/crypto-builtins.md` §7),
//! through `tests/programs/crypto_builtins_driver.ls`: FIPS 197's examples,
//! NIST's GCM test case 2 assembled from the two builtins, and a
//! differential against the plain reference implementations below over
//! random keys, blocks and lengths. On LLVM, `hw_aes_gcm()` must answer
//! true on this suite's two hosts (x86-64 Linux and aarch64 macOS), so a
//! silent fall back to software cannot hide; on Cranelift it answers false
//! and the block builtins trap.

use super::json::feed;
use super::*;

/// Each test its own directory: tests run in parallel, and each removes
/// its directory when done.
fn build_driver(test: &str, backend: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(&format!("crypto-builtins-{test}-{backend}"));
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std", "--backend", backend])
        .arg(repo_root().join("tests/programs/crypto_builtins_driver.ls"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2).map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()).collect()
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
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

// ---- The references: FIPS 197 and SP 800-38D as written, not fast ----

fn sbox() -> [u8; 256] {
    // The S-box from its definition: the inverse in GF(2^8), then the
    // affine map (FIPS 197 §5.1.1).
    let mul = |mut a: u8, mut b: u8| {
        let mut p = 0u8;
        while b != 0 {
            if b & 1 != 0 {
                p ^= a;
            }
            let carry = a & 0x80;
            a <<= 1;
            if carry != 0 {
                a ^= 0x1b;
            }
            b >>= 1;
        }
        p
    };
    let mut s = [0u8; 256];
    for (x, entry) in s.iter_mut().enumerate() {
        let inv = if x == 0 { 0 } else { (1..=255u8).find(|&y| mul(x as u8, y) == 1).unwrap() };
        let mut v = inv;
        for k in 1..5 {
            v ^= inv.rotate_left(k);
        }
        *entry = v ^ 0x63;
    }
    s
}

fn expand(key: &[u8], s: &[u8; 256]) -> (Vec<u8>, usize) {
    let nk = key.len() / 4;
    let nr = nk + 6;
    let mut w: Vec<[u8; 4]> = key.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    let mut rcon = 1u8;
    for i in nk..4 * (nr + 1) {
        let mut t = w[i - 1];
        if i % nk == 0 {
            t = [s[t[1] as usize] ^ rcon, s[t[2] as usize], s[t[3] as usize], s[t[0] as usize]];
            rcon = (rcon << 1) ^ if rcon & 0x80 != 0 { 0x1b } else { 0 };
        } else if nk > 6 && i % nk == 4 {
            t = t.map(|b| s[b as usize]);
        }
        let prev = w[i - nk];
        w.push([prev[0] ^ t[0], prev[1] ^ t[1], prev[2] ^ t[2], prev[3] ^ t[3]]);
    }
    (w.concat(), nr)
}

fn encrypt(rk: &[u8], nr: usize, block: &[u8], s: &[u8; 256]) -> Vec<u8> {
    let xtime = |b: u8| (b << 1) ^ if b & 0x80 != 0 { 0x1b } else { 0 };
    let mut st: Vec<u8> = block.iter().zip(&rk[..16]).map(|(a, b)| a ^ b).collect();
    for round in 1..=nr {
        let sub: Vec<u8> = st.iter().map(|&b| s[b as usize]).collect();
        // ShiftRows: byte (row r, column c) is at 4c + r.
        let mut sh = [0u8; 16];
        for c in 0..4 {
            for r in 0..4 {
                sh[4 * c + r] = sub[4 * ((c + r) % 4) + r];
            }
        }
        if round != nr {
            for c in 0..4 {
                let a = [sh[4 * c], sh[4 * c + 1], sh[4 * c + 2], sh[4 * c + 3]];
                let all = a[0] ^ a[1] ^ a[2] ^ a[3];
                for r in 0..4 {
                    sh[4 * c + r] = a[r] ^ all ^ xtime(a[r] ^ a[(r + 1) % 4]);
                }
            }
        }
        st = sh.iter().zip(&rk[16 * round..16 * round + 16]).map(|(a, b)| a ^ b).collect();
    }
    st
}

fn gf_mul(x: u128, y: u128) -> u128 {
    // SP 800-38D §6.3, bit by bit.
    let r = 0xe1u128 << 120;
    let (mut z, mut v) = (0u128, y);
    for i in 0..128 {
        if (x >> (127 - i)) & 1 == 1 {
            z ^= v;
        }
        v = if v & 1 == 1 { (v >> 1) ^ r } else { v >> 1 };
    }
    z
}

fn ghash(h: &[u8], y: &[u8], data: &[u8]) -> Vec<u8> {
    let hh = u128::from_be_bytes(h.try_into().unwrap());
    let mut acc = u128::from_be_bytes(y.try_into().unwrap());
    for block in data.chunks(16) {
        acc = gf_mul(acc ^ u128::from_be_bytes(block.try_into().unwrap()), hh);
    }
    acc.to_be_bytes().to_vec()
}

/// FIPS 197, NIST's GCM test case 2 from the builtins, and 600 random
/// cases of each against the references, on LLVM.
#[test]
fn the_hardware_builtins_agree_with_fips_nist_and_the_references_on_llvm() {
    let (dir, exe) = build_driver("agree", "llvm");
    let s = sbox();
    let mut lines = vec!["H".to_string()];
    let mut want = vec!["1".to_string()];
    // FIPS 197 Appendix C.1 and C.3.
    for (key, out) in [
        ((0..16).collect::<Vec<u8>>(), "69c4e0d86a7b0430d8cdb78070b4c55a"),
        ((0..32).collect::<Vec<u8>>(), "8ea2b7ca516745bfeafc49904b496089"),
    ] {
        let (rk, nr) = expand(&key, &s);
        lines.push(format!("A {nr} {} 00112233445566778899aabbccddeeff", hex(&rk)));
        want.push(out.to_string());
    }
    // NIST GCM test case 2 (all-zero key, IV and plaintext): H = AES(K, 0),
    // the tag AES(K, J0) XOR GHASH(C || lengths). H and AES(K, J0) are the
    // builtin's own answers, checked here against the standard's values.
    let (rk, nr) = expand(&[0u8; 16], &s);
    lines.push(format!("A {nr} {} {}", hex(&rk), "00".repeat(16)));
    want.push("66e94bd4ef8a2c3b884cfa59ca342b2e".to_string());
    lines.push(format!("A {nr} {} {}00000001", hex(&rk), "00".repeat(12)));
    want.push("58e2fccefa7e3061367f1d57a4e7455a".to_string());
    let lengths = format!("{}{:016x}", "00".repeat(8), 128);
    lines.push(format!(
        "G 66e94bd4ef8a2c3b884cfa59ca342b2e {} 0388dace60b6a392f328c2b971b2fe78{lengths}",
        "00".repeat(16)
    ));
    // The tag is ab6e47d42cec13bdf53a67b21257bddf = AES(K, J0) XOR this.
    let tag = unhex("ab6e47d42cec13bdf53a67b21257bddf");
    let ek = unhex("58e2fccefa7e3061367f1d57a4e7455a");
    want.push(hex(&tag.iter().zip(&ek).map(|(a, b)| a ^ b).collect::<Vec<u8>>()));

    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..600 {
        let size = [16, 24, 32][(rng.next() % 3) as usize];
        let key = rng.bytes(size);
        let block = rng.bytes(16);
        let (rk, nr) = expand(&key, &s);
        lines.push(format!("A {nr} {} {}", hex(&rk), hex(&block)));
        want.push(hex(&encrypt(&rk, nr, &block, &s)));
    }
    for _ in 0..600 {
        let (h, y) = (rng.bytes(16), rng.bytes(16));
        let blocks = 1 + (rng.next() % 12) as usize;
        let data = rng.bytes(16 * blocks);
        lines.push(format!("G {} {} {}", hex(&h), hex(&y), hex(&data)));
        want.push(hex(&ghash(&h, &y, &data)));
    }
    // The reference is checked too: FIPS 197's first example.
    let (rk, nr) = expand(&(0..16).collect::<Vec<u8>>(), &s);
    assert_eq!(
        hex(&encrypt(&rk, nr, &unhex("00112233445566778899aabbccddeeff"), &s)),
        "69c4e0d86a7b0430d8cdb78070b4c55a"
    );

    let mut input = lines.join("\n");
    input.push('\n');
    let out = feed(&exe, input.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let got: Vec<String> =
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect();
    assert_eq!(got.len(), want.len(), "one answer per case");
    for (k, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "case {k}: {}", lines[k]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Cranelift has none of the instructions: `hw_aes_gcm()` is false and a
/// block builtin reached anyway traps (`docs/crypto-builtins.md` §5).
#[test]
fn on_cranelift_the_hardware_is_absent_and_a_block_builtin_traps() {
    let (dir, exe) = build_driver("absent", "cranelift");
    let out = feed(&exe, b"H\n");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
    let block = format!("A 10 {} {}\n", "00".repeat(176), "00".repeat(16));
    let out = feed(&exe, block.as_bytes());
    assert_ne!(out.status.code(), Some(0), "a block builtin on Cranelift traps");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A wrong length or round count traps, as an index out of bounds does,
/// before any instruction runs (`docs/crypto-builtins.md` §3).
#[test]
fn a_wrong_length_or_round_count_traps_on_llvm() {
    let (dir, exe) = build_driver("traps", "llvm");
    for case in [
        format!("A 11 {} {}", "00".repeat(192), "00".repeat(16)),
        format!("A 10 {} {}", "00".repeat(160), "00".repeat(16)),
        format!("A 10 {} {}", "00".repeat(176), "00".repeat(15)),
        format!("G {} {} {}", "00".repeat(15), "00".repeat(16), "00".repeat(16)),
        format!("G {} {} {}", "00".repeat(16), "00".repeat(16), "00".repeat(17)),
    ] {
        let out = feed(&exe, format!("{case}\n").as_bytes());
        assert_ne!(out.status.code(), Some(0), "{case}: traps");
        assert!(out.stdout.is_empty(), "{case}: nothing written");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
