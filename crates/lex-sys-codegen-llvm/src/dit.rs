//! Arm's data-independent-timing bit, set before any lex-sys code runs
//! (`docs/crypto-builtins.md` §6).
//!
//! `PSTATE.DIT` is the CPU's promise that the instructions Arm lists take
//! a time independent of their operands. On the Apple M4 of
//! `docs/tls-assurance.md` §6.1 it removed the ECDH timing failures on
//! LLVM and most of AES-GCM's on Cranelift. It is a per-thread bit, and
//! an instruction that sets it is undefined on a CPU without FEAT_DIT, so
//! `main` asks the operating system first: `getauxval(AT_HWCAP)`'s
//! `HWCAP_DIT` on Linux, `sysctlbyname("hw.optional.arm.FEAT_DIT")` on
//! Darwin. The answer is public, the same for the life of the process,
//! and the program cannot observe the read (§9 question 2's answer).
//!
//! Only `main`'s thread: a Linux thread inherits the bit from the thread
//! that made it, a Darwin thread starts with it clear (both measured,
//! `docs/crypto-builtins.md` §6). Only the LLVM backend: Cranelift has no
//! inline assembly to set it with.

use target_lexicon::{Architecture, OperatingSystem, Triple};

/// `msr dit, #1`, as its encoding, so the assembler needs no `+dit`.
const MSR_DIT_1: &str = "0xd503415f";

/// Whether `triple` gets the DIT start-up: aarch64 Linux and Darwin.
pub(crate) fn applies(triple: &Triple) -> bool {
    matches!(triple.architecture, Architecture::Aarch64(_))
        && matches!(triple.operating_system, OperatingSystem::Linux | OperatingSystem::Darwin(_))
}

/// The libc function `@lexsys_set_dit` calls, for the module header's
/// declarations: its name and signature.
pub(crate) fn libc_declaration(triple: &Triple) -> (&'static str, &'static str) {
    match triple.operating_system {
        OperatingSystem::Darwin(_) => {
            ("sysctlbyname", "i32 @sysctlbyname(ptr, ptr, ptr, ptr, i64)")
        }
        _ => ("getauxval", "i64 @getauxval(i64)"),
    }
}

/// `@lexsys_set_dit`, which `main` calls first: sets `PSTATE.DIT` when the
/// CPU has it, and does nothing otherwise.
pub(crate) fn definition(triple: &Triple) -> String {
    let probe = match triple.operating_system {
        OperatingSystem::Darwin(_) => "\
  %value = alloca i32
  %size = alloca i64
  store i32 0, ptr %value
  store i64 4, ptr %size
  %status = call i32 @sysctlbyname(ptr @lexsys_dit_name, ptr %value, ptr %size, ptr null, i64 0)
  %read = icmp eq i32 %status, 0
  %bit = load i32, ptr %value
  %set = icmp ne i32 %bit, 0
  %has = and i1 %read, %set
"
        .to_owned(),
        // AT_HWCAP is 16; HWCAP_DIT is bit 24.
        _ => "\
  %hwcap = call i64 @getauxval(i64 16)
  %bit = and i64 %hwcap, 16777216
  %has = icmp ne i64 %bit, 0
"
        .to_owned(),
    };
    let name = match triple.operating_system {
        OperatingSystem::Darwin(_) => {
            "@lexsys_dit_name = private unnamed_addr constant [25 x i8] c\"hw.optional.arm.FEAT_DIT\\00\"\n"
        }
        _ => "",
    };
    format!(
        "{name}define internal void @lexsys_set_dit() {{\nentry:\n{probe}  br i1 %has, label %on, label %done\non:\n  call void asm sideeffect \".inst {MSR_DIT_1}\", \"\"()\n  br label %done\ndone:\n  ret void\n}}\n"
    )
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use lex_sys_syntax::parse;

    const PROGRAM: &str = "\
extern fn lexsys_test_dit[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(fs);
    release(io);
    release(heap);
    var result = 0;
    let libc = narrow(ffi, \"libc\");
    borrow libc as &f in {
        result = lexsys_test_dit(f);
    }
    release(libc);
    return result;
}
";

    const HELPER: &str = r#"
#ifdef __APPLE__
#include <sys/sysctl.h>
static long has_dit(void) { int v = 0; size_t n = sizeof v; return sysctlbyname("hw.optional.arm.FEAT_DIT", &v, &n, 0, 0) == 0 && v; }
#else
#include <sys/auxv.h>
static long has_dit(void) { return (getauxval(AT_HWCAP) >> 24) & 1; }
#endif
long lexsys_test_dit(void) {
    long v;
    __asm__ volatile(".inst 0xd53b42a0\n\tmov %0, x0" : "=r"(v) :: "x0");
    return 2 * has_dit() + ((v >> 24) & 1);
}
"#;

    fn module(triple: &str) -> String {
        let ast = parse(PROGRAM).expect("the fixture parses");
        let program = lex_sys_ir::lower(&ast).expect("the fixture type-checks");
        crate::emit::emit_module(&program, "main", &triple.parse().expect("a triple"))
            .expect("the module is emitted")
    }

    /// `main` sets the bit first on aarch64 Linux and Darwin, asking the
    /// operating system each uses, and nowhere else.
    #[test]
    fn main_sets_dit_first_on_aarch64_linux_and_darwin_only() {
        for (triple, probe) in [
            ("aarch64-unknown-linux-gnu", "@getauxval(i64 16)"),
            ("aarch64-apple-darwin", "@sysctlbyname(ptr @lexsys_dit_name"),
        ] {
            let text = module(triple);
            let main = &text[text.find("define i32 @main(").expect("a main")..];
            let call = main.find("call void @lexsys_set_dit()").expect("main calls it");
            let first_lex_call = main.find("@lexs_main(").expect("main calls the program");
            assert!(call < first_lex_call, "{triple}: the bit is set before any lex-sys code");
            assert!(text.contains(probe), "{triple}: the CPU is asked first");
            assert!(text.contains(".inst 0xd503415f"), "{triple}: `msr dit, #1`");
        }
        for triple in ["x86_64-unknown-linux-gnu", "x86_64-apple-darwin"] {
            assert!(!module(triple).contains("lexsys_set_dit"), "{triple}: no DIT");
        }
    }

    /// On an aarch64 host with FEAT_DIT, a compiled program runs with
    /// `PSTATE.DIT` set: read back through a C function linked beside it.
    #[test]
    fn a_program_runs_with_dit_set_on_an_aarch64_host() {
        if !cfg!(target_arch = "aarch64") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("lex-sys-dit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let host = lex_sys_codegen::host_triple();
        // As the backend builds: `clang` makes the object, `cc` links.
        let object =
            crate::run_clang(&module(&host.to_string()), &host).expect("clang compiles it");
        let obj = dir.join("p.o");
        std::fs::write(&obj, object).expect("the object is written");
        // The helper answers 2 * (the CPU has FEAT_DIT) + PSTATE.DIT, the
        // bit read with `mrs x0, dit` by its encoding (bit 24 of x0).
        let c = dir.join("dit.c");
        std::fs::write(&c, HELPER).expect("the helper is written");
        let exe = dir.join("p");
        let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
        let built = Command::new(&cc)
            .arg("-O2")
            .arg(&obj)
            .arg(&c)
            .arg("-o")
            .arg(&exe)
            .status()
            .expect("the C compiler runs");
        assert!(built.success(), "the program and the helper link");
        let status = Command::new(&exe).status().expect("the program runs");
        let _ = std::fs::remove_dir_all(&dir);
        // With FEAT_DIT the bit is set (3); without it, left alone (0).
        // CI's darwin-aarch64 runner is an Apple M-series, which has it.
        let code = status.code();
        assert!(code == Some(3) || code == Some(0), "PSTATE.DIT as the CPU allows: {code:?}");
    }
}
