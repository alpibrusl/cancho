# A backend of cancho's own

> **Status: it builds programs for AArch64 on Linux, and objects for AArch64 with no system; it is not the default, and writes no image** (epic #840; N0 in #858, N1 in #860, N2 in #863. What exists: `scripts/ll-census.cho` and `tests/native/subset.txt` (what the compiler may write, by name), the reader
> and its printer (`compiler/native/`, section 10), `--backend roundtrip` (every program a test of the reader) and `--backend native`,
> which reads the module with vectors refused and, for a machine it has no code for, refuses the program by `unsupported-by-backend`. The AArch64 encoder (section 11, N2) is held to clang's assembler, and the first code generator (section 12, N3) makes code that was
> run under qemu, the compiler's own included; N4 writes ELF objects, and `--backend native` builds programs for AArch64 on Linux and objects for no system (section 13). Images, and the other machines, are N5 to N15. Every number below was produced by the command next to it
> on linux-x86_64 (clang 18.1.3), at the commit that adds the section it is in, unless it says it is the epic's.

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
- **cancho-os's kernel** (0.0.30 at `adaf83e`, with cancho-ssh at the commit it pins), counted again with N1, when cancho-ssh
  could be read: 143,933 lines, **834 functions, 846 globals and 18 strings of assembly, as the epic said**, and every form of
  it is in the set. (N0 wrote here that it had not been measured, and relied on probes for the forms a kernel uses.)

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
  as a refusal about the program (no place in its source is at fault), after the front end has accepted the program: a program
  that is wrong is refused by its own rule first. `check --output json` carries the tag; the status is 1; nothing is written.
- Since N1 the reader runs first, and a module with a form it does not read is refused **naming the line of the module** and the
  reason (thirteen of them, `llmod.why_text`): `line 3008 of the module (--emit ll writes it): a vector, which the native backend
  has no code for`. A module it reads is refused for the code generator there is not yet. A person who meets
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
cancho build x.cho --backend native   # for AArch64 on Linux or with no system: the program, with no clang and no LLVM (section 13)
cancho build x.cho --backend native --host linux   # another machine: status 1, `unsupported-by-backend`, nothing written
cancho check x.cho --backend native --host linux --output json   # the same refusal, as data
```

(Until N4 the second line was true of every machine, and this section said so.)

`--backend` is taken by `build`, `check`, `run` and `test` (`compiler/cliprog.cho`, `clitest.cho`, `clibase.backend_of`); a
command that builds nothing (`ids`, `layout`, `authority`) refuses it as a usage error, and so is a value that is neither.
All four check the program first. (`test --backend native` refused before it read the files until N1, when there was nothing
to read a module with.) `tests/reject/backend_native_not_built.cho` is the rule's fixture, with a new header line,
`//~ BACKEND native --host linux`, that the fixture walker passes to `check` as it passes `//~ TARGET` (the words after the
backend's name are passed too, since N4: the fixture must be refused on a machine the backend does write code for).

---

## 9. Not done

- Any encoder, code generator, object or image writer: N2 to N8.
- The census as a job over every module of every host; today it is the gate's host and the bare probe.
- Section 10.7 lists what the reader itself leaves for later.

---

## 10. The reader (N1)

`compiler/native/`: `llmod.cho` (the table, the tokens, the names, the types), `llread.cho` (types, values, constants),
`llbody.cho` (instructions), `lltop.cho` (the lines outside a function, and `read`), `llprint.cho` (the module written back),
`llback.cho` (what the command line asks of them). 3,700 lines; all but the printer are pure: no capability, no heap, and no trap
on any text.

```
room(text)                 how many cells the table needs
read(text, m, strict)      0, or 1 with the line, the reason and the place in the header of m
print(heap, text, m)       the module as LLVM text
```

### 10.1 One table

The module is one `[int]` the caller allocates. Nothing is copied out of the text: a name, a number and a string are where
they are in it, so the text must live as long as the table. The first 64 cells are the header; then come regions laid out once,
from a count of the text (lines, tokens, the longest line, the longest function, names, brackets):

| region | what it holds | sized from |
|---|---|---|
| tokens | the tokens of the line being read: kind, start, end | the longest line |
| operands | a stack: the operands of the instruction being read, the fields of the type being read | the longest line and the longest function (a `switch` keeps its cases there across lines) |
| types | a record a type, found again by a hash of its kind and parts | the brackets in the text |
| names of the module | a hash table: where the name is, its length, the record that defines it, whether it is used | the lines that begin `@`, `define`, `declare` |
| names of the function | a hash table stamped with the function's number, so that a new function empties it at no cost; and what is known of each name | the longest function |
| records | everything else, in the order of the text | 16 cells a line and 2 a token |

A write past a region's end is a refusal (`the module is larger than the room the reader was given for it`), never a trap; a test
gives the reader every table size from none to enough.

Measured on the compiler's module (453,709 lines, 19.9 MB): `room` asks for 37.1 million cells (297 MB), of which the records use
9.9 million (79 MB). The bound is loose on purpose: it must hold for any text, and the count that makes it is one pass. Tightening
it is worth doing before the compiler runs on a board with 1 GB (section 10.7).

### 10.2 Types

A type is a number. `void i1 i8 i16 i32 i64 i128 float double ptr label` are 0 to 10; every other type is made when the text first
names it and is the same number each time after (466 types in the compiler's module). A type's record is its kind, a count and
its parts:

| kind | count | parts |
|---|---|---|
| integer | bits (1 to 128) | |
| struct | fields | the type of each |
| array, vector | length | the element |
| function | parameters | the result, each parameter, 1 if more may follow |
| named (`%struct.x`) | | the slot of its name, where the type it names is kept |

### 10.3 Values, and what a use refers to

An operand is four cells: its type, its kind, and two cells the kind gives a meaning to.

| kind | first cell | second cell |
|---|---|---|
| local | its number in the function | where its name is |
| global | the slot of its name | where its name is |
| integer | the value | where it is written |
| integer too large for 64 bits | | where it is written |
| floating point | its 64 bits (`0x` and sixteen digits) | where it is written |
| word | which of `null undef poison zeroinitializer true false none` | |
| constant with parts | its record | |
| label | the block's number | where its name is |
| type alone | | |
| index (of `extractvalue`) | the index | |
| inline assembly | where its two strings start | where they end |
| `align N` of a call's argument | N | |

**A local is a number, dense in its function**, given at its first sight, use or definition. When the function ends its names
are written as a record, four cells each (the name, the record that defines it, its type, whether it is a label), so that a use
finds its definition in one step; a name used and never defined is refused there, with the line of the use. **A global is a
slot** in the module's table, so a function may be called before the line that defines it; the slot holds the defining record
once that line is read, and a name still undefined at the end is refused, unless it is a function of LLVM's own (`llvm.`), which
clang too takes without a declaration (section 3.2).

**A constant with parts is a record written before the record that uses it**: an array, a struct, a vector, a `c"..."` string,
or a constant expression (`getelementptr`, `ptrtoint`, `inttoptr`, `bitcast`, each with the operands of the instruction of that
name). An array whose elements are all integers of one type is one cell an element: the compiler's string literals are such
arrays, 1.9 million elements in its own module.

### 10.4 Records

Each begins with its kind and its size, so a reader of the table walks it with no index:

| record | cells after the kind and the size |
|---|---|
| line kept as written | start, end (`source_filename`, `target`) |
| named type | the slot of its name, the type |
| global | name, slot, flags (`private internal external common unnamed_addr constant`), type, alignment, line, value (an operand; kind 0 for none) |
| declaration | name, slot, function type, line |
| function | name, slot, flags, result type, parameters, where its attributes start and end, the record of its names, blocks, instructions, the line of its first vector, line; then each parameter's type, number and name |
| block | the label's number, where its name is (-1: the first block, with no label written) |
| instruction | its number in the list below, flags, the local it defines and where that name is, the type of its result, a number (alignment or predicate), line, operands |
| constant | its kind, type, parts, two cells for the kind, the parts |
| names of a function | how many, then four cells each |
| end of a function | |

### 10.5 The shape of each instruction

A line of another shape is refused. `T` is a type, `a` a value of the type before it, `N` a number.

| instruction | shape | flags | result |
|---|---|---|---|
| `add sub mul sdiv srem udiv and or xor shl ashr lshr` | `T a, b` | `nsw`, `nuw` | `T` |
| `fadd fsub fmul fdiv` · `fneg` | `T a, b` · `T a` | | `T` |
| `icmp` · `fcmp` | `PREDICATE T a, b` | | `i1`, or a vector of them |
| `select` | `T c, T a, T b` | | the type of `a` |
| `phi` | `T [ a, %from ], ...` | | `T` |
| `trunc zext sext fptrunc fpext fptosi sitofp bitcast inttoptr ptrtoint` | `T a to T` | | the second `T` |
| `alloca` | `T [, T count] [, align N]` | | `ptr` |
| `load` | `[atomic] [volatile] T, ptr p [ORDERING] [, align N]` | | `T` |
| `store` | `[atomic] [volatile] T a, ptr p [ORDERING] [, align N]` | | none |
| `getelementptr` | `[inbounds] [nuw] T, ptr p, T i, ...` | | `ptr` |
| `extractvalue` · `insertvalue` | `T a, N, ...` · `T a, T b, N, ...` | | the part the indices lead to · `T` |
| `call` | `T [(T, ..., [...])] CALLEE(T [align N] a, ...)` where the callee is `@name`, `%pointer` or `asm [sideeffect] "text", "constraints"` | | `T` |
| `ret` · `unreachable` | `void` or `T a` · nothing | | none |
| `br` | `label %to` or `i1 c, label %then, label %else` | | none |
| `switch` | `T a, label %default [ T k, label %to ... ]`, the cases a line each or not | | none |
| `atomicrmw` | `[volatile] OPERATION ptr p, T a ORDERING [, align N]` | | `T` |
| `cmpxchg` | `[volatile] ptr p, T old, T new ORDERING ORDERING [, align N]` | | `{ T, i1 }` |
| `insertelement` · `extractelement` · `shufflevector` | `T v, T a, T i` · `T v, T i` · `T a, T b, T mask` | | the vector · its element · a vector as long as the mask |

Predicates, orderings and the operations of `atomicrmw` are LLVM's lists (`llread.cho`); an attribute of a parameter or of a
function other than `noreturn noinline alwaysinline nounwind` and a string, metadata, `tail`, `exact`, a fast-math flag and a
decimal floating-point number are refused, each with the reason of section 5 that fits.

**Vectors are read, and refused only when asked.** N1's issue has the reader refuse a vector; but the compiler's own module has
them (section 3.2), so a reader that refused them could not be tested on the compiler. The reader reads them, notes for each
function the line of its first one, and takes a flag: with `strict` a vector type is a refusal. `--backend roundtrip` reads
without it; `--backend native` reads with it, which is what a code generator with no vectors needs to be told.

**A first block with no label** has a number all the same (LLVM numbers what has no name as it comes: the parameters, that
block, the instructions), and a `phi` may name it by that number. The reader gives it that number when the function ends; found
by the third test module, which is as clang wrote the runtime.

### 10.6 What was measured

```sh
cancho build FILES --std --emit ll --backend roundtrip -o module.ll     # the module read and written back
cancho build | run | test ... --backend roundtrip                       # the same, then clang as usual
cancho test --std compiler/native/llmod.cho compiler/native/llread.cho compiler/native/llbody.cho compiler/native/lltop.cho compiler/native/llprint.cho tests/native/reader_test.cho
```

`--backend roundtrip` is not in `--help`: it is a way to test, not a backend to build with.

| what | result |
|---|---|
| The compiler's module (478,646 lines with the reader in it), read | 0.9 s; writing it takes 7.2 s. Read and written back: 1.2 s on top of the 7.2 |
| The same, for `--host linux`, `darwin`, `linux-aarch64` | all three read; 2,947 functions and 366,000 instructions each at N0's commit |
| What clang makes of the compiler's module before and after (`clang -O2 -c`) | **the same object, byte for byte** |
| The census of the compiler's module before and after | the same line for line: every form counted the same (the gate's test) |
| The compiler built from modules that went through the reader, three times over | stage 2 equals stage 3, **and both equal the compiler built without it** (one sha256) |
| `tests/accept` and `examples` with every build through the reader (stage `walk`) | 133 and 16 fixtures pass |
| The `cancho test` suites of `tests/cancho-tests.txt` through the reader | all 14 pass (the issue says 18; the list has 13, and this adds one) |
| Probes for no system (`freestanding`, `cpu`, `mmio`, `bare_heap`), `clang -O1 -c` before and after | the same objects (the gate's test) |
| A vector, `invoke`, `urem`, `tail`, metadata, an attribute, a quoted name, an unknown target: 18 forms | each refused with its reason and its line |
| Every byte of three modules (5,000 bytes) made each of ten bytes, strict and not; every truncation | each read or refused; none reaches a trap (100,000 reads, 6 s) |
| The reader broken on purpose in seven ways (the printer drops `volatile`; an undefined name, a name defined twice, a vector and an unknown instruction let through; the room's check removed; a hexadecimal number of any length taken) | five were caught by the suite as it was written; the last two had no test, and have one now |
| cancho-os's kernel (0.0.30, `adaf83e`) built by its own `build.sh` with every module through the reader | `kernel.o`, `kernel8.img` and `kernel-virt.elf` are **the same bytes** as without it; the image boots in QEMU 8.2.2 (`raspi3b`) and `./test.sh about 'echo ci' ticks crash about` answers as it should, the trap of `crash` included |

The round trip is stronger than N1 asked: it asked that programs behave the same, and the objects are the same bytes, so the
printer loses nothing clang reads. What it does lose is what clang does not read: comments, and the spacing.

### 10.7 Left for later

- **The room.** `room` is an upper bound that holds for any text; it is four times what the compiler's module uses.
- **`byte_mask64` under the native backend** (section 3.4): still a vector, so `--backend native` refuses a program that
  searches bytes, at the line. N3 decides between a scalar form in the front end and vectors in the code generator.
- **Undeclared intrinsics** are accepted, as clang accepts them; the driver could declare them instead.
- **A layout module** (the sizes and offsets of section 4, from the type table) is N3's first step; the table has what it needs.

---

## 11. The AArch64 encoder (N2)

`compiler/native/a64.cho`: one function a form, the operands checked, answering the 32 bits or -1 for an operand that does not
fit. `compiler/native/a64sys.cho`: the system registers by name. Both are pure, with no trap on any operand, and neither knows
anything of a program: choosing instructions is N3's, and relocations N4's.

### 11.1 The forms

The sections are those of the Arm Architecture Reference Manual for A-profile (DDI 0487), chapter C4, "A64 Instruction Set
Encoding", and C6 for each instruction by name. A register is its number; 31 is the stack pointer or the zero register, as
the form has it. An offset of a branch is counted in instructions.

| form | instructions | section |
|---|---|---|
| `movw` | `movn movz movk` | C4.1.86, move wide |
| `adr` | `adr adrp` | C4.1.86, PC-relative addressing |
| `addsub_imm` | `add adds sub subs` with twelve bits, shifted twelve or not (`cmp cmn`, `mov` to and from `sp`) | C4.1.86, add/subtract immediate |
| `logic_imm`, with `logical_imm` and its inverse `logical_value` | `and orr eor ands` with a logical immediate (`tst`, `mov` of a mask) | C4.1.86, logical immediate |
| `bitfield`, `shift_imm`, `extend` | `sbfm bfm ubfm`; `lsl lsr asr` by a constant; `sxtb sxth sxtw uxtb uxth` | C4.1.86, bitfield |
| `extr` | `extr` (`ror` by a constant) | C4.1.86, extract |
| `addsub_shift` | `add adds sub subs` with a shifted register (`cmp`, `neg`) | C4.1.89, add/subtract shifted register |
| `addsub_ext` | the same with an extended register, the form that takes `sp` | C4.1.89, add/subtract extended register |
| `logic_shift` | `and orr eor ands bic orn eon bics` with a shifted register (`mov`, `mvn`, `tst`) | C4.1.89, logical shifted register |
| `dp2` | `udiv sdiv lslv lsrv asrv rorv` | C4.1.89, data-processing 2 source |
| `dp1` | `rbit rev16 rev32 rev clz cls` | C4.1.89, data-processing 1 source |
| `madd`, `mulh` | `madd msub` (`mul`, `mneg`); `smulh umulh` | C4.1.89, data-processing 3 source |
| `csel` | `csel csinc csinv csneg` (`cset`, `csetm`, `cinc`) | C4.1.89, conditional select |
| `branch`, `branch_cond`, `compare_branch`, `bit_branch`, `branch_reg` | `b bl`; `b.cond`; `cbz cbnz`; `tbz tbnz`; `br blr ret` | C4.1.87 |
| `udf`, `exception`, `eret` | `udf`; `svc brk`; `eret` | C4.1.87; `udf` is C6.2 |
| `hint`, `barrier` | `nop yield wfe wfi`; `dsb dmb isb` | C4.1.87, hints and barriers |
| `system_register`, `sys` | `mrs msr`; `sys` (`tlbi`, `dc`) | C4.1.87, system register move and system instructions |
| `load_store` | `ldr str ldrb strb ldrh strh ldrsb ldrsh ldrsw`, and `ldr str` of `s` and `d`, at an index | C4.1.88, load/store register (unsigned immediate) |
| `load_store_offset` | the same at a byte offset (`ldur stur ...`), post-index and pre-index | C4.1.88, unscaled immediate, immediate post-indexed and pre-indexed |
| `load_store_reg` | the same at a register, extended and scaled or not | C4.1.88, register offset |
| `load_store_pair` | `ldp stp` of two registers of 32 or 64 bits, or two of `s` or `d`: offset, post-index, pre-index | C4.1.88, load/store register pair |
| `load_literal` | `ldr` from a distance | C4.1.88, load register (literal) |
| `exclusive`, `ordered` | `ldxr stxr ldaxr stlxr`; `ldar stlr`, of 8 to 64 bits | C4.1.88, load/store exclusive and ordered |
| `fp2`, `fp1`, `fcmp`, `fcsel` | `fmul fdiv fadd fsub`; `fmov fabs fneg fsqrt fcvt`; `fcmp` (with zero too); `fcsel` | C4.1.90, floating-point data-processing, compare, conditional select |
| `fp_int` | `scvtf ucvtf fcvtzs fcvtzu`, and `fmov` between a register and its floating-point twin | C4.1.90, conversion between floating-point and integer |

That is what N3 needs for the subset of section 3.2: `cmpxchg` and `atomicrmw` are loops of `ldaxr` and `stlxr`, since a
Raspberry Pi 4's processor (Cortex-A72, ARMv8.0) has no `cas`; the 128-bit multiply is `mul` and `umulh`; `llvm.ctlz` is `clz`,
`llvm.cttz` is `rbit` and `clz`. Left out, with the task: NEON, SVE, the cryptography extension, `cas`, pointer authentication.
`llvm.ctpop.i64` has no instruction outside NEON (`cnt`); N3 writes it as arithmetic or takes the one NEON instruction then.

### 11.2 Held to another implementation

`tests/native/a64_vectors.py` writes each form's operands as assembly (150 choices a form, from a generator of its own so that the
file is the same on every machine), has `clang --target=aarch64-none-elf` assemble them, and writes the words to
`tests/native/a64.vectors`: **6,144 vectors for 40 forms**, a line each (`addsub_imm 0 1 1 28 2 0 0 7100005c ; subs w28, w2, #0`).
The script knows how an instruction is written, clang knows what it is, and neither is the encoder. The gate does not run the
script: `tests/native/a64_check.cho` reads the file and needs no assembler.

**The system registers are clang's answers too.** For each of the 171 names `Cpu` accepts (`compiler/bare.cho`), the script
assembles `mrs x9, NAME` or `msr NAME, x9` (both, where both are allowed: 284 lines), takes the fifteen bits out of the word, and
writes them into `a64sys.cho`'s table. So the table is not a second copy of the reference manual typed by hand; and 150 more
vectors name a register by its numbers (`S3_0_C1_C0_0`), which is where the encoder's own layout of the fifteen bits is tested.

| what | result |
|---|---|
| The 6,144 vectors | every one is the word the encoder gives, the first time it was run |
| Every thirteen bits of a logical immediate, decoded, and the encoder asked for that value | the encoder's bits say the same value; 5,334 values for 64 bits and 1,302 for 32, each with one spelling the encoder gives back as it is |
| A million values made of one to three runs of ones | taken exactly when some bits say them |
| 63 calls with an operand out of range | each refused |
| A million tuples of operands for each of the 40 forms, of every size and sign (40 million calls, 4 s) | each answer is 32 bits or a refusal; none traps |
| The encoder broken 189 ways: one bit of each of its 38 constants, one more place for each of its 151 shifts | 188 caught by the vectors; the last is in `logical_value`, which only the suite uses, and the suite catches it |

### 11.3 Not done

- **`adrp` has one vector**, for a distance of zero: with any other, the assembler leaves a relocation and a zero in the word.
  It shares its layout with `adr` (150 vectors); N4, which resolves such a relocation itself, is where it is tested whole.
- **The vectors regenerated on linux-aarch64 and on darwin-aarch64** give the same file, the task says. They were made on
  linux-x86_64 with clang 18.1.3 only; `python3 tests/native/a64_vectors.py --check` is the command, and exits 1 on a difference.
- Nothing called the encoder when this was written; N3 (section 12) does, and brought it into `compiler/cancho.files`.

---

## 12. The first code generator (N3)

`compiler/native/`: `lllayout.cho` (the sizes and offsets of section 4, from the reader's types), `llout.cho` (what is written: the
words, the relocations, where each function is), `a64frame.cho` (constants, slots, copies, a value of the module into a register
or into memory), `a64call.cho` (what crosses a call, and where), `a64gen.cho` (one instruction of the module), `a64fn.cho` (a
function: frame, entry, blocks, branches, end; and `generate`). Pure, with no trap on any module the reader read.

**Every value is in memory** (D5). An instruction of the module is lowered alone: its operands are loaded from their slots into
scratch registers, worked on, and the result stored. Nothing is in a register between two instructions, so no instruction's
lowering knows another's. That is slow on purpose; N9 changes where a value lives, and the lowering goes through the two
functions that fetch an operand and store a result so that "where" can then be a register.

### 12.1 The frame

```
            the caller's arguments that did not fit registers
x29 + 16 ─▶
x29      ─▶ the caller's x29, the return address          stp x29, x30, [sp, #-16]! ; mov x29, sp
            the bytes of each alloca
            a slot for each value; a second for each phi
            the scratch bytes (as large as the largest value any instruction works on)
sp       ─▶ the arguments of this function's calls that do not fit registers
```

The stack pointer does not move between the entry and the end, so a slot is at one offset from it throughout, and that offset
is not negative. An `alloca` is bytes of the frame and its value is their address; one whose count is not a number is refused
(the compiler writes none). A slot is the type's size, rounded up to eight bytes, at the type's alignment (at least eight).

**Offsets the instructions do not reach.** A load or a store reaches 4,095 times its size; past that the address is worked out
in `x17` first, in one `add` (to 4 KiB), two (to 16 MiB) or a constant and an `add` (beyond). The frame itself is taken off the
stack pointer the same three ways. `tests/native/gen/tricky.ll` has a function with 5,000 bytes and 2.4 MiB of `alloca`, which
writes at both ends of each and was run.

**The registers.** `x9` to `x15` hold what one instruction works on; `x16` is the address of a name and of what is called
through a pointer; `x17` is an address the immediates do not reach, and a constant on its way to a floating-point register;
`d16` and `d17` are the floating-point operands. All are registers a callee may overwrite, so a function saves nothing but
`x29` and `x30`.

### 12.2 Values

| type | in its slot | in a register |
|---|---|---|
| `i1` | a byte, 0 or 1 | bit 0; a parameter and a result are masked on arrival, since the other bits of their register are not defined |
| `i8 i16 i32 i64 ptr` | its bytes | zero-extended to 64 bits when loaded, or sign-extended for the instruction that reads the sign |
| `float double` | its bytes | `s` or `d` |
| `i128` | sixteen bytes, the low word first | never whole: see `mul` and `lshr` below |
| a struct, an array | its bytes, laid out as section 4 | never: copied, in pieces of the alignment its place is known to have |
| a vector | refused, with its line | |

A constant with parts (a struct of constants, a string) is written into memory part by part where it is used. A constant
expression is worked out when the program is built: a name and a number added to it, which is one relocation.

### 12.3 The lowering

`tests/native/lowering.txt` has a line for each instruction and each function of LLVM's own that `tests/native/subset.txt` lets
the compiler write: `lowered`, or `refused` with the reason. A test fails when a form is in one file and not the other.

| instruction | what it becomes |
|---|---|
| `add sub mul and or xor shl lshr udiv` | both operands zero-extended, one instruction of 64 bits, the result stored at its width |
| `sdiv ashr` · `srem` | the same, sign-extended · `sdiv` and `msub` |
| `mul i128`, `lshr i128` by 64 or more, `zext` to and `trunc` from `i128` | `mul`, `umulh` and the two cross products; the high word; a word and a zero; the low word. Any other use of 128 bits is refused |
| `fadd fsub fmul fdiv fneg` | the instruction of that name on `d16`, `d17` |
| `icmp` | `cmp` and `cset`, sign-extended for `slt sle sgt sge` |
| `fcmp` | `fcmp` and `cset`; `one` and `ueq` are two conditions joined |
| `select` | `tst` of bit 0 and `csel` or `fcsel`; for a value in parts, one of two copies |
| `trunc zext sext inttoptr ptrtoint` | the load that extends, the store that cuts; `sext i1` is a negation |
| `bitcast` | the same bytes |
| `fptosi sitofp fptrunc fpext` | `fcvtzs`, `scvtf`, `fcvt` |
| `alloca` | the address of its bytes in the frame |
| `load store` | through the pointer, at the type's width; a value in parts is a copy; `atomic` with `acquire`, `release` or `seq_cst` is `ldar`, `stlr` |
| `getelementptr` | the pointer, plus each index times the size of what it counts; the indices that are numbers are added up when the program is built |
| `extractvalue insertvalue` | a copy of the part's bytes; a copy of the whole and then the part |
| `phi` | nothing where it is written: see the blocks below |
| `br` | `tbz` of the condition's bit 0 over a jump; each jump is an unconditional `b`, which reaches 128 MiB |
| `switch` | a compare a case, in the order written, then the default |
| `ret` | the value into `x0`... or to where `x8` said; the frame given back |
| `unreachable` | `udf #0` |
| `atomicrmw` · `cmpxchg` | a loop of `ldaxr` and `stlxr` (a Raspberry Pi 4's processor has no `cas`) |
| `call` | section 12.4 |
| `llvm.sadd/ssub.with.overflow` · `uadd/usub` | `adds`/`subs` and `cset vs` · `cset cs`/`cc` |
| `llvm.smul.with.overflow` · `umul` | `mul`, `smulh`, and whether the high half is the sign of the low · `umulh` and whether it is zero |
| `llvm.memset` | a loop of byte stores (no call: the module may have no `memset`) |
| `llvm.sqrt` · `ctlz` · `cttz` · `ctpop` | `fsqrt` · `clz` · `rbit` and `clz` · arithmetic on pairs, fours and bytes, and a multiply (NEON has the one instruction) |
| inline assembly | the strings the compiler writes and no other: the trap `udf #0xc11f`; the value barrier (a copy: there is no optimiser to hide from); `mrs`, `msr` and `isb` of a register `Cpu` accepts; `wfi`; `isb`; `tlbi vmalle1` with `dsb nsh` and `isb` |
| `insertelement shufflevector extractelement`, and any vector | refused |

**Blocks and `phi`.** A `phi` has two slots. A jump to its block writes the value it has "when come to from here" into the
second; the block, when it begins, moves every `phi`'s second slot into its first. So two `phi`s that read each other (a swap
in a loop) each see the other's old value, with no temporary to choose: the test module has that loop, run for five turns and
for six. Because a jump carries those copies, a conditional branch is a test over one jump to another, never a conditional
jump to a block.

### 12.4 Calls

The rule is the one LLVM applies to the types of the text, since the code this meets (the C library, and for now any function
clang still compiles) was compiled by it:

- a value in parts is its scalars, one after another, each as if it were an argument of its own;
- integers and pointers take `x0` to `x7`, floating-point values `d0` to `d7`; then eight bytes of the stack each;
- an array goes whole in registers or whole on the stack; an `i128` takes two registers from an even one;
- a result is in `x0`..., `d0`... the same way; one that needs more than eight registers of a kind is written where `x8` says,
  and the caller passes the address of the result's own slot;
- a function with more arguments than its type names (`printf`) is called the same way on Linux. On macOS it is not, which is
  one reason `--backend native` refuses to make code for that machine (N15).

One walk over the parts of a type (`a64call.walk`) does all five things that need this: counting the stack a call takes, an
argument out, a parameter in, a result out, a result in.

### 12.5 What is written

`llout`: the words of every function, one after another; for each function its record, its first word and the word after its
last; and the relocations, each a kind, a word, a name and a number: `bl name`; `adrp` and `add` of a name this module defines;
`adrp` and `ldr` through the linker's table of addresses for a name it does not. N4 puts them in an object.

### 12.6 How it was run, and what was found

There is no object writer yet, so the code was run by another road: `tests/native/gen_dump.cho` writes the words as assembly
(`.inst 0x...`, and the instruction itself where a name is), clang assembles that and compiles the module's data, and the two are
linked and run under `qemu-aarch64` (`tests/native/gen_run.sh`). The gate has no AArch64 machine and runs none of it; what the
gate holds is that the generator writes again, word for word, the text that was run.

| what | result |
|---|---|
| `tests/native/gen/tricky.ll` (15 functions, every lowered form) | prints what clang's build prints, line for line; the gate keeps its 1,581 words |
| `tests/accept` and `examples`, each built for linux-aarch64 both ways and run under qemu on the same input (150 programs that build alone) | **148 the same.** `word_scan` traps, as it must: it searches bytes with the vector builtin (section 3.4). `process_group_kill` prints what its fixture expects and exits 0 natively, where clang's build did not finish in 20 s under qemu, twice; not looked into further |
| **The compiler itself** (3,245 functions; 19 that use a vector made traps for the run) | 1.78 million instructions, 7.1 MB. It runs: it says its version, checks a program, and **writes the compiler's own module, 29 MB, byte for byte as the clang-built compiler does** (5 min 31 s under qemu) |
| The time from text to code for the compiler's module | 1.1 s, reader included |
| cancho-os's kernel (834 functions) | code for all of it: 422,119 instructions, 1.69 MB. **It does not link**: the kernel's link script refuses an image past 2 MB, and clang's whole object is 0.4 MB. Booting it is N8, and the size is N8's first number |
| The probes for no system, with all 171 registers | code for each (the gate's test) |
| A vector, an `alloca` of unknown size, assembly and a register that are not in the table, `add i128`, an unknown function of LLVM's, a `phi` with no value for a block | each refused with its line and its reason |
| Every byte of a module of most forms made each of fourteen bytes, and every truncation, read and then generated (26,000 modules) | none traps |
| Tables for the code of every size below what is needed | each a refusal |

Two mistakes were found by running and by no review: `cttz` was written as `clz` alone (the test of the name looked at the
wrong letter), which only clang's output for the same module showed; and the pattern for `msr NAME, $0` was one byte too long,
which the kernel's module showed.

### 12.7 Not done

- ~~Objects, and so `--backend native` building anything~~: done, section 13.
- ~~Vectors: `byte_mask64` is still one~~: for the native backend the driver writes it with none, section 13.2. The generator
  still has no vector, and refuses one.
- ~~The hardware ciphers~~: the driver writes the two answers "no" and the traps that the runs above were given by hand,
  section 13.2. The instructions themselves are still not used by the native backend's programs.
- **Speed.** Nothing is measured against clang's code but the 5 min 31 s above, which is mostly qemu's.
- **The stack is not probed.** A frame of megabytes is taken in one step past any guard page.
- macOS and x86-64 (N15, N14).

---

## 13. Objects (N4)

`compiler/native/elf.cho` writes what the generator made as an ELF relocatable object for AArch64, and `cancho build
--backend native` is then a program: the object is linked by the system's linker, started through the same `--clang` as
before, which is given an object and compiles nothing. For a machine with no system, `--emit obj` is the object and nothing
is started at all.

### 13.1 The file

Eleven sections, always the same ones in the same order, so that two runs write the same bytes:

| section | holds |
|---|---|
| `.text` | the functions, one after another, each aligned to 4 |
| `.rodata` | the constants in which no address is written |
| `.data` | the other values of the module, a constant that holds an address among them |
| `.bss` | the values that are all zero (no bytes in the file) |
| `.symtab`, `.strtab` | a symbol for each name the module defines (the `internal` ones first, as the format asks), and an undefined one for each name it only uses |
| `.rela.text` | `R_AARCH64_CALL26` for a call; `ADR_PREL_PG_HI21` and `ADD_ABS_LO12_NC` for the address of a name of the module; `ADR_GOT_PAGE` and `LD64_GOT_LO12_NC` for one of another object |
| `.rela.data` | `R_AARCH64_ABS64` for an address written in a value |
| `.shstrtab`, `.note.GNU-stack` | the sections' names; and the empty note that says the stack is not code |

The generator's relocations (section 12.5) already had these six kinds; the writer gives each its symbol and its addend and
changes no word of the code. There is one `.text`, not a section for each function, so a link drops nothing the program does
not reach: N7 needs that for a kernel, and section 12.6's 1.69 MB is what it costs today.

### 13.2 What the module carries for this backend

The generator has no vectors and none of the processor's cipher instructions, and three pieces of the text the driver writes
are made of them. Asked for the native backend, the driver (`compiler/nativert.cho`, a bit of the feature cell) writes each
another way; asked for LLVM it writes what it always wrote, byte for byte.

| what | for LLVM | for the native backend |
|---|---|---|
| `byte_mask64`, the search of `std.bytes` | four compares of 16 bytes | `@cancho_byte_mask64`: a loop of 64 single compares |
| "has the processor AES and the carry-less multiply?", "has it SHA-256?" | asked of the processor | no, both |
| the functions that use those instructions | the instructions | traps that nothing reaches, since the answers are no |

So the library takes the paths it takes on a processor with no such instruction, which the suites already test. This is
slower, and it is the answer section 3.4 asked for only in part: a search of bytes is 64 loads where it was four.

### 13.3 The gate

Stage `native` of `scripts/gate.sh` builds the programs of stage `walk` that are run (`tests/accept`, `examples`) and every
`cancho test` run of stage `tests` with `--backend native`, and holds each to what its fixture says it prints and to its number
of tests. It runs where the backend has a machine, Linux on AArch64 (a job of its own in CI, `gate-native`); elsewhere it says
it does not run and passes. The programs of `tests/reject` are not run again: a program the front end refuses never reaches a
backend, and `tests/commands/native_elf.sh` holds three of them to the same answer with either.

On every machine, `tests/commands/native_elf.sh` holds the writer to the bytes of the object that was linked and run
(`tests/native/gen/tricky.o.hex`, 9,592 bytes; `llvm-readelf -a` and `llvm-objdump -dr` read it with no warning, and what
`readelf` says is kept beside it), shows that one instruction or one value changed is another object, and builds the four
programs for no system as objects with a `--clang` that does not exist.

### 13.4 What was measured

Linux on x86-64, clang 18.1.3 as the other compiler, `qemu-aarch64` 8.2.2 as the machine. The linker was `aarch64-linux-gnu-gcc`,
given as `--clang`, and for the runs below it left a script at the output that starts the program under qemu, so that the
compiler on x86-64 and the walker could run what they had built.

| what | result |
|---|---|
| `tests/native/gen/tricky.ll` as an object | 9,592 bytes; `llvm-readelf -a` and `llvm-objdump -dr` read it with no warning; linked, it prints what section 12.6's run printed |
| **Every `cancho test` run of `tests/cancho-tests.txt`** (17 runs, 266 tests), built with `--backend native` and run under qemu | **all pass**, with the number of tests each must have. `native_reader`, whose tests damage a module a byte at a time, takes 410 s there |
| **`tests/accept` and `examples`, by the gate's own walker** (134 and 16 programs) | all the examples, and all of `tests/accept` **but two**: `exec_spawn_under` and `exec_spawn_detached_under` print `worker code 127`. Built by clang and run under the same qemu they print the same: the emulator does not start a program by a directory and a name. **On a real machine they pass**: the first run of the CI job of section 13.3 (Linux on AArch64, a GitHub runner) passed the whole stage, these two included, in 9 minutes from the checkout, the compiler's own build from the seed among them |
| **The compiler, built by the native backend, building itself with the native backend** (under qemu), and that one again | the second object and the third are the same 11,673,352 bytes, **and they are the bytes the clang-built compiler writes for the same sources**. 517 s for one build under qemu |
| The compiler's object, from its sources, on x86-64 | 10.6 s with `--backend native`, of which 9.7 s is the front end writing the module; clang takes 45 s for the object alone. 11.7 MB against clang's 5.2 MB |
| The four programs for no system (`freestanding`, `cpu`, `mmio`, `bare_heap`) with `--emit obj` and a `--clang` that does not exist | four objects for AArch64; the LLVM backend, asked the same, fails for want of clang |
| `llvm-nm -u` on the objects | for a program of Linux, functions of the C library and nothing else; for `bare_heap`, the heap's two ends and nothing else. (The first objects also named LLVM's own functions, `llvm.ctlz.i64` and eleven more, as symbols for the linker to find; nothing used them, so every link passed, and `nm` showed it) |
| One `sub` made an `add` in the module, and one value 5 made 6 | each one line of the object's bytes changed: the gate's comparison can fail |

A `cancho test` run built natively was also the first time section 13.2 was run whole: the searches of `std.bytes` and the
ciphers' tests pass with no vector and no cipher instruction in the program.

### 13.5 Not done

- **A section for each function**, and so a link that drops what is not reached: N7.
- **Debug information, and unwinding tables.** A debugger sees symbols and no lines.
- **Vectors and the cipher instructions** in the generator (section 13.2 goes round them).
- **The linker.** It is still the system's; N7 writes an image for no system without one.
- macOS and x86-64 (N15, N14): `--backend native` refuses them by the rule.

