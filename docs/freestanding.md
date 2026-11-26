# A program with no system under it

> **Status: built** (`--target aarch64-none-elf`, `--emit ll`, `compiler/runtime/bare.ll`). The asker is
> [cancho-os](https://github.com/alpibrusl/cancho-os), a kernel for the Raspberry Pi written in cancho: it built with a wrapper around
> `clang` that stole the module, a `sed` over the trap instruction, and 27 lines of C standing in for a libc. With this slice it builds
> with one command and has no C. Every number below was produced by the command next to it, on darwin-aarch64, at the commit that adds
> this file.

---

## 1. What was in the way

A kernel is a cancho program like another: `main` takes the `World`, and the hardware is a foreign library, so `cancho authority`
lists what it touches. Three things in the compiler assumed a system all the same.

| | what | how cancho-os got round it |
|---|---|---|
| 1 | `--target` took the triple of the machine the compiler runs on and nothing else | `--clang WRAPPER`, where the wrapper copied the `.ll` it was handed and the build compiled that copy for `aarch64-none-elf` |
| 2 | the trap follows the machine the compiler runs on: `ud2` on x86-64 | `sed` over the module (the module is otherwise the same on every host) |
| 3 | the module carries the hosted runtime, and a region takes its chunks from `malloc` through three `pthread` calls | `--gc-sections` at the link, and `malloc`, `free`, `calloc`, `memset`, `memcpy`, `memmove`, `pthread_key_create`, `pthread_getspecific`, `pthread_setspecific` and `abort` written in C |

Measured on the kernel (`src/kernel.cho` of cancho-os at `c9af55f`, 490 lines), built with the compiler before this slice and its
module compiled for `aarch64-none-elf`: `nm -u` on the object lists **51** symbols. 11 are the kernel's own foreign functions and
2 would be the heap; **38 are a libc** (`fopen`, `posix_spawn`, `waitpid`, ...), none of them reached by `main`. With this slice the
same command lists **13**: the 11 and the 2.

## 2. What is built

```sh
cancho build kernel.cho --target aarch64-none-elf --emit obj -o kernel.o   # an ELF object for AArch64, to be linked by the program's own script
cancho build kernel.cho --target aarch64-none-elf --emit ll  -o kernel.ll  # the module, and no clang
cancho build program.cho --emit ll                                         # the module for this machine: `--emit ll` is not tied to the target
```

- **`--target aarch64-none-elf`** is target 3 of the driver (0 Linux on x86-64, 1 Darwin, 2 Linux on aarch64). It is accepted by
  `build` and `check`; `run` refuses it, as it refuses any triple that is not the machine's, since there is nothing to run it on.
- **It makes an object or a module, never an executable.** Where the image is loaded, what comes first in it and where the heap is
  are the program's decisions, written in its link script. `build` without `--emit obj` or `--emit ll` is a usage error (status 2)
  that says so.
- **`--emit ll`** writes the module to `-o` (default: the stem and `.ll`) and does not run clang. It exists for every target: a tool
  that wants the module no longer needs a wrapper.
- **The trap is the target's**, as it already was for the two aarch64 targets: `udf #0xc11f`.
- **clang is called with** `--target=aarch64-none-elf -mstrict-align -ffunction-sections -fdata-sections`. See §3.

## 3. `-mstrict-align`, and how it was found

Before a kernel turns its MMU on, the processor treats every address as device memory, and an access that is not aligned to its own
size is a fault. clang at `-O2` joins two 8-byte stores into one 16-byte store wherever it can. The first region the kernel opened
(`alloc_slice[r](16, 0)`) was filled that way, at an address aligned to 8: a data abort with fault status `0x21` (alignment), before
the first byte reached the UART. `-mstrict-align` is what a kernel is built with for this reason, and it costs nothing a kernel
measures. The two section flags let `--gc-sections` drop a function the program does not reach.

## 4. The runtime: `compiler/runtime/bare.ll`

For target 3 the driver writes **none** of the hosted support (`runtime.ll`, `os_*.ll`: files, processes, `errno`), not the
`cancho_write_bytes` that calls `fwrite`, not `region_*.ll`, and none of the modules a hosted program gets for what it uses
(directories, sockets, signals, processes, serial ports, the camera). *(Corrected: this list ended with "the hardware AES". Its
functions are written for this target since §8, as ones that call nothing.)* *(Corrected, the day it was written: the
last group was first left in. A program built with `--std` got the directory and socket support because the library mentions
them, 45 undefined functions of a libc, though the program called none. cancho-os found it on its first `import std.math`; the
probe of the test is built with `--std` since.)* It writes the two globals the C `main` stores its arguments in, and
`bare.ll`:

- **Regions.** A chunk is 64 KiB, as everywhere. `cancho_region_take` answers one that was given back, or the next of the memory
  between two symbols the link provides, `__cancho_heap_start` and `__cancho_heap_end`; past the end it traps. `cancho_region_give`
  clears what was used and keeps the chunk in a list threaded through the chunks themselves. There is one thread, so no key and no
  per-thread stack: `cancho_region_init` is empty. (The C stub this replaces never gave a chunk back: a shell that opens a region
  per command would have walked through the RAM.)
- **`memset`, `memcpy`, `memmove`, `memchr`.** LLVM calls the first three for a fill or a copy it does not write in line, on every
  target, whether or not the program names them; `memchr` is what the byte search calls. Each is a loop, marked `"no-builtins"`:
  without that the loop of `memset` is recognised as a fill and compiled into a call of `memset`.
- **`calloc`, for statics only.** A `static` is made once, before `main`, in zeroed memory that is never given back; `std.math`
  has some. So `calloc` is defined: the next bytes of the same memory, cleared. `malloc` and `free` are not: there is no heap (§7).
- **The arena's own trap says nothing.** On a hosted target a region that is full writes a sentence on the standard error before it
  traps. There is no standard error here; it traps.

The link script of the program names the heap:

```
__cancho_heap_start = ALIGN(4096);
__cancho_heap_end = 0x3c000000;
```

## 5. What a program for this target may use

What is checked is the language: ownership, regions, effects, bounds, overflow. All of it compiles for target 3, because none of it
calls a system.

What needs a system does not, and **is refused by a rule** (`unsupported-on-target`), where the call is written: a builtin whose
code calls the hosted runtime (`putchar`, `write_bytes`, the files, the sockets, the processes, the clock, `box` and the rest of the
heap, `spawn`). `bare.needs_system` says which, by the
builtin's number: of the 197, 57 do not need one (the capabilities, the slices, the bits, the arithmetic, the floats, `Mmio`, `Cpu`,
and the ten hardware cipher and hash builtins). *(Corrected: this said 47, and listed "the ciphers and hashes, whose support is not
written for this target" among the refused. That refused `std.crypto`'s SHA-256, whose software path needs nothing; §8 is what was
measured and what replaced it.)*

- **Only a call the program reaches is refused.** The checker writes every function of every module the program imports, so a
  library with one function that prints would refuse every program that imports it. Instead the call leaves a line in the function
  being written (`; needs-system FROM TO`, the place in the source); `prune.cho` then leaves out the functions nothing calls, lines
  and all, and a line still in the module is the refusal. `build` and `check` both say it, since `check` writes the code too.
- **Measured.** Every single-file program of `tests/probes`, `tests/accept` and `examples` (158), built with
  `--std --target aarch64-none-elf --emit obj` by the compiler at this commit: 138 refused by the rule, 14 built with no undefined
  function, 3 built and call a foreign function of their own, 3 do not build for this machine either. None reaches clang with a name
  it does not define. Before the rule the 138 were clang's (`use of undefined value '@cancho_open'`, status 3).

*(Corrected: this section said the rule was not raised, and that the failure was a located error from the wrong tool.)*

The entry is still `main(world)`, called by the C `main(argc, argv)` the driver writes; a boot stub calls `main(0, 0)`. The
`World` is whole: `split` hands out an `Io` and an `Fs` that nothing can be done with. A `World` that says what the machine has
(no `Fs`, a hardware root) is the capability design of MMIO and is not this slice (§7).

## 6. The test

`tests/commands/freestanding.sh` (six checks; it said four, and had five) builds `tests/probes/freestanding.cho`, a region, a slice and one foreign function:

- the module for target 3, built with `--std` and calling `math.cos`, calls and does not define exactly `probe_out`; it has one `udf #0xc11f`, the external heap symbol and its
  own `memset`;
- **the same program for this machine calls `malloc`**, so the first check is one that can fail: it reads a list that is a libc
  when the runtime is the hosted one;
- the object is an ELF (magic `7f454c46`) for machine 183 (`b700` at offset 18), read with `od`, since the `nm` of a Linux x86-64
  machine does not read an AArch64 object;
- `build` without `--emit`, and `run`, answer 2.

- a builtin that needs a system is refused only if the program reaches it (§5), and SHA-256 with HMAC builds and calls nothing (§8).

No check runs the program: the gate has no AArch64 machine without a system. The program that runs is the asker. cancho-os builds
with this target in its own CI and boots in `qemu-system-aarch64 -M raspi3b`, where a scripted shell session checks the timer, the
MMU and a trap; on the day this was written it also booted on `-M raspi4b`, from the same image.

## 7. What this does not do

- **A foreign function is not looked at.** *(Corrected: this item said there was no rule for a hosted builtin on this target; there is, §5.)* An `extern fn` is the program's own, and whether the link has it is the link's.
- **No hardware SHA-256 or AES.** `hw_sha256()` and `hw_aes_gcm()` answer no on this target whatever the processor has (§8), so
  `std.crypto` and `std.gcm` run in software. Asking the processor is a register read, which is `Cpu`'s to do; no program has
  asked for the speed.
- **No heap.** `box`, `box_slice` and `std.buffer` call `malloc`. A kernel that wants them needs an allocator over the same memory
  as the regions; no program has asked.
- **No interrupt entry.** *(Corrected again: system registers are built too, [`cpu.md`](cpu.md), edition 14, and cancho-os calls no
  foreign function.)* What follows is the earlier state. *(Corrected: memory by address is built since, [`mmio.md`](mmio.md), edition 13: a
  capability whose type names the address ranges, so that `authority` lists what a driver touches.)* cancho-os reached the hardware
  through eleven foreign functions of one or two instructions, written in assembly; the six that took an address are gone and five
  remain (the system registers, `wfi`, the TLB).
- **One triple.** `x86_64-unknown-uefi` (a second asker: the same kernel on a laptop) needs a PE/COFF object and another trap; the
  driver's target number is the place for it.
- **No `--entry`.** The C `main` is what a boot stub calls. Naming another entry would save two instructions and add an option.
- **The seed is not changed.** A compiler built from the seed alone (`scripts/seed-build.sh`) does not have the target until the
  next reseed; `scripts/build-standalone.sh` builds one that does.

## 8. SHA-256, HMAC and AES-GCM: the hardware answers no

*(Added after the rule of §5, by the change that corrected it. Every number here is from the command next to it, on linux-x86_64
with clang 18.1.3: "before" is the compiler at `e9cbde8`, "after" the compiler of this change.)*

**What was refused.** `std.crypto` asks `hw_sha256()` before it compresses, and takes the processor's instructions
(`sha256_blocks`, `sha256_block_words`) or the software rounds (`docs/sha256-builtin.md` §5). §5 counted all three builtins as
needing a system, so the question itself was the refusal, and the software path behind it could not be reached:

```sh
cancho build tests/probes/bare_sha256.cho --std --target aarch64-none-elf --emit obj -o sha.o
# before: <std>/crypto.cho:268:8: error: The program reaches something this target cannot do   (`if hw_sha256() {`)
# after:  sha.o, and `llvm-nm -u sha.o` lists __cancho_heap_end, __cancho_heap_start, probe_out
```

The asker is [cancho-ssh](https://github.com/alpibrusl/cancho-ssh), a server for inside cancho-os, which needs SHA-256 for
`curve25519-sha256` and carried a copy of the software path (`src/sha256.cho`) to get round this. `std.hmac` and `std.hkdf` over
SHA-256 were refused at the same line.

**The same question for AES**, measured with one program per entry point, built the same way:

| the program calls | before | after |
|---|---|---|
| `crypto.sha256` | refused, `std/crypto.cho:268` | builds |
| `hmac.sha256` | refused, `std/crypto.cho:268` | builds |
| `crypto.sha512` | builds | builds |
| `std.aes`: `expand`, `encrypt_block`, `ctr32`, `round_keys` | builds | builds |
| `gcm.seal` | refused, `std/gcm.cho:335` (`if hw_aes_gcm() {`) | builds |
| `gcm.prepare`, then `gcm.seal_software` | refused, `std/gcm.cho:335` | builds |
| the four `gcm.*_software`, with no `prepare` | builds | builds |

`std.aes` never asks the processor, so it always built. `std.gcm`'s `*_software` entry points build too, but that was no way
round: the only thing that makes the key they take is `gcm.prepare`, and it asks `hw_aes_gcm()`. AES-GCM was refused in practice
exactly as SHA-256 was, and one change answers both. Each object in the "after" column has the same three undefined symbols as
above, and none has an instruction of the extensions (`llvm-objdump -d | grep -ci 'sha256h\|aese\|pmull'`: 0).

**What was chosen: the hardware answers no.** The ten builtins (`hw_aes_gcm`, `aes_encrypt_block`, `aes_ctr32`, `ghash_powers`,
`ghash_update`, `gcm_tag`, `gcm_tag_diff`, `hw_sha256`, `sha256_blocks`, `sha256_block_words`) no longer need a system
(`bare.needs_system`), and their functions are written for target 3 as they are for the hosted ones: `compiler/runtime/crypto_bare.ll`,
in the module only if the program names one. There `cancho_hw_sha256` and `cancho_hw_aes_gcm` are `ret i8 0`, and the other eight
trap. Nothing in the compiler's own code changes, and no branch is pruned by a new analysis: the branch on a constant is clang's
to remove, and it does: in the four objects of the table that were refused, `llvm-nm` finds no `cancho_hw_*`, `cancho_sha256_blocks`,
`cancho_aes_ctr32` or `cancho_gcm_tag` left.

This is the contract the builtins already had, not a new one. `docs/sha256-builtin.md` §3: "both trap on a target or a backend
without the instructions; a program reaches them only after `hw_sha256()` said true", and `docs/crypto-builtins.md` §5 says the
same of AES. Cranelift answered false and trapped; so does this target. A program that calls `sha256_blocks` without asking is
wrong on every x86-64 machine without SHA-NI as well, and here it stops at the language's trap and not at an instruction the
machine may not have.

**What was not chosen: the instructions on this target.** Three reasons, the first enough alone.

1. *Asking has no answer the compiler can write.* A hosted target asks the system (`getauxval`, `sysctlbyname`). With none, the
   answer is in `ID_AA64ISAR0_EL1`, which is read at EL1 or above and is undefined at EL0, and the compiler does not know where
   the program runs. Reading a system register is also what edition 14 gave a capability and a label for (`docs/cpu.md`):
   `hw_sha256() -> [] bool` takes no `Cpu` and has an empty row, so it cannot be the thing that reads one.
2. *The instructions use the vector registers*, which trap until the program has set `CPACR_EL1.FPEN`. A hash called before that
   would fault where the software path does not.
3. *The asker's machines would say no anyway.* The Raspberry Pi 3 and 4, where cancho-os boots (§6), are Cortex-A53 and
   Cortex-A72 cores without the cryptography extension. This is their documentation's claim and is not measured here: the gate
   has no such machine.

A program that knows its processor and wants the speed can read the register itself through `Cpu`; a builtin for that waits for
a program that asks (§7).

**Measured over the repository's programs**, as §5 was: every single-file program of `tests/probes`, `tests/accept` and `examples`
with `--std --target aarch64-none-elf --emit obj`. Before, 159 programs: 138 refused by the rule, 18 built, 3 do not build for
this machine either. After, 160 (the new probe): 138 refused, 19 built, the same 3. No other program changes: the ones that hash
(`tests/accept/sha256.cho`) print the digest, and are refused at the print.

**The test** is the sixth check of `tests/commands/freestanding.sh`, on `tests/probes/bare_sha256.cho` (`crypto.sha256` and
`hmac.sha256` over 80 bytes, so whole blocks and a partial one): the module calls and does not define exactly `probe_out`,
`cancho_hw_sha256` is `ret i8 0`, `cancho_sha256_blocks` is the trap, no function has `target-features`, and the object is for
machine 183. The same program for this machine has a `cancho_hw_sha256` that asks and functions with `target-features`, so the
checks can fail; and run with the compiler at `e9cbde8` the check fails at the build, with the refusal above. As in §6 nothing
runs the program: that the software path computes SHA-256 is `tests/commands/sha256_builtins.sh` and the vectors, on the hosts.
