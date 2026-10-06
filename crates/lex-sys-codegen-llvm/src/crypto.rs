//! The hardware AES and carry-less-multiply builtins' out-of-line
//! functions (`docs/crypto-builtins.md` §4).
//!
//! Each builtin is a call to one of the functions below, defined only in a
//! module that calls it. Each carries the target features its instructions
//! need on its own definition, never the module's, so the rest of the
//! program stays at the baseline and runs on a CPU without them; LLVM will
//! not inline a function with more features into one with fewer, so the
//! features never reach a caller. Whether the CPU has them is
//! `@lexsys_hw_aes_gcm`'s answer, which `std.aes` and `std.gcm` branch on.
//!
//! `@lexsys_ghash_update` uses the instruction only for the 64-by-64-bit
//! carry-less products. The bit order and the reduction are plain integer
//! arithmetic on `i128` and `i256`: a block is read into the polynomial's
//! natural order (bit `i` the coefficient of `x^i`) by reversing the bits of
//! each byte, multiplied as two 64-bit halves (four products), and reduced
//! modulo `x^128 + x^7 + x^2 + x + 1` by folding the high half twice. Every
//! shift is by a constant, so none depends on a secret.

use target_lexicon::{Architecture, OperatingSystem, Triple};

/// Whether this target can have the instructions: x86-64, and aarch64 on
/// Linux and Darwin (where the operating system says whether the CPU has
/// them). Elsewhere `hw_aes_gcm()` is false and the block builtins trap.
pub(crate) fn hardware_target(triple: &Triple) -> bool {
    match triple.architecture {
        Architecture::X86_64 => true,
        Architecture::Aarch64(_) => {
            matches!(triple.operating_system, OperatingSystem::Linux | OperatingSystem::Darwin(_))
        }
        _ => false,
    }
}

fn features(triple: &Triple) -> &'static str {
    match triple.architecture {
        Architecture::X86_64 => "+aes,+pclmul,+ssse3,+sse2",
        _ => "+aes,+neon",
    }
}

/// The definitions a module needs, given its text: only those it calls.
pub(crate) fn definitions(triple: &Triple, text: &str) -> String {
    let mut out = String::new();
    if !hardware_target(triple) {
        return out;
    }
    if text.contains("@lexsys_hw_aes_gcm()") {
        out.push_str(&hw_aes_gcm(triple));
    }
    if text.contains("@lexsys_aes_encrypt_block(") {
        out.push_str(&aes_encrypt_block(triple));
    }
    if text.contains("@lexsys_ghash_update(") {
        out.push_str(&ghash_update(triple));
    }
    out
}

/// `i8` 1 when the CPU has every instruction the two block builtins use,
/// asked once and kept in `@lexsys_hw_cache` (0 not asked, 1 no, 2 yes).
/// The answer is the same for the life of the process, so two threads
/// asking at once both store the same value.
fn hw_aes_gcm(triple: &Triple) -> String {
    let (names, probe) = match (triple.architecture, triple.operating_system) {
        // cpuid leaf 1: ecx bit 25 AES, 1 PCLMULQDQ, 9 SSSE3.
        (Architecture::X86_64, _) => (
            String::new(),
            "\
  %regs = call { i32, i32, i32, i32 } asm sideeffect \"cpuid\", \"={ax},={bx},={cx},={dx},{ax},{cx}\"(i32 1, i32 0)
  %ecx = extractvalue { i32, i32, i32, i32 } %regs, 2
  %want = and i32 %ecx, 33554946
  %has = icmp eq i32 %want, 33554946
"
            .to_owned(),
        ),
        (_, OperatingSystem::Darwin(_)) => (
            "@lexsys_aes_name = private unnamed_addr constant [25 x i8] c\"hw.optional.arm.FEAT_AES\\00\"\n\
             @lexsys_pmull_name = private unnamed_addr constant [27 x i8] c\"hw.optional.arm.FEAT_PMULL\\00\"\n"
                .to_owned(),
            "\
  %aes = alloca i32
  %pmull = alloca i32
  %size = alloca i64
  store i32 0, ptr %aes
  store i32 0, ptr %pmull
  store i64 4, ptr %size
  %s1 = call i32 @sysctlbyname(ptr @lexsys_aes_name, ptr %aes, ptr %size, ptr null, i64 0)
  store i64 4, ptr %size
  %s2 = call i32 @sysctlbyname(ptr @lexsys_pmull_name, ptr %pmull, ptr %size, ptr null, i64 0)
  %a = load i32, ptr %aes
  %p = load i32, ptr %pmull
  %r1 = icmp eq i32 %s1, 0
  %r2 = icmp eq i32 %s2, 0
  %h1 = icmp ne i32 %a, 0
  %h2 = icmp ne i32 %p, 0
  %b1 = and i1 %r1, %r2
  %b2 = and i1 %h1, %h2
  %has = and i1 %b1, %b2
"
            .to_owned(),
        ),
        // getauxval(AT_HWCAP): HWCAP_AES is bit 3, HWCAP_PMULL bit 4.
        _ => (
            String::new(),
            "\
  %hwcap = call i64 @getauxval(i64 16)
  %want = and i64 %hwcap, 24
  %has = icmp eq i64 %want, 24
"
            .to_owned(),
        ),
    };
    format!(
        "@lexsys_hw_cache = internal global i8 0\n{names}\
define internal i8 @lexsys_hw_aes_gcm() {{\n\
entry:\n\
  %cached = load atomic i8, ptr @lexsys_hw_cache monotonic, align 1\n\
  %known = icmp ne i8 %cached, 0\n\
  br i1 %known, label %answer, label %ask\n\
ask:\n\
{probe}\
  %fresh = select i1 %has, i8 2, i8 1\n\
  store atomic i8 %fresh, ptr @lexsys_hw_cache monotonic, align 1\n\
  br label %answer\n\
answer:\n\
  %value = phi i8 [ %cached, %entry ], [ %fresh, %ask ]\n\
  %yes = icmp eq i8 %value, 2\n\
  %result = zext i1 %yes to i8\n\
  ret i8 %result\n\
}}\n"
    )
}

/// FIPS-197's cipher: `rounds + 1` round keys of 16 bytes at `%rk`, one
/// block at `%blk`, the result at `%out`. The caller checked every length.
fn aes_encrypt_block(triple: &Triple) -> String {
    let f = features(triple);
    match triple.architecture {
        // `AESENC` does AddRoundKey last, so the first key is XORed in
        // before the rounds and `AESENCLAST` ends with the last.
        Architecture::X86_64 => format!(
            "define internal void @lexsys_aes_encrypt_block(ptr %rk, i64 %rounds, ptr %blk, ptr %out) noinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %s0 = load <2 x i64>, ptr %blk, align 1\n\
  %k0 = load <2 x i64>, ptr %rk, align 1\n\
  %x0 = xor <2 x i64> %s0, %k0\n\
  br label %loop\n\
loop:\n\
  %i = phi i64 [ 1, %entry ], [ %i1, %body ]\n\
  %s = phi <2 x i64> [ %x0, %entry ], [ %s1, %body ]\n\
  %more = icmp ult i64 %i, %rounds\n\
  br i1 %more, label %body, label %end\n\
body:\n\
  %off = mul i64 %i, 16\n\
  %kp = getelementptr i8, ptr %rk, i64 %off\n\
  %k = load <2 x i64>, ptr %kp, align 1\n\
  %s1 = call <2 x i64> @llvm.x86.aesni.aesenc(<2 x i64> %s, <2 x i64> %k)\n\
  %i1 = add i64 %i, 1\n\
  br label %loop\n\
end:\n\
  %offl = mul i64 %rounds, 16\n\
  %klp = getelementptr i8, ptr %rk, i64 %offl\n\
  %kl = load <2 x i64>, ptr %klp, align 1\n\
  %r = call <2 x i64> @llvm.x86.aesni.aesenclast(<2 x i64> %s, <2 x i64> %kl)\n\
  store <2 x i64> %r, ptr %out, align 1\n\
  ret void\n\
}}\n\
declare <2 x i64> @llvm.x86.aesni.aesenc(<2 x i64>, <2 x i64>)\n\
declare <2 x i64> @llvm.x86.aesni.aesenclast(<2 x i64>, <2 x i64>)\n"
        ),
        // `AESE` does AddRoundKey first: AESE then AESMC for all but the
        // last round, AESE with the second-to-last key, XOR the last.
        _ => format!(
            "define internal void @lexsys_aes_encrypt_block(ptr %rk, i64 %rounds, ptr %blk, ptr %out) noinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %s0 = load <16 x i8>, ptr %blk, align 1\n\
  %last = sub i64 %rounds, 1\n\
  br label %loop\n\
loop:\n\
  %i = phi i64 [ 0, %entry ], [ %i1, %body ]\n\
  %s = phi <16 x i8> [ %s0, %entry ], [ %s2, %body ]\n\
  %more = icmp ult i64 %i, %last\n\
  br i1 %more, label %body, label %end\n\
body:\n\
  %off = mul i64 %i, 16\n\
  %kp = getelementptr i8, ptr %rk, i64 %off\n\
  %k = load <16 x i8>, ptr %kp, align 1\n\
  %e = call <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8> %s, <16 x i8> %k)\n\
  %s2 = call <16 x i8> @llvm.aarch64.crypto.aesmc(<16 x i8> %e)\n\
  %i1 = add i64 %i, 1\n\
  br label %loop\n\
end:\n\
  %offl = mul i64 %last, 16\n\
  %klp = getelementptr i8, ptr %rk, i64 %offl\n\
  %kl = load <16 x i8>, ptr %klp, align 1\n\
  %e2 = call <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8> %s, <16 x i8> %kl)\n\
  %offf = mul i64 %rounds, 16\n\
  %kfp = getelementptr i8, ptr %rk, i64 %offf\n\
  %kf = load <16 x i8>, ptr %kfp, align 1\n\
  %r = xor <16 x i8> %e2, %kf\n\
  store <16 x i8> %r, ptr %out, align 1\n\
  ret void\n\
}}\n\
declare <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8>, <16 x i8>)\n\
declare <16 x i8> @llvm.aarch64.crypto.aesmc(<16 x i8>)\n"
        ),
    }
}

/// GHASH over `%n` blocks at `%data`, `%y` updated in place, `%h` the key.
fn ghash_update(triple: &Triple) -> String {
    let f = features(triple);
    let (clmul, declare) = match triple.architecture {
        Architecture::X86_64 => (
            "\
  %va = insertelement <2 x i64> undef, i64 %a, i32 0
  %vb = insertelement <2 x i64> undef, i64 %b, i32 0
  %vp = call <2 x i64> @llvm.x86.pclmulqdq(<2 x i64> %va, <2 x i64> %vb, i8 0)
  %p = bitcast <2 x i64> %vp to i128
",
            "declare <2 x i64> @llvm.x86.pclmulqdq(<2 x i64>, <2 x i64>, i8)\n",
        ),
        _ => (
            "\
  %vp = call <16 x i8> @llvm.aarch64.neon.pmull64(i64 %a, i64 %b)
  %p = bitcast <16 x i8> %vp to i128
",
            "declare <16 x i8> @llvm.aarch64.neon.pmull64(i64, i64)\n",
        ),
    };
    format!(
        "define internal i128 @lexsys_clmul64(i64 %a, i64 %b) alwaysinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
{clmul}\
  ret i128 %p\n\
}}\n\
{declare}\
define internal i128 @lexsys_gf128_order(i128 %v) alwaysinline {{\n\
entry:\n\
  %s = call i128 @llvm.bswap.i128(i128 %v)\n\
  %r = call i128 @llvm.bitreverse.i128(i128 %s)\n\
  ret i128 %r\n\
}}\n\
declare i128 @llvm.bswap.i128(i128)\n\
declare i128 @llvm.bitreverse.i128(i128)\n\
define internal i128 @lexsys_gf128_mul(i128 %x, i128 %y) alwaysinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %x0 = trunc i128 %x to i64\n\
  %xs = lshr i128 %x, 64\n\
  %x1 = trunc i128 %xs to i64\n\
  %y0 = trunc i128 %y to i64\n\
  %ys = lshr i128 %y, 64\n\
  %y1 = trunc i128 %ys to i64\n\
  %p00 = call i128 @lexsys_clmul64(i64 %x0, i64 %y0)\n\
  %p11 = call i128 @lexsys_clmul64(i64 %x1, i64 %y1)\n\
  %p01 = call i128 @lexsys_clmul64(i64 %x0, i64 %y1)\n\
  %p10 = call i128 @lexsys_clmul64(i64 %x1, i64 %y0)\n\
  %mid = xor i128 %p01, %p10\n\
  %w00 = zext i128 %p00 to i256\n\
  %w11 = zext i128 %p11 to i256\n\
  %wm = zext i128 %mid to i256\n\
  %wh = shl i256 %w11, 128\n\
  %wmm = shl i256 %wm, 64\n\
  %wa = xor i256 %w00, %wmm\n\
  %prod = xor i256 %wa, %wh\n\
  %lo = trunc i256 %prod to i128\n\
  %hs = lshr i256 %prod, 128\n\
  %hi = trunc i256 %hs to i128\n\
  %t0 = zext i128 %hi to i256\n\
  %t1 = shl i256 %t0, 1\n\
  %t2 = shl i256 %t0, 2\n\
  %t7 = shl i256 %t0, 7\n\
  %ta = xor i256 %t0, %t1\n\
  %tb = xor i256 %t2, %t7\n\
  %t = xor i256 %ta, %tb\n\
  %tl = trunc i256 %t to i128\n\
  %ths = lshr i256 %t, 128\n\
  %th = trunc i256 %ths to i128\n\
  %u1 = shl i128 %th, 1\n\
  %u2 = shl i128 %th, 2\n\
  %u7 = shl i128 %th, 7\n\
  %ua = xor i128 %th, %u1\n\
  %ub = xor i128 %u2, %u7\n\
  %u = xor i128 %ua, %ub\n\
  %la = xor i128 %lo, %tl\n\
  %r = xor i128 %la, %u\n\
  ret i128 %r\n\
}}\n\
define internal void @lexsys_ghash_update(ptr %h, ptr %y, ptr %data, i64 %n) noinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %hraw = load i128, ptr %h, align 1\n\
  %hh = call i128 @lexsys_gf128_order(i128 %hraw)\n\
  %yraw = load i128, ptr %y, align 1\n\
  %y0 = call i128 @lexsys_gf128_order(i128 %yraw)\n\
  br label %loop\n\
loop:\n\
  %i = phi i64 [ 0, %entry ], [ %i1, %body ]\n\
  %acc = phi i128 [ %y0, %entry ], [ %next, %body ]\n\
  %more = icmp ult i64 %i, %n\n\
  br i1 %more, label %body, label %end\n\
body:\n\
  %off = mul i64 %i, 16\n\
  %bp = getelementptr i8, ptr %data, i64 %off\n\
  %braw = load i128, ptr %bp, align 1\n\
  %b = call i128 @lexsys_gf128_order(i128 %braw)\n\
  %mixed = xor i128 %acc, %b\n\
  %next = call i128 @lexsys_gf128_mul(i128 %mixed, i128 %hh)\n\
  %i1 = add i64 %i, 1\n\
  br label %loop\n\
end:\n\
  %yout = call i128 @lexsys_gf128_order(i128 %acc)\n\
  store i128 %yout, ptr %y, align 1\n\
  ret void\n\
}}\n"
    )
}
