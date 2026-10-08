//! GHASH with one reduction for up to eight blocks, and the GCM tag built
//! on it (`docs/gcm-wide.md` §3.2).
//!
//! A block is held byte-reversed, so that bit `127 - i` of the 128-bit
//! integer is the coefficient of `x^i` (GCM's "reflected" order) and no bit
//! reversal is ever needed. The carry-less product of two such integers has
//! the coefficient of `x^m` at bit `254 - m`, which as a 256-bit window with
//! its top bit as `x^0` is `x` times the product: so every power of H is
//! stored times `x^-1` (`twist`) and no product is shifted. The window's
//! upper half is the product's low half in the same order and its lower half
//! the high half, to be folded back by `x^7 + x^2 + x + 1` (`@cancho_gh_reduce`,
//! by shifts of 64-bit lanes). The sum of eight products needs one fold, not
//! eight: `(a*h^8 + b*h^7 + ...) mod p`.
//!
//! The model this is written from, checked against the bitwise algorithm of
//! SP 800-38D §6.3 before any IR was written, is `scripts/ghash_reflected_model.py`
//! and `docs/gcm-wide.md` §3.2.

use super::{Gen, Isa, V};
use std::fmt::Write;

/// The carry-less product of one 64-bit half of `a` by one of `b`.
fn clmul(isa: Isa, g: &mut Gen, a: &str, a_hi: bool, b: &str, b_hi: bool) -> String {
    if isa.x86 {
        let imm = u8::from(a_hi) | u8::from(b_hi) << 4;
        g.op(&format!("call {V} @llvm.x86.pclmulqdq({V} {a}, {V} {b}, i8 {imm})"))
    } else {
        let x = g.op(&format!("extractelement {V} {a}, i32 {}", u8::from(a_hi)));
        let y = g.op(&format!("extractelement {V} {b}, i32 {}", u8::from(b_hi)));
        let p = g.op(&format!("call <16 x i8> @llvm.aarch64.neon.pmull64(i64 {x}, i64 {y})"));
        g.op(&format!("bitcast <16 x i8> {p} to {V}"))
    }
}

/// The three unreduced sums of `x_j * h_j`: low halves, high halves and the
/// two cross products together.
struct Sums {
    lo: String,
    mid: String,
    hi: String,
}

/// Adds `x * h` to `sums` (`None` starts them).
fn accumulate(isa: Isa, g: &mut Gen, sums: Option<Sums>, x: &str, h: &str) -> Sums {
    let lo = clmul(isa, g, x, false, h, false);
    let hi = clmul(isa, g, x, true, h, true);
    let m1 = clmul(isa, g, x, false, h, true);
    let m2 = clmul(isa, g, x, true, h, false);
    let mid = g.op(&format!("xor {V} {m1}, {m2}"));
    match sums {
        None => Sums { lo, mid, hi },
        Some(s) => Sums {
            lo: g.op(&format!("xor {V} {}, {lo}", s.lo)),
            mid: g.op(&format!("xor {V} {}, {mid}", s.mid)),
            hi: g.op(&format!("xor {V} {}, {hi}", s.hi)),
        },
    }
}

/// `x` byte-reversed: a block read in reflected order.
fn reverse(g: &mut Gen, x: &str) -> String {
    let b = g.op(&format!("bitcast {V} {x} to <16 x i8>"));
    let r = g.op(&format!(
        "shufflevector <16 x i8> {b}, <16 x i8> undef, <16 x i32> <i32 15, i32 14, i32 13, i32 12, i32 11, i32 10, i32 9, i32 8, i32 7, i32 6, i32 5, i32 4, i32 3, i32 2, i32 1, i32 0>"
    ));
    g.op(&format!("bitcast <16 x i8> {r} to {V}"))
}

/// A block of the message at `ptr`, reflected.
fn load_block(g: &mut Gen, ptr: &str) -> String {
    let raw = g.op(&format!("load {V}, ptr {ptr}, align 1"));
    reverse(g, &raw)
}

/// The helpers: `@cancho_gh_reduce`, `@cancho_gh_agg{n}` for `n` in 1, 2, 4
/// and 8, `@cancho_gh_mul1`, and `@cancho_gh_absorb`.
pub(super) fn core(isa: Isa) -> String {
    let f = isa.features;
    let mut out = String::new();
    // The reduction of the 256-bit product `(hi + mid >> 64 : lo + mid << 64)`, by shifts of 64-bit lanes only
    // (the sequence of the Linux kernel's `ghash-clmulni-intel`, checked in `scripts/ghash_reflected_model.py`):
    // a first phase that folds the low half's lanes into the next lane, a second that folds what it made.
    let _ = write!(
        out,
        "define internal {V} @cancho_gh_reduce({V} %lo, {V} %mid, {V} %hi) alwaysinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %midl = shufflevector {V} %mid, {V} zeroinitializer, <2 x i32> <i32 2, i32 0>\n\
  %midh = shufflevector {V} %mid, {V} zeroinitializer, <2 x i32> <i32 1, i32 2>\n\
  %data = xor {V} %lo, %midl\n\
  %top = xor {V} %hi, %midh\n\
  %a1 = shl {V} %data, <i64 1, i64 1>\n\
  %a2 = xor {V} %a1, %data\n\
  %a3 = shl {V} %a2, <i64 5, i64 5>\n\
  %a4 = xor {V} %a3, %data\n\
  %t3 = shl {V} %a4, <i64 57, i64 57>\n\
  %t2 = shufflevector {V} %t3, {V} zeroinitializer, <2 x i32> <i32 2, i32 0>\n\
  %t3h = shufflevector {V} %t3, {V} zeroinitializer, <2 x i32> <i32 1, i32 2>\n\
  %data2 = xor {V} %data, %t2\n\
  %top2 = xor {V} %top, %t3h\n\
  %b1 = lshr {V} %data2, <i64 5, i64 5>\n\
  %b2 = xor {V} %b1, %data2\n\
  %b3 = lshr {V} %b2, <i64 1, i64 1>\n\
  %b4 = xor {V} %b3, %data2\n\
  %b5 = lshr {V} %b4, <i64 1, i64 1>\n\
  %r0 = xor {V} %top2, %b5\n\
  %r = xor {V} %r0, %data2\n\
  ret {V} %r\n\
}}\n"
    );
    // y' = (y + x) * H, for a block already reflected: `@cancho_gh_mul1`.
    {
        let mut g = Gen::new();
        let h = g.op(&format!("load {V}, ptr %tab, align 1"));
        let s = accumulate(isa, &mut g, None, "%x", &h);
        let r =
            g.op(&format!("call {V} @cancho_gh_reduce({V} {}, {V} {}, {V} {})", s.lo, s.mid, s.hi));
        let _ = write!(
            out,
            "define internal {V} @cancho_gh_mul1(ptr %tab, {V} %x) alwaysinline \"target-features\"=\"{f}\" {{\nentry:\n{}  ret {V} {r}\n}}\n",
            g.out
        );
    }
    // `@cancho_gh_agg{n}`: n blocks at `%data` absorbed into `%y`, block j
    // multiplied by H^(n-j), one reduction.
    for n in [1usize, 2, 4, 8] {
        let mut g = Gen::new();
        let mut sums = None;
        for j in 0..n {
            let p = g.op(&format!("getelementptr i8, ptr %data, i64 {}", 16 * j));
            let mut x = load_block(&mut g, &p);
            if j == 0 {
                x = g.op(&format!("xor {V} {x}, %y"));
            }
            let hp = g.op(&format!("getelementptr i8, ptr %tab, i64 {}", 16 * (n - 1 - j)));
            let h = g.op(&format!("load {V}, ptr {hp}, align 1"));
            sums = Some(accumulate(isa, &mut g, sums, &x, &h));
        }
        let s = sums.expect("n is at least 1");
        let r =
            g.op(&format!("call {V} @cancho_gh_reduce({V} {}, {V} {}, {V} {})", s.lo, s.mid, s.hi));
        let _ = write!(
            out,
            "define internal {V} @cancho_gh_agg{n}(ptr %tab, ptr %data, {V} %y) alwaysinline \"target-features\"=\"{f}\" {{\nentry:\n{}  ret {V} {r}\n}}\n",
            g.out
        );
    }
    out.push_str(&absorb(f));
    out
}

/// `@cancho_gh_absorb(tab, y, data, len)`: `y` after `len` bytes of `data`,
/// zero-padded to a multiple of 16. Groups of eight blocks go straight
/// through; the rest (under 128 bytes) is split into groups of 4, 2 and 1
/// block by the bits of its block count, with the last partial block copied
/// into a zeroed stack buffer first. Every branch is on `len`.
fn absorb(f: &str) -> String {
    let mut g = String::new();
    let _ = write!(
        g,
        "define internal {V} @cancho_gh_absorb(ptr %tab, {V} %y, ptr %data, i64 %len) \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %buf = alloca [128 x i8], align 16\n\
  %nb = lshr i64 %len, 7\n\
  br label %h8\n\
h8:\n\
  %i = phi i64 [ 0, %entry ], [ %i1, %b8 ]\n\
  %y8 = phi {V} [ %y, %entry ], [ %yn, %b8 ]\n\
  %more = icmp ult i64 %i, %nb\n\
  br i1 %more, label %b8, label %tail\n\
b8:\n\
  %off = shl i64 %i, 7\n\
  %dp = getelementptr i8, ptr %data, i64 %off\n\
  %yn = call {V} @cancho_gh_agg8(ptr %tab, ptr %dp, {V} %y8)\n\
  %i1 = add i64 %i, 1\n\
  br label %h8\n\
tail:\n\
  %rem = and i64 %len, 127\n\
  %any = icmp ne i64 %rem, 0\n\
  br i1 %any, label %tail_do, label %fin\n\
tail_do:\n\
  %full = lshr i64 %rem, 4\n\
  %part = and i64 %rem, 15\n\
  %ragged = icmp ne i64 %part, 0\n\
  %rg = zext i1 %ragged to i64\n\
  %nbl = add i64 %full, %rg\n\
  %base = shl i64 %nb, 7\n\
  %src0 = getelementptr i8, ptr %data, i64 %base\n\
  br i1 %ragged, label %copy, label %c8\n\
copy:\n\
  call void @llvm.memset.p0.i64(ptr %buf, i8 0, i64 128, i1 false)\n\
  call void @llvm.memcpy.p0.p0.i64(ptr %buf, ptr %src0, i64 %rem, i1 false)\n\
  br label %c8\n\
c8:\n\
  %src = phi ptr [ %src0, %tail_do ], [ %buf, %copy ]\n"
    );
    // Stages 8, 4, 2, 1: do the group when its bit is set in the count.
    let stages = [8usize, 4, 2, 1];
    let mut y_in = "%y8".to_owned();
    let mut o_in = "0".to_owned();
    for (idx, n) in stages.iter().enumerate() {
        let (y_out, o_out) = (format!("%yo{n}"), format!("%oo{n}"));
        let next = stages.get(idx + 1).map_or("fin".to_owned(), |m| format!("c{m}"));
        if idx > 0 {
            let _ = writeln!(g, "c{n}:");
        }
        let _ = write!(
            g,
            "  %t{n} = and i64 %nbl, {n}\n\
  %d{n} = icmp ne i64 %t{n}, 0\n\
  br i1 %d{n}, label %do{n}, label %j{n}\n\
do{n}:\n\
  %p{n} = getelementptr i8, ptr %src, i64 {o_in}\n\
  %yd{n} = call {V} @cancho_gh_agg{n}(ptr %tab, ptr %p{n}, {V} {y_in})\n\
  %od{n} = add i64 {o_in}, {}\n\
  br label %j{n}\n\
j{n}:\n\
  {y_out} = phi {V} [ {y_in}, %c{n} ], [ %yd{n}, %do{n} ]\n\
  {o_out} = phi i64 [ {o_in}, %c{n} ], [ %od{n}, %do{n} ]\n\
  br label %{next}\n",
            16 * n
        );
        y_in = y_out;
        o_in = o_out;
    }
    let _ = write!(g, "fin:\n  %yf = phi {V} [ %y8, %tail ], [ {y_in}, %j1 ]\n  ret {V} %yf\n}}\n");
    g
}

/// `v` times x^-1 modulo the GCM polynomial, in the reflected order: a shift left by one, and the constant
/// `0xC2000000000000000000000000000001` (x^127 + x^6 + x + 1, x^-1 itself) when the bit that left was set.
fn twist(g: &mut Gen, v: &str) -> String {
    let b = g.op(&format!("bitcast {V} {v} to i128"));
    let carry = g.op(&format!("lshr i128 {b}, 127"));
    let mask = g.op(&format!("sub i128 0, {carry}"));
    let fold = g.op(&format!("and i128 {mask}, 257870231182273679343338569694386847745"));
    let shifted = g.op(&format!("shl i128 {b}, 1"));
    let r = g.op(&format!("xor i128 {shifted}, {fold}"));
    g.op(&format!("bitcast i128 {r} to {V}"))
}

/// `@cancho_ghash_powers(ptr h, ptr table)`: the powers H^1 to H^8, each times x^-1 (`twist`), one 16-byte entry
/// each. A product of two blocks, one of them twisted, comes out of `@cancho_gh_reduce` already aligned, so the
/// products need no shift (`docs/gcm-wide.md` §3.2). The powers themselves are made with the same products: H^(k+1) is
/// H^k times the twisted H.
pub(super) fn powers(isa: Isa) -> String {
    let f = isa.features;
    let mut g = Gen::new();
    let h = load_block(&mut g, "%h");
    let first = twist(&mut g, &h);
    g.line(&format!("store {V} {first}, ptr %table, align 1"));
    let mut power = h.clone();
    for k in 1..8 {
        let s = accumulate(isa, &mut g, None, &power, &first);
        power =
            g.op(&format!("call {V} @cancho_gh_reduce({V} {}, {V} {}, {V} {})", s.lo, s.mid, s.hi));
        let t = twist(&mut g, &power);
        let p = g.op(&format!("getelementptr i8, ptr %table, i64 {}", 16 * k));
        g.line(&format!("store {V} {t}, ptr {p}, align 1"));
    }
    format!(
        "define internal void @cancho_ghash_powers(ptr %h, ptr %table) noinline \"target-features\"=\"{f}\" {{\nentry:\n{}  ret void\n}}\n",
        g.out
    )
}

/// `@cancho_gcm_tag_value`, and the two builtins' functions on it: the tag
/// written (`write`), or compared with the expected one (`diff`).
pub(super) fn tag(isa: Isa, write: bool, diff: bool) -> String {
    let f = isa.features;
    let params = "ptr %rk, i64 %rounds, ptr %tab, ptr %nonce, ptr %aad, i64 %aadlen, ptr %text, i64 %textlen";
    let call = "ptr %rk, i64 %rounds, ptr %tab, ptr %nonce, ptr %aad, i64 %aadlen, ptr %text, i64 %textlen";
    let mut g = Gen::new();
    // J0 = nonce || 1; its encryption is issued first so that its latency
    // runs under the hashing.
    g.line("%n0 = load i96, ptr %nonce, align 1");
    g.line("%n1 = zext i96 %n0 to i128");
    g.line("%nv = bitcast i128 %n1 to <4 x i32>");
    g.line("%j0 = insertelement <4 x i32> %nv, i32 16777216, i32 3");
    g.line(&format!("%j0v = bitcast <4 x i32> %j0 to {V}"));
    g.line(&format!("%mask = call {V} @cancho_aes1(ptr %rk, i64 %rounds, {V} %j0v)"));
    g.line(&format!(
        "%y1 = call {V} @cancho_gh_absorb(ptr %tab, {V} zeroinitializer, ptr %aad, i64 %aadlen)"
    ));
    g.line(&format!(
        "%y2 = call {V} @cancho_gh_absorb(ptr %tab, {V} %y1, ptr %text, i64 %textlen)"
    ));
    // The lengths block, reflected: the text's bits low, the associated
    // data's high.
    g.line("%abits = shl i64 %aadlen, 3");
    g.line("%tbits = shl i64 %textlen, 3");
    g.line(&format!("%l0 = insertelement {V} undef, i64 %tbits, i32 0"));
    g.line(&format!("%l1 = insertelement {V} %l0, i64 %abits, i32 1"));
    g.line(&format!("%y3 = xor {V} %y2, %l1"));
    g.line(&format!("%y4 = call {V} @cancho_gh_mul1(ptr %tab, {V} %y3)"));
    let raw = reverse(&mut g, "%y4");
    g.line(&format!("%tag = xor {V} {raw}, %mask"));
    let mut out = format!(
        "define internal {V} @cancho_gcm_tag_value({params}) \"target-features\"=\"{f}\" {{\nentry:\n{}  ret {V} %tag\n}}\n",
        g.out
    );
    if write {
        let _ = write!(
            out,
            "define internal void @cancho_gcm_tag({params}, ptr %out) noinline \"target-features\"=\"{f}\" {{\nentry:\n  %t = call {V} @cancho_gcm_tag_value({call})\n  store {V} %t, ptr %out, align 1\n  ret void\n}}\n"
        );
    }
    if diff {
        // Both halves of the difference are ORed, so every byte counts.
        let _ = write!(
            out,
            "define internal i64 @cancho_gcm_tag_diff({params}, ptr %expected) noinline \"target-features\"=\"{f}\" {{\nentry:\n  %t = call {V} @cancho_gcm_tag_value({call})\n  %e = load {V}, ptr %expected, align 1\n  %d = xor {V} %t, %e\n  %lo = extractelement {V} %d, i32 0\n  %hi = extractelement {V} %d, i32 1\n  %o = or i64 %lo, %hi\n  ret i64 %o\n}}\n"
        );
    }
    out
}
