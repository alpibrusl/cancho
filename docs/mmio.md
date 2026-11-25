# The memory of the machine, by address: `Mmio`

> **Status: built** (edition 13: the capability `Mmio`, the `mmio` field of `Split`, six builtins, the label `mmio`). The asker is
> [cancho-os](https://github.com/alpibrusl/cancho-os), a kernel written in cancho ([`freestanding.md`](freestanding.md)): its drivers
> reached every register through six foreign functions that took any address, so any function of the kernel could touch any hardware and
> the authority report could only say `UNBOUNDED`. Every number below was produced by the command next to it, on darwin-aarch64, at the
> commit that adds this file.

---

## 1. What was in the way

A device is a few registers at fixed addresses. A program with no system under it reads and writes them itself, and cancho had no way
to say "load from this address": the kernel declared `board_r32(ffi, addr)` and five like it, each one instruction of assembly.

That is foreign code, so the report of the kernel was the report of any program that calls C: the library is named and what it does is
not bounded. More to the point for a kernel that will drive a robot arm, the function that prints a character and the function that
talks to the servos were lent the same `Ffi("board")`, and either could write to the other's device.

## 2. What it is now

```
edition 13;

fn put[&m](uart: &!m Mmio("0x3f201000..0x3f201048"), c: int) -> [mmio("0x3f201000..0x3f201048")] int {
    while mmio_read32(uart, 0x3f201018) & 32 != 0 {
    }
    return mmio_write32(uart, 0x3f201000, c);
}
```

- `Mmio` is a capability like `Fs`: `split` gives `main` one that holds every address, `narrow` cuts it, and a function reaches an
  address only through one it was lent.
- The literal of the type is the **bound**: the addresses it holds. `put` above can reach 72 bytes of the machine and its signature says
  which.
- Its row says the same thing, and `cancho authority` adds the rows up: the report of a kernel is the list of addresses it touches.

## 3. The bound

A bound is one or more ranges separated by commas, each `lo..hi` with `hi` not included. An address is `0x` and up to fifteen lower-case
hexadecimal digits with no zero in front. Each range holds at least one address, and each starts after the one before it ends with at
least one address between them (two ranges that touch are written as one).

So a set of addresses has **one spelling**, and two bounds are the same set exactly when they are the same text. The rules follow from
that:

| | rule | refused as |
|---|---|---|
| a literal that is not a bound, in a type or a `narrow` | checked where it is written | `capability-not-narrowable` |
| `narrow(m, "...")` | every address of the target is one `m` holds, and it is not `m`'s own bound | `capability-not-narrowable` |
| `narrow(m, "a", "b", ...)` (up to eight) | no two share an address: a register has one holder | `capability-not-narrowable` |
| passing an `Mmio` where another is expected | the two bounds are the same text | `type-mismatch` |
| `Mmio("")` written in a signature | not allowed: every address is what `split` gives `main`, which cuts it | `capability-not-narrowable` |

More than one range in a bound is for one driver on several machines: cancho-os boots one image on two boards whose UART is at two
addresses, and its console is `Mmio("0x3f201000..0x3f201048,0xfe201000..0xfe201048")`, one type, one driver.

**The fourth row was new for the compiler when `Mmio` was built.** Unifying two prelude types compared their index and not their
literal. The check a builtin makes is written from the type at the call (section 4), so the type has to be right. Each literal is
therefore kept as the first string token of the program spelled the same (`pass1.first_spelled`), and `types.unify` compares the
tokens.

**For the other capabilities it was a hole, and this section said so; it is closed.** *(Corrected in place: this paragraph read "the
other capabilities do not have it yet", and "only `Mmio` has it" above.)* Measured while building `Mmio`, with an edition-5 program:
`main` narrows `Fs` to `/tmp/cancho-hw/hole/a/b` and lends it to a function declared to take `&Fs("/tmp/cancho-hw/hole/a")`; the
program was accepted, and that function opened `/tmp/cancho-hw/hole/a/other.txt`, outside what it was lent, and read it.
[`capability-literals.md`](capability-literals.md) is the change that closed it for `Fs`, `Net`, `Out`, `Ports`, `Exec`, `Signals`,
`Ffi` and the devices, by the same mechanism, with the programs of the repository as its measurement. One thing differs from `Mmio`:
for the others `("")` in a signature is the capability `split` gives, where `Mmio("")` cannot be written (the fifth row), so the
`Mmio` of `split` is lent to no function before it is narrowed.

## 4. The six builtins

| | signature |
|---|---|
| `mmio_read8`, `mmio_read32`, `mmio_read64` | `(&Mmio, int) -> int` |
| `mmio_write8`, `mmio_write32`, `mmio_write64` | `(&!Mmio, int, int) -> int` (the answer is 0) |

The address is absolute. Each is **one** `load volatile` or `store volatile` of its width, after three checks, each a trap
([`defined-behaviour.md`](defined-behaviour.md)):

1. the address is a multiple of the width (a device register is read whole, and LLVM is told the alignment);
2. the bytes read or written are inside one range of the bound: for each range, `address - lo <= hi - lo - width` as unsigned numbers,
   with the constants worked out when the program is compiled;
3. for a write of 8 or 32 bits, the value is one the width holds. Nothing is cut off silently.

A read takes a shared reference and a write a unique one. A read can change a device (reading a UART's data register takes the byte),
so this is not a claim that reads are harmless: it is the same split every other capability has, and a driver that must be the only one
talking to its device asks for `&!`.

The capability is nothing in the code: no pointer is passed. The check costs a subtract and a compare for each range.

**With a system under the program there is no address to reach**, and an access traps. `Split` has the `mmio` field on every target,
because the shape of `Split` belongs to the edition and not to the target; a hosted program releases it.

## 5. The row is exact

A read or a write performs `mmio("<the bound of the capability it went through>")`. For this label the row is held to the argument, in
both directions, which the checker does not yet do for the other labels that carry one:

- every bound a body reached hardware through, itself or by the row of a function it calls, is an `mmio("...")` of its row spelled the
  same (else `effect-not-declared`);
- every `mmio("...")` of the row is one the body performed (else `effect-declared-not-performed`);
- a function that owns a `World`, or an `Mmio` of that bound, needs no label for it, as for every capability.

So a signature cannot name addresses the function was not lent, and cannot leave out ones it uses.

## 6. Edition 13

`Mmio`, the fifteenth field of `Split`, and the six builtins are absent before edition 13
([`editions.md`](editions.md) section 5: an addition). An older file compiles unchanged, and may use the names for itself.

## 7. Measured on the kernel

cancho-os at the commit that moves it to `Mmio`, against the one before, built with this compiler:

| | before | after |
|---|---:|---:|
| foreign functions the kernel calls (`nm -u build/kernel.o`, less the two symbols of the heap) | 11 | **5** |
| of them, ones that take an address | 6 | **0** |
| accesses to a register or a table | 44, each able to reach any address | 45, each inside a bound in its function's type |
| capabilities the hardware is cut into | 1 (`Ffi("board")`) | 7, sharing no address |
| the image (`kernel8.img`) | 61,512 bytes | 65,608 bytes |

The five that remain are the system registers, `wfi`, the TLB flush and the table of addresses the link script fills: instructions, not
memory, and still foreign, so the report still says `UNBOUNDED` and names them. What it now also says is every address:

```
performs
    ffi("board")    <- unbounded
    mmio("0x0..0x1,0x80000..0x80001")
    mmio("0x200000..0x208000")
    mmio("0x3f200004..0x3f200008,0xfe200004..0xfe200008")
    mmio("0x3f201000..0x3f201048,0xfe201000..0xfe201048")
    mmio("0x3f215004..0x3f21506c,0xfe215004..0xfe21506c")
    mmio("0x40000040..0x40000064,0xff841000..0xff842014")
    mmio("0x80001..0x200000")
```

The kernel boots in QEMU as both boards with every command answering as before, including the two that fault on purpose (a read of
page 0 and a write to the kernel's code now go through a capability that holds exactly those two bytes, so they reach the MMU and not
the bound).

## 8. What this does not do

- **A system register is not memory.** `msr` and `mrs` name the register in the instruction. *(Built since: [`cpu.md`](cpu.md).)*
- **No barrier builtins.** The kernel's one sequence that needs them (turning translation on) is in its assembly with the registers.
- **Nothing proves a bound is the right device.** `0x3f201000` is the UART because a datasheet says so. What is proved is that the
  function touches nothing else.
- **One address space.** A bound is a physical address, or a virtual one where the kernel maps them the same, as cancho-os does.
- **`Mmio` over the program's own memory is the program's risk.** cancho-os holds its translation tables and reads its own code this
  way, at addresses its link script fixes. A bound that covered a region's chunks would let a write go behind the checker's back; the
  language cannot see the link script. The kernel's bounds are listed in one place, in `main`, for that reason.
- *(Corrected in place, twice: this line said the other capabilities' literals were not part of their type, which
  [`capability-literals.md`](capability-literals.md) closed; then that a function passed as a value is not compared with the type it is
  passed for, so that a bound could still be evaded through one, which [`function-signatures.md`](function-signatures.md) closed. A
  function over `Mmio("0x1000..0x2000")` passed for a `fn(&m Mmio("0x1800..0x1900")) -> ...` was accepted before that change and is
  `type-mismatch` since, measured.)*

## 9. Where it is

| | |
|---|---|
| the bound: its grammar, inside, overlap | `compiler/mmio.cho` |
| the tables: type 66, `Split` 67, builtins 188 to 193 | `compiler/tables.cho`, `builtins.cho`, `signatures.cho` (`scripts/builtin_tables.py`) |
| `narrow` | `narrowing.refused_ranges` |
| the literal of a type, and its one token | `pass1.cho` (`first_same`, `first_spelled`), `types.unify` ([`capability-literals.md`](capability-literals.md)) |
| the code | `wordcode.mmio`, `atoms.mmio_builtin` |
| the row | `effects.mmio_inexact` |
| tests | `tests/commands/mmio.sh`, `tests/probes/mmio.cho`, nine `tests/reject/mmio_*.cho` |
