# std.linalg: dense linear algebra on flat slices

> **Status: built.** `std.linalg` has `dot`, `axpy`, `gemv`, `gemm`, `transpose`, Cholesky (`cholesky`, `cholesky_solve`), LU with partial pivoting
> (`lu`, `lu_solve`) and Householder least squares (`lstsq`), on row-major matrices held in `[float]` slices. Gated by
> `tests/accept/linalg_hand_checked.cho` (answers worked out by hand), `scripts/linalg_mutants.py` (14 mutants, all killed) and
> `scripts/linalg_differential.py` (680 random problems against numpy).

---

## 1. Why, and the bar

`CONTRIBUTING.md` asks for two askers before a feature enters the standard library.

* **cancho-ml** has its own `lib/linalg` (`gemm`, `gemv`, `syrk`, `chol`, …), pinned into its sibling packages as stores: it is the library `ml` and `stats`
  are built on, written because there was nowhere to put one. A function a standard library has is a function those do not each carry.
* **cancho-opt** (`opt minimize`, Nelder-Mead today) has Levenberg-Marquardt and curve fitting next in `docs/design.md` §1 of that repository; each
  step of LM solves a least-squares or normal-equations problem.
* **cancho-quantum** applies small dense unitaries and channels to state vectors and density matrices (`src/density.cho`), and its hardware
  analysis solves a few small systems; both write the loops out by hand.

That is the bar, counted by reading those programs, and the maintainer asked for it. What it does **not** claim: that `std.linalg` replaces `ml`'s `lib/linalg`.
That library is parallel and blocked for large matrices; this one is the plain loop, for matrices that fit in a region.

## 2. The surface

Matrices are flat row-major slices: `a[i * n + j]` is row `i`, column `j` of an `m x n` matrix. Nothing allocates: the caller supplies every output.

| Function | Does |
|---|---|
| `dot(x, y) -> float` | the sum of `x[k] * y[k]` over the length of `x` |
| `axpy(alpha, x, y)` | `y = y + alpha x` |
| `gemv(m, n, a, x, y)` | `y = A x` |
| `gemm(m, k, n, a, b, c)` | `C = A B` |
| `transpose(m, n, a, b)` | `B = A^T` |
| `cholesky(n, a) -> int` | `A = L L^T` in place (lower triangle; the strict upper one is not read or written); 0, or `k + 1` for the column `k` whose pivot is not positive |
| `cholesky_solve(n, l, b)` | `b` becomes the solution of `A x = b` |
| `lu(n, a, piv) -> int` | `P A = L U` in place with partial pivoting, the first of equal largest entries; 0, or `k + 1` when the whole column `k` is exactly 0 below the diagonal |
| `lu_solve(n, a, piv, b)` | `b` becomes the solution of `A x = b` |
| `lstsq(m, n, a, b) -> int` | the `x` minimising `|A x - b|` for `m >= n`, by Householder reflections; `b[0..n]` is `x`, and the norm of `b[n..m]` is the residual; 0, or `k + 1` for a column that is exactly 0 after the earlier reflections |

A size that does not fit its slice traps like any out-of-range index (`docs/defined-behaviour.md`). A *numerical* failure is the function's answer, not a trap: a matrix that is not
positive definite, or singular, is the program's to handle.

**Not here:** eigenvalues and the SVD, a QR that keeps `Q`, complex matrices, banded or sparse storage, blocked or parallel kernels, and `f32`. Each is added when two programs ask.

## 3. Choices

* **Sequential sums.** Every sum runs in increasing index order, so the bits do not depend on a summation order the library chose. That they are the same on every platform also needs the backends not to contract `a * b + c` into a fused
  multiply-add. **Measured, on x86-64 only:** `a * a - b` with `a = 1 + 2^-27` and `b = 1 + 2^-26` (0 unfused, `2^-54` fused) gives 0 on both backends; aarch64 was not measured here, so
  that half of the claim is open. This costs speed against BLAS, which sums in blocks, and is the reason the results agree with numpy only to what the conditioning allows (§4), not bit for bit.
* **A Householder reflection to `-sign(x0) |x|`**, not `+`: the other sign subtracts two nearly equal numbers when the rest of the column is small. The fixture has a column where that loses the answer
  (`[1, 1e-8]` with `b = [1, 1]`), and `scripts/linalg_mutants.py` has the mutant that flips the sign; it was *not* killed by the first fixture (which rounded to 9 places on a column whose tail was
  too large to matter), and the case above was added because of it.
* **Pivot ties go to the first row.** LAPACK's `idamax` does the same, which is why `lu` agrees with numpy's `solve` to the last place in the differential check (§4).
* **No condition estimate and no refinement.** A solve of an ill-conditioned system returns the answer the arithmetic gives; the caller who needs to know computes what it needs.

## 4. What was measured

| Gate | Result |
|---|---|
| `tests/accept/linalg_hand_checked.cho` | every printed number was derived by hand before the run (a dot, a 2 x 3 by 3 x 2 product, a Cholesky of `L L^T` for an integer `L`, an LU that needs a pivot, a singular and a not-positive-definite refusal, a line fitted through four points by the normal equations) and agreed on the first run |
| `scripts/linalg_mutants.py` | 14 mutants (index swapped, sign flipped, a term dropped, a divide changed to a multiply, the tie rule, the singular test, the reflector's factor and sign, the reflection of `b`), **14 killed** |
| `scripts/linalg_differential.py` | 680 seeded random problems against numpy 2.5.3: 150 products, 150 Cholesky solves (condition up to about 10^8), 150 LU solves, 150 least squares (tall, columns scaled over 6 orders of magnitude), 80 numerical refusals. Worst error over its bound: product 0.02 (bound `20 k eps |A||B|`), Cholesky 0.05 (`20 cond eps`), LU 0.000 (the same algorithm as LAPACK's: equal to the last place), least squares 0.10 (the first-order bound with the `cond^2 |r|` term) and its residual 0.17. |

**Corrected in place:** the bound first written for least squares in `scripts/linalg_differential.py`, `20 cond eps`, was too small for problems whose residual is large (the error was 1.4 of it at the worst, for
solutions that agree with numpy's to better than 1e-9); the standard first-order bound has a `cond^2 |r| / (|A| |x|)` term, which is the one now used. The code did not change.
