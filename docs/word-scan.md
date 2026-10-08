# Scanning a word at a time: `load_le64`, `byte_mask64` and the bit counts

Status: **built**, edition 8, both backends, and `wasm32-wasip1`.

## 1. Why

The CSV scanner of `cancho-table` (`docs/gap-scan.md` there, branch `gap-scan-speculate`: the brief for this slice called
the branch `gap-scan-design`, which does not exist) is 40-48 percent of every question it answers, and the language could
not make it faster than a byte loop. Its section 4.1 lists what was missing:

* a 64-bit word cannot be loaded. Eight indexed byte loads merge into one load only when written inline, in a loop
  guarded by `i + 8 <= len`; in a function they are eight checks and eight `ldrb`;
* there is no count of trailing zeros and no population count, so the lowest flagged byte of a SWAR word costs eight
  instructions of multiply and shift;
* a bit-gather of sixteen byte compares written out is half-vectorised by SLP into widened 64-bit lanes, with no
  `movemask`: the vectoriser does not see the idiom.

With two prototype builtins (a 216-line LLVM-only patch) the table reader ran at 0.84x-0.91x of the time on the four
everyday questions and 0.62x-0.65x on a 200-column file (`gap-scan.md` section 4.3; those numbers are that document's,
measured there, and were not re-measured for this one). This slice builds the primitives properly: both backends, total
or defined everywhere, an edition, fixtures, and a differential test.

## 2. The set

```
load_le64(text: &t [byte], at: int) -> int               // the 8 bytes text[at..at + 8], little-endian
byte_mask64(text: &t [byte], at: int, b: byte) -> int    // bit k set iff text[at + k] == b, k in 0..64
trailing_zeros(x: int) -> int                            // 64 for 0
leading_zeros(x: int) -> int                             // 64 for 0, 0 for a negative x
popcount(x: int) -> int                                  // 0 for 0, 64 for -1
```

All five are pure and capability-free: the row is empty, the effects are `Effects::pure()`, and the slice is only read.
They are edition 8 (section 6).

### 2.1 What happens out of range: a trap, as indexing does

A 64-bit word has no spare value, so an answer cannot say "out of range" in band. The other two shapes were weighed:

* **a refusal value** (an `Option`, or a `Result`, or an out parameter) would be the only trap-free form. The prelude has no
  option type a builtin could return, and adding one for this would be a prelude type, a `Split`-sized change in the
  checker and in the selfhost tables, for a primitive whose point is to be one load. It would also put a branch and a
  tag on the hot path, which is what the primitive is for avoiding;
* **zero padding** (bytes past the end read as 0) is total, and wrong: a scanner looking for the byte 0 finds a match that
  is not in the text, silently. `defined-behaviour.md` section 1 already says what this language does with a silently
  wrong answer.

So `load_le64` and `byte_mask64` have a **precondition, checked, and a violation traps**, exactly as `text[i]` does:
`0 <= at` and `at + 8 <= len(text)` (64 for the mask). The sum `at + 8` is never formed, so `at = 2^63 - 1` cannot wrap
past the bound, and a slice shorter than the word is tested on its own (`tests/conformance/word_scan.rs`, 20 calls on both
backends, the edge ones that must not trap among them). A trap is a deterministic stop, `SIGILL` natively and 134
under wasmtime (`defined-behaviour.md` section 1); no input reaches a compiler panic, a refusal without a rule tag, or
undefined behaviour. The three counts are total: they take any `int`.

The guard a caller writes is the loop condition it already has: `while at + 8 <= len(text)`. A tail shorter than the
word is a byte loop, as `docs/gap-scan.md`'s kernels do.

### 2.2 Why these five and not more

* **`byte_mask64` takes one byte.** The prototype took three (`a`, `b`, `c`) so that one call did the three compares of a
  CSV record in one pass over the loads. One byte is the smaller primitive, and the three-byte mask is
  `byte_mask64(t, at, 44) | byte_mask64(t, at, 34) | byte_mask64(t, at, 10)`: each call is its own bounds check and its own four
  loads of the same bytes; section 5 (kernel 9 against kernel 3) measures what three calls cost, and it is less than three
  times one.
* **64 bytes, not 16.** One mask is one `int`, which is what `trailing_zeros` and `x & (x - 1)` then walk. A 16-byte mask
  would be 16 bits of an `int` and a call per 16 bytes. Not measured against the 64-byte form: it is not built.
* **No SWAR builtin** (a `byte_eq_mask(word, b)` with the high bit per matching byte). The exact SWAR test is a handful of integer
  operations, all expressible in the language once `load_le64` and `popcount` exist (`benches/word_scan/scan.cho`,
  kernels 2 and 5), and section 5 measures it: it is faster than the byte loop and slower than `byte_mask64`. A builtin would
  add nothing the language cannot say and nothing the vector form does not beat.
* **Little-endian on every target.** All targets cancho builds for (x86-64, arm64, wasm32) are little-endian; the
  Cranelift load names its endianness explicitly, and a big-endian target would have to byte-swap, not change the
  meaning.
* **`leading_zeros` and `popcount` beside `trailing_zeros`.** The brief asked for the three. They are not one
  instruction everywhere: on arm64 `popcount` is `cnt` and `addv`, and on baseline x86-64 it is a software sequence
  (section 5).

## 3. How each backend lowers it

| | LLVM | Cranelift |
|---|---|---|
| `load_le64` | bounds check, `load i64, align 1` | bounds check, `load.i64` with `MemFlags` little-endian and no alignment requirement |
| `byte_mask64` | four `load <16 x i8>, align 1`, `icmp eq` against a splat, `bitcast <16 x i1> to i16`, `zext`, `shl`, `or` | four `load.i8x16`, `icmp eq`, `vhigh_bits.i16`, `uextend`, `ishl_imm`, `bor` |
| `trailing_zeros` | `llvm.cttz.i64(x, false)` | `ctz` |
| `leading_zeros` | `llvm.ctlz.i64(x, false)` | `clz` |
| `popcount` | `llvm.ctpop.i64` | `popcnt` |

The `false` is "zero is not poison", which is what makes the counts total. Cranelift's `ctz` and `clz` answer 64 for
zero, which the conformance test pins on both backends.

Neither backend calls the C library. The mask is `pcmpeqb` and `pmovmskb` on x86-64 (in the disassembly of both backends' kernel 3) and a
`cmeq`/`and`/`ext`/`zip1`/`addv`/`fmov` run per 16 bytes on arm64 (LLVM's; read from the disassembly of the same kernel); on `wasm32-wasip1` without
`simd128` LLVM scalarises the vectors, so the program is correct and not fast there.

**WASI.** Supported, not refused: nothing here needs the host. `wasm_gap` (`cancho-ir/src/target.rs`) lists the five as
`None`, so the count of refused builtins in `docs/wasm.md` (61, pinned by `os_tables.rs`; the 48 of the brief is a stale
number that `docs/wasm.md` has corrected several times) does not move. Measured under wasmtime: `tests/accept/word_scan.cho` exits 0 and a
mutant of it (one mask answer plus one) exits 1, so the program really ran there.

**An interpreter.** There is none in this compiler (`grep -ri interp crates/*/src` finds only comments about reinterpreting bits), so
there is no third table to fill.

## 4. The effect row

None. `Builtin::effects` falls through to `Effects::pure()` for all five, as for `wrapping_add`, and `Builtin::regions` is 1 for the two
that take a slice, as for `index_of_byte`.

## 5. What it is worth

`benches/word_scan/` (`scan.cho`, driven by `run.py`): a 1 GiB buffer on the heap, filled from a 64 KiB block of letters
with a `,` about one byte in 16, a `"` every 64 and a `\n` every 100, then three passes of one kernel; each kernel is its own
binary. Every kernel of a group answers the same number, which `run.py` prints and the rows below agree on (count of `,`
64,339,968; the byte at the very end 1,073,741,823; the structural bytes 92,045,312). The byte loops are written as a
program would write them: checked `+`, a bounds check per load. The SWAR rows are the exact test (`~(((x & 0x7f..) +
0x7f..) | x | 0x7f..)`, with `wrapping_add`) on `load_le64`, counted with `popcount` or located with `trailing_zeros`.

Each cell is **milliseconds a pass, instructions a byte, cycles a byte**, the median of 7 rounds in which every kernel runs
once against that round's fill-only binary (so the fill's variance cancels), divided by three. Instructions and cycles are
`perf stat` (`cpu_core/instructions/u`, `cpu_core/cycles/u`) on Linux and `time -l`'s *instructions retired* / *cycles
elapsed* on macOS; milliseconds are the children's user CPU time.

* arm64: Apple M4 Max, macOS, **load average 13-19 from other work on the machine during the runs** (not quiet).
* x86-64: gram, Linux, niced and pinned to cores 0-5 (`nice -n 19 ionice -c3 taskset -c 0-5`), load average 5-6 (the
  host's soak was running on other cores).

| kernel | arm64 LLVM | arm64 Cranelift | x86-64 LLVM | x86-64 Cranelift |
|---|---:|---:|---:|---:|
| 1 count `,` (1 byte in 16): byte loop | 697 ms, 6.40, 2.62 | 1001 ms, 18.32, 3.65 | 1682 ms, 3.62, 3.26 | 2853 ms, 12.24, 4.69 |
| 2 the same, `load_le64` + SWAR + `popcount` | 72 ms, 2.14, 0.33 | 150 ms, 5.25, 0.57 | 400 ms, 3.51, 0.72 | 780 ms, 6.76, 1.28 |
| 3 the same, `byte_mask64` + `popcount` | 46 ms, 0.90, 0.26 | 48 ms, 1.47, 0.18 | 101 ms, 0.60, 0.18 | 143 ms, 1.08, 0.24 |
| 4 find a byte only at the very end: byte loop | 295 ms, 6.27, 1.16 | 639 ms, 18.00, 2.30 | 448 ms, 2.76, 0.67 | 1526 ms, 12.01, 2.52 |
| 5 the same, `load_le64` + SWAR + `trailing_zeros` | 66 ms, 1.77, 0.33 | 128 ms, 4.50, 0.45 | 142 ms, 1.32, 0.22 | 524 ms, 4.25, 0.82 |
| 6 the same, `byte_mask64` + `trailing_zeros` | 42 ms, 0.83, 0.24 | 40 ms, 1.38, 0.15 | 109 ms, 0.36, 0.13 | 112 ms, 0.75, 0.17 |
| 7 the same, `index_of_byte` (`memchr`) | 39 ms, 0.70, 0.20 | 18 ms, 0.45, 0.07 | 99 ms, 0.10, 0.12 | 54 ms, 0.10, 0.10 |
| 8 count `,` `"` `\n`: byte loop | 923 ms, 9.48, 3.44 | 1417 ms, 31.05, 5.14 | 1940 ms, 4.85, 3.42 | 4917 ms, 22.07, 8.04 |
| 9 the same, three `byte_mask64` or-ed + `popcount` | 100 ms, 1.79, 0.43 | 112 ms, 3.24, 0.40 | 197 ms, 1.16, 0.30 | 277 ms, 1.78, 0.39 |

Raw output: `benches/word_scan/results-arm64-macos.txt` and `results-x86_64-linux.txt`.

What the table says, with the ratios computed from it (time, and instructions, of the byte loop over `byte_mask64` for the count):

* **Counting a byte: 15x (arm64) and 17x (x86-64) faster on LLVM, 21x and 20x on Cranelift**, in time; 7.1x and 6.1x
  fewer instructions on LLVM, 12.5x and 11.3x on Cranelift. LLVM's byte loop is already 6.4 (arm64) and 3.6 (x86-64)
  instructions a byte; Cranelift's is 18.3 and 12.2.
* **Finding a byte: 7.0x and 4.1x on LLVM, 15.9x and 13.6x on Cranelift** over the byte loop. Against `memchr`
  (`index_of_byte`), the C library's search, `byte_mask64` plus `trailing_zeros` is 1.10x slower on LLVM on both machines
  and 2.2x and 2.1x slower on Cranelift (the two backends' `memchr` rows differ, 38.6 against 18.4 ms on arm64; not explained
  here). A program that wants the first match of one byte should still call `index_of_byte`; the mask earns its place when
  the loop needs the *set* of matches, as a structural scan does.
* **Three bytes at once (the structural bytes of a CSV): 9.2x and 9.9x on LLVM, 12.6x and 17.7x on Cranelift**, from three
  `byte_mask64` calls or-ed. The cost is not three times one: 1.80 instructions a byte against 0.90 on arm64 LLVM, 1.16
  against 0.60 on x86-64 LLVM, 3.24 against 1.47 and 1.78 against 1.08 on Cranelift (kernel 9 against kernel 3).
* **The SWAR form on `load_le64` is worth having without the vector mask**: 9.7x and 4.2x faster than the byte loop on LLVM
  for the count, 6.7x and 3.7x on Cranelift. `byte_mask64` is a further 1.6x (arm64) and 4.0x (x86-64) on LLVM.
* **x86-64 `popcount` is not the `popcnt` instruction.** Neither backend emitted `popcnt` on gram, whose CPU has it
  (`/proc/cpuinfo`): the disassembly of the LLVM and of the Cranelift kernel 3 has none, and the LLVM one has the
  shift-and-mask sequence ending in `imul` by `0x0101..` (the code is for baseline x86-64, which has no `popcnt`). That sequence is 16 instructions in the LLVM kernel 3, once
  per 64 bytes: 0.25 of its 0.60 instructions a byte. Kernel 2 pays it once per 8 bytes, which would be 2 of its 3.5
  instructions a byte (arithmetic from the sequence's length, not a disassembly of kernel 2), and may be why SWAR is 4.0x behind
  the mask on x86-64 and 1.6x on arm64, where `cnt` exists. `popcnt`, `lzcnt` and `tzcnt` need a target-feature flag that
  `cancho` does not expose; not done (section 8).
* The mask is not free of the language's checks: the loop around it still pays a checked `+` and the bounds check
  (`jo`, `cmp`/`jl`, `cmp`/`ja` in the disassembly above the loads). The check is the primitive's contract, and
  LLVM did not remove it from the loop although the loop condition implies it.

**Noise.** The two machines were shared. Instruction counts repeat to three digits across the runs (kernel 1 on x86-64 LLVM:
3.623, 3.622, 3.623 in three complete runs); times do not: for kernel 4 on x86-64 LLVM three complete runs, with the
measurement changed between them, gave 1,050, 792 and 448 ms, and the table's row is the last. Read the instruction counts as
the result and the ratios of times as "several times", not as the second digit. Not measured: a quiet machine.

## 6. The edition, and what does not move

Edition 8, opened for these five names. `editions.md` section 6.4 closes edition 7 (the freeze: a name added to a closed
edition could shadow a name a program declared for itself), and `popcount` and `trailing_zeros` are the names a program
writes by hand before the language has them. Previous builtins of this kind (`float_of_bits`, `dir_rename_new`) joined
the then-latest edition because no file in the repository declared the name; here `examples/selfhost/ast.cho` has a local
variable named `leading_zeros`: these names are in use, so a new edition is the safe choice. Costs, all one-off: the parser's range
(`1..=8`, its message, the cancho-written parser's `value > 8`), `Builtin::since`, the selfhost tables regenerated, and
`tests/reject/unknown_edition.cho` moving to edition 9. `Split` is unchanged.

**No existing hash moves.** A hash is over a declaration's text, and the edition marker adds bytes only to a file that
declares it (`editions.md` section 6.3); nothing here declares `edition 8` except the new fixtures. Checked, not assumed:
every `.cho` file under `tests/accept`, `tests/programs`, `examples`, `std`, `packages` and `benches` that exists on
`origin/main` was given to `cancho ids --std` by an `origin/main` build and by this build, and the outputs compared: 226
files that either build accepts, 224 byte-identical and 2 different, `examples/selfhost/tables.cho` and `builtins.cho`,
the generated files whose text this change rewrites on purpose; another 146 are refused by both (modules that need their
siblings), so they were not compared. The golden-hash tests of `identity.rs` pass.

An edition-7 file that declares its own `popcount`, `trailing_zeros`, `leading_zeros`, `load_le64` or `byte_mask64` keeps
meaning its own (`tests/accept/word_scan_beside_own_names.cho`, on both backends), and an edition-7 file that names one
without declaring it is refused as `not-a-function` (`tests/reject/word_scan_is_edition_eight.cho`).

## 7. What it is checked by

* `tests/accept/word_scan.cho`, on **both backends** (`backends.rs`) and under wasmtime: each primitive against a loop written
  a byte or a bit at a time, over two 400-byte pseudo-random buffers (all 256 values; five values only), ten start offsets,
  every length 0..=140 and every `at` the primitive allows, the last one included; the counts on zero, each single bit, each
  run of low ones and the extremes.
* `tests/conformance/word_scan.rs`, on both backends: a **differential test against Rust** (`u64::from_le_bytes`,
  `trailing_zeros`, `leading_zeros`, `count_ones`, a plain per-byte mask) on three seeds, one printed line per
  (buffer, start, length) with a hash of every result: 2,820 lines a seed; **the trap edges**: 20 calls, each bound on both
  sides, a negative `at`, `i64::MIN`, `i64::MAX`, a slice of 7 or 63 bytes, an empty slice, and the exactly-fitting calls
  that must not trap; the counts on the extremes.
* `tests/reject/word_scan_is_edition_eight.cho` and `tests/reject/byte_mask64_needs_a_byte.cho`.
* On gram (x86-64) the same five conformance tests pass, with Cranelift's `vhigh_bits` lowering to `pmovmskb`.

**Mutants.** Twenty-one, each one edit to the lowering, run against the `word_scan` tests (which include the two backends' agreement and the own-names fixture) and the reject-fixture walk: LLVM and
Cranelift without the `length < width` test (a slice shorter than the word), with the bound off by one, with a signed
compare in place of the unsigned one (a negative `at` gets through), with the width wrong (7 for 8, 56 for 64); the mask
with its four lanes shifted by 8 rather than 16, with `ne` in place of `eq`; `load_le64` reading one byte on (LLVM) or
big-endian (Cranelift); `cttz`/`ctz` swapped with `ctlz`/`clz`, `ctpop`/`popcnt` replaced by another count, and (LLVM) `cttz`
with "zero is poison"; and the five names visible from edition 7. **All 21 killed.** One was killed by a hang: with zero
poison, the accept fixture's loop never ends, and the script's 240-second timeout stopped it. None survived; none was judged
equivalent. (A first, looser run of the same 21 filtered on more tests, among them two unrelated ones that fail when another
checkout runs them at the same time; it is not the result quoted: this one is, with a baseline that must pass first.)
`scripts/word_scan_mutants.py` applies each and restores the file; a mutant counts as killed when the filtered tests fail, and
one that does not build is an error, not a kill.

## 8. What was not done

* **An interpreter or a third backend.** There is none in this compiler, so nothing to refuse or implement.
* **The table tool itself.** The micro-benchmark is a scan for one byte (and for three); the 0.84x-0.91x of
  `gap-scan.md` was measured on the prototype and is not re-measured here, on the table tool, with these primitives.
  The mask is the same four 16-byte compares, so the instruction count of the scan should match the prototype's; that is an
  expectation, not a result.
* **`simd128` on wasm.** The vectors are scalarised there; wasm was checked for correctness, not speed.
* **A 16-byte mask, a SWAR builtin, a three-byte mask.** Argued in section 2.2; only the SWAR form is measured (kernels 2 and 5).
* **AVX2/AVX-512 or SVE.** 16-byte vectors are what both baseline targets have; wider vectors are a target-feature
  question for another document.
* **A total form that cannot trap.** Section 2.1: a trap on a violated precondition, as indexing does, was chosen over a
  new prelude type; if the table scanner wants to avoid a check per block, the check is the loop's own condition.
