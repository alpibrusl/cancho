//! AES: one block, and counter mode eight blocks at a time
//! (`docs/gcm-wide.md` §3.1).
//!
//! `@cancho_aes1` and `@cancho_aes8` are the cipher on one and on eight
//! blocks. The eight are independent, so the instruction latency of one
//! round is hidden behind the other seven: a round of `aesenc` takes 3 to
//! 4 cycles to answer and one or two a cycle to issue, so one block alone
//! waits and eight keep the unit busy.

use super::{Gen, Isa, V};
use std::fmt::Write;

/// The two cipher functions. `with_eight` is whether counter mode needs
/// the wide one.
pub(super) fn core(isa: Isa, with_eight: bool) -> String {
    let mut out = lanes(isa, 1);
    if with_eight {
        out.push_str(&lanes(isa, 8));
    }
    out
}

fn ret_type(n: usize) -> String {
    if n == 1 { V.to_owned() } else { format!("{{ {} }}", vec![V; n].join(", ")) }
}

/// `@cancho_aes{n}(ptr %rk, i64 %rounds, <2 x i64> %b0, ...)`: `n` blocks
/// through `rounds` rounds of the key at `%rk`.
///
/// The instructions differ in where they place AddRoundKey (`AESENC` last,
/// `AESE` first), so the loop differs and the answer does not.
fn lanes(isa: Isa, n: usize) -> String {
    let f = isa.features;
    let ret = ret_type(n);
    let params: String = (0..n).map(|k| format!(", {V} %b{k}")).collect();
    let mut g = String::new();
    let _ = write!(
        g,
        "define internal {ret} @cancho_aes{n}(ptr %rk, i64 %rounds{params}) alwaysinline \"target-features\"=\"{f}\" {{\nentry:\n"
    );
    let (first, limit, start): (usize, String, Vec<String>);
    if isa.x86 {
        // The first key is XORed in before the rounds.
        g.push_str(&format!("  %k0 = load {V}, ptr %rk, align 1\n"));
        for k in 0..n {
            g.push_str(&format!("  %x{k} = xor {V} %b{k}, %k0\n"));
        }
        first = 1;
        limit = "%rounds".to_owned();
        start = (0..n).map(|k| format!("%x{k}")).collect();
    } else {
        g.push_str("  %lim = sub i64 %rounds, 1\n");
        first = 0;
        limit = "%lim".to_owned();
        start = (0..n).map(|k| format!("%b{k}")).collect();
    }
    g.push_str("  br label %loop\nloop:\n");
    g.push_str(&format!("  %i = phi i64 [ {first}, %entry ], [ %i1, %body ]\n"));
    for (k, init) in start.iter().enumerate() {
        g.push_str(&format!("  %s{k} = phi {V} [ {init}, %entry ], [ %n{k}, %body ]\n"));
    }
    g.push_str(&format!(
        "  %more = icmp ult i64 %i, {limit}\n  br i1 %more, label %body, label %end\nbody:\n"
    ));
    g.push_str("  %off = shl i64 %i, 4\n  %kp = getelementptr i8, ptr %rk, i64 %off\n");
    g.push_str(&format!("  %k = load {V}, ptr %kp, align 1\n"));
    if isa.x86 {
        for k in 0..n {
            g.push_str(&format!("  %n{k} = call {V} @llvm.x86.aesni.aesenc({V} %s{k}, {V} %k)\n"));
        }
    } else {
        g.push_str("  %kb = bitcast <2 x i64> %k to <16 x i8>\n");
        for k in 0..n {
            g.push_str(&format!("  %sb{k} = bitcast {V} %s{k} to <16 x i8>\n"));
            g.push_str(&format!("  %e{k} = call <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8> %sb{k}, <16 x i8> %kb)\n"));
            g.push_str(&format!(
                "  %m{k} = call <16 x i8> @llvm.aarch64.crypto.aesmc(<16 x i8> %e{k})\n"
            ));
            g.push_str(&format!("  %n{k} = bitcast <16 x i8> %m{k} to {V}\n"));
        }
    }
    g.push_str("  %i1 = add i64 %i, 1\n  br label %loop\nend:\n");
    g.push_str(&format!(
        "  %offl = shl i64 {limit}, 4\n  %klp = getelementptr i8, ptr %rk, i64 %offl\n"
    ));
    g.push_str(&format!("  %kl = load {V}, ptr %klp, align 1\n"));
    if isa.x86 {
        for k in 0..n {
            g.push_str(&format!(
                "  %r{k} = call {V} @llvm.x86.aesni.aesenclast({V} %s{k}, {V} %kl)\n"
            ));
        }
    } else {
        // The last round has no MixColumns; the final key is XORed in.
        g.push_str("  %klb = bitcast <2 x i64> %kl to <16 x i8>\n");
        g.push_str("  %offf = shl i64 %rounds, 4\n  %kfp = getelementptr i8, ptr %rk, i64 %offf\n");
        g.push_str(&format!("  %kf = load {V}, ptr %kfp, align 1\n"));
        for k in 0..n {
            g.push_str(&format!("  %sb{k}l = bitcast {V} %s{k} to <16 x i8>\n"));
            g.push_str(&format!("  %el{k} = call <16 x i8> @llvm.aarch64.crypto.aese(<16 x i8> %sb{k}l, <16 x i8> %klb)\n"));
            g.push_str(&format!("  %eb{k} = bitcast <16 x i8> %el{k} to {V}\n"));
            g.push_str(&format!("  %r{k} = xor {V} %eb{k}, %kf\n"));
        }
    }
    if n == 1 {
        g.push_str("  ret <2 x i64> %r0\n}\n");
    } else {
        let mut prev = "undef".to_owned();
        for k in 0..n {
            let name = format!("%agg{k}");
            g.push_str(&format!("  {name} = insertvalue {ret} {prev}, {V} %r{k}, {k}\n"));
            prev = name;
        }
        g.push_str(&format!("  ret {ret} {prev}\n}}\n"));
    }
    g
}

/// FIPS-197's cipher: `rounds + 1` round keys of 16 bytes at `%rk`, one
/// block at `%blk`, the result at `%out`. The caller checked every length.
pub(super) fn encrypt_block(isa: Isa) -> String {
    let f = isa.features;
    format!(
        "define internal void @cancho_aes_encrypt_block(ptr %rk, i64 %rounds, ptr %blk, ptr %out) noinline \"target-features\"=\"{f}\" {{\n\
entry:\n\
  %b = load {V}, ptr %blk, align 1\n\
  %r = call {V} @cancho_aes1(ptr %rk, i64 %rounds, {V} %b)\n\
  store {V} %r, ptr %out, align 1\n\
  ret void\n\
}}\n"
    )
}

/// The counter blocks `nonce || be32(counter + k)` for `k` in `0..n`, as
/// `<2 x i64>` values `%cb{k}`. `%base` is the nonce in a `<4 x i32>` and
/// `%cvec` has the counter in its last lane. The counter is added in the
/// machine's byte order and swapped to big-endian after, so it is a vector
/// add and a byte shuffle, and wraps modulo 2^32 inside its lane.
fn counter_blocks(g: &mut Gen, n: usize, base: &str, cvec: &str) -> Vec<String> {
    (0..n)
        .map(|k| {
            let sum = g.op(&format!("add <4 x i32> {cvec}, <i32 0, i32 0, i32 0, i32 {k}>"));
            let swapped = g.op(&format!("call <4 x i32> @llvm.bswap.v4i32(<4 x i32> {sum})"));
            let merged = g.op(&format!(
                "shufflevector <4 x i32> {base}, <4 x i32> {swapped}, <4 x i32> <i32 0, i32 1, i32 2, i32 7>"
            ));
            g.op(&format!("bitcast <4 x i32> {merged} to {V}"))
        })
        .collect()
}

/// `@cancho_aes_ctr32(rk, rounds, nonce, counter, in, out, len)`: counter
/// mode over `len` bytes, 128 at a time, then the rest (under 128 bytes,
/// which may end inside a block) from one more set of eight keystream
/// blocks kept on the stack and wiped after. Every loop bound is `len`.
pub(super) fn ctr32(isa: Isa) -> String {
    let f = isa.features;
    let ret = ret_type(8);
    let mut g = String::new();
    let _ = write!(
        g,
        "define internal void @cancho_aes_ctr32(ptr %rk, i64 %rounds, ptr %nonce, i64 %counter, ptr %in, ptr %out, i64 %len) noinline \"target-features\"=\"{f}\" {{\nentry:\n  %buf = alloca [128 x i8], align 16\n"
    );
    g.push_str("  %n0 = load i96, ptr %nonce, align 1\n  %n1 = zext i96 %n0 to i128\n  %base = bitcast i128 %n1 to <4 x i32>\n");
    g.push_str("  %c = trunc i64 %counter to i32\n  %cv = insertelement <4 x i32> zeroinitializer, i32 %c, i32 3\n");
    g.push_str("  %big = lshr i64 %len, 7\n  br label %head\nhead:\n");
    g.push_str("  %i = phi i64 [ 0, %entry ], [ %i1, %body ]\n  %cur = phi <4 x i32> [ %cv, %entry ], [ %next, %body ]\n");
    g.push_str("  %more = icmp ult i64 %i, %big\n  br i1 %more, label %body, label %tail\nbody:\n");
    let mut body = Gen::new();
    let blocks = counter_blocks(&mut body, 8, "%base", "%cur");
    let args: String = blocks.iter().map(|b| format!(", {V} {b}")).collect();
    body.line(&format!("%ks = call {ret} @cancho_aes8(ptr %rk, i64 %rounds{args})"));
    body.line("%off = shl i64 %i, 7");
    for k in 0..8 {
        let key = body.op(&format!("extractvalue {ret} %ks, {k}"));
        let ip = body.op("getelementptr i8, ptr %in, i64 %off");
        let ipk = body.op(&format!("getelementptr i8, ptr {ip}, i64 {}", 16 * k));
        let data = body.op(&format!("load {V}, ptr {ipk}, align 1"));
        let x = body.op(&format!("xor {V} {data}, {key}"));
        let op = body.op("getelementptr i8, ptr %out, i64 %off");
        let opk = body.op(&format!("getelementptr i8, ptr {op}, i64 {}", 16 * k));
        body.line(&format!("store {V} {x}, ptr {opk}, align 1"));
    }
    g.push_str(&std::mem::take(&mut body.out));
    g.push_str("  %next = add <4 x i32> %cur, <i32 0, i32 0, i32 0, i32 8>\n  %i1 = add i64 %i, 1\n  br label %head\n");
    // The rest, under 128 bytes.
    g.push_str("tail:\n  %rem = and i64 %len, 127\n  %has = icmp ne i64 %rem, 0\n  br i1 %has, label %tail_do, label %done\ntail_do:\n");
    let tail = &mut body;
    let blocks = counter_blocks(tail, 8, "%base", "%cur");
    let args: String = blocks.iter().map(|b| format!(", {V} {b}")).collect();
    tail.line(&format!("%tks = call {ret} @cancho_aes8(ptr %rk, i64 %rounds{args})"));
    for k in 0..8 {
        let key = tail.op(&format!("extractvalue {ret} %tks, {k}"));
        let p = tail.op(&format!("getelementptr i8, ptr %buf, i64 {}", 16 * k));
        tail.line(&format!("store {V} {key}, ptr {p}, align 16"));
    }
    g.push_str(&std::mem::take(&mut tail.out));
    g.push_str("  %start = shl i64 %big, 7\n  %chunks = lshr i64 %rem, 4\n  br label %chead\n");
    // Whole blocks of the rest.
    g.push_str("chead:\n  %j = phi i64 [ 0, %tail_do ], [ %j1, %cbody ]\n  %cmore = icmp ult i64 %j, %chunks\n  br i1 %cmore, label %cbody, label %bpre\ncbody:\n");
    g.push_str("  %o = shl i64 %j, 4\n  %pos = add i64 %start, %o\n");
    g.push_str(&format!(
        "  %ip = getelementptr i8, ptr %in, i64 %pos\n  %d = load {V}, ptr %ip, align 1\n  %kp = getelementptr i8, ptr %buf, i64 %o\n  %kk = load {V}, ptr %kp, align 16\n  %xx = xor {V} %d, %kk\n  %op = getelementptr i8, ptr %out, i64 %pos\n  store {V} %xx, ptr %op, align 1\n  %j1 = add i64 %j, 1\n  br label %chead\n"
    ));
    // Then the bytes after the last whole block.
    g.push_str("bpre:\n  %bstart = shl i64 %chunks, 4\n  br label %bhead\nbhead:\n  %q = phi i64 [ %bstart, %bpre ], [ %q1, %bbody ]\n  %bmore = icmp ult i64 %q, %rem\n  br i1 %bmore, label %bbody, label %wipe\nbbody:\n");
    g.push_str("  %bpos = add i64 %start, %q\n  %bip = getelementptr i8, ptr %in, i64 %bpos\n  %bd = load i8, ptr %bip, align 1\n  %bkp = getelementptr i8, ptr %buf, i64 %q\n  %bk = load i8, ptr %bkp, align 1\n  %bx = xor i8 %bd, %bk\n  %bop = getelementptr i8, ptr %out, i64 %bpos\n  store i8 %bx, ptr %bop, align 1\n  %q1 = add i64 %q, 1\n  br label %bhead\n");
    // The keystream is secret: wipe it with stores the optimiser keeps.
    g.push_str("wipe:\n");
    for k in 0..8 {
        g.push_str(&format!(
            "  %wp{k} = getelementptr i8, ptr %buf, i64 {}\n  store volatile {V} zeroinitializer, ptr %wp{k}, align 16\n",
            16 * k
        ));
    }
    g.push_str("  br label %done\ndone:\n  ret void\n}\n");
    g
}
