# `sqrt`, and the capability question it was waiting on

> **Status: settled and built, and the measurement is the argument.**
>
> `floating-point.md` §7 has carried this row since `float` landed:
>
> > *`std.math` over floats — `sqrt`, `sin`, `exp`. Each is either a libc
> > call, gated by `Ffi`, which would make arithmetic need a capability,
> > or an implementation with its own error analysis. **The capability
> > question has to be settled first.***
>
> The capability question turns out to be **the wrong question for
> `sqrt`**, and answering the right one took measuring what the programs
> that hand-rolled it actually compute. One of them is wrong by 143
> orders of magnitude.

---

## 1. Two programs asked, by writing it themselves

`line-reading.md` §1 set the rule and then enforced it against a library
function that had one asker. This one has two, counted the same way — by
reading them:

| | what it hand-rolls | why |
|---|---|---|
| `examples/newton.cho` | a square root by Newton's method, five steps | the program *is* the demonstration |
| `benches/game/spectral.cho` | `sqrt_of`, twenty steps | spectral-norm needs one, and the source says so |

`spectral.cho` names the blocker in its own header: *"`sqrt` is written
here because `floating-point.md` §7 leaves `std.math` over floats open —
the capability question comes first."*

Two askers, two *different* algorithms, and neither is right.

---

## 2. Both hand-rolled roots are wrong, and one is spectacularly wrong

`sqrt_of` against a correctly-rounded square root, over 40,008 values —
the specials, twenty thousand uniform in 10⁻⁶…10⁶, and twenty thousand
spread across the exponent range:

| | |
|---|---|
| not correctly rounded | **23,362 — 58.4%** |
| worst error | **2.15 × 10¹⁸ ulp** |

That last number is not a rounding error. It is a different answer:

| x | `sqrt_of(x)` | correct |
|---|---|---|
| 10¹⁰⁰ | 4.768372 × 10⁹³ | 10⁵⁰ |
| 10²⁰⁰ | 4.768372 × 10¹⁹³ | 10¹⁰⁰ |
| 10³⁰⁰ | 4.768372 × 10²⁹³ | **10¹⁵⁰** |

The first guess is `x / 2`, and a Newton step roughly halves the distance
to the root, so twenty steps move 5 × 10²⁹⁹ down by a factor of 2²⁰ and
stop — 143 orders of magnitude short. The function does not converge and
does not say so.

Its comment reads: *"Twenty is past the point where binary64 stops
changing."* That is true of the inputs spectral-norm feeds it, which sit
near 1.27, and false of the function, which takes any `float` and
returns without complaint.

**This is `cut`'s long line again** (`line-reading.md` §2): a shipped
program, checked against a published expected output, correct on every
input anyone tried, and wrong on the dimension the test never varied.
The benchmark's answer is right — `1.274219991` — because it never asks
for a root outside a narrow band. The *function* is wrong.

`newton.cho` is better and still not correct: five steps from a fixed
guess of 2.0 gives √2 to **1 ulp**. That is fine for what it
demonstrates — it prints residuals and is about convergence — and it is
not a square root anyone should call.

---

## 3. So the capability question was the wrong question

§7's row assumed two options: a libc call gated by `Ffi`, or an
implementation with its own error analysis. **Neither is what `sqrt`
is.**

`sqrt` is **one instruction**: `sqrtsd` on x86-64, `fsqrt` on
aarch64. Cranelift emits it directly. So:

* It reaches **no library**, so there is no `Ffi` to gate, so
  arithmetic does not acquire a capability. The fear in §7's row was
  real and does not apply here.
* Its row is `[]` and `cancho authority` reports nothing, which is
  correct: a square root observes nothing outside the program.

And the error analysis option is closed by §2 rather than by taste:
**IEEE-754 requires `sqrt` to be correctly rounded**, the instruction
is, and no sequence of `+`, `-`, `*` and `/` in cancho reliably is —
which the 58.4% measures.

### 3.1 Which is why this one is a builtin and printing is not

`float-printing.md` made the opposite call for the same-shaped question,
and both calls are right for the same reason: **put it where it can be
correct.**

Printing a float is a *decision procedure* — Steele and White over exact
integers — and cancho can express it, so `std.fmt.float_into` is
library code and the compiler's whole contribution is `bits_of`, a
bitcast (plus a select that gives every NaN one pattern,
[`differential.md`](differential.md) §4). A correctly-rounded square root is *not* expressible here,
because the only correct implementation is an instruction. One goes in
the library because it can; the other goes in the compiler because it
cannot.

---

## 4. What is not in this slice

**Not `sin`, `exp`, `log` or `pow`.** Those are the half of §7's row
that is genuinely about error analysis: none is a single instruction,
each is a routine with an argument reduction and a polynomial, and each
would be library code with a stated accuracy. **No program here has
asked for one**, and §1's rule is the rule.

> **Corrected (§7 below).** `exp`, `log` and `pow` are now built, once
> three programs asked for them by name. `sin` stays open — nothing has
> asked, and it needs its own range reduction, not the one the other
> three share.

**Not float `abs`, `min` or `max`.** One asker between them —
`newton.cho`'s `magnitude` — and it has an alternative that works:
`if x < 0.0 { return -x; }` is three lines and correct.

That is the same test `line-reading.md` §4 used to *admit*
`buffer.clear` with one asker: the question is not how many programs
want it but whether the ones that want it have a working alternative.
`clear` had none — a buffer could not be reused at all. `magnitude` has
one, so it stays in the program that needs it.

**Not a `std.math` module for floats at all.** `sqrt` is a builtin, so
there is nothing to import and nothing to put in one. `std.math` stays
integer-only until something earns a place beside it.

> **Corrected (§7).** `exp`, `log` and `pow` earned that place, and
> `std.math` is where they went — the same module `sqrt` itself never
> needed to join.

---

## 5. What it fixed

| | before | after |
|---|---|---|
| `benches/game/spectral.cho` | its own 20-step Newton, 58.4% not correctly rounded | `sqrt`, and the benchmark still answers `1.274219991` |
| `examples/newton.cho` | keeps its Newton loop — **on purpose**, it is the demonstration | prints `sqrt`'s answer beside its own, so the program now shows what five steps are worth |

`sqrt_agrees_with_the_hardware` checks the builtin against Rust's own
`f64::sqrt` over the same 40,008 values, including the four exponents
where `sqrt_of` was off by 10⁴³ and more.

---

## 6. Open

| Question | Why it waits |
|---|---|
| ~~`exp`, `log`, `pow`~~ | **Built** — §7 |
| ~~`sin` (and `cos`)~~ | **Built** — §8, with its own reduction by quarter turns, to a stated domain of `|x| ≤ 10⁶` |
| ~~Float `abs`, `min`, `max`~~ | **Built** as `fabs`/`fmin`/`fmax` — §8 says why the §1 rule was set aside |
| A total order | `floating-point.md` §7's other row, untouched here |
| `sqrt` of a negative | Answers NaN, which is what the instruction does and what IEEE-754 says. Not a trap: `floating-point.md` §2.1 already settled that NaN announces the absence of a value rather than lying about one, and a square root of −1 is exactly that case |

---

## 7. `exp`, `log` and `pow`, closed the way §6 said they would be

> **Superseded by §9.** The algorithms and the accuracy figures below
> describe the first version — a Taylor series for `exp`, an `atanh`
> series for `log`, `exp(y * log(x))` for `pow` — which was off by up to a
> hundred ulp. They are kept because §7.1's bug and the reasoning behind
> the guards are still true; the *numbers* are not, and §9 has the
> measured ones. `exp`, `log` and `pow` are now fdlibm's algorithms and a
> double-double product, within 1, 1 and 4 ulp of the C library.

Three askers, the two-per-half bar §1 already used, each wanting more
than one of the three: `examples/growth.cho` (continuous and discrete
compound growth, plus a doubling time — `exp`, `pow` and `log` in one
program), `examples/decay.cho` (a half-life table, computed the
differential-equation way and the definitional way side by side — `exp`
and `pow` again, cross-checked against each other), `examples/entropy.cho`
(Shannon entropy of standard input's byte distribution — `log`, the
third caller). Between them: `log` three askers, `exp` and `pow` two
each.

**Library code with a stated accuracy**, exactly as §4 said it would be,
in `std/math.cho`:

* `exp(x)`: range reduction to `x = k*ln2 + r` with `|r| <= ln2/2` (`ln2`
  split into a high part and a low residual, the standard technique, so
  `k*ln2_hi` loses no precision for the `k` this produces), a 14-term
  Taylor series for `e^r`, and `pow2(k)` — 2^k by exponentiation by
  squaring on ordinary multiplication — to rescale.
* `log(x)`: pull the unbiased binary exponent `e` out of `x`'s own bits
  with `bits_of` so `x / pow2(e)` is a mantissa `m` in `[1, 2)`, then a
  14-term series in `y = (m-1)/(m+1)`.
* `pow(x, y)`: `exp(y * log(x))` for `x > 0`, which is most of what a
  caller wants it for; `x <= 0` gets its own cases, matching the two
  conventions C's `pow` already settled rather than reinventing them.

None of the three needs `bits_of`'s missing other half — a builtin that
builds a `float` back up from bits, which does not exist. Scaling by an
integer power of two is exact under ordinary multiplication as long as
it does not overflow, so `pow2` gets there by squaring rather than by
bit construction. The one place that bit missing, if it existed, would
have simplified something: `exp`'s own scaling still had to split its
exponent in half before multiplying (below), where a direct `ldexp`
would not have.

**Measured, not asserted**: `crates/cancho/tests/conformance/floats.rs`
checks all three against Rust's own `f64::exp`/`f64::ln`/`f64::powf`
over roughly 4,500 generated values, plus specials, within **1e-9
relative error** — two orders of magnitude looser than what was actually
measured while writing this (2.4e-14 worst case for `exp`, 6e-14 for
`log`, away from where relative error stops meaning anything, the same
caveat §2 already states for `sqrt`'s own tails). **Not correctly
rounded, and not claimed to be** — §2 already found that unreachable for
a hand-rolled `sqrt`, and nothing about `exp`/`log`/`pow` makes it more
reachable.

### 7.1 The bug the differential test found

The first version of `exp` answered **infinity for `exp(709.5)`**, which
is finite (≈1.3549863 × 10³⁰⁸, comfortably under `f64::MAX`). The cause
was `pow2(k)` computed as one call: at `x = 709.5`, `k = 1024`, and
`2^1024` alone overflows a `float` even though `sum * 2^1024` (`sum`
always sitting in `[0.5, 2)`) would not have. Splitting the exponent —
`sum * pow2(k - k/2) * pow2(k/2)` — keeps every intermediate value in
range up to the true overflow point (≈709.7827) and reaches infinity
correctly exactly there, through ordinary IEEE overflow rather than a
guard. `log`'s own `pow2(e)` call never hits this: a normal `float`'s
unbiased exponent never reaches 1024, only an infinite input's raw bits
do, and that is caught earlier by an explicit check.

The other two guards each answer a different hazard than the arithmetic
does: `is_nan(x)` in both `exp` and `log`, because `truncate` — which
both use internally, to round to the nearest integer `k` or to check
whether `y` is a whole number in `pow` — traps on NaN
(`docs/floating-point.md` §4), and a library function should not trap on
an input its own domain does not exclude. `exp`'s `|x| > 750` guard and
`log`'s `x > f64::MAX` guard exist for the same reason, one step further
out: an infinite `x` would also send `x / ln2` or `x / pow2(e)` somewhere
`truncate` traps on.

---

## 8. `fabs`, `fmin`, `fmax`, `floor`, `ceil`, `round`, `sin`, `cos`

**Not asked for by a program. Added anyway, and the reason is the
change.** §1's rule — a function is earned when programs write it for
themselves — was the right rule while the question was what a handful of
example programs needed. The question this repository is answering now
is what an agent writing a program here would reach for and not find,
and rounding to a whole number is the first thing on that list: `floor`
and `ceil` are what every "how many pages" and "which bucket" sum is
written with, and the only way to get one before this was
`float_of(truncate(x))`, which is wrong for every negative non-integer
(it rounds toward zero) and traps near 2⁶³. §4 declined `abs`/`min`/`max`
because `if x < 0.0 { return -x; }` is three lines; that stays true, and
it is also three lines every caller gets subtly wrong at `-0.0` and NaN,
which is what a library function is for.

All eight are library code in `std/math.cho`, not builtins — a few lines
of `float` and `truncate` arithmetic each, like `exp`/`log`/`pow`.

* **`fabs`**: C's. `fabs(-0.0)` is `+0.0`, a NaN stays a NaN.
* **`fmin`, `fmax`**: C's. A NaN is missing data — the other argument is
  the answer, and the answer is NaN only if both are.
* **`floor`, `ceil`, `round`**: exact, not approximate. For `|x| < 2⁵²`
  the distance `x - truncate(x)` is computed without rounding, so the
  three compare it against zero or one half directly. Every float at or
  past 2⁵² is already whole and is returned as it is, which keeps
  `truncate` away from its own trap near 2⁶³; NaN and the infinities come
  back unchanged. `round` is ties-away-from-zero, as C's; the one-line
  `floor(x + 0.5)` is wrong at `0.49999999999999994`, where the addition
  itself rounds up to `1.0`, and `tests/accept/math_floats.cho` pins that
  case. One difference from C: a zero result is `+0.0` where C's
  `floor(-0.0)` keeps the minus sign, which only `bits_of` can see.
* **`sin`, `cos`**: fdlibm's two kernel polynomials on `[-π/4, π/4]`
  (minimax coefficients, chosen to minimise the worst error over the
  interval, not a Taylor series's), and an argument reduction by
  quarter turns, with `π/2` subtracted in three 33-bit pieces so that
  `k × piece` is exact for `|k| < 2²⁰`. ~~That bound is the **domain**: `|x| ≤ 10⁶`; outside it, **it traps**.~~ **Corrected, §10.2:** that was the first version. Past `|x| = 10⁶` it now uses Payne–Hanek reduction and answers for every finite `x`; only an infinity, which has no sine, gives NaN. A NaN argument is answered with that NaN.

**Measured against the C library**, not asserted. A foreign signature
cannot carry a `float` (`opaque-pointers.md`), so libm cannot be called
from a cancho program; the comparison is made from outside.
`tests/programs/math_samples.cho` prints the bit pattern of a function's
answers at a fixed sample, `conformance/mathfn.rs` replays the same
arguments (a fixed linear-congruential sequence, every step exact or
correctly rounded, so Rust and cancho compute the same argument bit for
bit) and measures the distance from glibc's answer in units in the last
place. The first measurement used 20,000 arguments per range; it is now
the same harness as §9's, 10,000 per range, and the worst error is **1
ulp on `[-10³, 10³]` and below and on both neighbourhoods of a multiple
of π/2, and 2 ulp on `[-10⁶, 10⁶]`** — `sin` and `cos` alike, between 8%
and 28% of answers differing from libm's at all (and none for `cos` near
a zero of `cos`, where the answer is exactly the reduced argument).

**Not correctly rounded, and not claimed to be** — up to one answer in
four is one float away from the library's, which is the usual
state of a small hand-written `sin` and is what a test bound of "within
a few ulp" is for. The test allows one ulp more than measured, because
the reference is itself a library and another platform's rounds
differently from glibc's in the last place.

### 8.1 What this does not do

* ~~No `tan`, `atan`, `atan2`, `asin`, `acos`.~~ **Built — §10.**
* ~~No reduction past `10⁶`.~~ **Built — §10.2.**
* **No `π` constant.** `3.141592653589793` is what a caller writes.

---

## 9. `exp`, `log` and `pow` fixed; `expm1`, `log1p`, `log2`, `log10` and the hyperbolics added

§7 shipped `exp`, `log` and `pow` with an accuracy stated honestly as
"2.4e-14 relative" and "6e-14". That is a hundred ulp, and the first
thing measured against the C library in the unit that matters (ulps, not
a relative tolerance two orders of magnitude loose, which is what
`floats.rs` checks) was worse than the figure suggested for `pow`: **60
ulp at `5e5 ^ 3.7`, and 275 at `1.7 ^ -552`**. A program that computes
`pow(x, 2.0)` and compares it to `x * x` would not have noticed; one that
compares `pow(7.0, 2.0)` to `49.0` would, because it was not 49.

### 9.1 What changed

* **`exp`** is fdlibm's: reduction by `ln2` in two pieces, a degree-4
  minimax polynomial in a rational form, scaling by `2^k`. **1 ulp.**
* **`log`** is fdlibm's: the exponent out of the float's own bits, a
  mantissa in `[√2/2, √2]`, a degree-14 minimax polynomial in `s²`
  with `s = f/(2+f)`. **1 ulp**, including near `x == 1`, where the
  series it replaces had an *absolute* floor of 1e-12, and for
  **subnormal** `x`, which the old one got wrong: `log(5e-324)` is
  `-744.4400719213812`, to the bit. (A subnormal is scaled by `2^54`
  first, which is exact.)
* **`pow`** carries `y * log(x)` as a pair of floats. A relative error
  `d` in the exponent is a relative error `d` in the result, and the
  exponent reaches 700, so rounding it to one float was the whole
  error. `log(x)` is kept as `hi + lo` (`two_sum` keeps what `k * ln2`
  loses) and the product through `two_product`, Dekker's, which needs no
  fused multiply-add. What remains is `log`'s own error times `y` — about
  `y/3` ulp, small for the exponents programs use. Four cases are
  answered exactly rather than approximately: `y == 1` (`x`), `y == 2`
  (`x * x`), `y == -1` (`1 / x`) and `y == 0.5` (`sqrt`); and **a whole
  exponent whose power is representable is computed by repeated
  squaring** with every multiplication checked for exactness, so
  `pow(7.0, 2.0) == 49.0`, `pow(10.0, 15.0) == 1e15` and
  `pow(2.0, 100.0)` are exact, as C's are. Infinite exponents and bases
  and the overflow and underflow edges are settled before the pair is
  formed (`pow(1.0, inf)` is 1, as C says; it was NaN).
* **New: `expm1`, `log1p`** (Kahan's identities over the new `exp`/`log`:
  `u = exp(x)`, `(u - 1) * x / log(u)`, in which `u`'s rounding error
  cancels), **`log2`** and **`log10`** (`k + log(m)/ln2`, so
  `log2` of a power of two is exact, which `log(x)/ln2` is not;
  `log10` with `log10(2)` in two pieces), and **`sinh`, `cosh`, `tanh`,
  `asinh`, `acosh`, `atanh`** — fdlibm's, built on `expm1`/`log1p`
  because `(e^x - e^-x)/2` and `log(x + √(x²-1))` lose the whole answer
  to cancellation for small arguments.

### 9.2 Measured

`conformance/mathfn.rs` replays a fixed sample (a linear-congruential
sequence, exact at every step, so Rust and cancho agree on every
argument bit for bit — `tests/programs/math_samples.cho` prints the
answers' bit patterns, one process per function and range) and measures
the distance from glibc's answer in ulps. 10,000 arguments per range,
three to six ranges per function, chosen to include the places each is
hard: near zero, near 1, the overflow and underflow edges, and
`5e299`.

| function | ranges | worst error | answers that differ from libm at all |
|---|---|---|---|
| `exp` | ±1, ±10, ±700, ±1e-3, ±1e-9, [-745, -695] | **1 ulp** | 0 – 9.7% |
| `log` | [0.5, 1.5], [1, 100], [0, 1e6], 1 ± 1e-3, (0, 1e-3), up to 1e300 | **1 ulp** | 0 – 6.4% |
| `expm1` | ±1, ±10, ±30, ±1e-3, ±1e-9 | **2 ulp** | 19 – 31% |
| `log1p` | ±0.999, (0, 1e-3), ±1e-9, [1, 100], [0, 1e6] | **2 ulp** | 8 – 38% |
| `log2` | as `log` | **1 ulp** | 0 – 28% |
| `log10` | as `log` | **2 ulp** | 0 – 16% |
| `sinh` | ±1, ±10, ±30, ±1e-3, ±1e-9, [0, 710] | **3 ulp** | 0 – 38% |
| `cosh` | ±1, ±10, ±30, ±1e-3, [0, 710] | **2 ulp** | 0 – 9.5% |
| `tanh` | ±1, ±10, ±30, ±1e-3, ±1e-9 | **3 ulp** | 0.8 – 25% |
| `asinh` | ±1, ±10, ±1e6, ±1e-3, ±1e-9, up to 1e300 | **2 ulp** | 0 – 37% |
| `acosh` | [1, 3], [1, 100], [0, 1e6], up to 1e300 | **2 ulp** | 0 – 14% |
| `atanh` | ±0.999, ±1e-3, ±1e-9, ±1 | **2 ulp** | 0 – 38% |
| `pow(x, 3.7)` | [0.5, 1.5], [1, 100], [0, 1e6] | **2 ulp** (was 60) | 31 – 40% |
| `pow(x, 0.3)` | same | **2 ulp** | 4.5 – 38% |
| `pow(1.7, x)` | ±1, ±700, ±1e-3 | **4 ulp** (was 275) | 0 – 78% |
| `pow(x, 2)` | same as `pow(x, 3.7)` | **1 ulp** | 0.1 – 0.6% |
| `pow(x, 7)` | [0.5, 1.5], [1, 100] | **3 ulp** | 43 – 49% |
| `pow(x, -3)` | same as `pow(x, 3.7)` | **2 ulp** | 28 – 39% |

**Not correctly rounded, and not claimed to be.** `expm1`/`log1p` are
Kahan's identities, not fdlibm's long rational approximations, which is
why they sit at 2 ulp where `exp`/`log` sit at 1; the hyperbolics inherit
that through `expm1`. The test bounds are the measured worst plus one,
because the reference is itself a library: another platform's rounds
differently in the last place. `pow(1.7, x)` at `x = ±700` is the case
the `y/3`-ulp rule is about, and 4 ulp is what it costs.

`tests/accept/math_floats.cho` holds the claims that are not about ulps:
exact powers, `log2` of a power of two, `log10` of a power of ten,
specials (NaN, ±0, ±inf, the overflow and underflow edges) for every
function, and the inverse pairs.

### 9.3 What this still does not do

* **No `tan`, `atan`, `atan2`, `asin`, `acos`.** `atan2` is the one most
  likely to be asked for next; each is its own reduction and its own
  table.
* **`pow` is not fdlibm's.** fdlibm computes `log2` to about 68 bits
  internally, which gives < 1 ulp for every `y`; this gets there for
  `|y|` up to a few dozen and degrades as `y/3` ulp beyond. Closing it
  is a longer `log_pair`, not a new design.
* **A subnormal result is rounded twice** (once in `exp`, once by the
  scaling), so the last place can be a ulp off in that range. The
  `[-745, -695]` row above is it: still 1 ulp, but it is not the whole
  range's guarantee.
* **`floats.rs`'s old sweep still checks `exp`, `log` and `pow` to 1e-9
  relative**, which is what §7 claimed. It is a floor under the ulp
  table, not the claim: that is `mathfn.rs`.

---

## 10. The rest of the trigonometric family, and a reduction that has no limit

§8 shipped `sin` and `cos` with two stated gaps: no `tan`/`atan`/
`atan2`/`asin`/`acos`, and a trap for `|x| > 10⁶`. A trap is the honest
answer to "there is a right answer and this does not compute it", and it
is still a hole: `sin(1e22)` is a perfectly good question with a
well-known answer (`-0.8522008497671888`), and an agent that computes a
phase from a timestamp times a frequency will reach it.

### 10.1 The functions

* **`atan`, `asin`, `acos`**: fdlibm's — a minimax rational/odd
  polynomial, and the half-angle identities that keep `1 - x` from
  cancelling near 1. `asin` and `acos` take `|x| ≤ 1` and answer NaN
  beyond; `atan` takes any float, and is `±π/2` to the last bit from
  `2⁶⁶` up. fdlibm clears the low 32 bits of a float to get a "high
  half" of a square root; there is no way to write one here, so this
  clears the low 27 by Veltkamp's split (`pow` already needed it), which
  does the same job.
* **`atan2(y, x)`**, argument order as in C: every case C settles is
  settled the same way — the sign of a zero decides the half-plane
  (`atan2(+0, -1) = π`, `atan2(-0, -1) = -π`; told apart by `bits_of`,
  because `x < 0.0` cannot), an infinity gives one of eight multiples of
  π/4, and otherwise `atan(|y/x|)` is moved to the quadrant the two signs
  name, with `π` in two pieces so the subtraction does not round. A ratio
  past `2⁶⁰` is `π/2` outright. (The first version fell through to
  `π - (π/2 - tiny)` there and was a float off for every `x < 0`, which
  `5e299`'s sweep showed as 10,000 of 10,000 differing.)
* **`tan`**: `sin(r)/cos(r)` on the same reduced argument, or
  `-cos(r)/sin(r)` in an odd quarter turn. A quotient of two answers
  each good to a fraction of an ulp is good to about two; fdlibm's own
  `__kernel_tan` gets one by a longer path, which is a separate piece of
  work and is listed in §10.4.

### 10.2 Payne–Hanek, and what it took

`remainder_of` subtracts `k·π/2` in three 33-bit pieces, exact while
`k < 2²⁰`. Past that, the product `x · 2/π` has to be taken with enough
bits of `2/π` that its fractional part survives: for the worst-case
`double` (`6381956970095103 × 2⁷⁹⁷`) the fraction starts 62 zero bits
down, so the reduction needs 115+ bits *after* them.

`reduce_large` does it in base 2²⁴ so that every partial product fits an
`int`: `x = mant × 2^e` is four 24-bit limbs, `2/π` is a **52-limb
`static` table, 1,248 bits, generated here with integer arithmetic**
(Machin's formula; its first nine limbs match fdlibm's `ipio2`), and the
limb of the product at each weight is a four-term sum plus a carry.
Limbs of weight `2²⁴` and up are multiples of 4 and are never computed;
limbs below `2⁻¹⁹²` are dropped. Limb 0's low two bits are the
quadrant; limbs `-1..-8` are the fraction. If the fraction is a half or
more, the quadrant is rounded up and the remainder is the **negative
complement, formed from the limbs by subtraction with a borrow** — not
`1 - f` in floating point, which would throw away exactly the bits the
rest of this was done to keep. The limbs become a float by Horner's rule
from the low limb up, so a fraction with leading zero limbs still gets
all 53 bits of those below.

It answers for every finite `x`. An infinity has no sine; it is NaN, as
`sqrt(-1)` is, which is the trap's replacement: a trap was for "right
answer exists, not computed", and now it is computed.

### 10.3 Measured

Same harness as §9 (`conformance/mathfn.rs`, 10,000 arguments per range,
ulps against glibc's own functions). The sweeps cover the old domain, the
new one (`[0, 10⁹]`, `±10¹⁵`, `[0, 10³⁰⁰]`), and the neighbourhoods of
multiples of π/2 where reduction cancels the most.

| function | ranges | worst error | answers that differ from libm at all |
|---|---|---|---|
| `sin` | ±1 … ±10⁶, near 100·π/2 and 10⁵·π/2, **[0, 10⁹], ±10¹⁵, [0, 10³⁰⁰]** | **2 ulp** (1 below 10⁶) | 8 – 28% |
| `cos` | same | **2 ulp** (1 below 10⁶) | 0 – 24% |
| `tan` | same, and π/2 ± 10⁻³ | **4 ulp** (2 below 10⁶) | 36 – 52% |
| `asin` | ±0.999, ±1, ±10⁻⁹, [0.981, 0.999] | **1 ulp** | 0 – 6% |
| `acos` | same | **1 ulp** | 0 – 8% |
| `atan` | ±1, ±10, ±10⁶, ±10⁻⁹, up to 10³⁰⁰ | **1 ulp** | 0 – 6% |
| `atan2(x, 1.7)` | ±1, ±10, ±10⁶, up to 10³⁰⁰ | **1 ulp** | 0 – 24% |
| `atan2(1.7, x)` | ±1, ±10, ±10⁻³, ±10⁻⁹ | **1 ulp** | 16 – 24% |
| `atan2(x, -0.9)` | ±1, ±10, ±10⁶, up to 10³⁰⁰ | **1 ulp** | 0 – 45% |

**The worst case.** `x = 6381956970095103 × 2⁷⁹⁷` is the double whose
`cos` is closest to zero (`-4.687165924254627611…e-19`, 62 bits down). It
is pinned in `tests/accept/math_floats.cho` against that literature value,
to 3e-16 relative. The C library on the machine this was measured on (through
Python's `math.cos`) answers `-4.68716592425462e-19` for it — **8 ulp off the true value, which
this reduction gets to the digit**. It is in no sweep because a random
sample will not find it; it is exactly why the reduction keeps 115 bits.

### 10.4 What this still does not do

* **`tan` is 2 – 4 ulp, not 1.** fdlibm's `__kernel_tan` (a degree-13
  polynomial with a reciprocal correction for `|x| > 0.67`) is the one
  to port if a program needs better.
* **`sin`/`cos` are 2 ulp past `10⁶`**, 1 below it: the reduced argument
  is one float, so its own rounding is carried into the result.
  Carrying the tail through the kernels (fdlibm's `__kernel_sin(x, y)`)
  would cost the 2 back.
* **No `sincos`, `sinpi`/`cospi`, `atanpi`, `hypot`, `cbrt`.** Not asked
  for, and the three `*pi` forms are the better answer to "an angle in
  turns" than a large argument to `sin`.
