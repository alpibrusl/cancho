# `value_barrier`: a value the optimiser cannot see through

> **Status: built (#207, PR 3).** Found while measuring `std.ecdh`'s timing (`docs/ecdh.md` §3): the LLVM backend turned two
> masked selections into branches on the secret they were written to hide. This is the builtin that stops it, and the one
> rule for constant-time code that comes with it.

---

## 1. What was found

Constant-time code selects without branching. Two places in `std.ecdh` do it the textbook way:
- **`bigmod.ct_reduce` subtracts n under a mask.** It computes `m = under - 1` from the borrow, which is 0 or -1, then
  subtracts `w[n + i] & m` from every limb.
- **`ecdh.select` reads all sixteen table entries.** It ORs each one in under `m = (i ^ idx) - 1 >> 63`.

The source has no branch on `m`. The object `clang -O2` makes from it has one:

```
27bb:  test   %r9,%r9          ; the borrow, before `- 1`
27be:  js     27d0             ; skip the load of n's limb
27c0:  mov    0x10(%rdi,%r11,8),%r12
```

LLVM knows a value built as `x >> 63`, or as a 0/1 minus one, can only be 0 or -1. InstCombine rewrites `a & m` as
`select(m != 0, a, 0)`, and loop unswitching then hoists that select out of the loop as a branch. `ecdh.select` became a
`test %r14,%r14; jns` on whether `i == idx`: which table entry the secret window picked, as a branch.

**The timing test saw it** (`docs/ecdh.md` §3):
- a scalar of 1 against random scalars gave |t| = 13 on P-256;
- a scalar of 1 runs 1.3% faster, because its accumulator stays at infinity, so most reductions are not taken.

`std.x25519`'s `cswap` (`docs/x25519.md` §3) is written the same way and passed its audit. It passed only because LLVM
happened not to see that its mask comes from one bit: nothing in the source guaranteed it.

## 2. Why the source cannot fix it

LLVM sees every cancho function: the backend emits one module and runs `clang -O2` over it (`docs/llvm-backend.md`). Any
arithmetic spelling of a mask (`0 - bit`, `x >> 63`, a multiply by a 0/1 value) is something InstCombine can prove is 0 or
-1. So none survives. A value LLVM cannot reason about has to come from outside its reasoning: inline assembly, or a
volatile access. The language has neither, so the fix is a builtin.

## 3. The builtin

```cancho
edition 6;

value_barrier(x: int) -> [] int
```

**What it means:**
- **It answers `x`.** It has no effect, no trap, and the row `[]`.
- **What it promises** is that the optimiser treats the answer as unknown: no range, no known bits, no relation to `x`.

**How each backend emits it:**
- **LLVM:** `call i64 asm "", "=r,0"(i64 %x)`, an empty assembly statement whose output register is tied to its input.
  BoringSSL's `value_barrier_w` and Rust's `subtle` crate do the same.
- **Cranelift:** the identity. Cranelift does not turn a `select` or an `and` into a branch, so it has nothing to stop.

**Edition 6, the latest.** A new builtin is additive (`docs/editions.md` §5). `value_barrier` is a name a program could
already declare, so it exists from the current edition on, as `flush_out` did in edition 5. `std/bigmod.cho` and
`std/ecdh.cho` declare `edition 6;`.

**Compile-time evaluation** (`docs/compile-time.md`) declines it, as it declines every builtin it does not know. A call
with a constant argument therefore stays a call.

## 4. The rule

**A mask that hides a secret passes through `value_barrier` once, where it is made.** With that:
- InstCombine sees an `and` with an unknown value, which it leaves as an `and`;
- nothing is left to unswitch.

The rule is enforced in two ways:
- **Each module's audit** (`scripts/chacha20_branches.py`) checks that no conditional jump other than a loop counter's or a
  length's remains in the module's object.
- **Each module's timing test** checks that no class of inputs runs at a different speed.

`std.x25519` and `std.field25519` are left as they are. Their audit is clean today, and moving them onto the barrier is a
change of its own, with its own timing run (`docs/ecdh.md` §6). *Done after review finding A-1 (#316): both are edition 6, and
`cswap` and `cswap_at` make their masks through `value_barrier`. After it, on LLVM: ctgrind 0 reports for X25519
(`scripts/curve25519_ctgrind.sh`), no conditional jump but traps in `cswap`, `cswap_at`, `pack` or `scalarmult`
(`scripts/chacha20_branches.py`), and the timing test of `docs/tls-assurance.md` §6 at max |t| 2.13 (a fixed scalar) and 2.68 (a
sparse one), 20,000 samples each on the Apple M4 of that section, with the machine loaded (load average 6.7).*

## 5. Cost

**The statement itself emits no instruction.** It forbids a transformation, so the cost is what that transformation saved. In
§1's two functions, the branch skipped loading n's limb, or a table entry's, when the mask was 0. Now every limb is loaded
every time, which is what constant time costs. `docs/ecdh.md` §6 measures it on RSA and ECDSA verification, which share
`ct_reduce`.
