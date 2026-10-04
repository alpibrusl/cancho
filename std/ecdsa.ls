module std.ecdsa;
import std.bigmod;

// `std.ecdsa` -- ECDSA signature verification on P-256 and P-384
// (SEC 1 §4.1.4; `docs/ecdsa.md`). Sub-issue 7 (#204) of the pure TLS 1.3
// client (#197). Not independently reviewed (#209).
//
// Verification only, on public data: nothing here is constant time,
// and the ladder branches on the scalars' bits. The arithmetic is
// `std.bigmod`'s registers, modulo p for points and modulo n for
// scalars (`docs/ecdsa.md` §2). Every function answers 0 for a valid
// signature or a negative code whose name is `refusal_tag(code)`.

pub fn ok() -> [] int {
    return 0;
}

pub fn curve_p256() -> [] int {
    return 256;
}

pub fn curve_p384() -> [] int {
    return 384;
}

// The stable name of a refusal code, `std.bigmod`'s included.
pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == -30 {
        return "ecdsa-curve";
    }
    if code == -31 {
        return "ecdsa-point-encoding";
    }
    if code == -32 {
        return "ecdsa-point-infinity";
    }
    if code == -33 {
        return "ecdsa-point-range";
    }
    if code == -34 {
        return "ecdsa-point-not-on-curve";
    }
    if code == -35 {
        return "ecdsa-raw-length";
    }
    if code == -36 {
        return "ecdsa-der-structure";
    }
    if code == -37 {
        return "ecdsa-der-non-minimal";
    }
    if code == -38 {
        return "ecdsa-r-range";
    }
    if code == -39 {
        return "ecdsa-s-range";
    }
    if code == -40 {
        return "ecdsa-result-infinity";
    }
    if code == -41 {
        return "ecdsa-mismatch";
    }
    if code == -42 {
        return "ecdsa-work-length";
    }
    return bigmod.refusal_tag(code);
}

// ---- Curve constants ----
//
// Printed by `scripts/ecdsa_params.py` from `openssl ecparam -param_enc
// explicit`, which also checks them (`docs/ecdsa.md` §2.1); not typed by
// hand.

fn p256_p() -> [] &static [byte] {
    return "ffffffff00000001000000000000000000000000ffffffffffffffffffffffff";
}

fn p256_b() -> [] &static [byte] {
    return "5ac635d8aa3a93e7b3ebbd55769886bc651d06b0cc53b0f63bce3c3e27d2604b";
}

fn p256_gx() -> [] &static [byte] {
    return "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296";
}

fn p256_gy() -> [] &static [byte] {
    return "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5";
}

fn p256_n() -> [] &static [byte] {
    return "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551";
}

fn p384_p() -> [] &static [byte] {
    return "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeffffffff0000000000000000ffffffff";
}

fn p384_b() -> [] &static [byte] {
    return "b3312fa7e23ee7e4988e056be3f82d19181d9c6efe8141120314088f5013875ac656398d8a2ed19d2a85c8edd3ec2aef";
}

fn p384_gx() -> [] &static [byte] {
    return "aa87ca22be8b05378eb1c71ef320ad746e1d3b628ba79b9859f741e082542a385502f25dbf55296c3a545e3872760ab7";
}

fn p384_gy() -> [] &static [byte] {
    return "3617de4a96262c6f5d9e98bf9292dc29f8f41dbd289a147ce9da3113b5f0b8c00a60b1ce1d7e819d7a431d7c90ea0e5f";
}

fn p384_n() -> [] &static [byte] {
    return "ffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973";
}

// `which`: 0 p, 1 b, 2 G's x, 3 G's y, 4 n.
fn constant(curve: int, which: int) -> [] &static [byte] {
    if curve == 256 {
        if which == 0 {
            return p256_p();
        }
        if which == 1 {
            return p256_b();
        }
        if which == 2 {
            return p256_gx();
        }
        if which == 3 {
            return p256_gy();
        }
        return p256_n();
    }
    if which == 0 {
        return p384_p();
    }
    if which == 1 {
        return p384_b();
    }
    if which == 2 {
        return p384_gx();
    }
    if which == 3 {
        return p384_gy();
    }
    return p384_n();
}

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

fn unhex[&h, &o](hex: &h [byte], out: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(out) {
        out[i] = byte_of(nibble(int_of(hex[2 * i])) * 16 + nibble(int_of(hex[2 * i + 1])));
        i = i + 1;
    }
    return 0;
}

// Curve parameter `which` (0 p, 1 b, 2 G's x, 3 G's y, 4 n) of `curve`
// (256 or 384) as big-endian bytes into `out`, the curve's size.
// `std.ecdh` takes its constants from here.
pub fn curve_param[&o](curve: int, which: int, out: &!o [byte]) -> [] int {
    return unhex(constant(curve, which), out);
}

// ---- Registers ----
//
// Field registers (modulo p): points are three consecutive registers
// X, Y, Z in Jacobian coordinates and Montgomery form, Z = 0 for
// infinity (`docs/ecdsa.md` §2.2).

fn pt_g() -> [] int {
    return 0;
}

fn pt_q() -> [] int {
    return 3;
}

fn pt_gq() -> [] int {
    return 6;
}

fn pt_r() -> [] int {
    return 9;
}

fn reg_b() -> [] int {
    return 12;
}

// Temporaries t0..t12 for `double` and `add`.
fn t(i: int) -> [] int {
    return bigmod.reg(13 + i);
}

fn x_of(p: int) -> [] int {
    return bigmod.reg(p);
}

fn y_of(p: int) -> [] int {
    return bigmod.reg(p + 1);
}

fn z_of(p: int) -> [] int {
    return bigmod.reg(p + 2);
}

pub fn work_len() -> [] int {
    return bigmod.registers_len(26);
}

fn copy_point[&w](w: &!w [int], from: int, to: int) -> [] int {
    bigmod.copy_reg(w, x_of(from), x_of(to));
    bigmod.copy_reg(w, y_of(from), y_of(to));
    bigmod.copy_reg(w, z_of(from), z_of(to));
    return 0;
}

// d = 2p (dbl-2001-b, a = -3). Gives Z = 0 by itself for p infinite or
// Y = 0. `d` may be `p`.
fn double[&w](w: &!w [int], p: int, d: int) -> [] int {
    let x = x_of(p);
    let y = y_of(p);
    let z = z_of(p);
    // delta = Z^2, gamma = Y^2, beta = X gamma.
    bigmod.mul(w, z, z, t(0));
    bigmod.mul(w, y, y, t(1));
    bigmod.mul(w, x, t(1), t(2));
    // alpha = 3 (X - delta)(X + delta).
    bigmod.sub(w, x, t(0), t(3));
    bigmod.add(w, x, t(0), t(4));
    bigmod.mul(w, t(3), t(4), t(3));
    bigmod.add(w, t(3), t(3), t(4));
    bigmod.add(w, t(4), t(3), t(5));
    // X3 = alpha^2 - 8 beta.
    bigmod.mul(w, t(5), t(5), t(6));
    bigmod.add(w, t(2), t(2), t(7));
    bigmod.add(w, t(7), t(7), t(7));
    bigmod.add(w, t(7), t(7), t(8));
    bigmod.sub(w, t(6), t(8), t(9));
    // Z3 = (Y + Z)^2 - gamma - delta.
    bigmod.add(w, y, z, t(10));
    bigmod.mul(w, t(10), t(10), t(10));
    bigmod.sub(w, t(10), t(1), t(10));
    bigmod.sub(w, t(10), t(0), t(10));
    // Y3 = alpha (4 beta - X3) - 8 gamma^2.
    bigmod.sub(w, t(7), t(9), t(11));
    bigmod.mul(w, t(5), t(11), t(11));
    bigmod.mul(w, t(1), t(1), t(12));
    bigmod.add(w, t(12), t(12), t(12));
    bigmod.add(w, t(12), t(12), t(12));
    bigmod.add(w, t(12), t(12), t(12));
    bigmod.sub(w, t(11), t(12), t(11));
    bigmod.copy_reg(w, t(9), x_of(d));
    bigmod.copy_reg(w, t(11), y_of(d));
    bigmod.copy_reg(w, t(10), z_of(d));
    return 0;
}

// d = p + q (add-2007-bl), with the cases it does not cover handled
// here: an infinite input, and equal x (a doubling, or infinity). `d`
// may be `p`.
fn add[&w](w: &!w [int], p: int, q: int, d: int) -> [] int {
    if bigmod.is_zero(w, z_of(p)) {
        return copy_point(w, q, d);
    }
    if bigmod.is_zero(w, z_of(q)) {
        return copy_point(w, p, d);
    }
    // Z1Z1, Z2Z2, U1 = X1 Z2Z2, U2 = X2 Z1Z1, S1 = Y1 Z2 Z2Z2, S2 = Y2 Z1 Z1Z1.
    bigmod.mul(w, z_of(p), z_of(p), t(0));
    bigmod.mul(w, z_of(q), z_of(q), t(1));
    bigmod.mul(w, x_of(p), t(1), t(2));
    bigmod.mul(w, x_of(q), t(0), t(3));
    bigmod.mul(w, y_of(p), z_of(q), t(4));
    bigmod.mul(w, t(4), t(1), t(4));
    bigmod.mul(w, y_of(q), z_of(p), t(5));
    bigmod.mul(w, t(5), t(0), t(5));
    // H = U2 - U1, r = S2 - S1.
    bigmod.sub(w, t(3), t(2), t(6));
    bigmod.sub(w, t(5), t(4), t(7));
    if bigmod.is_zero(w, t(6)) {
        if bigmod.is_zero(w, t(7)) {
            return double(w, p, d);
        }
        return bigmod.set_small(w, z_of(d), 0);
    }
    // r = 2 (S2 - S1), I = (2H)^2, J = H I, V = U1 I.
    bigmod.add(w, t(7), t(7), t(7));
    bigmod.add(w, t(6), t(6), t(8));
    bigmod.mul(w, t(8), t(8), t(8));
    bigmod.mul(w, t(6), t(8), t(9));
    bigmod.mul(w, t(2), t(8), t(10));
    // X3 = r^2 - J - 2V.
    bigmod.mul(w, t(7), t(7), t(11));
    bigmod.sub(w, t(11), t(9), t(11));
    bigmod.sub(w, t(11), t(10), t(11));
    bigmod.sub(w, t(11), t(10), t(11));
    // Y3 = r (V - X3) - 2 S1 J.
    bigmod.sub(w, t(10), t(11), t(12));
    bigmod.mul(w, t(7), t(12), t(12));
    bigmod.mul(w, t(4), t(9), t(3));
    bigmod.add(w, t(3), t(3), t(3));
    bigmod.sub(w, t(12), t(3), t(12));
    // Z3 = ((Z1 + Z2)^2 - Z1Z1 - Z2Z2) H.
    bigmod.add(w, z_of(p), z_of(q), t(3));
    bigmod.mul(w, t(3), t(3), t(3));
    bigmod.sub(w, t(3), t(0), t(3));
    bigmod.sub(w, t(3), t(1), t(3));
    bigmod.mul(w, t(3), t(6), t(3));
    bigmod.copy_reg(w, t(11), x_of(d));
    bigmod.copy_reg(w, t(12), y_of(d));
    bigmod.copy_reg(w, t(3), z_of(d));
    return 0;
}

// Bit `i` of big-endian `b`.
fn bit_of[&b](b: &b [byte], i: int) -> [] int {
    return int_of(b[len(b) - 1 - i / 8]) >> i % 8 & 1;
}

// ---- Verification ----

// Steps 2 to 4 of `docs/ecdsa.md` §3, modulo n: r and s in range, and
// u1 = e/s, u2 = r/s as big-endian bytes.
fn scalars[&r, &s, &d, &n, &a, &b, &w](curve: int, r: &r [byte], s: &s [byte], digest: &d [byte], n: &n [byte], u1: &!a [byte], u2: &!b [byte], w: &!w [int]) -> [] int {
    var code = bigmod.setup(n, w);
    if code != 0 {
        return code;
    }
    let rr = bigmod.reg(0);
    let sr = bigmod.reg(1);
    let er = bigmod.reg(2);
    let inv = bigmod.reg(3);
    let u = bigmod.reg(4);
    if bigmod.load_reg(r, w, rr) != 0 || bigmod.is_zero(w, rr) {
        return -38;
    }
    if bigmod.load_reg(s, w, sr) != 0 || bigmod.is_zero(w, sr) {
        return -39;
    }
    // e: the digest's leftmost bits(n) bits, whole bytes for both curves.
    var m = len(digest);
    if m > curve / 8 {
        m = curve / 8;
    }
    bigmod.load_reduced(digest[0..m], w, er);
    bigmod.to_mont(w, sr, inv);
    bigmod.inverse(w, inv, inv);
    bigmod.to_mont(w, er, er);
    bigmod.mul(w, er, inv, u);
    bigmod.from_mont(w, u, u);
    bigmod.store_reg(w, u, u1);
    bigmod.to_mont(w, rr, rr);
    bigmod.mul(w, rr, inv, u);
    bigmod.from_mont(w, u, u);
    bigmod.store_reg(w, u, u2);
    return 0;
}

// Step 1 and 5, modulo p: Q decoded and checked, then the x coordinate
// of u1 G + u2 Q as big-endian bytes.
fn point_sum[&q, &a, &b, &x, &w](curve: int, point: &q [byte], u1: &a [byte], u2: &b [byte], x: &!x [byte], w: &!w [int]) -> [] int {
    let size = curve / 8;
    var code = 0;
    region c {
        let p = alloc_slice[c](size, byte_of(0));
        let k = alloc_slice[c](size, byte_of(0));
        unhex(constant(curve, 0), p);
        code = bigmod.setup(p, w);
        if code == 0 {
            if bigmod.load_reg(point[1..1 + size], w, x_of(pt_q())) != 0 || bigmod.load_reg(point[1 + size..1 + 2 * size], w, y_of(pt_q())) != 0 {
                code = -33;
            }
        }
        if code == 0 {
            bigmod.to_mont(w, x_of(pt_q()), x_of(pt_q()));
            bigmod.to_mont(w, y_of(pt_q()), y_of(pt_q()));
            bigmod.set_small(w, z_of(pt_q()), 1);
            bigmod.to_mont(w, z_of(pt_q()), z_of(pt_q()));
            unhex(constant(curve, 1), k);
            bigmod.load_reg(k, w, bigmod.reg(reg_b()));
            bigmod.to_mont(w, bigmod.reg(reg_b()), bigmod.reg(reg_b()));
            // y^2 = x^3 - 3x + b.
            let qx = x_of(pt_q());
            bigmod.mul(w, y_of(pt_q()), y_of(pt_q()), t(0));
            bigmod.mul(w, qx, qx, t(1));
            bigmod.mul(w, t(1), qx, t(1));
            bigmod.add(w, qx, qx, t(2));
            bigmod.add(w, t(2), qx, t(2));
            bigmod.sub(w, t(1), t(2), t(1));
            bigmod.add(w, t(1), bigmod.reg(reg_b()), t(1));
            if !bigmod.equal(w, t(0), t(1)) {
                code = -34;
            }
        }
        if code == 0 {
            unhex(constant(curve, 2), k);
            bigmod.load_reg(k, w, x_of(pt_g()));
            unhex(constant(curve, 3), k);
            bigmod.load_reg(k, w, y_of(pt_g()));
            bigmod.to_mont(w, x_of(pt_g()), x_of(pt_g()));
            bigmod.to_mont(w, y_of(pt_g()), y_of(pt_g()));
            bigmod.copy_reg(w, z_of(pt_q()), z_of(pt_g()));
            add(w, pt_g(), pt_q(), pt_gq());
            // Shamir's trick: one doubling chain for both scalars.
            bigmod.set_small(w, z_of(pt_r()), 0);
            var i = 8 * size - 1;
            while i >= 0 {
                double(w, pt_r(), pt_r());
                let b1 = bit_of(u1, i);
                let b2 = bit_of(u2, i);
                if b1 == 1 && b2 == 1 {
                    add(w, pt_r(), pt_gq(), pt_r());
                } else if b1 == 1 {
                    add(w, pt_r(), pt_g(), pt_r());
                } else if b2 == 1 {
                    add(w, pt_r(), pt_q(), pt_r());
                }
                i = i - 1;
            }
            if bigmod.is_zero(w, z_of(pt_r())) {
                code = -40;
            }
        }
        if code == 0 {
            // x = X / Z^2.
            bigmod.mul(w, z_of(pt_r()), z_of(pt_r()), t(0));
            bigmod.inverse(w, t(0), t(0));
            bigmod.mul(w, x_of(pt_r()), t(0), t(0));
            bigmod.from_mont(w, t(0), t(0));
            bigmod.store_reg(w, t(0), x);
        }
    }
    return code;
}

// SEC 1 §4.1.4 on `r` and `s` as big-endian bytes (`docs/ecdsa.md` §3).
fn verify[&d, &q, &r, &s, &w](curve: int, digest: &d [byte], point: &q [byte], r: &r [byte], s: &s [byte], work: &!w [int]) -> [] int {
    let size = curve / 8;
    if len(point) == 1 && int_of(point[0]) == 0 {
        return -32;
    }
    if len(point) != 1 + 2 * size || int_of(point[0]) != 4 {
        return -31;
    }
    var code = 0;
    region v {
        let n = alloc_slice[v](size, byte_of(0));
        let u1 = alloc_slice[v](size, byte_of(0));
        let u2 = alloc_slice[v](size, byte_of(0));
        let x = alloc_slice[v](size, byte_of(0));
        unhex(constant(curve, 4), n);
        code = scalars(curve, r, s, digest, n, u1, u2, work);
        if code == 0 {
            code = point_sum(curve, point, u1, u2, x, work);
        }
        if code == 0 {
            // x mod n = r? x < p < 2n, so one subtraction reduces it.
            code = bigmod.setup(n, work);
            if code == 0 {
                bigmod.load_reduced(x, work, bigmod.reg(0));
                bigmod.load_reg(r, work, bigmod.reg(1));
                if !bigmod.equal(work, bigmod.reg(0), bigmod.reg(1)) {
                    code = -41;
                }
            }
        }
    }
    return code;
}

fn check_curve[&w](curve: int, work: &w [int]) -> [] int {
    if curve != 256 && curve != 384 {
        return -30;
    }
    if len(work) < work_len() {
        return -42;
    }
    return 0;
}

// `sig` = r || s, each exactly the curve's size (IEEE P1363).
pub fn verify_raw[&d, &q, &s, &w](curve: int, digest: &d [byte], point: &q [byte], sig: &s [byte], work: &!w [int]) -> [] int {
    let code = check_curve(curve, work);
    if code != 0 {
        return code;
    }
    let size = curve / 8;
    if len(sig) != 2 * size {
        return -35;
    }
    return verify(curve, digest, point, sig[0..size], sig[size..2 * size], work);
}

// A DER length at `at`: info[0] the length, info[1] where the content
// starts. 0, -36 or -37.
fn der_length[&s, &i](sig: &s [byte], at: int, info: &!i [int]) -> [] int {
    if at >= len(sig) {
        return -36;
    }
    let first = int_of(sig[at]);
    if first < 0x80 {
        info[0] = first;
        info[1] = at + 1;
        return 0;
    }
    let n = first & 0x7f;
    if n == 0 || n > 2 || at + n >= len(sig) {
        return -36;
    }
    if int_of(sig[at + 1]) == 0 {
        return -37;
    }
    var v = 0;
    var i = 1;
    while i <= n {
        v = v * 256 + int_of(sig[at + i]);
        i = i + 1;
    }
    if v < 0x80 {
        return -37;
    }
    info[0] = v;
    info[1] = at + 1 + n;
    return 0;
}

// The INTEGER at `at`, ending by `end`: info[0..2] its value's bytes
// (after one leading 00), info[2] where it ends. `range` is the code for
// a negative value.
fn der_integer[&s, &i](sig: &s [byte], at: int, end: int, range: int, info: &!i [int]) -> [] int {
    if at >= end || int_of(sig[at]) != 0x02 {
        return -36;
    }
    let code = der_length(sig, at + 1, info);
    if code != 0 {
        return code;
    }
    let start = info[1];
    let stop = start + info[0];
    if stop > end || info[0] == 0 {
        return -36;
    }
    let a = int_of(sig[start]);
    if info[0] > 1 {
        let b = int_of(sig[start + 1]);
        if a == 0 && b < 0x80 || a == 0xff && b >= 0x80 {
            return -37;
        }
    }
    if a >= 0x80 {
        return range;
    }
    info[0] = start;
    if a == 0 && info[0] + 1 < stop {
        info[0] = start + 1;
    }
    info[1] = stop;
    info[2] = stop;
    return 0;
}

// `sig` = SEQUENCE { INTEGER r, INTEGER s } in strict DER.
pub fn verify_der[&d, &q, &s, &w](curve: int, digest: &d [byte], point: &q [byte], sig: &s [byte], work: &!w [int]) -> [] int {
    var code = check_curve(curve, work);
    if code != 0 {
        return code;
    }
    if len(sig) < 2 || int_of(sig[0]) != 0x30 {
        return -36;
    }
    region p {
        let info = alloc_slice[p](3, 0);
        let rs = alloc_slice[p](4, 0);
        code = der_length(sig, 1, info);
        if code == 0 && info[1] + info[0] != len(sig) {
            code = -36;
        }
        let end = len(sig);
        var at = info[1];
        if code == 0 {
            code = der_integer(sig, at, end, -38, info);
            rs[0] = info[0];
            rs[1] = info[1];
            at = info[2];
        }
        if code == 0 {
            code = der_integer(sig, at, end, -39, info);
            rs[2] = info[0];
            rs[3] = info[1];
            if code == 0 && info[2] != end {
                code = -36;
            }
        }
        if code == 0 {
            code = verify(curve, digest, point, sig[rs[0]..rs[1]], sig[rs[2]..rs[3]], work);
        }
    }
    return code;
}
