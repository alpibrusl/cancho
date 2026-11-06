# Printing a float

> **Status: settled and built.**
>
> `floating-point.md` §7 listed printing as *"the one that makes `float`
> awkward rather than incomplete"*, and that was the right word. A
> language with `float` and no way to print one reports numbers through
> `truncate` and a scale factor, which is fixed point with extra steps —
> `examples/newton.cho` did exactly that, and said so.
>
> This document is that row closed. The answer is `std.fmt.float_into`,
> and the thing worth reading it for is **where** the answer lives:
> not in the compiler.

---

## 1. What "printing a float" means

There are three things a printer could aim at, and only one of them is
a contract:

| Aim | Example for `0.1` | Verdict |
|---|---|---|
| The exact value | `0.1000000000000000055511151231257827` | True, and useless. Nobody typed that |
| Some fixed number of digits | `0.100000` | Loses values. `1e300` and `1e-300` both print as `0.000000` |
| The **shortest decimal that reads back to the same bits** | `1e-1` | The only one that is a round trip |

The third is the contract, and it is the one this implements: for every
`float` `x`, reading back what `float_into` wrote gives `x` again, and no
shorter decimal would have.

That property is worth stating precisely, because it is stronger than
"accurate". `0.1000000000000000055511151231257827` is more accurate and
still wrong for this purpose: the value it names is the same `float`, so
the extra digits carry no information and cost thirty characters.

### 1.1 The form

```
d[.ddd]e[-]k
```

One digit, optionally a point and the rest, then `e` and a decimal
exponent. `0.1` is `1e-1`, `1.5` is `1.5e0`, `100.0` is `1e2`, zero is
`0e0`.

This is not the prettiest choice and it is deliberate. Positional
notation — printing `0.1` as `0.1` and `1e21` as `1000000000000000000000`
— needs a *second* set of decisions (at which exponent does it switch?
how many leading zeros before it gives up?) which have nothing to do with
the digits and which every language answers differently. Those decisions
are a formatting layer's business. The digits are the hard part, and this
is the layer that gets them right.

`inf`, `-inf` and `NaN` are spelled rather than refused, because
`floating-point.md` §2 made them values. `-0.0` prints as `-0e0`: the
sign bit is part of the value, and losing it on the way out would make
printing the one place the language forgot.

---

## 2. Where it lives, which is the point

`float_into` is **written in cancho**, in `std/fmt.cho`. It is not a
builtin, not a call into libc, and not a special case in the code
generator. The compiler's entire contribution is one builtin:

```
bits_of(x: float) -> int          // the same 64 bits, read as an integer
```

A bitcast, plus two instructions that give every NaN one pattern
(`movq`, `ucomisd`, `cmovp` on x86-64; [`differential.md`](differential.md)
§4, #76). From there the algorithm is ordinary cancho: masks and
shifts to pull the mantissa and exponent apart (`bitwise.md`), a
`region` to hold the working numbers, `while` loops, and a `[byte]` to
write into. Its effect row is `[]` — printing a float performs nothing,
because writing the *bytes somewhere* is the caller's effect, not this
function's.

This matters more than it looks. The usual arrangement is that a
language's float printing is a thousand lines of C or Rust inside the
runtime, reachable only through whatever interface it chose to expose.
Here the worst piece of numerical formatting there is turned out to be
*library code in the language itself*, and the thing that made it
possible is a builtin small enough to describe in six words.

It is also the honest test of `bits_of`. A primitive that only the
compiler's own code could use would not have earned its place;
`float_into` is the program that proves a user could have written this.

---

## 3. The algorithm

Steele and White's, which Dragon4 is a refinement of and Grisu and Ryū
are the fast approximations to.

Keep the value as an exact ratio of integers `R / S`, and keep alongside
it the distance to the midpoint of each neighbouring float, `M+` and
`M-`. Scale until the value sits in `[0.1, 1)`. Then, repeatedly:
multiply `R` by ten, take the digit, and ask whether what remains is
already closer to this float than to either neighbour. When it is, stop —
the digits emitted so far are the shortest that round-trip.

The stopping rule is the whole algorithm. It is why the output is
shortest rather than merely correct, and it is what Grisu2 cannot
guarantee and falls back to Dragon4 for.

The integers involved reach **1080 bits** (§4), so `std.bignum` is
underneath — also written in cancho, also in this repository, and
ninety lines rather than four hundred for the reason in §3 below.

### 3.1 The factors of two, and why the setup has four cases

A `float` is `m * 2^e` exactly. To make `R`, `S`, `M+` and `M-` all
integers, everything is scaled by a power of two:

| | `R` | `S` | `M-` | `M+` |
|---|---|---|---|---|
| `e >= 0`, even neighbours | `m * 2^(e+1)` | `2` | `2^e` | `2^e` |
| `e >= 0`, uneven | `m * 2^(e+2)` | `4` | `2^e` | `2^(e+1)` |
| `e < 0`, even neighbours | `m * 2` | `2^(1-e)` | `1` | `1` |
| `e < 0`, uneven | `m * 4` | `2^(2-e)` | `1` | `2` |

"Uneven" is the case every paper on this spends a paragraph on: at a
power of two the gap to the float *below* is half the gap to the float
*above*, because the mantissa just rolled over. A printer that ignores
it is wrong on exactly the values a test suite is most likely to
contain — `1.0`, `0.5`, `2.0`, every `2^k` — which is why the corpus in
§5 contains all 2046 of them rather than a sample.

### 3.2 No division, which is why `std.bignum` is ninety lines

A digit is `R / S` for two bignums. That sounds like it needs bignum
division, which is the single nastiest routine in an arbitrary-precision
library — Knuth 4.3.1 algorithm D, normalisation, quotient estimation,
the correction step.

It does not, because **the digit is 0 to 9**. Nine subtractions settle
it:

```
var digit = 0;
while bignum.compare(r, s) >= 0 {
    bignum.subtract(r, s);
    digit = digit + 1;
}
```

`std.bignum` therefore has no division at all. It has `compare`,
`subtract`, `add_into`, `mul_small`, `shift_left`, `copy` and `set`, all
in-place over a fixed-length `[int]` of base-2³² limbs, all
allocation-free — which is what lets the caller keep five of them in one
`region` and never touch the heap.

Base 2³² rather than 2⁶³ for one reason: a limb has to survive being
multiplied by ten. `10 * (2^32 - 1)` is about 2^35.7 and fits; the same
product in base 2⁶³ would trap on overflow.

### 3.3 The boundaries are inclusive for an even mantissa — a measured digit

Reading a decimal back uses round-to-nearest-**even**. So when the
mantissa is even, a decimal sitting exactly on the midpoint between this
float and its neighbour still reads back to *this* float: the tie goes to
the even one, which is this one. The boundary is inclusive.

That turns four comparisons from `<` into `<=`, and it is not cosmetic.
Without it, `5.299064834871378e16` printed as
`5.2990648348713776e16` — seventeen digits where sixteen round-trip.
Correct, and one digit too long, which is precisely the failure the
shortest-printing contract exists to rule out. Two of the first 4000
corpus values caught it.

### 3.4 A dead-level tie, where two answers are both right

Once in a while the exact value sits *exactly* halfway between two
shortest decimals. `2^-25` is one: its exact value is

```
0.0000000298023223876953125
```

and truncating to seventeen significant digits leaves exactly `5` with
nothing after it. Both `2.9802322387695312e-8` and
`2.9802322387695313e-8` are seventeen digits, and both read back to
`2^-25`. Neither is more correct.

So the rule is a **choice**, and the choice is observable:

| | `2^-25` prints as |
|---|---|
| Rust's `{:e}` | `2.9802322387695313e-8` |
| Python's `repr` | `2.9802322387695312e-8` |
| cancho | `2.9802322387695312e-8` (was `...313e-8` until the tie rule below) |

The first version of `float_into` followed Steele and White — round the
tie up — which is what Rust does. Python rounds the tie to the even
digit. Over the 9000-value corpus in §5 the two disagreed on **three**
values, all of them exact ties, and every one of the six spellings
round-trips.

**That was a defect, not a choice, and it is fixed.** A table tool that
wrote floats and was checked against Python found the same double
printing as two different strings depending on which tool wrote it
(`...86.125` is `...86.12` in Python's `repr` and was `...86.13` here),
which is the one thing a *shortest* printer is for: one answer. The rule
now is **the shortest decimal that reads back; among those the closest to
the exact value; and an exact tie goes to the even last digit**. That is
a definition with one answer for every double, and it is the rule of
Ryu, of Python's `repr`, and of Java since 19. Only Rust's `{:e}` (and
the first version here) differs, and only on exact ties: **1,543 of
100,000 doubles of the check corpus (which is built to be tie-rich:
`j / 2^k` with `j` odd and 17 or 18 digits ending in a 5) printed
differently from Python before; 0 of 1,002,109 after.** In the test
corpus of §5 it is the same three values: `2^-25` prints
`2.9802322387695312e-8` now.

This is worth a section rather than a footnote because it is the kind of
thing a document usually asserts and nobody checks. Both rules were
implemented and both were run against both oracles; the numbers above are
measurements, not recollections.

---

## 4. 1080 bits, measured rather than bounded

The working numbers get large: near the bottom of the subnormals the
numerator picks up a factor of 10³²³ on the way to the first digit, while
the denominator is already 2¹⁰⁷⁶.

The first draft of this comment claimed "about 2200 bits". Instrumenting
the algorithm over the corpus says otherwise:

```
widest 1080 bits, at 4.3374988771064606e-308  →  34 limbs
```

So `limbs()` is 80 — more than double the widest observed, because the
cost of the slack is one bump of an arena pointer and the cost of being
wrong is a silent carry off the top. But the comment now says 1080,
because the comment that said 2200 was made up.

---

## 5. Verified against an oracle

A printer is exactly the kind of code that passes every example someone
thought of and fails on the 4000th random bit pattern. So it is checked
against Rust's `{:e}` over a generated corpus, in
`shortest_printing_agrees_with_an_oracle`.

*(Since the tie rule of §3.4 changed, the oracle is `{:e}` or, where they differ, the exact tie rounded to the even digit,
decided from the exact value: `is_rust_or_even_tie` in `float_text.rs`.)*

The test has a pleasant property: **the expected output is the input**.
`{:e}` is the shortest round-tripping form, and the lexer's
`f64::from_str` is correctly rounded, so writing each value into the
generated program as `{:e}` and expecting the program to print that same
string back tests the digits and nothing else. A failure is a
disagreement about digits, never about notation.

The corpus is 9000 values:

| Group | Count | Why |
|---|---|---|
| Every normal power of two | 2046 | §3.1's uneven-neighbour case, exhaustively, not sampled |
| Every subnormal power of two, and the top of each binade | 104 | Where the exponent stops being an exponent |
| Every power of ten in range | 616 | Where the decimal and binary grids line up worst |
| The ones with a reputation | 14 | `0.1`, `1e23`, `9.999999999999999e22`, `f64::MAX`, `5e-324` |
| Deterministic pseudo-random bit patterns | the rest | The cases nobody thought of. splitmix64 with a fixed seed, so a failure reproduces |

Plus `the_values_with_no_literal_are_spelled` for `inf`, `-inf`, `NaN`
and `-0.0`, which have no literal syntax to generate and so are built
from arithmetic inside the program.

**At scale, against Python.** `scripts/float_differential.py` prints **1,002,109
doubles** through `float_of_bits` and `float_into` (random bit patterns, every
power of two, the edges, and 100,000 built to be exact ties: `j / 2^k` with `j`
odd and an exact expansion of 17 or 18 digits ending in a 5) and compares the
digits and the exponent with Python's `repr`, and that every string reads back to
the same 64 bits: **0 differences**; before the tie rule changed, 1,543 of the
first 100,000. `float_text.rs` does the same against Rust's `{:e}` for 220,000
doubles, accepting only an exact tie rounded to the even digit.

During development this corpus found two real defects: §3.3's missing
inclusive boundary, and §3.4's tie rule. Both were one-line changes and
neither would have been found by an example.

---

## 6. What it costs the caller

```
pub fn float_into[&o](out: &!o [byte], x: float) -> [] int
```

A `[byte]` to write into and the value. It answers how many bytes it
wrote, or `-1` if the buffer was too short — threaded rather than
trapped, because the caller chose the buffer.

**24 bytes is always enough**: a sign, seventeen digits, a point, `e`, a
sign and three exponent digits. The longest output there is is
`-1.7976931348623157e308`, at 23.

No `Heap`. The five bignums and the digits live in a `region`, so the
whole thing is one arena that dies at the closing brace, and a program
with no heap capability can still print a float. That is not an
optimisation — `examples/newton.cho` releases `heap` in its first three
lines, and would not compile otherwise.

---

## 7. What is still open

| Question | Why it waits |
|---|---|
| Positional notation | §1.1. `0.1` as `0.1` rather than `1e-1` needs a switch-over rule, which is a formatting policy and not a digit question. It is a layer on top of this one, and cheap once wanted |
| Reading a float from bytes | The inverse. `std.fmt` writes; nothing parses a `float`. It needs the same bignum and the opposite loop, and no program has asked yet. (`f32` has it: `std.fmt32.f32_of_text`, correctly rounded straight to binary32; [`f32.md`](f32.md) §5.3) |
| A width or a precision | `{:.3}` — round to a stated number of digits rather than the shortest. Different algorithm (the stopping rule goes away), same machinery. (Built for `f32` as `std.fmt32.f32_fixed_into`, exact ties to even; [`f32.md`](f32.md) §5.3; **for `float` see §10**) |
| Positional notation for `f32` | Built in `std.fmt32.f32_into` as Rust's `{:?}` (positional from `1e-4` to below `1e16`); `float_into` is unchanged |
| `float_of_bits` | Built (edition 7, [`floating-point.md`](floating-point.md) §4.1); it is how `scripts/float_differential.py` feeds a million exact bit patterns |

---

## 8. The suite

| Test | Rule | § |
|---|---|---|
| `shortest_printing_agrees_with_an_oracle` | 9000 values print exactly what Rust's `{:e}` prints | 1, 3, 5 |
| `the_values_with_no_literal_are_spelled` | `inf`, `-inf`, `NaN`, `-0e0` | 1.1 |
| `a_short_buffer_is_refused_rather_than_overrun` | `-1` rather than a trap or a partial line | 6 |
| `fixed_printing_rounds_the_exact_value_half_to_even_and_refuses_what_does_not_fit` | about 67,000 values and precisions against the exact expansion rounded half to even; five refusals and two exact fills of the buffer | 10 |
| `the_rounding_oracle_of_fixed_printing_is_itself_right_on_known_values` | the test's own oracle on twelve values worked by hand | 10 |

| Accepting | Shows |
|---|---|
| `examples/newton.cho` | The residuals printed as floats rather than scaled through `truncate` — the thing this slice was for |

---

## 9. A fast path for the numbers that are not short: Grisu3

**What was found.** `float_into` has two tiers: a short form (`short_form`: `m / 10^k` with `m` under 2^50, a few float operations) and the exact algorithm of §3 on bignums. A number with sixteen or seventeen digits has no short form, and
those are most of what arithmetic produces: every amplitude of a quantum state after a few gates (`1.5624999999999984e-2`), every sum, every ratio. cancho-quantum's `qsim` printed a dense 14-qubit state, 16,384 such numbers, in 250 to 316 ms
(`docs/benchmark.md` there), the whole of it `float_into`: the exact tier costs about 50 microseconds on a random double (15 on those amplitudes), because every digit is up to nine subtractions on bignums of 34 limbs.

**What it is now.** A third tier between the two, **Grisu3** (Loitsch, *Printing Floating-Point Numbers Quickly and Accurately with Integers*, 2010), written in cancho in `std/fmt.cho` like the rest: the number and the midpoints to its
neighbours are scaled by a cached power of ten (87 of them, for the decimal exponents -348 to 340 in steps of 8, generated by exact rational arithmetic with their error asserted to be at most half a unit in the last place), and digits are generated
with 64-bit integer arithmetic until the remainder is inside the scaled interval; `round_weed` then either certifies that the digits are the shortest and the closest to the value, or says it cannot tell. **When it cannot, the exact algorithm of §3 runs,
unchanged**, so the answer is the exact algorithm's whenever Grisu3 gives one; it gave one for 99.49 percent of 3,000,000 random doubles (2,983,228) in the development module.

The integers are 64 bits *unsigned* and cancho's are signed (`bitwise.md`): the comparisons go through `ult` (flip the sign bit and compare), the shifts through `lsr` (an arithmetic shift and a mask), the product of two 64-bit numbers through four 32-bit
limbs, and everything that can pass 2^63 through `wrapping_add`, `wrapping_sub` and `wrapping_mul`. Nothing traps: a fractional remainder times ten can exceed `i64`, and does, in the unsigned reading.

**What was measured.**

| | before | after |
|---|---|---|
| 300,000 random doubles through `float_into`, LLVM | 14.9 s (50 microseconds each) | 0.148 s (0.49 microseconds each): **100 times** |
| the same, Cranelift | 33.4 s | 0.315 s: **106 times** |
| `qsim` printing a dense 14-qubit state (16,384 amplitudes) | 316 ms | 14.4 ms: 22 times, the whole process; the output byte for byte the same (`md5sum`) |

Correctness, in three layers:

1. In the development module, **3,000,000 random doubles** printed by Grisu3 alone and by the exact algorithm: 0 differences in the 2,983,228 that Grisu3 answered.
2. Through `std.fmt`, `scripts/float_differential.py` against Python's `repr`: **1,051,299 doubles, 0 wrong**: the corpus of §5 (random bit patterns, every power of two, the edges, 100,000 exact ties) and since this section the classes a fast path is most likely to get wrong,
   `grisu_edges`: the two neighbours on each side of every power of two, the three on each side of every power of ten, the eight extreme mantissas (`0`, `1`, `2`, `3`, the half, the top) of every binade, and 20,000 decimals of 15 to 17 digits at every exponent.
3. **Four deliberate bugs** in the new code, each of which the corpus must notice: the rounding bit of the 128-bit product left out, `unit` not subtracted in `round_weed`, one cached power wrong in its last bit, and the weeding test answering `true`. All four make `float_differential.py` fail. **The first fails narrowly:** four wrong
   digit strings in 250,000 doubles, so a corpus of tens of thousands would not have caught it, and the million is not generous.

One difference from §3.3 deserves a sentence: Grisu3's interval is *exclusive* where the exact algorithm's is inclusive for an even mantissa. It is conservative, so it either returns the same digits or gives up; the corpus contains the boundary cases (the neighbours of
every power of two and of ten) and found no difference. That is measured, not proved.

**What it costs the caller.** Nothing changes: same signature, same effect row `[]`, same output for every double (§5's rule holds as before), the same region-only allocation (the digits are a 24-cell array in a `region`). The tables are two `static` slices of 87 cells,
built the first time a program prints a float that needs them.

**What is still open.** Ryu (Adams, 2018) would remove the fallback, and with it the 0.5 percent that run at the old speed; it needs 128-bit multiplications by a table of 5^k that cancho would have to compute or carry in full, and nobody has asked for the last 0.5 percent.
`std.fmt32` (`f32`) is unchanged and has its own exact tier.

---

## 10. A precision: `std.fmt.float_fixed_into`

> **Status: a design, written before the code.** Tags: **[read]** found by reading a program, **[choice]** a decision that can be overturned, **[measured]** printed by a command of this repository (none yet: §10.5 says what will be measured).

### 10.1 Who asks

The bar is two programs that ask (`standard-library.md` §1; `CONTRIBUTING.md`, "A feature earns its way in when a program asks for it"). **[read]** Both are in `cancho-consulting/metasearch-bidding`: `src/daily/num.cho` (`scaled`, `add_fixed`: a `float` with a number of decimals, rounded half to even **exactly**, with thousands separators, a sign, negative zero; about 100 lines, built from an error-free product), and `src/sim.cho` (`fixed`: a non-negative `float` with `digits` decimals, written as `truncate(x * 10^digits + 0.5)`, 25 lines). The two do not round alike: `sim.cho`'s is half **up** on the *rounded* product `x * 10^digits`, so it differs from the exact rule where the product is not exact or a tie falls on an even digit. `sim.cho` defines a simulated world whose bytes are compared across runs, so moving it to the library's exact rule is that program's decision and would change those bytes in the rare cases where the two differ; this document does not ask for it. `f32` has the function already (`fmt32.f32_fixed_into`, `f32.md` §5.3); `float`, the type programs use, has not, which is the row of §7 ("A width or a precision").

### 10.2 What it is

```
std.fmt.float_fixed_into(out: &!o [byte], x: float, prec: int) -> int
```

Writes `x` with exactly `prec` digits after the point (none, and no point, for `prec == 0`) and answers how many bytes it wrote, or **-1** if `out` is too short or `prec` is negative. **[choice]** The contract is `f32_fixed_into`'s, for the same reasons: the value is the exact binary value of `x`, expanded and **rounded half to even** at `prec` places (`0.125` with two places is `0.12`, `2.5` with none is `2`, `0.375` with two is `0.38`); the sign is kept on a negative value that rounds to zero (`-0.00`, as Rust's `{:.2}` and Python's `%.2f` print it) and on `-0.0`; `NaN`, `inf` and `-inf` are written as they are whatever `prec` is. There is no thousands separator, no sign for positive values and no width: those are the caller's, and `num.cho` keeps its own. A `prec` the buffer cannot hold (`prec > len(out) - 2`) is refused before any work is done.

### 10.3 How

The same machinery as §3, with the stopping rule gone. Write `x = m * 2^e` exactly. The number to print is `N = round(x * 10^prec)`, an integer: let `R = m * 2^e * 10^prec` and `S = 1` when `e >= 0`, else `R = m * 10^prec` and `S = 2^-e`; then `N = round(R / S)`, ties to even. `std.bignum` has what that takes (`set`, `shift_left`, `mul_pow10`, `mul_small`, `compare`, `subtract`, `copy`) and no division, and none is needed, as in §3.2: scale `S` by ten while the next scaling still fits under `R` (so `S1 = S * 10^(q-1)` has `R / S1` in `[1, 10)`, and `q` is the number of digits of `floor(R / S)`), then emit `q` digits, each by at most nine subtractions of `S1` from `R`, multiplying `R` by ten between digits. What is left, `r`, satisfies `frac(R / S) = r / S1`: compare `2r` with `S1` for the rounding (above: up; equal: up only if the last digit is odd), and carry through the digits written (`999` becomes `1000`: one digit more). The point goes `prec` digits from the right, with zeros in front when `N` has fewer digits (`0.003`). When `R < S` the one digit is `0` and `r = R`.

Sizes: `R` is under `1024 + 3.33 * prec` bits and `S1` no wider, so three bignums of `(1100 + 4 * prec) / 32 + 3` limbs, in a `region` like §6: no `Heap`, the row stays `[]`. A call costs `q` scalings plus `q` digits of up to nine subtractions on those limbs, so it is linear in the number of digits times the limbs; §10.5 measures it. **[open]** A fast path for the common case (a value and a precision for which `m * 10^prec` fits an `int`) would answer from integer arithmetic alone, as `short_form` does for the shortest form; it is not designed here, and would be checked against the exact path on every value.

### 10.4 Gates (written before the code; each must be able to fail)

| id | judges | how |
|---|---|---|
| F1 | the rounding, against an oracle | Rust `{:.*}` (correctly rounded, exact ties to even) in `float_text.rs`, and Python `'%.*f'` and exact `Fraction` arithmetic in `scripts/float_differential.py`: random bit patterns at precisions 0 to 30, **exact ties** at the precision (a dyadic value whose expansion ends in a 5 exactly there), every power of two at several precisions, the neighbours of every power of ten, the subnormals and the extremes at precisions 0, 1, 2, 17, 340 and 1100 |
| F2 | the edges | `-0.0`, a tiny negative (`-0.00`), `0.5` and `1.5` and `2.5` at 0, `9.5` and `99.5` and `0.995` (carry to a new digit), `0.0` at 0 and 5, `1e22`, `5e-324` at 1074 places (its whole expansion), `NaN`, `inf`, `-inf` at any precision |
| F3 | the refusals | a short buffer, `prec < 0`, `prec` too large for the buffer: `-1`, no trap, no partial line |
| F4 | the gate can fail | four deliberate bugs, each of which F1 must notice: ties rounded up, the carry through `9`s dropped, the sign dropped on a negative value that rounds to zero, the half taken as `>=` rather than `>` with the odd-digit test |
| F5 | the library is a library | `stdsource.cho` regenerated; `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace` |

### 10.5 What was built and measured

> **Status: built.** `std.fmt.float_fixed_into` (about 170 lines of `std/fmt.cho`, row `[]`, no `Heap`), the driver `tests/programs/float_fixed.cho`, the Rust test `fixed_printing_rounds_the_exact_value_half_to_even_and_refuses_what_does_not_fit` and its oracle's own test in `float_text.rs`, the `--fixed` part of `scripts/float_differential.py`, and `scripts/float_fixed_mutants.py`.

**Checked [measured].** (F1, F2, F3) The Rust test: about 67,000 cases (random bit patterns at 0 to 25 places, 18,600 exact ties at the precision, the neighbours of every seventh power of two and every fifth power of ten at seven precisions up to 1100, carries through nines, the extremes, `NaN`, `inf`, `-0.0`), each against the exact decimal expansion rounded half to even by hand in the test, which shares no code with the library; five refusals (`prec` negative, too large for the buffer, a number too long for it) answer -1 and two cases that fill the 1,500-byte buffer exactly are written whole. `scripts/float_differential.py --fixed` against Python's `'%.*f'`: 58,301 values with 0 wrong in the first run. (F4) `scripts/float_fixed_mutants.py`: nine deliberate bugs, ties rounded up, ties rounded down, the carry through nines dropped, the new leading digit of a carry dropped, the sign dropped on a negative that rounds to zero, the zeros before a short number one too few, `S` one bit too wide, `R` not scaled by `10^prec` for a whole number, and the digit count one too many; each makes the differential fail (the last by trapping the driver), and the first of them, applied to `std/fmt.cho` itself, fails the Rust test with 9,378 wrong. The `inf` and `NaN` spellings are `f32_fixed_into`'s.

**Cost [measured].** Apple M4 Max, macOS, release `cancho`, the driver of the test over 300,000 values, the minimum of three runs, wall time of the whole process (JSON in, text out). Values like those a report prints (1e-3 to 1e6, either sign): `float_fixed_into` 0.246 s at two places and 0.290 s at six, against `float_into`'s 0.172 s on the same values through its driver, and Python's `'%.*f'` 0.053 s in-process (not the same measurement: no process, no parsing). Random bit patterns, whose outputs are hundreds of digits (an exponent anywhere from -308 to 308 at two places is up to 310 digits): 2.7 s, about 9 microseconds a value, against `float_into`'s 0.196 s, because `float_into` prints 17 digits and this prints all the integer digits; Python 0.56 s. So the cost is linear in the digits written, as §10.3 read it from the loop, and for the values a report prints it is of the order of the shortest printer's fast tiers. **Not measured:** against `num.cho`'s 100 lines of `float` arithmetic, which would be the comparison that matters to its program. **[open]** the fast path of §10.3 (an `int` answer when `m * 10^prec` fits) is not built; the numbers above do not call for it.
