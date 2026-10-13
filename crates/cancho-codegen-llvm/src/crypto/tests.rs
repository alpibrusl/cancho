//! The generated text of the hardware AES and GHASH functions
//! (`docs/gcm-wide.md` §3, §5), for both instruction sets whatever the host:
//! what no run can see because it is absent from the answer.

use super::definitions;
use target_lexicon::Triple;

const ALL_CALLS: &str = "@cancho_hw_aes_gcm() @cancho_aes_encrypt_block( @cancho_aes_ctr32( \
                         @cancho_ghash_powers( @cancho_gcm_tag( @cancho_gcm_tag_diff( \
                         @cancho_ghash_update( @cancho_hw_sha256() @cancho_sha256_blocks( \
                         @cancho_sha256_block_words(";

fn triples() -> [Triple; 3] {
    [
        "x86_64-unknown-linux-gnu".parse().expect("a valid triple"),
        "aarch64-unknown-linux-gnu".parse().expect("a valid triple"),
        "aarch64-apple-darwin".parse().expect("a valid triple"),
    ]
}

/// The keystream of the last, partial group is secret and is left on the
/// stack by the function that used it: it must be overwritten by stores the
/// optimiser may not delete as dead. No answer shows the difference, so the
/// text is what is checked.
#[test]
fn the_keystream_tail_is_wiped_with_volatile_stores() {
    for triple in triples() {
        let text = definitions(&triple, ALL_CALLS);
        let wipe = text.split("wipe:").nth(1).expect("the counter-mode function has a wipe");
        let wipe = wipe.split("done:").next().expect("and it ends");
        assert_eq!(wipe.matches("store volatile").count(), 8, "{triple}: eight blocks of 16 bytes");
        assert!(wipe.matches("zeroinitializer").count() == 8, "{triple}: wiped with zeros");
    }
}

/// An intrinsic is declared once: a second declaration is a refused module.
/// Every definition that holds an instruction carries the target features
/// (`docs/crypto-builtins.md` §4), so none can be inlined into a caller at
/// baseline.
#[test]
fn each_intrinsic_is_declared_once_and_each_instruction_has_its_features() {
    for triple in triples() {
        let text = definitions(&triple, ALL_CALLS);
        let mut declared: Vec<&str> = text.lines().filter(|l| l.starts_with("declare ")).collect();
        let total = declared.len();
        declared.sort_unstable();
        declared.dedup();
        assert_eq!(declared.len(), total, "{triple}: a declaration is repeated");
        for line in text.lines().filter(|l| l.starts_with("define ")) {
            if line.contains("@cancho_hw_aes_gcm") || line.contains("@cancho_hw_sha256") {
                assert!(!line.contains("target-features"), "{triple}: the probe runs on any CPU");
            } else if line.contains("@cancho_sha256_") {
                assert!(line.contains("\"target-features\"=\"+sha"), "{triple}: {line}");
            } else {
                assert!(line.contains("\"target-features\"=\"+aes"), "{triple}: {line}");
            }
        }
        // The functions the builtins call are out of line, so the features
        // never reach a caller; the helpers are folded into them.
        for name in [
            "aes_encrypt_block",
            "aes_ctr32",
            "ghash_powers",
            "gcm_tag",
            "gcm_tag_diff",
            "ghash_update",
            "sha256_blocks",
            "sha256_block_words",
        ] {
            let line = text
                .lines()
                .find(|l| l.starts_with("define ") && l.contains(&format!("@cancho_{name}(")))
                .unwrap_or_else(|| panic!("{triple}: {name} is defined"));
            assert!(line.contains(" noinline "), "{triple}: {name} is out of line");
        }
    }
}

/// A module that calls nothing gets nothing, and one that calls only
/// `aes_encrypt_block` does not get the GHASH code.
#[test]
fn only_what_a_module_calls_is_defined() {
    for triple in triples() {
        assert_eq!(definitions(&triple, "define i64 @main() { ret i64 0 }"), "");
        let text = definitions(&triple, "call void @cancho_aes_encrypt_block(");
        assert!(text.contains("@cancho_aes1("), "{triple}");
        assert!(!text.contains("@cancho_gh_") && !text.contains("@cancho_aes8("), "{triple}");
    }
    // WebAssembly has none of the instructions.
    let wasm: Triple = "wasm32-wasip1".parse().expect("a valid triple");
    assert_eq!(definitions(&wasm, ALL_CALLS), "");
}

/// The builtins call nothing outside themselves: no `memcpy` or `memset`
/// (whose code the branch audit does not read), only the intrinsics that
/// are instructions and each other. The CPU probe alone calls the operating
/// system (`docs/crypto-builtins.md` §4).
#[test]
fn the_builtins_call_no_library_code() {
    for triple in triples() {
        let text = definitions(&triple, ALL_CALLS);
        let mut function = "";
        for line in text.lines() {
            if line.starts_with("define ") {
                function = line;
            }
            if let Some(at) = line.find("call ") {
                let callee =
                    line[at..].split('@').nth(1).unwrap_or("").split('(').next().unwrap_or("");
                let probe = (function.contains("@cancho_hw_aes_gcm")
                    || function.contains("@cancho_hw_sha256"))
                    && (callee == "getauxval" || callee == "sysctlbyname");
                let asm = line[at..].contains(" asm ");
                assert!(
                    callee.starts_with("cancho_")
                        || (callee.starts_with("llvm.") && !callee.starts_with("llvm.mem"))
                        || probe
                        || asm,
                    "{triple}: `{callee}` called in {function}"
                );
            }
        }
    }
}

/// H = AES(K, 0) is made in a register inside `ghash_powers` and never
/// stored: its only stores are the eight entries of the table (gcm-wide.md
/// §3.4). A copy of H in memory would outlive the call.
#[test]
fn h_is_never_stored() {
    for triple in triples() {
        let text = definitions(&triple, "call void @cancho_ghash_powers(");
        let body = text.split("@cancho_ghash_powers(").nth(1).expect("defined");
        let body = body.split("\n}\n").next().expect("ends");
        let stores = body.lines().filter(|l| l.trim_start().starts_with("store ")).count();
        assert_eq!(stores, 8, "{triple}: only the table's eight entries are stored");
    }
}

/// The SHA-256 functions keep the state and the message in registers: the
/// only store either makes is the eight words written back to the state, so
/// no word of a key's hash is left anywhere else (`docs/sha256-builtin.md`
/// §6). And the AES code is not pulled in by them, nor theirs by it.
#[test]
fn sha256_stores_only_the_state_and_comes_alone() {
    for triple in triples() {
        let text = definitions(
            &triple,
            "call void @cancho_sha256_blocks( call void @cancho_sha256_block_words(",
        );
        for name in ["@cancho_sha256_blocks(", "@cancho_sha256_block_words("] {
            let body = text.split(name).nth(1).expect("defined");
            let body = body.split("\n}\n").next().expect("ends");
            let stores: Vec<&str> =
                body.lines().filter(|l| l.trim_start().starts_with("store ")).collect();
            assert_eq!(stores, ["  store <8 x i64> %o64, ptr %state, align 8"], "{triple}: {name}");
        }
        assert!(!text.contains("@cancho_aes1(") && !text.contains("@cancho_gh_"), "{triple}");
        let aes = definitions(&triple, "call void @cancho_aes_encrypt_block(");
        assert!(!aes.contains("sha256"), "{triple}");
        // One `bswap` declaration when both are in a module.
        let both = definitions(&triple, ALL_CALLS);
        assert_eq!(both.matches("declare <4 x i32> @llvm.bswap.v4i32").count(), 1, "{triple}");
    }
}
