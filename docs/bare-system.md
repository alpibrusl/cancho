# A system lent for a block: programs written for a system, built into one that has none

> **Status: design, not built.** Decided in #857 (option A, 2026-11-26). Nothing here is implemented; the
> measurements are of what exists today, and every "would" is a claim to check when it is built. This
> document is the first step the repository asks for: design before code.

## 1. The asker

[cancho-os](https://github.com/alpibrusl/cancho-os) is a kernel in cancho for `--target aarch64-none-elf`
([`freestanding.md`](freestanding.md)). It wants to carry other people's cancho programs inside it,
unchanged: first the tools of [cancho-tools](https://github.com/alpibrusl/cancho-tools) as what an agent
calls on the board, later this compiler (cancho-os#35).

Those programs are written for a system. `hash` of cancho-tools, built for the bare target, stops at:

```
contract/cli.cho:357:31: error: The program reaches something this target cannot do ...
        if c.offset > 0 && c.at < arg_count(args) {
```

which is [`freestanding.md`](freestanding.md) §5 doing its job. Its entry point is

```
fn run[&h, &g, &f, &i](heap: &!h Heap, args: &g Args, fs: &f Fs(""), io: &!i Io)
    -> [heap, args, fs_read(""), dir_read, file_read, io_write, err_write] int
```

Since #839 the heap is there. What is missing is `Args`, `Fs` and `Io` that do something.

## 2. What the tools ask of a system, measured

The nine tools (`seek list peek jsonq tally hash write replace move`), cancho-tools `main`, 2026-11-26.
Their rows beyond `heap`:

| tools | effects |
|---|---|
| `hash`, `peek`, `seek` | `args`, `fs_read("")`, `dir_read`, `file_read`, `io_write`, `err_write` |
| `list` | `args`, `fs_read("")`, `dir_read`, `io_write`, `err_write` |
| `tally`, `jsonq` | the first row and `io_read` |
| `write` | the first row and `dir_write`, `file_write`, `io_read` |
| `replace`, `move` | the first row and `dir_write`, `file_write` |

No network, no clock, no process, no thread, no foreign code.

**The runtime already has a narrow waist.** A hosted builtin does not call the C library where it is
written; it calls a function of `compiler/runtime.ll` (or `runtime/dirs_*.ll`), and that one calls the C
library. Each tool was built to LLVM text for the host and the calls counted. Beyond what every hosted
module carries, the ones a tool's own code reaches are:

| what | the runtime's function, as it is defined today |
|---|---|
| arguments | `ptr @cancho_arg(i64 %n)`, and the count in a global the C `main` stores |
| open a file | `i64 @cancho_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, i64 %mode)` |
| read, write | `i64 @cancho_fd_read(i64 %fd, ptr %p, i64 %n)`, `@cancho_fd_write` the same |
| at a place | `i64 @cancho_fd_pread(i64 %fd, ptr %p, i64 %n, i64 %at)`, `@cancho_fd_pwrite` |
| the rest of a file | `@cancho_fd_close`, `@cancho_fd_size`, `@cancho_fd_sync`, `@cancho_fd_truncate`, `@cancho_fd_lock`, each `i64 (i64 %fd ...)` |
| directories | `@cancho_dir_open`, `_step`, `_list`, `_next`, `_list_close`, `_stat`, `_rename`, `_remove`, `_sync`: `void (..., ptr %out)` |
| the streams | `i64 @cancho_write_bytes(ptr %p, i64 %n)`, `putchar`, `getchar`, `@cancho_flush_out`, and the one for the standard error |
| a path held to its prefix | `ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)`, `i1 @cancho_path_covers(...)` |

How many of the nine reach each: `cancho_fd_read` 8; `getchar`, `cancho_dir_rename`, `cancho_dir_sync`,
`cancho_fd_lock` 3; `cancho_dir_remove`, `cancho_dir_stat`, `cancho_fd_sync`, `cancho_fd_write` 2;
`cancho_dir_list`, `_next`, `_list_close`, `cancho_fd_pread`, `cancho_fd_size` 1; the others in the table
all nine. (The count is of the whole hosted module, which is not pruned of the runtime's own functions,
so "all nine" is an upper bound for those rows; §9 asks for the exact list as its first step.)

So the need is about twenty-five functions with plain signatures: integers, pointers with lengths, one
`out` pointer for the directory ones. **A system, to these programs, is whoever defines those.**

## 3. Why defining them is not enough

On a hosted target they are defined over the C library, which is ambient: its descriptor table, buffers
and `errno` are global state, reachable from anywhere.

A kernel in cancho has nothing ambient, on purpose. cancho-os's disk driver is a function that is *lent*
an `Mmio` for the card's registers and the kernel's state; `cancho authority` lists exactly those
addresses; a function that was not lent them cannot touch the card. When `file_read` is called six calls
deep inside a tool, the tool holds an `Fs`, not an `Mmio`, and must not be given one.

So the functions of §2, on the bare target, need two things a hosted runtime gets for free: **the
kernel's state, and the capabilities for its devices.** They can only come from the kernel, at the place
where the kernel decides to run the tool.

## 4. The form

For the bare target only, a statement:

```
system kernel_files(k, vio, dma) as (args, fs, io) in {
    status = hash.run(heap, args, fs, io);
}
```

- **`kernel_files`** is a module of the program. It must define one function for each operation of §2,
  by fixed names (`open`, `read`, `write`, `pread`, `pwrite`, `close`, `size`, `sync`, `truncate`,
  `lock`, the directory ones, `arg`, `arg_count`, `out`, `err`, `in`, `flush`). Their parameters are the
  values lent, in order, and then the operation's own.
- **`(k, vio, dma)`** are lent to the system for the block, as `borrow ... in { }` lends: they cannot be
  used in the block by anything else, and are back after it.
- **`as (args, fs, io)`** names three capabilities, an `Args`, an `Fs("")` and an `Io`, that exist in the
  block and nowhere else.
- Inside the block, and only while it runs, a hosted builtin reached through those three is answered by
  the module's function, called with what was lent.

It is a block and not a builtin with a function-value argument because of what a function value is here
([`function-values.md`](function-values.md)): captureless and non-generic. The functions of the system
are generic in the regions of what they are lent and mention the `Mmio` bounds of the kernel in their
types; a table of them cannot be one type. Named by module, each is an ordinary generic function called
directly, and is checked like any call.

**One system at a time.** A `system` block inside a `system` block is refused where it is written
(`nested-system`), at first. The bare target has one thread.

## 5. What it compiles to

Nothing changes at a builtin's call site: `file_read` still compiles to `call i64 @cancho_fd_read(...)`.
What changes is who defines `@cancho_fd_read` in a module for target 3.

- If the program has no `system` block, nobody does, and the builtin is refused as today.
- If it has one, the driver writes, for each operation, a small function with the runtime's exact
  signature (§2) that loads the lent values from one global of the module and calls the program's
  function. The block stores the lent values in that global on entry and clears it on exit. A call that
  arrives while it is clear traps.

That is one global for the bare runtime, added to the audited list of
[`unsafe-and-global-state.md`](unsafe-and-global-state.md) §4, and it holds borrowed values only between
the block's braces, which is the lifetime the checker already gave them.

`cancho_checked_path` and `cancho_path_covers` are string code with no system under it. They are compiled
for target 3 as they are, so that an `Fs` narrowed by a tool (`--root`) confines it by the same code that
confines it on Linux. (`bare.ll` gains `memcmp` and `strlen`, which they call.)

**Handles are the system's own.** `open` answers a number; what it means is the module's business (an
index into a table in the kernel's state). The runtime's convention for failure (how an errno comes back)
is taken from `runtime.ll` as it is, and is written down in §9's first step, not guessed here.

## 6. Effects and authority

- The module's functions have rows of their own: for cancho-os, the `mmio(...)` of its card. The block's
  row is those rows and the body's.
- The body's hosted effects (`args`, `fs_read`, `fs_write`, `file_read`, `file_write`, `dir_read`,
  `dir_write`, `io_read`, `io_write`, `err_write`) are **discharged** by the block: they do not appear in
  the row of the function that contains it. This is the one place a row stops being the union of what is
  called, and it is sound for the reason a handler is: within the block those effects are *implemented*
  by functions whose own effects were just added.
- So `cancho authority` of the image still prints addresses and registers and nothing else, which is
  what it prints today; and `authority` of a tool on its own still prints `fs_read("")` and the rest,
  which keeps its manifest (`tools.toml` in cancho-tools) true on both kinds of target.
- A tool holds `Args`, `Fs`, `Io`. It holds no `Mmio`. That a function not lent a device cannot reach it
  stays true of the tool's code: it reaches the card only by asking the system, which is what a program
  does to a kernel.

## 7. What is static and what is not

Static: the module has every function, with the right types; the block is not nested; the three
capabilities do not leave the block (they are in a region of the block's own, like a borrow's).

Not static, and stated: a hosted builtin can be reached on the bare target with a capability that did
not come from a block, since `split(world)` still hands a bare program an `Fs`, an `Args` and an `Io`.
Today that is refused if reached (§5 of `freestanding.md`). With a `system` block somewhere in the
program the refusal can no longer be "this builtin is reached", because it is reached, rightly, from
inside the block. Two ways:

- (a) **Trap at run time** when the global is clear. Simple; a mistake is found the first time it runs.
- (b) **Make the capabilities of `split(world)` unusable on this target**: `Args`, `Fs` and `Io` from the
  world are a different, uninhabited flavour there, so a hosted builtin type-checks only with a block's.
  Static; costs a rule in the checker and words in `freestanding.md`.

Proposed: (a) first, (b) before the feature is called done. The owner may prefer (b) at once.

## 8. What it does not do

- **It is not isolation.** The tool's code runs in the kernel's address space with the kernel's
  privileges. What holds it is the type system: it was compiled by this compiler, in the same image.
  Loading a program that was compiled elsewhere is cancho-os's open question and is not answered here;
  this block is meant to be the shape that answer plugs into (the module is the system-call table).
- **No clock, network, process or thread** for the code in the block: those builtins stay refused.
- **One system at a time, one thread.**
- **A tool's `main` is not used.** The kernel calls the tool's `run`; each tool's `main` only splits the
  world and calls it. Two tools in one image need their top-level names not to collide, which is the
  ordinary rule for modules.

## 9. Steps, each with what shows it works

1. **The exact list.** A census, like `scripts/ll-census.cho` of the native-backend epic, of the runtime
   functions each of the nine tools reaches after the hosted module is pruned of what nothing calls; and
   the runtime's convention for each (arguments, result, how an error is told). A table in this document.
   *Shows:* the table, and a test that fails when a tool starts to reach one more.
2. **Streams and arguments.** `out`, `err`, `in`, `flush`, `arg`, `arg_count`. The form, the checker's
   rules for it (§4, §6), the driver's functions (§5), the trap.
   *Shows:* `tests/probes/bare_system.cho`, a program for target 3 whose system writes into a slice,
   builds into a module that calls nothing it does not define; the same source built hosted with the same
   module answers the same bytes; cancho-os prints `jsonq`'s answer to a document given on its console.
3. **Files and directories, read.** `open`, `read`, `pread`, `size`, `close`, the directory ones, with
   `cancho_checked_path` on the bare target.
   *Shows:* a system whose files are slices in memory; `hash`, `peek`, `seek`, `list`, `tally` built into
   cancho-os and run over SSH against its FAT card, their answers compared with the same tools run on
   another machine over the same files.
4. **Files and directories, written.** `write`, `pwrite`, `sync`, `truncate`, `lock`, rename, remove.
   *Shows:* `write`, `replace`, `move` in cancho-os, each change a version and a line in its record;
   cancho-tools' own conformance suite, as far as it can be pointed at a remote board.
5. **§7 (b)**, if (a) was built first.

## 10. Open, for the owner

- The word: `system ... as ... in { }`, or another.
- §7: (a) then (b), or (b) at once.
- Whether the names of the module's functions are fixed by this document or by a module of the library
  (`std.system`) that a program's module must mirror, which would let the checker's message point at one
  place.
