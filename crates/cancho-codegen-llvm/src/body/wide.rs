//! `mul_wide`, `add_carry` and `sub_borrow` (`docs/wide-multiply.md` §4):
//! unsigned 64-bit words held in `int`s, a pair of words out. Every form here
//! is straight-line arithmetic on registers: no branch, no memory access, and
//! no call (a 128-bit multiply is not left to `__multi3`), so each takes the
//! same time whatever its operands are.

use target_lexicon::{Architecture, Triple};

use super::FuncEmitter;
use crate::{LValue, operand};

/// Whether the target has a 64x64 -> 128-bit multiply in hardware, so that
/// LLVM's `i128` multiply of two zero-extended words is one instruction (`mul`
/// or `mulx` on x86-64, `mul` and `umulh` on aarch64). Elsewhere (WebAssembly,
/// 32-bit targets) the same IR would become a call to `__multi3`, so the
/// product is built from four 32 x 32 -> 64 multiplies instead.
pub(crate) fn native_wide_multiply(triple: &Triple) -> bool {
    matches!(triple.architecture, Architecture::X86_64 | Architecture::Aarch64(_))
}

impl<'a> FuncEmitter<'a> {
    fn words<const N: usize>(
        evaluated: Vec<Vec<LValue>>,
        name: &str,
    ) -> Result<[LValue; N], String> {
        let flat: Vec<LValue> = evaluated.into_iter().flatten().collect();
        <[LValue; N]>::try_from(flat)
            .map_err(|v| format!("`{name}` needs {N} `int` arguments but {} were given", v.len()))
    }

    fn op(&mut self, text: String) -> String {
        let r = self.fresh();
        self.out.push_str(&format!("  {r} = {text}\n"));
        r
    }

    /// `(hi, lo)` of `a * b`, both read as unsigned.
    pub(crate) fn mul_wide(&mut self, evaluated: Vec<Vec<LValue>>) -> Result<Vec<LValue>, String> {
        let [a, b] = Self::words::<2>(evaluated, "mul_wide")?;
        let (a, b) = (operand(&a), operand(&b));
        if native_wide_multiply(self.triple) {
            let wa = self.op(format!("zext i64 {a} to i128"));
            let wb = self.op(format!("zext i64 {b} to i128"));
            let p = self.op(format!("mul i128 {wa}, {wb}"));
            let lo = self.op(format!("trunc i128 {p} to i64"));
            let top = self.op(format!("lshr i128 {p}, 64"));
            let hi = self.op(format!("trunc i128 {top} to i64"));
            return Ok(vec![LValue::Reg(hi), LValue::Reg(lo)]);
        }
        // Knuth's Algorithm M on 32-bit halves, in Hacker's Delight's `mulhu`
        // order. No step overflows: `p01 + (p00 >> 32)` and `(mid & m) + p10`
        // are each at most (2^32 - 1)^2 + 2^32 - 1 < 2^64, and `lo` is the
        // ordinary wrapping product.
        //
        // The low halves pass through an empty `asm`, as `value_barrier`'s
        // argument does. Without it LLVM's AggressiveInstCombine recognises
        // this idiom as a 64 x 64 -> 128-bit multiply and rewrites it to an
        // `i128` one, which wasm32 lowers to a call to `__multi3`, a symbol
        // the WASI sysroot does not ship (`docs/wasm.md`, W0.3).
        let a0m = self.op(format!("and i64 {a}, 4294967295"));
        let a0 = self.op(format!("call i64 asm \"\", \"=r,0\"(i64 {a0m})"));
        let a1 = self.op(format!("lshr i64 {a}, 32"));
        let b0m = self.op(format!("and i64 {b}, 4294967295"));
        let b0 = self.op(format!("call i64 asm \"\", \"=r,0\"(i64 {b0m})"));
        let b1 = self.op(format!("lshr i64 {b}, 32"));
        let p00 = self.op(format!("mul i64 {a0}, {b0}"));
        let p01 = self.op(format!("mul i64 {a0}, {b1}"));
        let p10 = self.op(format!("mul i64 {a1}, {b0}"));
        let p11 = self.op(format!("mul i64 {a1}, {b1}"));
        let c0 = self.op(format!("lshr i64 {p00}, 32"));
        let mid1 = self.op(format!("add i64 {p01}, {c0}"));
        let m1lo = self.op(format!("and i64 {mid1}, 4294967295"));
        let mid2 = self.op(format!("add i64 {m1lo}, {p10}"));
        let h1 = self.op(format!("lshr i64 {mid1}, 32"));
        let h2 = self.op(format!("lshr i64 {mid2}, 32"));
        let h01 = self.op(format!("add i64 {p11}, {h1}"));
        let hi = self.op(format!("add i64 {h01}, {h2}"));
        let lo = self.op(format!("mul i64 {a}, {b}"));
        Ok(vec![LValue::Reg(hi), LValue::Reg(lo)])
    }

    /// `(sum, carry_out)` of `a + b + (carry != 0)`, or, with `sub`,
    /// `(difference, borrow_out)` of `a - b - (borrow != 0)`. The two partial
    /// overflows cannot both be set (the first leaves a value one short of
    /// the limit), so their `or` is the carry.
    pub(crate) fn add_carry(
        &mut self,
        evaluated: Vec<Vec<LValue>>,
        sub: bool,
    ) -> Result<Vec<LValue>, String> {
        let name = if sub { "sub_borrow" } else { "add_carry" };
        let intrinsic = if sub { "usub" } else { "uadd" };
        let [a, b, c] = Self::words::<3>(evaluated, name)?;
        let (a, b, c) = (operand(&a), operand(&b), operand(&c));
        let nz = self.op(format!("icmp ne i64 {c}, 0"));
        let cin = self.op(format!("zext i1 {nz} to i64"));
        let first = self.op(format!(
            "call {{ i64, i1 }} @llvm.{intrinsic}.with.overflow.i64(i64 {a}, i64 {b})"
        ));
        let v1 = self.op(format!("extractvalue {{ i64, i1 }} {first}, 0"));
        let o1 = self.op(format!("extractvalue {{ i64, i1 }} {first}, 1"));
        let second = self.op(format!(
            "call {{ i64, i1 }} @llvm.{intrinsic}.with.overflow.i64(i64 {v1}, i64 {cin})"
        ));
        let v2 = self.op(format!("extractvalue {{ i64, i1 }} {second}, 0"));
        let o2 = self.op(format!("extractvalue {{ i64, i1 }} {second}, 1"));
        let both = self.op(format!("or i1 {o1}, {o2}"));
        let out = self.op(format!("zext i1 {both} to i64"));
        Ok(vec![LValue::Reg(v2), LValue::Reg(out)])
    }
}
