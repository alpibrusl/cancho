//! Scanning memory a word at a time (`docs/word-scan.md`): a little-endian 64-bit load, a 64-byte compare mask, and
//! the three bit counts. Each is Cranelift's own instruction, with no call to the C library.

use crate::*;

impl<'a, 'f> BodyEmitter<'a, 'f> {
    /// Traps unless `0 <= at` and `at + width <= length`, without forming the sum: `length - width` cannot overflow, and a
    /// `length` below `width` is tested on its own because that difference is then negative, which is huge unsigned.
    fn trap_unless_room(&mut self, length: Value, at: Value, width: i64) {
        let room = self.builder.ins().iadd_imm(length, -width);
        let past = self.builder.ins().icmp(IntCC::UnsignedGreaterThan, at, room);
        let short = self.builder.ins().icmp_imm(IntCC::SignedLessThan, length, width);
        let failed = self.builder.ins().bor(past, short);
        self.builder.ins().trapnz(failed, TrapCode::HEAP_OUT_OF_BOUNDS);
    }

    /// `load_le64(text, at)`: `args` is the slice's pointer and length, then `at`. One unaligned 64-bit load.
    pub(crate) fn load_le64(&mut self, args: &[Value]) -> Vec<Value> {
        let (base, length, at) = (args[0], args[1], args[2]);
        self.trap_unless_room(length, at, 8);
        let place = self.builder.ins().iadd(base, at);
        let flags = MemFlags::new().with_endianness(cranelift_codegen::ir::Endianness::Little);
        vec![self.builder.ins().load(types::I64, flags, place, 0)]
    }

    /// `byte_mask64(text, at, b)`: `args` is the slice's pointer and length, `at`, then the byte. Four unaligned 16-byte
    /// loads, a lane-wise compare each, and `vhigh_bits` (`pmovmskb` on x86-64, a narrow-and-extract on arm64).
    pub(crate) fn byte_mask64(&mut self, args: &[Value]) -> Vec<Value> {
        let (base, length, at, wanted) = (args[0], args[1], args[2], args[3]);
        self.trap_unless_room(length, at, 64);
        let start = self.builder.ins().iadd(base, at);
        let splat = self.builder.ins().splat(types::I8X16, wanted);
        let flags = MemFlags::new();
        let mut acc = None;
        for q in 0..4 {
            let lanes = self.builder.ins().load(types::I8X16, flags, start, 16 * q);
            let eq = self.builder.ins().icmp(IntCC::Equal, lanes, splat);
            let bits = self.builder.ins().vhigh_bits(types::I16, eq);
            let wide = self.builder.ins().uextend(types::I64, bits);
            acc = Some(match acc {
                None => wide,
                Some(low) => {
                    let shifted = self.builder.ins().ishl_imm(wide, 16 * i64::from(q));
                    self.builder.ins().bor(low, shifted)
                }
            });
        }
        vec![acc.expect("four lanes were folded")]
    }

    /// `trailing_zeros`, `leading_zeros` and `popcount`: Cranelift's `ctz`, `clz` and `popcnt`, which answer the width, 64,
    /// for a zero input, so the builtins are total.
    pub(crate) fn bit_count(&mut self, builtin: Builtin, x: Value) -> Vec<Value> {
        vec![match builtin {
            Builtin::TrailingZeros => self.builder.ins().ctz(x),
            Builtin::LeadingZeros => self.builder.ins().clz(x),
            _ => self.builder.ins().popcnt(x),
        }]
    }
}
