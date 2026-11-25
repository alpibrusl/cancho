# The processor's own registers: `Cpu`

> **Status: built** (edition 14: the capability `Cpu`, the `cpu` field of `Split`, three builtins, the label `cpu`). The asker is
> [cancho-os](https://github.com/alpibrusl/cancho-os) again. After [`mmio.md`](mmio.md) its drivers reached memory through
> capabilities, and five foreign functions were left, in assembly: read a system register, write one, `wfi`, flush the TLB, and a table
> of addresses. A program that calls foreign code is reported `UNBOUNDED`, so the kernel was. Every number below was produced by the
> command next to it, on darwin-aarch64, at the commit that adds this file.

---

## 1. What was in the way

A system register is not memory: `mrs x0, midr_el1` names the register in the instruction, and there is no address to bound. The
kernel had one foreign function that took a number and compared it against a list, one `mrs` or `msr` a line.

So the timer, the MMU switch and the fault report all went through `Ffi("board")`, the report named five symbols and said that what
they do is the linked library's, and nothing in a signature said that the function reading the clock could not also turn the MMU off.

## 2. What it is now

```
edition 14;

fn uptime[&c](cpu: &c Cpu) -> [cpu("cntfrq_el0"), cpu("cntvct_el0")] int {
    return cpu_read(cpu, "cntvct_el0") / cpu_read(cpu, "cntfrq_el0");
}
```

- `Cpu` is a capability, given whole to `main` by `split` and lent from there. It is not narrowed.
- The register is a **string literal at the call**, and it is the argument of the label: a function's row lists every register and
  instruction it reaches, one `cpu("...")` each.
- Nothing is foreign. `cancho authority` lists the registers, and a program that reaches hardware only this way and through `Mmio` is
  bounded.

## 3. The three builtins

| | signature | the code |
|---|---|---|
| `cpu_read(cpu, "reg")` | `(&Cpu, literal) -> int` | `mrs x, reg` |
| `cpu_write(cpu, "reg", v)` | `(&!Cpu, literal, int) -> int` (0) | `msr reg, x` and `isb` |
| `cpu_do(cpu, "what")` | `(&!Cpu, literal) -> int` (0) | `wfi`; `isb`; or, for `tlbi_vmalle1`, `tlbi vmalle1`, `dsb nsh`, `isb` |

- **The name is one of a table.** It is written into the assembly of the instruction, so it must be a register: one of the 171
  `compiler/bare.cho` lists, in three lists, read and written (113), read only (50) and written only (8). Anything but a string
  literal, a name that is not in the table (`"midr_el9"`), a write of a read-only register (`"midr_el1"`) and a read of a
  write-only one are refused (`capability-misused`), where they are written. *(Corrected: until this table the name was held only
  to its letters, and whether it existed was the assembler's word, section 7.)*
- **`cpu_do` performs one of three instructions**, none of which leaves the function. There is no `eret`, no `svc`, no `brk`.
- **A write is followed by `isb`**, so that it holds for the next statement. A sequence that wants several writes and one barrier
  pays for the others; the kernel has one such sequence, of four writes, run once.
- **On any target but a bare AArch64 the access traps**, and none of the assembly is in the module. `Split` has the field on every
  target, because its shape belongs to the edition; a hosted program releases it.

## 4. The row is exact

A call performs `cpu("<the name>")`. The row is held to it in both directions, by the same check `mmio` has
([`mmio.md`](mmio.md) section 5, `effects.mmio_inexact` with the label's number): every name a body reached, itself or by the row of a
function it calls, is in its row, and every `cpu("...")` of the row was performed. A function that owns a `World` or the `Cpu` needs no
label.

This is where the bound is. The capability is one for the whole processor, and what holds a function to "the timer and nothing
else" is its row: `fn tick(cpu: &!Cpu) -> [cpu("cntv_cval_el0")]` cannot write `sctlr_el1`, and a caller reading that signature knows
it.

**Why not narrow it, as `Mmio` is?** It could be: a `Cpu("cntfrq_el0,cntvct_el0")` whose type is the set. Then the set would be in the
type *and* in the row, and every function between `main` and a driver would carry the union twice. The row already says it, exactly;
the type would add that two drivers cannot share a register, which for memory is the point (a device has one driver) and for system
registers is not what a kernel wants (the counter is read everywhere).

## 5. Edition 14

`Cpu`, the sixteenth field of `Split` and the three builtins are absent before edition 14 ([`editions.md`](editions.md) section 5:
an addition). An older file compiles unchanged.

## 6. Measured on the kernel

cancho-os at the commit that moves it to `Cpu`, against the one before, built with this compiler:

| | before | after |
|---|---:|---:|
| foreign functions the kernel calls (`nm -u build/kernel.o`, less the two symbols of the heap) | 5 | **0** |
| `cancho authority` | `UNBOUNDED`, 5 symbols under "unbounded by" | **bounded**; "never touches ... foreign code" |
| system registers and instructions named in the report | 0 | 15 |
| the image (`kernel8.img`) | 65,608 bytes | 64,080 bytes |

The table of addresses went a different way: the two the kernel needs from the link are two words at a fixed place in its image,
read through the `Mmio` it already holds over its code, and the note of which vector a fault took is one word of memory with a
capability of its own. The fault report reads `esr_el1`, `elr_el1` and `far_el1` itself, so the assembly no longer saves them.

The kernel boots in QEMU as both boards and every command answers as before: the timer ticks (`cntv_*`), `uptime` counts
(`cntvct_el0`), the MMU comes on (`mair_el1`, `tcr_el1`, `ttbr0_el1`, `tlbi_vmalle1`, `sctlr_el1`) and the three faults are reported
with the registers read through the capability.

## 7. What this does not do

- **The table is not the architecture.** *(Corrected: this said that whether a register exists was the assembler's word, and that a
  wrong name failed in `clang` with no rule tag. The table of section 3 closes that.)* It has the registers of a plain ARMv8-A core
  at the three exception levels, the generic timer, the performance monitors and the system-register interface of the interrupt
  controller: 171 names. A register that is real and not listed (pointer authentication keys, SVE, the debug breakpoints, anything
  after ARMv8.0) is refused like one that does not exist, and is added by a line in `compiler/bare.cho`; a test builds a read or a
  write of every name listed, so a name clang does not have cannot be added.
- **One architecture.** The names are AArch64's. Another target gets the trap until it has its own instructions.
- **Nothing proves a value is a sane one.** `cpu_write(cpu, "sctlr_el1", 0)` is the MMU off, and the row says exactly that it may.
- **Not every instruction.** `cpu_do` knows three. Exception return, the system-instruction forms of cache maintenance and the other
  `tlbi` variants are not here; no program has asked.
- **Boot and exception entry are still assembly.** They run before `main`, or instead of it, on no stack. cancho-os keeps a page of
  it; that it calls nothing in the other direction is what makes the kernel bounded.

## 8. Where it is

| | |
|---|---|
| the tables: type 68, `Split` 69, builtins 194 to 196 | `compiler/tables.cho`, `builtins.cho`, `signatures.cho` |
| the name, and the label | `atoms.cpu_builtin`, `bare.register`, `effects.note_cpu` |
| the code | `wordcode.cpu` |
| the row | `effects.mmio_inexact`, label 9 |
| tests | `tests/commands/cpu.sh`, `tests/probes/cpu.cho`, `cpu_registers.cho`, ten `tests/reject/cpu_*.cho` |
