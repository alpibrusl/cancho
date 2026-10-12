//! Scanning memory a word at a time (`docs/word-scan.md`): a little-endian 64-bit load, a 64-byte compare mask, and
//! the three bit counts. Every one is plain LLVM IR the optimiser sees through, with no call to the C library.

use super::*;

impl<'a> FuncEmitter<'a> {
    /// Traps unless `0 <= at` and `at + width <= length`, without forming the sum: `length - width` cannot overflow, and a
    /// `length` below `width` is tested on its own because that difference is then negative, which is huge unsigned.
    fn trap_unless_room(&mut self, length: &str, at: &str, width: u32) -> Result<(), String> {
        let room = self.fresh();
        self.out.push_str(&format!("  {room} = sub i64 {length}, {width}\n"));
        let past = self.fresh();
        self.out.push_str(&format!("  {past} = icmp ugt i64 {at}, {room}\n"));
        let short = self.fresh();
        self.out.push_str(&format!("  {short} = icmp slt i64 {length}, {width}\n"));
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = or i1 {past}, {short}\n"));
        self.trap_if(&failed)
    }

    /// `load_le64(text, at)`: `args` is the slice's pointer and length, then `at`. One unaligned `i64` load.
    pub(crate) fn load_le64(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let (base, length, at) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        self.trap_unless_room(&length, &at, 8)?;
        let place = self.fresh();
        self.out.push_str(&format!("  {place} = getelementptr i8, ptr {base}, i64 {at}\n"));
        let word = self.fresh();
        self.out.push_str(&format!("  {word} = load i64, ptr {place}, align 1\n"));
        Ok(vec![LValue::Reg(word)])
    }

    /// `byte_mask64(text, at, b)`: `args` is the slice's pointer and length, `at`, then the byte. Four 16-byte loads and
    /// compares; LLVM turns each `bitcast <16 x i1> to i16` into `pmovmskb` (x86-64) or a narrow-and-extract (arm64), and
    /// scalarises it where there is no vector unit (wasm32 without `simd128`).
    pub(crate) fn byte_mask64(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        let (base, length, at) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let wanted = operand(&args[3]);
        self.trap_unless_room(&length, &at, 64)?;
        let start = self.fresh();
        self.out.push_str(&format!("  {start} = getelementptr i8, ptr {base}, i64 {at}\n"));
        let ins = self.fresh();
        self.out
            .push_str(&format!("  {ins} = insertelement <16 x i8> poison, i8 {wanted}, i64 0\n"));
        let splat = self.fresh();
        self.out.push_str(&format!(
            "  {splat} = shufflevector <16 x i8> {ins}, <16 x i8> poison, <16 x i32> zeroinitializer\n"
        ));
        let mut acc = String::new();
        for q in 0..4 {
            let p = self.fresh();
            self.out.push_str(&format!("  {p} = getelementptr i8, ptr {start}, i64 {}\n", 16 * q));
            let v = self.fresh();
            self.out.push_str(&format!("  {v} = load <16 x i8>, ptr {p}, align 1\n"));
            let eq = self.fresh();
            self.out.push_str(&format!("  {eq} = icmp eq <16 x i8> {v}, {splat}\n"));
            let bits = self.fresh();
            self.out.push_str(&format!("  {bits} = bitcast <16 x i1> {eq} to i16\n"));
            let wide = self.fresh();
            self.out.push_str(&format!("  {wide} = zext i16 {bits} to i64\n"));
            if q == 0 {
                acc = wide;
                continue;
            }
            let shifted = self.fresh();
            self.out.push_str(&format!("  {shifted} = shl i64 {wide}, {}\n", 16 * q));
            let joined = self.fresh();
            self.out.push_str(&format!("  {joined} = or i64 {acc}, {shifted}\n"));
            acc = joined;
        }
        Ok(vec![LValue::Reg(acc)])
    }

    /// `trailing_zeros`, `leading_zeros` and `popcount`: one intrinsic each. `cttz` and `ctlz` take `false` for "zero is
    /// poison", so 0 answers 64 and the builtin is total.
    pub(crate) fn bit_count(&mut self, builtin: Builtin, args: &[LValue]) -> Vec<LValue> {
        let x = operand(&args[0]);
        let n = self.fresh();
        let call = match builtin {
            Builtin::TrailingZeros => format!("call i64 @llvm.cttz.i64(i64 {x}, i1 false)"),
            Builtin::LeadingZeros => format!("call i64 @llvm.ctlz.i64(i64 {x}, i1 false)"),
            _ => format!("call i64 @llvm.ctpop.i64(i64 {x})"),
        };
        self.out.push_str(&format!("  {n} = {call}\n"));
        vec![LValue::Reg(n)]
    }
}
