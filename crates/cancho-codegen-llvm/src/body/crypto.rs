//! The call sites of the hardware AES and carry-less-multiply builtins
//! (`docs/crypto-builtins.md` §3, §4; `docs/gcm-wide.md` §4): each checks
//! its lengths, trapping as an index out of bounds does, then calls the
//! out-of-line function `crate::crypto` defines. On a target without the
//! instructions, `hw_aes_gcm()` is false and the builtins trap.

use crate::*;

impl<'a> FuncEmitter<'a> {
    pub(crate) fn hw_aes_gcm(&mut self) -> Result<Vec<LValue>, String> {
        if !crate::crypto::hardware_target(self.triple) {
            return Ok(vec![LValue::Reg(LKind::I8.zero().to_owned())]);
        }
        let answer = self.fresh();
        self.out.push_str(&format!("  {answer} = call i8 @cancho_hw_aes_gcm()\n"));
        Ok(vec![LValue::Reg(answer)])
    }

    /// Traps unless `rounds` is 10, 12 or 14 and the key is
    /// `16 * (rounds + 1)` bytes.
    fn check_round_keys(&mut self, rounds: &str, keys_len: &str) -> Result<(), String> {
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
        self.trap_if(&bad_keys)
    }

    /// Traps unless `len` is `want`.
    fn check_len(&mut self, len: &str, want: i64) -> Result<(), String> {
        let bad = self.fresh();
        self.out.push_str(&format!("  {bad} = icmp ne i64 {len}, {want}\n"));
        self.trap_if(&bad)
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
        self.check_round_keys(&rounds, &keys_len)?;
        self.check_len(&block_len, 16)?;
        self.check_len(&out_len, 16)?;
        self.out.push_str(&format!(
            "  call void @cancho_aes_encrypt_block(ptr {keys}, i64 {rounds}, ptr {block}, ptr {out})\n"
        ));
        Ok(vec![LValue::Const(0)])
    }

    /// `[round_keys ptr, len, rounds, nonce ptr, len, counter, input ptr,
    /// len, out ptr, len]`.
    pub(crate) fn aes_ctr32(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 10 {
            return Err(format!("`aes_ctr32` needs 10 leaves but {} were given", args.len()));
        }
        if !crate::crypto::hardware_target(self.triple) {
            self.trap_if("true")?;
            return Ok(vec![LValue::Const(0)]);
        }
        let (keys, keys_len, rounds) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let (nonce, nonce_len, counter) = (operand(&args[3]), operand(&args[4]), operand(&args[5]));
        let (input, input_len) = (operand(&args[6]), operand(&args[7]));
        let (out, out_len) = (operand(&args[8]), operand(&args[9]));
        self.check_round_keys(&rounds, &keys_len)?;
        self.check_len(&nonce_len, 12)?;
        // `counter` is a 32-bit value, so it is in 0..2^32.
        let above = self.fresh();
        self.out.push_str(&format!("  {above} = icmp ugt i64 {counter}, 4294967295\n"));
        self.trap_if(&above)?;
        let unequal = self.fresh();
        self.out.push_str(&format!("  {unequal} = icmp ne i64 {input_len}, {out_len}\n"));
        self.trap_if(&unequal)?;
        self.out.push_str(&format!(
            "  call void @cancho_aes_ctr32(ptr {keys}, i64 {rounds}, ptr {nonce}, i64 {counter}, ptr {input}, ptr {out}, i64 {input_len})\n"
        ));
        Ok(vec![LValue::Const(0)])
    }

    /// `[h ptr, len, table ptr, len]`.
    pub(crate) fn ghash_powers(&mut self, args: &[LValue]) -> Result<Vec<LValue>, String> {
        if args.len() != 4 {
            return Err(format!("`ghash_powers` needs 4 leaves but {} were given", args.len()));
        }
        if !crate::crypto::hardware_target(self.triple) {
            self.trap_if("true")?;
            return Ok(vec![LValue::Const(0)]);
        }
        let (h, h_len) = (operand(&args[0]), operand(&args[1]));
        let (table, table_len) = (operand(&args[2]), operand(&args[3]));
        self.check_len(&h_len, 16)?;
        self.check_len(&table_len, 128)?;
        self.out.push_str(&format!("  call void @cancho_ghash_powers(ptr {h}, ptr {table})\n"));
        Ok(vec![LValue::Const(0)])
    }

    /// `gcm_tag` (`diff` false) or `gcm_tag_diff`: `[round_keys ptr, len,
    /// rounds, table ptr, len, nonce ptr, len, aad ptr, len, text ptr, len,
    /// tag ptr, len]`.
    pub(crate) fn gcm_tag(&mut self, args: &[LValue], diff: bool) -> Result<Vec<LValue>, String> {
        if args.len() != 13 {
            return Err(format!("`gcm_tag` needs 13 leaves but {} were given", args.len()));
        }
        if !crate::crypto::hardware_target(self.triple) {
            self.trap_if("true")?;
            return Ok(vec![LValue::Const(0)]);
        }
        let (keys, keys_len, rounds) = (operand(&args[0]), operand(&args[1]), operand(&args[2]));
        let (table, table_len) = (operand(&args[3]), operand(&args[4]));
        let (nonce, nonce_len) = (operand(&args[5]), operand(&args[6]));
        let (aad, aad_len) = (operand(&args[7]), operand(&args[8]));
        let (text, text_len) = (operand(&args[9]), operand(&args[10]));
        let (tag, tag_len) = (operand(&args[11]), operand(&args[12]));
        self.check_round_keys(&rounds, &keys_len)?;
        self.check_len(&table_len, 128)?;
        self.check_len(&nonce_len, 12)?;
        self.check_len(&tag_len, 16)?;
        let call = format!(
            "@{}(ptr {keys}, i64 {rounds}, ptr {table}, ptr {nonce}, ptr {aad}, i64 {aad_len}, ptr {text}, i64 {text_len}, ptr {tag})",
            if diff { "cancho_gcm_tag_diff" } else { "cancho_gcm_tag" }
        );
        if diff {
            let verdict = self.fresh();
            self.out.push_str(&format!("  {verdict} = call i64 {call}\n"));
            return Ok(vec![LValue::Reg(verdict)]);
        }
        self.out.push_str(&format!("  call void {call}\n"));
        Ok(vec![LValue::Const(0)])
    }
}
