//! The hardware AES and carry-less-multiply builtins' out-of-line
//! functions (`docs/crypto-builtins.md` §4, `docs/gcm-wide.md` §4).
//!
//! Each builtin is a call to one of the functions below, defined only in a
//! module that calls it. Each carries the target features its instructions
//! need on its own definition, never the module's, so the rest of the
//! program stays at the baseline and runs on a CPU without them; LLVM will
//! not inline a function with more features into one with fewer, so the
//! features never reach a caller. Whether the CPU has them is
//! `@cancho_hw_aes_gcm`'s answer, which `std.aes` and `std.gcm` branch on.
//!
//! The AES part is `aes.rs` (one block, and counter mode eight blocks at a
//! time) and the GHASH part `ghash.rs` (the powers of H, and the tag with
//! one reduction per eight blocks). Every vector here is `<2 x i64>`, the
//! type the x86 intrinsics take; the aarch64 ones are bitcast around each
//! call, which costs nothing. Every shift is by a constant, every loop
//! bound and every branch is on a length, and a length is public.

mod aes;
mod ghash;
#[cfg(test)]
mod tests;

use target_lexicon::{Architecture, OperatingSystem, Triple};

/// Whether this target can have the instructions: x86-64, and aarch64 on
/// Linux and Darwin (where the operating system says whether the CPU has
/// them). Elsewhere `hw_aes_gcm()` is false and the builtins trap.
pub(crate) fn hardware_target(triple: &Triple) -> bool {
    match triple.architecture {
        Architecture::X86_64 => true,
        Architecture::Aarch64(_) => {
            matches!(triple.operating_system, OperatingSystem::Linux | OperatingSystem::Darwin(_))
        }
        _ => false,
    }
}

/// The instruction set a module's functions are written for.
#[derive(Clone, Copy)]
pub(super) struct Isa {
    pub(super) x86: bool,
    /// The `"target-features"` string of every function that holds an
    /// instruction (§4).
    pub(super) features: &'static str,
}

impl Isa {
    fn of(triple: &Triple) -> Isa {
        match triple.architecture {
            Architecture::X86_64 => Isa { x86: true, features: "+aes,+pclmul,+ssse3,+sse2" },
            _ => Isa { x86: false, features: "+aes,+neon" },
        }
    }
}

/// The vector type every AES block and GHASH value is held in.
pub(super) const V: &str = "<2 x i64>";

/// Straight-line IR with fresh temporaries.
pub(super) struct Gen {
    pub(super) out: String,
    next: usize,
}

impl Gen {
    pub(super) fn new() -> Gen {
        Gen { out: String::new(), next: 0 }
    }

    /// A fresh name for a temporary.
    pub(super) fn t(&mut self) -> String {
        self.next += 1;
        format!("%t{}", self.next)
    }

    pub(super) fn line(&mut self, text: &str) {
        self.out.push_str("  ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// `%r = <expr>`, answering `%r`.
    pub(super) fn op(&mut self, expr: &str) -> String {
        let r = self.t();
        self.line(&format!("{r} = {expr}"));
        r
    }
}

/// The definitions a module needs, given its text: only those it calls.
pub(crate) fn definitions(triple: &Triple, text: &str) -> String {
    let mut out = String::new();
    if !hardware_target(triple) {
        return out;
    }
    let isa = Isa::of(triple);
    if text.contains("@cancho_hw_aes_gcm()") {
        out.push_str(&hw_aes_gcm(triple));
    }
    let block = text.contains("@cancho_aes_encrypt_block(");
    let ctr = text.contains("@cancho_aes_ctr32(");
    let powers = text.contains("@cancho_ghash_powers(");
    let tag = text.contains("@cancho_gcm_tag(");
    let diff = text.contains("@cancho_gcm_tag_diff(");
    if block || ctr || tag || diff {
        out.push_str(&aes::core(isa, ctr));
    }
    if block {
        out.push_str(&aes::encrypt_block(isa));
    }
    if ctr {
        out.push_str(&aes::ctr32(isa));
    }
    if powers || tag || diff {
        out.push_str(&ghash::core(isa));
    }
    if powers {
        out.push_str(&ghash::powers(isa));
    }
    if tag || diff {
        out.push_str(&ghash::tag(isa, tag, diff));
    }
    out.push_str(&declarations(isa, block || ctr || tag || diff, powers || tag || diff));
    out
}

/// The intrinsics the definitions above call, declared once.
fn declarations(isa: Isa, aes: bool, ghash: bool) -> String {
    let mut out = String::new();
    if aes {
        if isa.x86 {
            out.push_str("declare <2 x i64> @llvm.x86.aesni.aesenc(<2 x i64>, <2 x i64>)\n");
            out.push_str("declare <2 x i64> @llvm.x86.aesni.aesenclast(<2 x i64>, <2 x i64>)\n");
        } else {
            out.push_str("declare <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8>, <16 x i8>)\n");
            out.push_str("declare <16 x i8> @llvm.aarch64.crypto.aesmc(<16 x i8>)\n");
        }
        out.push_str("declare <4 x i32> @llvm.bswap.v4i32(<4 x i32>)\n");
    }
    if ghash {
        if isa.x86 {
            out.push_str("declare <2 x i64> @llvm.x86.pclmulqdq(<2 x i64>, <2 x i64>, i8)\n");
        } else {
            out.push_str("declare <16 x i8> @llvm.aarch64.neon.pmull64(i64, i64)\n");
        }
        out.push_str("declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)\n");
        out.push_str("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n");
    }
    out
}

/// `i8` 1 when the CPU has every instruction the two block builtins use,
/// asked once and kept in `@cancho_hw_cache` (0 not asked, 1 no, 2 yes).
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
            "@cancho_aes_name = private unnamed_addr constant [25 x i8] c\"hw.optional.arm.FEAT_AES\\00\"\n\
             @cancho_pmull_name = private unnamed_addr constant [27 x i8] c\"hw.optional.arm.FEAT_PMULL\\00\"\n"
                .to_owned(),
            "\
  %aes = alloca i32
  %pmull = alloca i32
  %size = alloca i64
  store i32 0, ptr %aes
  store i32 0, ptr %pmull
  store i64 4, ptr %size
  %s1 = call i32 @sysctlbyname(ptr @cancho_aes_name, ptr %aes, ptr %size, ptr null, i64 0)
  store i64 4, ptr %size
  %s2 = call i32 @sysctlbyname(ptr @cancho_pmull_name, ptr %pmull, ptr %size, ptr null, i64 0)
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
        "@cancho_hw_cache = internal global i8 0\n{names}\
define internal i8 @cancho_hw_aes_gcm() {{\n\
entry:\n\
  %cached = load atomic i8, ptr @cancho_hw_cache monotonic, align 1\n\
  %known = icmp ne i8 %cached, 0\n\
  br i1 %known, label %answer, label %ask\n\
ask:\n\
{probe}\
  %fresh = select i1 %has, i8 2, i8 1\n\
  store atomic i8 %fresh, ptr @cancho_hw_cache monotonic, align 1\n\
  br label %answer\n\
answer:\n\
  %value = phi i8 [ %cached, %entry ], [ %fresh, %ask ]\n\
  %yes = icmp eq i8 %value, 2\n\
  %result = zext i1 %yes to i8\n\
  ret i8 %result\n\
}}\n"
    )
}
