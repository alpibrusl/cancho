edition 6;
module std.bigmod;

// `std.bigmod` -- unsigned integers modulo an odd `n` of up to 4,096
// bits, in Montgomery form (`docs/rsa.md` §2). Sub-issue 6 (#203) of the
// pure TLS 1.3 client (#197), under `std.rsa`. Not independently
// reviewed (#209).
//
// `pow_mod` and `inverse` branch on the exponent's bits, which must be
// public (`docs/rsa.md` §1). The register arithmetic -- `mul`, `add`,
// `sub` and the reduction they share -- is constant time: since #207 it
// carries `std.ecdh`'s secret scalar multiples (`docs/ecdh.md` §2).
// Inputs and outputs are big-endian bytes; leading zero bytes are
// allowed. All the working
// numbers live in the caller's `work`, `work_len()` words, so nothing is
// allocated from a size the input names.
//
// A number is 30-bit limbs, least significant first: the widest limb
// for which Montgomery multiplication's fused step,
// `t[j] + a*b[j] + m*n[j] + carry`, stays below 2^62 with checked
// arithmetic (`docs/rsa.md` §2.1).

pub fn ok() -> [] int {
    return 0;
}

pub fn refused_even_modulus() -> [] int {
    return -1;
}

pub fn refused_modulus_size() -> [] int {
    return -2;
}

pub fn refused_not_reduced() -> [] int {
    return -3;
}

pub fn refused_exponent() -> [] int {
    return -4;
}

pub fn refused_output_length() -> [] int {
    return -5;
}

pub fn refused_work_length() -> [] int {
    return -6;
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "bigmod-even-modulus";
    }
    if code == -2 {
        return "bigmod-modulus-size";
    }
    if code == -3 {
        return "bigmod-not-reduced";
    }
    if code == -4 {
        return "bigmod-exponent";
    }
    if code == -5 {
        return "bigmod-output-length";
    }
    if code == -6 {
        return "bigmod-work-length";
    }
    return "unknown";
}

pub fn max_bits() -> [] int {
    return 4096;
}

fn mask() -> [] int {
    return 0x3fffffff;
}

// 4,096 bits in 30-bit limbs, rounded up.
fn max_limbs() -> [] int {
    return 137;
}

// One number's slot in `work`: `max_limbs()` and two more, for the
// Montgomery accumulator's carry limbs.
fn stride() -> [] int {
    return 139;
}

// `work`: [0] the limb count k, [1] n' = -n^-1 mod 2^30, then six slots.
fn slot(i: int) -> [] int {
    return 2 + i * stride();
}

fn slot_n() -> [] int {
    return slot(0);
}

fn slot_r2() -> [] int {
    return slot(1);
}

fn slot_base() -> [] int {
    return slot(2);
}

fn slot_acc() -> [] int {
    return slot(3);
}

fn slot_t() -> [] int {
    return slot(4);
}

fn slot_two() -> [] int {
    return slot(5);
}

pub fn work_len() -> [] int {
    return slot(6);
}

// The number of significant bits in big-endian `b` (0 for zero).
pub fn bit_length[&b](b: &b [byte]) -> [] int {
    var i = 0;
    while i < len(b) && int_of(b[i]) == 0 {
        i = i + 1;
    }
    if i == len(b) {
        return 0;
    }
    var top = int_of(b[i]);
    var bits = 0;
    while top > 0 {
        bits = bits + 1;
        top = top >> 1;
    }
    return (len(b) - i - 1) * 8 + bits;
}

// Big-endian `b` into the `k` limbs at `at`; -3 when it needs more.
fn load[&b, &w](b: &b [byte], w: &!w [int], at: int, k: int) -> [] int {
    var i = 0;
    while i < k + 2 {
        w[at + i] = 0;
        i = i + 1;
    }
    var p = 0;
    var j = len(b) - 1;
    while j >= 0 {
        let v = int_of(b[j]);
        if v != 0 {
            let idx = p / 30;
            let off = p % 30;
            if idx >= k {
                return -3;
            }
            w[at + idx] = w[at + idx] | v << off & mask();
            let high = v >> 30 - off;
            if off > 22 && high != 0 {
                if idx + 1 >= k {
                    return -3;
                }
                w[at + idx + 1] = w[at + idx + 1] | high;
            }
        }
        p = p + 8;
        j = j - 1;
    }
    return 0;
}

// The `k` limbs at `at` as big-endian `out`, which must be wide enough.
fn store[&w, &o](w: &w [int], at: int, k: int, out: &!o [byte]) -> [] int {
    var j = 0;
    while j < len(out) {
        let p = j * 8;
        let idx = p / 30;
        let off = p % 30;
        var v = 0;
        if idx < k {
            v = w[at + idx] >> off;
            if off > 22 && idx + 1 < k {
                v = v | w[at + idx + 1] << 30 - off;
            }
        }
        out[len(out) - 1 - j] = byte_of(v & 255);
        j = j + 1;
    }
    return 0;
}

// -1, 0 or 1 as the `k` limbs at `x` (plus the limb at `x + k`) compare
// with the modulus.
fn compare_n[&w](w: &w [int], x: int, k: int) -> [] int {
    if w[x + k] != 0 {
        return 1;
    }
    var i = k - 1;
    while i >= 0 {
        let a = w[x + i];
        let b = w[slot_n() + i];
        if a > b {
            return 1;
        }
        if a < b {
            return -1;
        }
        i = i - 1;
    }
    return 0;
}

// x -= n, for x >= n; clears the limb at `x + k`. Branches on nothing
// but `k`: the borrow is the sign of each limb's difference.
fn subtract_n[&w](w: &!w [int], x: int, k: int) -> [] int {
    var under = 0;
    var i = 0;
    while i < k {
        let d = w[x + i] - w[slot_n() + i] - under;
        w[x + i] = d & mask();
        under = d >> 63 & 1;
        i = i + 1;
    }
    w[x + k] = w[x + k] - under;
    return 0;
}

// x = x - n if x >= n, else x unchanged, for x (the `k` limbs at `x`
// and the one at `x + k`) below 2n. Constant time (`docs/ecdh.md` §2):
// the comparison is a borrow computed over every limb, and n is
// subtracted under a mask rather than behind a branch.
fn ct_reduce[&w](w: &!w [int], x: int, k: int) -> [] int {
    var under = 0;
    var i = 0;
    while i < k {
        let d = w[x + i] - w[slot_n() + i] - under;
        under = d >> 63 & 1;
        i = i + 1;
    }
    under = w[x + k] - under >> 63 & 1;
    // -1 (all ones) when x >= n, 0 when x < n, through the barrier so
    // the optimiser cannot make the subtraction below a branch on it
    // (`docs/value-barrier.md`).
    let m = value_barrier(under - 1);
    under = 0;
    i = 0;
    while i < k {
        let d = w[x + i] - (w[slot_n() + i] & m) - under;
        w[x + i] = d & mask();
        under = d >> 63 & 1;
        i = i + 1;
    }
    w[x + k] = w[x + k] - under;
    return 0;
}

fn copy[&w](w: &!w [int], from: int, to: int, k: int) -> [] int {
    var i = 0;
    while i < k + 1 {
        w[to + i] = w[from + i];
        i = i + 1;
    }
    return 0;
}

// dst = x * y * R^-1 mod n, for x, y < n (CIOS, with the reduction fused
// into the multiplication's inner loop). `dst` may be `x` or `y`.
fn mont_mul[&w](w: &!w [int], x: int, y: int, dst: int) -> [] int {
    let k = w[0];
    let ninv = w[1];
    let n = slot_n();
    let t = slot_t();
    var j = 0;
    while j < k + 2 {
        w[t + j] = 0;
        j = j + 1;
    }
    var i = 0;
    while i < k {
        let a = w[x + i];
        var s = w[t] + a * w[y];
        let m = (s & mask()) * ninv & mask();
        s = s + m * w[n];
        var c = s >> 30;
        j = 1;
        while j < k {
            s = w[t + j] + a * w[y + j] + m * w[n + j] + c;
            w[t + j - 1] = s & mask();
            c = s >> 30;
            j = j + 1;
        }
        s = w[t + k] + c;
        w[t + k - 1] = s & mask();
        w[t + k] = s >> 30;
        i = i + 1;
    }
    // Below 2n: one subtraction at most, made or not without a branch.
    ct_reduce(w, t, k);
    copy(w, t, dst, k);
    w[dst + k] = 0;
    return 0;
}

// -n^-1 mod 2^30, by Newton's iteration from n0 (right to 3 bits, as n
// is odd; each step doubles the bits right).
fn neg_inverse(n0: int) -> [] int {
    var inv = n0;
    var i = 0;
    while i < 5 {
        inv = inv * (2 - (n0 * inv & mask())) & mask();
        i = i + 1;
    }
    return (1 << 30) - inv & mask();
}

// R^2 mod n into its slot (`docs/rsa.md` §2.2): 2R mod n by doubling
// from 2^(bits - 1), then that (2 in Montgomery form) to the power 30k.
fn r_squared[&w](w: &!w [int], bits: int) -> [] int {
    let k = w[0];
    let two = slot_two();
    var i = 0;
    while i < k + 2 {
        w[two + i] = 0;
        i = i + 1;
    }
    w[two + (bits - 1) / 30] = 1 << (bits - 1) % 30;
    var d = 30 * k + 1 - (bits - 1);
    while d > 0 {
        var carry = 0;
        i = 0;
        while i < k {
            let v = w[two + i] << 1 | carry;
            w[two + i] = v & mask();
            carry = v >> 30;
            i = i + 1;
        }
        w[two + k] = carry;
        if compare_n(w, two, k) >= 0 {
            subtract_n(w, two, k);
        }
        d = d - 1;
    }
    // (2R)^(30k) in Montgomery form is 2^(30k) R = R^2 mod n.
    let e = 30 * k;
    var top = 0;
    while 1 << top + 1 <= e {
        top = top + 1;
    }
    let r2 = slot_r2();
    copy(w, two, r2, k);
    var b = top - 1;
    while b >= 0 {
        mont_mul(w, r2, r2, r2);
        if e >> b & 1 == 1 {
            mont_mul(w, r2, two, r2);
        }
        b = b - 1;
    }
    return 0;
}

// Checks `n` and makes `w` ready for arithmetic modulo it: the limb
// count, n', n itself and R^2 mod n. 0, or -6, -2 or -1 as `pow_mod`.
pub fn setup[&n, &w](n: &n [byte], w: &!w [int]) -> [] int {
    if len(w) < work_len() {
        return -6;
    }
    let bits = bit_length(n);
    if bits < 2 || bits > max_bits() {
        return -2;
    }
    if int_of(n[len(n) - 1]) & 1 == 0 {
        return -1;
    }
    let k = (bits + 29) / 30;
    w[0] = k;
    load(n, w, slot_n(), k);
    w[1] = neg_inverse(w[slot_n()]);
    r_squared(w, bits);
    return 0;
}

// out = a^e mod n (`docs/rsa.md` §2.3). `out` is exactly as long as `n`.
pub fn pow_mod[&n, &e, &a, &o, &w](n: &n [byte], e: &e [byte], a: &a [byte], out: &!o [byte], work: &!w [int]) -> [] int {
    if len(work) < work_len() {
        return -6;
    }
    if len(out) != len(n) {
        return -5;
    }
    let code = setup(n, work);
    if code != 0 {
        return code;
    }
    if len(e) == 0 {
        return -4;
    }
    let k = work[0];
    if load(a, work, slot_base(), k) != 0 || compare_n(work, slot_base(), k) >= 0 {
        return -3;
    }
    let eb = bit_length(e);
    let acc = slot_acc();
    if eb == 0 {
        // a^0 = 1, which is below n (n >= 3).
        set_small(work, acc, 1);
        store(work, acc, k, out);
        return 0;
    }
    let base = slot_base();
    mont_mul(work, base, slot_r2(), base);
    copy(work, base, acc, k);
    var b = eb - 2;
    while b >= 0 {
        mont_mul(work, acc, acc, acc);
        let byte_at = len(e) - 1 - b / 8;
        if int_of(e[byte_at]) >> b % 8 & 1 == 1 {
            mont_mul(work, acc, base, acc);
        }
        b = b - 1;
    }
    from_mont(work, acc, acc);
    store(work, acc, k, out);
    return 0;
}

// ---- Registers (`docs/ecdsa.md` §2) ----
//
// After `setup`, numbers modulo n live in registers: `reg(i)` is the
// offset of register i in the same `w`, which must then hold
// `registers_len(count)` words. Every value in a register is below n.
// `mul` is Montgomery multiplication, so values are kept in Montgomery
// form (`to_mont`) between `mul`s, and `add`, `sub`, `is_zero` and
// `equal` are the same in either form.

pub fn reg(i: int) -> [] int {
    return slot(6 + i);
}

pub fn registers_len(count: int) -> [] int {
    return slot(6 + count);
}

// Register `r` = big-endian `b`; -3 when `b` is not below n.
pub fn load_reg[&b, &w](b: &b [byte], w: &!w [int], r: int) -> [] int {
    let k = w[0];
    if load(b, w, r, k) != 0 || compare_n(w, r, k) >= 0 {
        return -3;
    }
    return 0;
}

// Register `r` = big-endian `b` mod n, for `b` below 2n (one
// subtraction); -3 when it is not.
pub fn load_reduced[&b, &w](b: &b [byte], w: &!w [int], r: int) -> [] int {
    let k = w[0];
    if load(b, w, r, k) != 0 {
        return -3;
    }
    if compare_n(w, r, k) >= 0 {
        subtract_n(w, r, k);
    }
    if compare_n(w, r, k) >= 0 {
        return -3;
    }
    return 0;
}

// Register `r` as big-endian `out`, which must be wide enough.
pub fn store_reg[&w, &o](w: &w [int], r: int, out: &!o [byte]) -> [] int {
    return store(w, r, w[0], out);
}

// Register `r` = v, for 0 <= v < 2^30 and v < n.
pub fn set_small[&w](w: &!w [int], r: int, v: int) -> [] int {
    var i = 0;
    while i < w[0] + 2 {
        w[r + i] = 0;
        i = i + 1;
    }
    w[r] = v;
    return 0;
}

pub fn copy_reg[&w](w: &!w [int], from: int, to: int) -> [] int {
    return copy(w, from, to, w[0]);
}

pub fn mul[&w](w: &!w [int], a: int, b: int, dst: int) -> [] int {
    return mont_mul(w, a, b, dst);
}

pub fn to_mont[&w](w: &!w [int], a: int, dst: int) -> [] int {
    return mont_mul(w, a, slot_r2(), dst);
}

// Out of Montgomery form: multiply by 1.
pub fn from_mont[&w](w: &!w [int], a: int, dst: int) -> [] int {
    let one = slot_two();
    set_small(w, one, 1);
    return mont_mul(w, a, one, dst);
}

// dst = a + b mod n.
pub fn add[&w](w: &!w [int], a: int, b: int, dst: int) -> [] int {
    let k = w[0];
    var carry = 0;
    var i = 0;
    while i < k {
        let v = w[a + i] + w[b + i] + carry;
        w[dst + i] = v & mask();
        carry = v >> 30;
        i = i + 1;
    }
    w[dst + k] = carry;
    ct_reduce(w, dst, k);
    return 0;
}

// dst = a - b mod n. Constant time: n is added back under a mask made
// from the final borrow.
pub fn sub[&w](w: &!w [int], a: int, b: int, dst: int) -> [] int {
    let k = w[0];
    var under = 0;
    var i = 0;
    while i < k {
        let v = w[a + i] - w[b + i] - under;
        w[dst + i] = v & mask();
        under = v >> 63 & 1;
        i = i + 1;
    }
    w[dst + k] = 0;
    // Below zero: add n back; the carry out of the top limb is the
    // borrow it cancels.
    let m = value_barrier(0 - under);
    var carry = 0;
    i = 0;
    while i < k {
        let v = w[dst + i] + (w[slot_n() + i] & m) + carry;
        w[dst + i] = v & mask();
        carry = v >> 30;
        i = i + 1;
    }
    return 0;
}

pub fn is_zero[&w](w: &w [int], a: int) -> [] bool {
    var i = 0;
    while i < w[0] {
        if w[a + i] != 0 {
            return false;
        }
        i = i + 1;
    }
    return true;
}

pub fn equal[&w](w: &w [int], a: int, b: int) -> [] bool {
    var i = 0;
    while i < w[0] {
        if w[a + i] != w[b + i] {
            return false;
        }
        i = i + 1;
    }
    return true;
}

// Bit `i` of register `a` (as stored: in Montgomery form if it is).
pub fn bit[&w](w: &w [int], a: int, i: int) -> [] int {
    return w[a + i / 30] >> i % 30 & 1;
}

// The number of bits in n.
pub fn modulus_bits[&w](w: &w [int]) -> [] int {
    var i = w[0] - 1;
    var top = w[slot_n() + i];
    var bits = 0;
    while top > 0 {
        bits = bits + 1;
        top = top >> 1;
    }
    return i * 30 + bits;
}

// dst = a^-1 mod n, by Fermat: a^(n-2). Only for a prime n and a != 0;
// `a` and `dst` in Montgomery form, and `dst` may be `a`.
pub fn inverse[&w](w: &!w [int], a: int, dst: int) -> [] int {
    let k = w[0];
    let base = slot_two();
    let e = slot_acc();
    copy(w, a, base, k);
    // e = n - 2 (n is odd and at least 3, so no borrow leaves the top).
    copy(w, slot_n(), e, k);
    var i = 0;
    var take = 2;
    while take > 0 {
        let v = w[e + i] - take;
        if v < 0 {
            w[e + i] = v + (1 << 30);
            take = 1;
        } else {
            w[e + i] = v;
            take = 0;
        }
        i = i + 1;
    }
    // dst = 1 in Montgomery form, then square-and-multiply.
    set_small(w, dst, 1);
    to_mont(w, dst, dst);
    var b = modulus_bits(w) - 1;
    while b >= 0 {
        mont_mul(w, dst, dst, dst);
        if bit(w, e, b) == 1 {
            mont_mul(w, dst, base, dst);
        }
        b = b - 1;
    }
    return 0;
}
