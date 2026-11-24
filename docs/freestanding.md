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
`cancho_write_bytes` that calls `fwrite`, and not `region_*.ll`. It writes the two globals the C `main` stores its arguments in, and
`bare.ll`:

- **Regions.** A chunk is 64 KiB, as everywhere. `cancho_region_take` answers one that was given back, or the next of the memory
  between two symbols the link provides, `__cancho_heap_start` and `__cancho_heap_end`; past the end it traps. `cancho_region_give`
  clears what was used and keeps the chunk in a list threaded through the chunks themselves. There is one thread, so no key and no
  per-thread stack: `cancho_region_init` is empty. (The C stub this replaces never gave a chunk back: a shell that opens a region
  per command would have walked through the RAM.)
- **`memset`, `memcpy`, `memmove`, `memchr`.** LLVM calls the first three for a fill or a copy it does not write in line, on every
  target, whether or not the program names them; `memchr` is what the byte search calls. Each is a loop, marked `"no-builtins"`:
  without that the loop of `memset` is recognised as a fill and compiled into a call of `memset`.
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

What needs a system does not, and **is not refused by a rule yet**: a builtin whose code calls the hosted runtime (`putchar`,
`write_bytes`, the files, the sockets, the processes, the clock, `box` and the rest of the heap, `spawn`) leaves a call to a
function the module does not define, and clang refuses the module (`use of undefined value '@cancho_open'`) with status 3. That is
a located error from the wrong tool. The rule that exists for this, `unsupported-on-target`, is raised today only by
`authority --target wasm32-*`; raising it from `build` for target 3 is the next slice (§7).

The entry is still `main(world)`, called by the C `main(argc, argv)` the driver writes; a boot stub calls `main(0, 0)`. The
`World` is whole: `split` hands out an `Io` and an `Fs` that nothing can be done with. A `World` that says what the machine has
(no `Fs`, a hardware root) is the capability design of MMIO and is not this slice (§7).

## 6. The test

`tests/commands/freestanding.sh` (four checks) builds `tests/probes/freestanding.cho`, a region, a slice and one foreign function:

- the module for target 3 calls, and does not define, exactly `probe_out`; it has one `udf #0xc11f`, the external heap symbol and its
  own `memset`;
- **the same program for this machine calls `malloc`**, so the first check is one that can fail: it reads a list that is a libc
  when the runtime is the hosted one;
- the object is an ELF (magic `7f454c46`) for machine 183 (`b700` at offset 18), read with `od`, since the `nm` of a Linux x86-64
  machine does not read an AArch64 object;
- `build` without `--emit`, and `run`, answer 2.

No check runs the program: the gate has no AArch64 machine without a system. The program that runs is the asker. cancho-os builds
with this target in its own CI and boots in `qemu-system-aarch64 -M raspi3b`, where a scripted shell session checks the timer, the
MMU and a trap; on the day this was written it also booted on `-M raspi4b`, from the same image.

## 7. What this does not do

- **No rule for a hosted builtin on this target** (§5). Until there is one, the failure is clang's.
- **No heap.** `box`, `box_slice` and `std.buffer` call `malloc`. A kernel that wants them needs an allocator over the same memory
  as the regions; no program has asked.
- **No MMIO, no system registers, no interrupt entry.** cancho-os reaches the hardware through eleven foreign functions of one or
  two instructions, written in assembly. Volatile loads and stores bounded by a capability, so that `authority` names the address
  ranges a driver touches, is a design of its own.
- **One triple.** `x86_64-unknown-uefi` (a second asker: the same kernel on a laptop) needs a PE/COFF object and another trap; the
  driver's target number is the place for it.
- **No `--entry`.** The C `main` is what a boot stub calls. Naming another entry would save two instructions and add an option.
- **The seed is not changed.** A compiler built from the seed alone (`scripts/seed-build.sh`) does not have the target until the
  next reseed; `scripts/build-standalone.sh` builds one that does.
