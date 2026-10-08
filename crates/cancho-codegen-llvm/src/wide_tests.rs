//! `docs/wide-multiply.md` §4, §5: what the LLVM text for `mul_wide`,
//! `add_carry` and `sub_borrow` looks like, checked without a toolchain for
//! the target. The object code of the two native targets is checked by
//! `scripts/wide_multiply_objdump.py` and recorded in the document.

use cancho_syntax::parse;
use target_lexicon::Triple;

use super::*;

const SOURCE: &str = "edition 7;\n\
    fn words(a: int, b: int, c: int) -> [] int {\n\
        let (h, l) = mul_wide(a, b);\n\
        let (s, co) = add_carry(h, l, c);\n\
        let (d, bo) = sub_borrow(s, co, c);\n\
        return d + bo;\n\
    }\n\
    fn main(world: World) -> [] int {\n\
        let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n\
        release(args); release(heap); release(fs); release(ffi); release(io);\n\
        release(net); release(clock); release(signals); release(exec);\n\
        return words(3, 4, 5);\n\
    }\n";

/// The text of `@words`, from its `define` to the closing brace.
fn words_function(triple: &str) -> String {
    let ast = parse(SOURCE).expect("should parse");
    let program = cancho_ir::lower(&ast).expect("should lower");
    let triple: Triple = triple.parse().expect("a valid triple");
    let text = emit::emit_module(&program, "main", &triple).expect("should emit");
    let from = text.find("define i64 @lexs_words(").expect("the function is emitted");
    let end = text[from..].find("\n}\n").expect("a closing brace");
    text[from..from + end].to_owned()
}

fn assert_straight_line(body: &str, what: &str) {
    // Only the checked `+` in `d + bo` may branch (to its trap); the three
    // builtins themselves add no block, so count blocks against a program
    // with the same `+` and no builtins would be fragile. Check the
    // instructions instead: no `select`, no call but the overflow intrinsics
    // and the trap, no libcall for the 128-bit product.
    assert!(!body.contains("__multi3"), "{what}: no libcall for the product:\n{body}");
    assert!(!body.contains(" select "), "{what}: no select:\n{body}");
    for line in body.lines().filter(|l| l.contains("call ")) {
        assert!(
            line.contains("@llvm.uadd.with.overflow")
                || line.contains("@llvm.usub.with.overflow")
                || line.contains("@llvm.sadd.with.overflow")
                || line.contains("asm sideeffect")
                || line.contains("asm \"\", \"=r,0\""),
            "{what}: an unexpected call `{line}`"
        );
    }
}

#[test]
fn on_x86_64_and_aarch64_the_product_is_one_128_bit_multiply_and_nothing_branches() {
    for triple in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin"]
    {
        let body = words_function(triple);
        assert_eq!(body.matches("mul i128").count(), 1, "{triple}:\n{body}");
        assert_eq!(body.matches("@llvm.uadd.with.overflow.i64").count(), 2, "{triple}");
        assert_eq!(body.matches("@llvm.usub.with.overflow.i64").count(), 2, "{triple}");
        assert_straight_line(&body, triple);
    }
}

#[test]
fn on_wasm32_the_product_is_four_narrow_multiplies_and_no_i128_appears() {
    let body = words_function("wasm32-wasip1");
    assert!(!body.contains("i128"), "a 128-bit type would become `__multi3`:\n{body}");
    assert_eq!(body.matches("mul i64").count(), 5, "four partial products and the low word");
    assert_straight_line(&body, "wasm32-wasip1");
}
