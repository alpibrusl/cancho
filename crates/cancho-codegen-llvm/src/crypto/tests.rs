//! The generated text of the hardware AES and GHASH functions
//! (`docs/gcm-wide.md` §3, §5), for both instruction sets whatever the host:
//! what no run can see because it is absent from the answer.

use super::definitions;
use target_lexicon::Triple;

const ALL_CALLS: &str = "@cancho_hw_aes_gcm() @cancho_aes_encrypt_block( @cancho_aes_ctr32( \
                         @cancho_ghash_powers( @cancho_gcm_tag( @cancho_gcm_tag_diff(";

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
            if line.contains("@cancho_hw_aes_gcm") {
                assert!(!line.contains("target-features"), "{triple}: the probe runs on any CPU");
            } else {
                assert!(line.contains("\"target-features\"=\"+aes"), "{triple}: {line}");
            }
        }
        // The functions the builtins call are out of line, so the features
        // never reach a caller; the helpers are folded into them.
        for name in ["aes_encrypt_block", "aes_ctr32", "ghash_powers", "gcm_tag", "gcm_tag_diff"] {
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
