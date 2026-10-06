//! The call sites of the hardware AES and carry-less-multiply builtins
//! (`docs/crypto-builtins.md` §3, §4): each checks its lengths, trapping as
//! an index out of bounds does, then calls the out-of-line function
//! `crate::crypto` defines. On a target without the instructions,
//! `hw_aes_gcm()` is false and the block builtins trap.

use crate::*;

impl<'a> FuncEmitter<'a> {
    pub(crate) fn hw_aes_gcm(&mut self) -> Result<Vec<LValue>, String> {
        if !crate::crypto::hardware_target(self.triple) {
            return Ok(vec![LValue::Reg(LKind::I8.zero().to_owned())]);
        }
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = call i8 @lexsys_hw_aes_gcm()\n"));
        Ok(vec![LValue::Reg(answer)])
    }

    /// `[round_keys ptr, len, rounds, block ptr, len, out ptr, len]`.
    pub(crate) fn aes_encrypt_block(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 7 {
            return Err(format!(
                "`aes_encrypt_block` needs 7 leaves but {} were given",
                args.len()
            ));
        }
        if !crate::crypto::hardware_target(self.triple) {
            self.trap_if("true")?;
            return Ok(vec![LValue::Const(0)]);
        }
        let (keys, keys_len, rounds) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let (block, block_len) = (operand(&args[3]), operand(&args[4]));
        let (out, out_len) = (operand(&args[5]), operand(&args[6]));
        // `rounds` is 10, 12 or 14, and the key `16 * (rounds + 1)` bytes.
        let r10 = self.fresh();
        self.out.push_str(&format!("  {r10} = icmp eq i64 {rounds}, 10\n"));
        let r12 = self.fresh();
        self.out.push_str(&format!("  {r12} = icmp eq i64 {rounds}, 12\n"));
        let r14 = self.fresh();
        self.out.push_str(&format!("  {r14} = icmp eq i64 {rounds}, 14\n"));
        let r1012 = self.fresh();
        self.out.push_str(&format!("  {r1012} = or i1 {r10}, {r12}\n"));
        let known = self.fresh();
        self.out.push_str(&format!("  {known} = or i1 {r1012}, {r14}\n"));
        let unknown = self.fresh();
        self.out.push_str(&format!("  {unknown} = xor i1 {known}, true\n"));
        self.trap_if(&unknown)?;
        let blocks = self.fresh();
        self.out.push_str(&format!("  {blocks} = add i64 {rounds}, 1\n"));
        let want = self.fresh();
        self.out.push_str(&format!("  {want} = mul i64 {blocks}, 16\n"));
        let bad_keys = self.fresh();
        self.out.push_str(&format!("  {bad_keys} = icmp ne i64 {keys_len}, {want}\n"));
        self.trap_if(&bad_keys)?;
        for len in [block_len, out_len] {
            let bad = self.fresh();
            self.out.push_str(&format!("  {bad} = icmp ne i64 {len}, 16\n"));
            self.trap_if(&bad)?;
        }
        self.out.push_str(&format!(
            "  call void @lexsys_aes_encrypt_block(ptr {keys}, i64 {rounds}, ptr {block}, ptr {out})\n"
        ));
        Ok(vec![LValue::Const(0)])
    }

    /// `[h ptr, len, y ptr, len, data ptr, len]`.
    pub(crate) fn ghash_update(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 6 {
            return Err(format!("`ghash_update` needs 6 leaves but {} were given", args.len()));
        }
        if !crate::crypto::hardware_target(self.triple) {
            self.trap_if("true")?;
            return Ok(vec![LValue::Const(0)]);
        }
        let (h, h_len) = (operand(&args[0]), operand(&args[1]));
        let (y, y_len) = (operand(&args[2]), operand(&args[3]));
        let (data, data_len) = (operand(&args[4]), operand(&args[5]));
        for len in [h_len, y_len] {
            let bad = self.fresh();
            self.out.push_str(&format!("  {bad} = icmp ne i64 {len}, 16\n"));
            self.trap_if(&bad)?;
        }
        let rest = self.fresh();
        self.out.push_str(&format!("  {rest} = and i64 {data_len}, 15\n"));
        let ragged = self.fresh();
        self.out.push_str(&format!("  {ragged} = icmp ne i64 {rest}, 0\n"));
        self.trap_if(&ragged)?;
        let blocks = self.fresh();
        self.out.push_str(&format!("  {blocks} = lshr i64 {data_len}, 4\n"));
        self.out.push_str(&format!(
            "  call void @lexsys_ghash_update(ptr {h}, ptr {y}, ptr {data}, i64 {blocks})\n"
        ));
        Ok(vec![LValue::Const(0)])
    }
}
