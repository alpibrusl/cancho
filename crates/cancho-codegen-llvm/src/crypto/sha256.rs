//! SHA-256's compression function on the CPU's SHA instructions
//! (`docs/sha256-builtin.md` §4): `sha256rnds2`, `sha256msg1` and
//! `sha256msg2` on x86-64, `sha256h`, `sha256h2`, `sha256su0` and
//! `sha256su1` on aarch64.
//!
//! The eight words of the state are held in two `<4 x i32>` across every
//! block of a call. The instructions want them in different orders: aarch64
//! as `ABCD` and `EFGH`, x86-64 as `ABEF` and `CDGH` with the first word in
//! the highest lane, so the state is shuffled into the ISA's form once a
//! call and back once. The message schedule is sixteen groups of four
//! words, each group the instruction's own update of the four groups before
//! it; LLVM schedules them. Every loop bound is a block count, and a length
//! is public; nothing here branches on, or indexes memory by, a word of the
//! state or the message.

use super::Gen;
use std::fmt::Write;
use target_lexicon::{Architecture, OperatingSystem, Triple};

/// The instruction set a module's SHA-256 functions are written for.
#[derive(Clone, Copy)]
pub(super) struct ShaIsa {
    pub(super) x86: bool,
    /// The `"target-features"` of every function that holds an instruction.
    features: &'static str,
}

impl ShaIsa {
    pub(super) fn of(triple: &Triple) -> ShaIsa {
        match triple.architecture {
            Architecture::X86_64 => ShaIsa { x86: true, features: "+sha,+sse2,+ssse3,+sse4.1" },
            _ => ShaIsa { x86: false, features: "+sha2,+neon" },
        }
    }
}

const W: &str = "<4 x i32>";

/// FIPS 180-4 §4.2.2.
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

/// The round constants of group `g` (rounds `4g` to `4g + 3`) as a vector
/// constant. LLVM reads an `i32` literal as signed, so each is written as
/// its two's-complement value.
fn k_group(g: usize) -> String {
    let lanes: Vec<String> =
        (0..4).map(|j| format!("i32 {}", i64::from(K[4 * g + j] as i32))).collect();
    format!("<{}>", lanes.join(", "))
}

/// Where each of the eight words `A..H` (lanes 0 to 7 of the state as
/// loaded) goes in the ISA's two vectors, and back.
fn state_lanes(isa: ShaIsa) -> (&'static str, &'static str, &'static str) {
    if isa.x86 {
        // `[F, E, B, A]` and `[H, G, D, C]`: ABEF and CDGH, lane 0 lowest.
        (
            "<i32 5, i32 4, i32 1, i32 0>",
            "<i32 7, i32 6, i32 3, i32 2>",
            "<i32 3, i32 2, i32 7, i32 6, i32 1, i32 0, i32 5, i32 4>",
        )
    } else {
        (
            "<i32 0, i32 1, i32 2, i32 3>",
            "<i32 4, i32 5, i32 6, i32 7>",
            "<i32 0, i32 1, i32 2, i32 3, i32 4, i32 5, i32 6, i32 7>",
        )
    }
}

/// `@cancho_sha256_rounds(s0, s1, w0, w1, w2, w3)`: one block's 64 rounds
/// and the feed-forward addition, on the state in the ISA's form and the
/// block's sixteen words, big-endian already undone.
pub(super) fn rounds(isa: ShaIsa) -> String {
    let f = isa.features;
    let mut g = Gen::new();
    let mut w: Vec<String> = (0..4).map(|k| format!("%w{k}")).collect();
    let (mut s0, mut s1) = ("%s0".to_owned(), "%s1".to_owned());
    for group in 0..16 {
        if group >= 4 {
            let (a, b, c, d) = (&w[group - 4], &w[group - 3], &w[group - 2], &w[group - 1]);
            let next = if isa.x86 {
                // W[g] = msg2(msg1(W[g-4], W[g-3]) + (W[g-2] W[g-1] >> 4 bytes), W[g-1]).
                let m1 = g.op(&format!("call {W} @llvm.x86.sha256msg1({W} {a}, {W} {b})"));
                let mid = g.op(&format!(
                    "shufflevector {W} {c}, {W} {d}, <4 x i32> <i32 1, i32 2, i32 3, i32 4>"
                ));
                let sum = g.op(&format!("add {W} {m1}, {mid}"));
                g.op(&format!("call {W} @llvm.x86.sha256msg2({W} {sum}, {W} {d})"))
            } else {
                let u0 =
                    g.op(&format!("call {W} @llvm.aarch64.crypto.sha256su0({W} {a}, {W} {b})"));
                g.op(&format!(
                    "call {W} @llvm.aarch64.crypto.sha256su1({W} {u0}, {W} {c}, {W} {d})"
                ))
            };
            w.push(next);
        }
        let wk = g.op(&format!("add {W} {}, {}", w[group], k_group(group)));
        if isa.x86 {
            // Two rounds on the low two lanes, then two on the high two; the
            // old ABEF becomes the new CDGH each time.
            let n1 = g.op(&format!("call {W} @llvm.x86.sha256rnds2({W} {s1}, {W} {s0}, {W} {wk})"));
            let hi = g.op(&format!(
                "shufflevector {W} {wk}, {W} poison, <4 x i32> <i32 2, i32 3, i32 0, i32 0>"
            ));
            let n0 = g.op(&format!("call {W} @llvm.x86.sha256rnds2({W} {s0}, {W} {n1}, {W} {hi})"));
            (s0, s1) = (n0, n1);
        } else {
            let abcd = g.op(&format!(
                "call {W} @llvm.aarch64.crypto.sha256h({W} {s0}, {W} {s1}, {W} {wk})"
            ));
            let efgh = g.op(&format!(
                "call {W} @llvm.aarch64.crypto.sha256h2({W} {s1}, {W} {s0}, {W} {wk})"
            ));
            (s0, s1) = (abcd, efgh);
        }
    }
    let r0 = g.op(&format!("add {W} {s0}, %s0"));
    let r1 = g.op(&format!("add {W} {s1}, %s1"));
    let agg = g.op(&format!("insertvalue {{ {W}, {W} }} undef, {W} {r0}, 0"));
    let agg = g.op(&format!("insertvalue {{ {W}, {W} }} {agg}, {W} {r1}, 1"));
    let mut out = String::new();
    let _ = write!(
        out,
        "define internal {{ {W}, {W} }} @cancho_sha256_rounds({W} %s0, {W} %s1, {W} %w0, {W} %w1, {W} %w2, {W} %w3) alwaysinline \"target-features\"=\"{f}\" {{\nentry:\n{}  ret {{ {W}, {W} }} {agg}\n}}\n",
        g.out
    );
    out
}

/// The state's eight words loaded from `%state` into the ISA's form, as
/// `%s0init` and `%s1init`.
fn load_state(isa: ShaIsa) -> String {
    let (lo, hi, _) = state_lanes(isa);
    format!(
        "  %h64 = load <8 x i64>, ptr %state, align 8\n  %h32 = trunc <8 x i64> %h64 to <8 x i32>\n  %s0init = shufflevector <8 x i32> %h32, <8 x i32> poison, <4 x i32> {lo}\n  %s1init = shufflevector <8 x i32> %h32, <8 x i32> poison, <4 x i32> {hi}\n"
    )
}

/// `%f0` and `%f1` stored back, each word zero-extended into its `int`.
fn store_state(isa: ShaIsa) -> String {
    let (_, _, back) = state_lanes(isa);
    format!(
        "  %o32 = shufflevector {W} %f0, {W} %f1, <8 x i32> {back}\n  %o64 = zext <8 x i32> %o32 to <8 x i64>\n  store <8 x i64> %o64, ptr %state, align 8\n"
    )
}

/// `@cancho_sha256_blocks(state, data, n)`: `n` blocks of 64 bytes at
/// `data`, into the eight words at `state`. The caller checked the lengths.
pub(super) fn blocks(isa: ShaIsa) -> String {
    let f = isa.features;
    let mut g = String::new();
    let _ = write!(
        g,
        "define internal void @cancho_sha256_blocks(ptr %state, ptr %data, i64 %n) noinline \"target-features\"=\"{f}\" {{\nentry:\n"
    );
    g.push_str(&load_state(isa));
    g.push_str("  br label %head\nhead:\n  %i = phi i64 [ 0, %entry ], [ %i1, %body ]\n");
    g.push_str(&format!("  %s0 = phi {W} [ %s0init, %entry ], [ %n0, %body ]\n"));
    g.push_str(&format!("  %s1 = phi {W} [ %s1init, %entry ], [ %n1, %body ]\n"));
    g.push_str("  %more = icmp ult i64 %i, %n\n  br i1 %more, label %body, label %done\nbody:\n");
    g.push_str("  %off = shl i64 %i, 6\n  %p = getelementptr i8, ptr %data, i64 %off\n");
    for k in 0..4 {
        let _ = write!(
            g,
            "  %q{k} = getelementptr i8, ptr %p, i64 {}\n  %b{k} = load {W}, ptr %q{k}, align 1\n  %m{k} = call {W} @llvm.bswap.v4i32({W} %b{k})\n",
            16 * k
        );
    }
    let _ = write!(
        g,
        "  %r = call {{ {W}, {W} }} @cancho_sha256_rounds({W} %s0, {W} %s1, {W} %m0, {W} %m1, {W} %m2, {W} %m3)\n  %n0 = extractvalue {{ {W}, {W} }} %r, 0\n  %n1 = extractvalue {{ {W}, {W} }} %r, 1\n  %i1 = add i64 %i, 1\n  br label %head\ndone:\n  %f0 = phi {W} [ %s0, %head ]\n  %f1 = phi {W} [ %s1, %head ]\n"
    );
    g.push_str(&store_state(isa));
    g.push_str("  ret void\n}\n");
    g
}

/// `@cancho_sha256_block_words(state, block)`: one block given as sixteen
/// words in `int`s (their low 32 bits), into the eight words at `state`.
pub(super) fn block_words(isa: ShaIsa) -> String {
    let f = isa.features;
    let mut g = String::new();
    let _ = write!(
        g,
        "define internal void @cancho_sha256_block_words(ptr %state, ptr %block) noinline \"target-features\"=\"{f}\" {{\nentry:\n"
    );
    g.push_str(&load_state(isa));
    g.push_str(
        "  %m64 = load <16 x i64>, ptr %block, align 8\n  %m32 = trunc <16 x i64> %m64 to <16 x i32>\n",
    );
    for k in 0..4 {
        let lanes: Vec<String> = (0..4).map(|j| format!("i32 {}", 4 * k + j)).collect();
        let _ = writeln!(
            g,
            "  %m{k} = shufflevector <16 x i32> %m32, <16 x i32> poison, <4 x i32> <{}>",
            lanes.join(", ")
        );
    }
    let _ = write!(
        g,
        "  %r = call {{ {W}, {W} }} @cancho_sha256_rounds({W} %s0init, {W} %s1init, {W} %m0, {W} %m1, {W} %m2, {W} %m3)\n  %f0 = extractvalue {{ {W}, {W} }} %r, 0\n  %f1 = extractvalue {{ {W}, {W} }} %r, 1\n"
    );
    g.push_str(&store_state(isa));
    g.push_str("  ret void\n}\n");
    g
}

/// The intrinsics the functions above call (but `llvm.bswap.v4i32`, which
/// the AES functions declare too: `crate::crypto` declares it once).
pub(super) fn declarations(isa: ShaIsa) -> String {
    if isa.x86 {
        format!(
            "declare {W} @llvm.x86.sha256rnds2({W}, {W}, {W})\ndeclare {W} @llvm.x86.sha256msg1({W}, {W})\ndeclare {W} @llvm.x86.sha256msg2({W}, {W})\n"
        )
    } else {
        format!(
            "declare {W} @llvm.aarch64.crypto.sha256h({W}, {W}, {W})\ndeclare {W} @llvm.aarch64.crypto.sha256h2({W}, {W}, {W})\ndeclare {W} @llvm.aarch64.crypto.sha256su0({W}, {W})\ndeclare {W} @llvm.aarch64.crypto.sha256su1({W}, {W}, {W})\n"
        )
    }
}

/// `i8` 1 when the CPU has every instruction the SHA-256 builtins use, asked
/// once and kept in `@cancho_hw_sha_cache` (0 not asked, 1 no, 2 yes), as
/// `hw_aes_gcm` keeps its own.
pub(super) fn hw_sha256(triple: &Triple) -> String {
    let (names, probe) = match (triple.architecture, triple.operating_system) {
        // cpuid leaf 0's eax is the highest leaf; leaf 7, sub-leaf 0, ebx
        // bit 29 is SHA; leaf 1's ecx bit 9 is SSSE3 and bit 19 SSE4.1. Leaf
        // 7 is asked only when it exists: on a CPU whose highest leaf is
        // below 7 the answer to it is not defined.
        (Architecture::X86_64, _) => (
            String::new(),
            "\
  %l0 = call { i32, i32, i32, i32 } asm sideeffect \"cpuid\", \"={ax},={bx},={cx},={dx},{ax},{cx}\"(i32 0, i32 0)
  %max = extractvalue { i32, i32, i32, i32 } %l0, 0
  %l1 = call { i32, i32, i32, i32 } asm sideeffect \"cpuid\", \"={ax},={bx},={cx},={dx},{ax},{cx}\"(i32 1, i32 0)
  %ecx = extractvalue { i32, i32, i32, i32 } %l1, 2
  %want1 = and i32 %ecx, 524800
  %has1 = icmp eq i32 %want1, 524800
  %leaf7 = icmp uge i32 %max, 7
  %ask7 = select i1 %leaf7, i32 7, i32 0
  %l7 = call { i32, i32, i32, i32 } asm sideeffect \"cpuid\", \"={ax},={bx},={cx},={dx},{ax},{cx}\"(i32 %ask7, i32 0)
  %ebx = extractvalue { i32, i32, i32, i32 } %l7, 1
  %sha = and i32 %ebx, 536870912
  %has7 = icmp ne i32 %sha, 0
  %has7b = and i1 %has7, %leaf7
  %has = and i1 %has1, %has7b
"
            .to_owned(),
        ),
        (_, OperatingSystem::Darwin(_)) => (
            "@cancho_sha256_name = private unnamed_addr constant [28 x i8] c\"hw.optional.arm.FEAT_SHA256\\00\"\n"
                .to_owned(),
            "\
  %sha = alloca i32
  %size = alloca i64
  store i32 0, ptr %sha
  store i64 4, ptr %size
  %s1 = call i32 @sysctlbyname(ptr @cancho_sha256_name, ptr %sha, ptr %size, ptr null, i64 0)
  %v = load i32, ptr %sha
  %r1 = icmp eq i32 %s1, 0
  %h1 = icmp ne i32 %v, 0
  %has = and i1 %r1, %h1
"
            .to_owned(),
        ),
        // getauxval(AT_HWCAP): HWCAP_SHA2 is bit 6.
        _ => (
            String::new(),
            "\
  %hwcap = call i64 @getauxval(i64 16)
  %want = and i64 %hwcap, 64
  %has = icmp ne i64 %want, 0
"
            .to_owned(),
        ),
    };
    format!(
        "@cancho_hw_sha_cache = internal global i8 0\n{names}\
define internal i8 @cancho_hw_sha256() {{\n\
entry:\n\
  %cached = load atomic i8, ptr @cancho_hw_sha_cache monotonic, align 1\n\
  %known = icmp ne i8 %cached, 0\n\
  br i1 %known, label %answer, label %ask\n\
ask:\n\
{probe}\
  %fresh = select i1 %has, i8 2, i8 1\n\
  store atomic i8 %fresh, ptr @cancho_hw_sha_cache monotonic, align 1\n\
  br label %answer\n\
answer:\n\
  %value = phi i8 [ %cached, %entry ], [ %fresh, %ask ]\n\
  %yes = icmp eq i8 %value, 2\n\
  %result = zext i1 %yes to i8\n\
  ret i8 %result\n\
}}\n"
    )
}
