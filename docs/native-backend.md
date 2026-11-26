# A backend of cancho's own

> **Status: designed; the contract and the flag are built, no backend is** (epic #840, task N0, #858). What exists: this document,
> `scripts/ll-census.cho` (the count as a program), `tests/native/subset.txt` (what the compiler may write, by name),
> `tests/commands/native_subset.sh` (the gate's test of both) and `--backend llvm|native`, where `native` is refused by a new
> rule, `unsupported-by-backend`. There is no reader, no encoder and no code generator: those are N1 to N15, each an issue.
> Every number below was produced by the command next to it on linux-x86_64 (clang 18.1.3), at the commit that adds this file,
> unless it says it is the epic's.

The goal, in one sentence: from a cancho program to machine code and to a file that runs, with no clang, no LLVM and no system
linker in the path. The first user is [cancho-os](https://github.com/alpibrusl/cancho-os), a kernel that should one day rebuild
itself on the board it runs on; so the first target is AArch64 with no system under the program, then AArch64 on Linux, then
x86-64, then macOS. LLVM stays the default and the release compiler until a measured gap says otherwise (N13).

---

## 1. The shape

```
source ──front end──▶ LLVM text (as today) ──┬──▶ clang -O2 ──▶ executable                                    (stays)
                                            └──▶ reader ─▶ SSA in memory ─▶ select ─▶ encode ─▶ object / image   (new)
```

| stage | what it is | task |
|---|---|---|
| **reader** | the text the compiler already writes, into functions, blocks, instructions with typed operands, globals and declarations; anything outside the subset of section 3 is refused by a rule, with the line | N1 |
| **SSA in memory** | the form the text is already in, kept so that register allocation and the few optimisations have something to work on | N1, used from N9 |
| **selection** | one instruction of the subset to the instructions of a processor; at first every value has a slot in the frame | N3 |
| **encoding** | an instruction to its four bytes, checked against clang's assembler | N2 |
| **object** | an ELF relocatable object, linked by the system's linker as today | N4 |
| **image** | the bare program laid out and written by the compiler: no linker | N7 |

The hand-kept runtime (`compiler/runtime/*.ll`, 19 files, 16,794 lines) goes through the same reader, because it is the same text.
The three files of hardware ciphers do not (section 3.3).

---

## 2. Decisions

Each is the epic's, with its reason and what would overturn it. N0 was asked to check D1 honestly; section 3 is that check, and
it changes what D1 costs but not D1.

| | decision | why | what would overturn it |
|---|---|---|---|
| **D1** | Read the LLVM text; do not emit a second form | One front end, one text, two consumers. Every existing test becomes a test of the reader by a round trip (N1), and the two backends are compared on the same input (D4). A second emitter would be a second `emit.cho` and its eleven companions (section 3.1), to be kept in step by hand | The subset turning out too wide to read cheaply, or the text losing something a backend needs. Measured: 171 forms (section 3.2), and one thing lost, which D6 names (the constant-time mark is in the text only as a value barrier). Neither overturns it |
| **D2** | AArch64 first; a target is a table of encodings, a calling convention and a list of system instructions. Nothing above selection names a processor | The first user is a Raspberry Pi; and the reader, the form in memory and the object writer are then shared by x86-64 (N14) | A second target that needs the form in memory changed. N14 is where that is found out |
| **D3** | ELF relocatable objects first, linked by the system's linker | The backend is useful and tested (the whole suite, the self-build) before a linker of our own exists | Nothing found. The image writer (N7) is the first place where no outside tool runs |
| **D4** | Differential testing is the definition of correct: a program built by both backends prints the same bytes and exits with the same status | There is an oracle, and it is the backend every release has shipped with. `docs/differential.md` is how the compiler in cancho was held to the Rust one | A difference where LLVM is the one that is wrong; then it is recorded, with the program, in this document |
| **D5** | Every value in memory first; no register allocation until the whole suite passes without it | The first code generator has one way to be wrong per instruction, not two | Nothing: it is an order of work |
| **D6** | A function that must run in constant time is marked, and the mark survives every optimisation; until N12 the native backend optimises nothing | Today the mark reaches the text as `call i64 asm "", "=r,0"(i64 x)`, a value barrier for LLVM's optimiser (section 3.2). A backend that does nothing clever keeps constant time by construction; one that does needs the mark on the function | N12 designs the mark. If it cannot be carried in the text, D1 gains a comment line the reader keeps, which is still one text |
| **D7** | The repository's rules: no file over 2,000 lines, design before code, every refusal a rule tag, no input reaches a panic | `CONTRIBUTING.md` | Nothing |

---

## 3. What the compiler writes: D1, checked

The epic counted two modules with a throwaway script and concluded: about forty instructions, a dozen intrinsics, six scalar
types, vectors only in the hardware ciphers. The count is now a program, it was run on 459 modules, and the sources were read
for what no module happened to use. **The names the epic gave are all found again; the list is wider than it said, in six
places**, and the reader's contract is the wider list.

### 3.1 Method

```sh
cancho build scripts/ll-census.cho --std -o census
cancho build $(sed 's|^|compiler/|' compiler/cancho.files) --std --emit ll --host darwin -o compiler.ll
./census compiler.ll --apart compiler/runtime/crypto_x86_64.ll --apart compiler/runtime/crypto_aarch64_darwin.ll --apart compiler/runtime/crypto_aarch64_linux.ll
```

- **The census** (`scripts/ll-census.cho`) prints a line for each form with its count, sorted by bytes: `instruction load`,
  `intrinsic llvm.sadd.with.overflow.i64`, `type i64`, `type {...}`, `constant undef`, `word add nsw`, `word icmp slt`,
  `word global unnamed_addr`, `asm "udf #0xc11f", ""`, `top define`. It has no list of LLVM's words: a word is classified by
  where it stands (the first word of an instruction is its name; any other bare word is a `word` of that instruction or of that
  top-level line), so a form nobody thought of appears under its own name. With `--apart FILE.ll` the functions that file defines
  are counted as `apart ...`; with `--subset SET` it prints only the forms that are not in SET and exits 1 if there are any.
- **The modules**: every program of `tests/accept` and `examples` that is one file (150 of 152 build so; the two others need a
  second file), and the compiler itself (115 files, 75,407 lines), each for `--host linux`, `darwin` and `linux-aarch64`: 453
  modules. Then the six probes of `tests/probes` that build for `--target aarch64-none-elf`.
- **The sources**: the string literals of the twelve files that write text (`emit.cho`, `driver.cho`, `wordcode.cho`,
  `floatcode.cho`, `threadcode.cho`, `cryptocode.cho`, `filecode.cho`, `netcode.cho`, `externcode.cho`, `consteval.cho`,
  `platform.cho`, `bare.cho`) and the runtime files, read for every form. Nothing was found there that the 459 modules do not
  also hold, so the committed set is the census and nothing was added by hand.
- **Not re-measured here: cancho-os's kernel.** It needs `cancho-ssh`, which this work could not read. The epic's count of the
  kernel stands as the epic's; every form it lists is in the set (the floating-point instructions, `inttoptr`, the volatile
  accesses and the registers all come from probes and fixtures here).

The compiler's module for darwin, counted again: **454,230 lines, 2,947 functions, 8,719 globals**, the epic's numbers exactly,
and `load` 114,283, `call` 51,779, `br` 46,825, `store` 44,788, as it said. Writing it takes 7.2 s on this machine (3.7 s on the
epic's); the census of it takes 0.39 s.

### 3.2 The set

`tests/native/subset.txt` is the set: **171 forms** and two patterns (the registers of `Cpu` are 171 names, each a string of
assembly, accepted by how the string begins), plus 85 lines for what the hardware ciphers' functions hold, counted apart. By kind:

| kind | the forms |
|---|---|
| **instructions** (46) | `add sub mul sdiv srem udiv and or xor shl ashr lshr` · `fadd fsub fmul fdiv fneg` · `icmp fcmp select phi` · `trunc zext sext fptrunc fpext fptosi sitofp bitcast inttoptr ptrtoint` · `alloca load store getelementptr extractvalue insertvalue` · `call ret br switch unreachable` · `atomicrmw cmpxchg` · `insertelement shufflevector` |
| **intrinsics** (12) | `llvm.sadd/ssub/smul/uadd/usub/umul.with.overflow.i64` · `llvm.memset.p0.i64` · `llvm.sqrt.f64` `llvm.sqrt.f32` · `llvm.ctlz.i64` `llvm.cttz.i64` `llvm.ctpop.i64` |
| **types** (15) | `i1 i8 i16 i32 i64 i128 float double ptr void label` · `{...}` (also `{}`, a capability erased) · `[N x T]` · `<N x T>` · `...` (a function type with more arguments, of a foreign call) |
| **constants** (11) | integers · floats as `0x` and sixteen hexadecimal digits, never decimal · `null true false undef poison zeroinitializer` · `c"..."` (escapes `\0A`, `\00`) · `[...]` · `{...}` |
| **predicates** | `icmp`: `eq ne slt sle sgt sge ult ule ugt uge` · `fcmp`: `oeq oge ogt ole olt une uno` |
| **flags** | `nsw` on `add sub mul` · `inbounds`, `nuw` on `getelementptr` · `align N` on `alloca load store atomicrmw cmpxchg`, and once on a call's argument · `volatile` on `load store` · `atomic` with `acquire`, `release` on `load store` · `seq_cst` on `atomicrmw cmpxchg` · `asm`, `sideeffect` on `call` |
| **constant expressions** | `ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)`, the size of `T` (section 4), as an operand of `add`, `mul`, `call` · `getelementptr inbounds (...)` as the address of `load`, `atomicrmw`, `call` · `inttoptr (i64 -1 to ptr)` in an `icmp` |
| **top level** | `define`, `declare`, a global, `%name = type {...}` (three, in `signals_darwin.ll`), `source_filename`. No `target triple`, no `target datalayout`, no `attributes`, no metadata |
| **words of a line** | `define`: `internal`, `noreturn`, `noinline`, and the string `"no-builtins"` · a global: `private internal external common global constant unnamed_addr align` |
| **inline assembly** | the trap (`ud2`; `udf #0xc11f`) · the value barrier of constant-time code, `asm "", "=r,0"(i64 x)` · for no system: `mrs $0, REG`, `msr REG, $0` with `isb`, `wfi`, `tlbi vmalle1` with `dsb nsh` and `isb` |

**What the epic's count did not have**, each found by the census and then in the sources:

| | the epic said | measured |
|---|---|---|
| 1 | vectors only in the hardware ciphers | **the emitter writes vectors itself**: `byte_mask64` is `insertelement`, `shufflevector`, `load <16 x i8>`, `icmp eq <16 x i8>`, `bitcast <16 x i1> ... to i16` (`compiler/wordcode.cho`, `docs/byte-search.md`). The compiler's own module holds it, so N6 needs it |
| 2 | no `i128`, no `float`, no `i16` in either module | `i128` in `mul_wide` (`zext`, `mul i128`, `lshr`, `trunc`: `docs/wide-multiply.md`); `float` for `f32` (`fptrunc`, `fpext`, `llvm.sqrt.f32`); `i16` in the mask above and in two calls of the runtime. The compiler's module holds all three types |
| 3 | no atomics but `cmpxchg` | `atomicrmw add ... seq_cst` (the count of threads, `threadcode.cho`, and the runtime), `load atomic` and `store atomic` (the network runtime) |
| 4 | `udiv` absent | once, in the network runtime (`net_*.ll`) |
| 5 | one string of assembly on a hosted target | two: the trap, and the value barrier |
| 6 | every instruction on one line | the one `switch` of `runtime.ll` is four lines, a case each |

Also wider than the two programs showed, and in the set: `llvm.usub.with.overflow.i64`, `llvm.cttz.i64`, `llvm.ctpop.i64`,
`fcmp uno`, `icmp ule`, `nuw` on `getelementptr`, a call through a pointer (`call T %t(...)`), and a call with its function type
written out.

**Not in the set, and so refused by the reader**: `urem`, `frem`, `fptoui`, `uitofp`, `freeze`, `fence`, `invoke`,
`landingpad`, `resume`, `indirectbr`, `callbr`, `va_arg`, `addrspacecast`; `llvm.memcpy` and `llvm.memmove` (the library's
`memmove` is an ordinary call); `exact` and the fast-math flags; `tail`, `musttail`; metadata and attribute groups; `thread_local`,
`section`, aliases, comdats, `weak`, `linkonce`; packed structs, `opaque`, `half`, `fp128`; decimal floating-point constants;
quoted names; a module with a `target triple` for a machine the backend has no table for.

One thing found that is the LLVM path's own: `llvm.cttz.i64` and `llvm.ctpop.i64` are called and declared nowhere, and
`llvm.ctlz.i64` is declared only in `bare.ll`. clang accepts a call of an intrinsic it knows without a declaration; a reader must
too, or the driver must declare them. N1 decides which; nothing is changed here.

### 3.3 The hardware ciphers, apart

`compiler/runtime/crypto_x86_64.ll`, `crypto_aarch64_darwin.ll` and `crypto_aarch64_linux.ll` (4,498 lines) are the AES, GHASH
and SHA-256 that use the processor's own instructions. Their functions hold what nothing else does: `extractelement`, eight
vector types, `i96`, `llvm.bswap.v4i32`, the `llvm.aarch64.*` and `llvm.x86.*` intrinsics, `cpuid`, the `"target-features"`
strings. With the native backend they are left out, the `hw_*` builtins answer "not here", and the software paths run (N5). The
census counts them `apart` so that the test can hold both things at once: today's modules pass, and a vector written anywhere
else is still named (the test writes one into the probe and sees it refused).

### 3.4 What this means for the tasks

- **N1 (the reader)** takes the set of section 3.2. Its issue says vectors are "parsed far enough to be refused"; that holds
  for the ciphers, but `<16 x i8>` in `byte_mask64` is the compiler's own, so the reader must take it, or the front end must
  write a scalar form of that one builtin when the backend is native (as it will for `hw_*`). **Proposal: the second.** It is
  one builtin, sixteen lines of `wordcode.cho`, and it keeps vectors out of the code generator altogether. N1 or N3 decides.
- **N3 (the code generator)** has `i128` only as `zext`, `mul`, `lshr 64`, `trunc` (one multiply and `umulh`), `float` as five
  operations, and atomics as two instructions with one ordering each. None is large; all were uncounted.
- **N5 (the runtime)** has the multi-line `switch`, numbered values (`%0`), names with a dot or a hyphen (`%.atomictmp`,
  `%atomic-temp`), and lines with no indentation, all from files clang once wrote.

---

## 4. The data layout the reader assumes

`emit.cho` names no size and no offset. It writes the type, and where it needs a size it writes the constant expression
`ptrtoint (ptr getelementptr (T, ptr null, i32 1) to i64)` (`size_of`) and leaves the arithmetic to LLVM, under clang's default
layout for the target (the module has no `target datalayout`). So the layout below is LLVM's, and a backend that reads the same
text must compute the same numbers. Measured by compiling exactly that expression for each type with clang 18.1.3, for
`aarch64-none-elf`, `aarch64-unknown-linux-gnu` and `x86_64-unknown-linux-gnu`; the three agree on every row:

| type | size | alignment |
|---|---|---|
| `i1`, `i8` | 1 | 1 |
| `i16` | 2 | 2 |
| `i32`, `float` | 4 | 4 |
| `i64`, `double`, `ptr` | 8 | 8 |
| `i128` | 16 | 16 |
| `{}` | 0 | 1 |
| `{ T1, T2, ... }` | each field at the next multiple of its alignment; the whole rounded up to the largest alignment | the largest of its fields' |
| `[N x T]` | N times the size of `T` | that of `T` |

Checked rows: `{ i8, i64 }` 16/8, `{ i64, i1 }` 16/8, `{ ptr, i64 }` 16/8 (a slice), `{ i32, i8 }` 8/4, `[3 x { i32, i8 }]`
24/4, `{ i8, { i8, i32 }, i8 }` 16/4, `{ i8, i16, i8 }` 6/2, `{ i1, double }` 16/8, `{ i8, i128 }` 32/16, `[5 x i8]` 5/1.
Little-endian on all four targets. `i1` is stored as a byte holding 0 or 1. An `alloca` or a global with no `align` takes its
type's alignment; `align N` where written (the runtime) is honoured. `arm64-apple-darwin` was not measured here (no macOS
machine); N15 measures it before it relies on it.

N1's first test is this table: the reader's sizes against clang's, for every type that appears in the compiler's module.

---

## 5. What is refused, and how

- **`unsupported-by-backend`** is the one rule (number 58, the 64th tag). Today it refuses every program given `--backend native`,
  as a refusal about the program (no place in it is at fault), after the front end has accepted the program: a program that is
  wrong is refused by its own rule first. `check --output json` carries the tag; the status is 1; nothing is written.
- From N1 the same rule refuses a module with a form outside section 3.2, **naming the line of the text**. A person who meets
  it has either called something the native backend does not build yet (said in words) or found a change to the compiler that
  widened the text without widening the reader (a bug, which the test of section 6 exists to catch first).
- No input reaches a panic: N1 carries a test in which every single-byte damage and every truncation of three modules is read
  or refused by this rule.

---

## 6. The contract as a test

`tests/commands/native_subset.sh`, in stage `tests` of the gate (11 s here):

1. builds the compiler (for the machine the gate runs on) and `tests/probes/freestanding.cho` (for no system) to LLVM text,
   and the census finds no form outside `tests/native/subset.txt`;
2. the census of the compiler counts what the module is made of, twice the same, sorted;
3. **it can fail**: one call of `llvm.bitreverse.i64` written into the probe's module is named, and so are an instruction
   (`urem`), a type (`half`), a flag (`nuw` on `add`) and a vector instruction in a function that is not a cipher's;
4. `--backend native` on `build` (an executable, a module, an object for no system), `check`, `run` and `test`: status 1, the
   rule, and nothing written; `--backend llvm` writes the module byte for byte as no flag does.

A later change to `emit.cho` or to `compiler/runtime/` that writes a new form fails step 1 with the form's name. The fix is to
add the line to `subset.txt` and the row to section 3.2 in the same pull request, which is the moment the reader's owner sees it.

What step 1 does not see: a form only another host writes (the gate runs on linux-x86_64 and darwin-aarch64, so linux-aarch64's
runtime is read by neither), and a form no part of the compiler uses (floating point). The 459-module census of section 3.1
covers both and is run by hand; it is cheap to make a nightly job of, and N1's round trip over the whole suite replaces it.

---

## 7. For the owner

| | the question | proposal |
|---|---|---|
| **O1** | The flag's name | `--backend native`, built so. `--backend` already existed (`llvm`; `cranelift` is refused as not in this build), so no flag was added: a value was |
| **O2** | How a bare program's page of boot assembly reaches the image without an assembler | **Bytes the program supplies**: `--prepend FILE` in N7, the file being the page assembled once by any assembler and committed with its source beside it, with a test that the two agree where clang is present. cancho-os's `board/boot.S` holds boot, the exception stub and the loader's entry and changes rarely; a table of encodings or a small assembler is more compiler for the same bytes. Revisit if the page starts to change often |
| **O3** | Whether the seed may be built by the native backend once N8 holds | **Not yet.** The seed stays LLVM text built by clang (`docs/seed.md`) until N13's numbers exist: it is the root of trust and its reseed compares hashes of text, which the native backend does not change. What can change sooner is the release binary, per target, under O4 |
| **O4** | When the default flips, per target | **Never by date.** For `aarch64-none-elf` first, when N8 has booted the board and N13 states the cost; hosted targets after the self-build (N6) has run in CI for a release with no difference found. `--backend llvm` stays for as long as clang is what the differential tests compare with |

---

## 8. The flag

```sh
cancho build x.cho --backend llvm     # the default; the module and the program are byte for byte what they were
cancho build x.cho --backend native   # status 1, `unsupported-by-backend`, nothing written
cancho check x.cho --backend native --output json   # the same refusal, as data
```

`--backend` is taken by `build`, `check`, `run` and `test` (`compiler/cliprog.cho`, `clitest.cho`, `clibase.backend_of`); a
command that builds nothing (`ids`, `layout`, `authority`) refuses it as a usage error, and so is a value that is neither.
`test --backend native` refuses before it reads the files, since there is no program to run the tests of; the other three
check the program first. `tests/reject/backend_native_not_built.cho` is the rule's fixture, with a new header line,
`//~ BACKEND native`, that the fixture walker passes to `check` as it passes `//~ TARGET`.

---

## 9. Not done

- Any reader, encoder, code generator, object or image writer: N1 to N8.
- The tables of the form in memory (how a value, a type, an instruction, a block and a function are numbered; where a constant
  expression is kept): N1 writes them here, before its code.
- The table of instruction shapes (operands, result type, flags) in full: section 3.2 is its outline, N1 completes it.
- The census of cancho-os's kernel, made again (section 3.1).
- The census as a job over every module of every host; today it is the gate's host and the bare probe.
