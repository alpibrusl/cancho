# A system lent for a block: programs written for a system, built into one that has none

> **Status: the mechanism is built, for the streams, the arguments, files and directories** ("What is built",
> below). The discharge of effects (§6) is designed in §12 and not built; the block's own capabilities (§4 `as`,
> §7 (b)) are not.
> Decided in #857 (option A, 2026-11-26). Sections 1 to 9 are the design as it was written before any code;
> where what was built differs, "What is built" says so.

## What is built

```
system probe_io(mem, used) in {
    status = tell(io, args);
}
```

- **The statement.** `system`, the name of a module, what is lent in brackets, `in`, a block. `system` is no
  reserved word: it is taken only where two names and a `(` begin a statement. There is no `as (...)` yet: the
  `Io` and the `Args` are the ones `split(world)` gave the program.
- **What is lent** is names, each locked for the block as a unique borrow locks it, eight at most.
- **The module's functions**, by these names, each with what is lent as its first parameters:
  `out(.., p: &[byte]) -> int` and `err` the same (bytes written); `key(..) -> int` (a byte, or -1 at the
  end of the input); `flush(..) -> int` (0, or why not); `arguments(..) -> int`; `argument(.., n: int,
  into: &![byte]) -> int` (its length; 255 bytes at most, and the first sixty-four arguments). They cannot
  be called `arg` and `arg_count`: those are the prelude's. One that the module does not have is a trap
  if it is reached.
- **What they answer**: `write_bytes` and `putchar` (`out`), `write_err` (`err`), `getchar` (`key`),
  `flush_out` (`flush`), `arg_count` and `arg`. On the bare target those builtins are no longer refused in
  a program that has a `system` block; in one that has none they are refused as before.
- **What it compiles to** is §5: the functions the builtins call (`@cancho_write_bytes`, `@putchar`,
  `@write`, `@getchar`, `@cancho_flush_out`, `@cancho_arg`) are written into the module, each loading
  what was lent from a global and calling the module's function; a call with nothing lent traps. How many
  arguments there are is asked once, at the block. `bare.ll` gains `strlen`, which `arg` measures with.
- **Where there is a system under the program** the block is only its body. The module's functions are
  still held to their places, and what their rows say the block performs, so the same source is checked
  the same way on either kind of target.
- **Files that are read** (§9, step 3). Seven more functions of the module, each answering a whole
  number, a handle or a count, or minus the reason it failed (the numbers an `errno` has: 2, no such name):
  `folder(.., path: &[byte])` for `open_dir`; `step(.., dir: int, name: &[byte], which: int)` for a name
  under an open directory (`which` 0 a directory, `dir_enter`; 1 a file to read, `dir_open_read`; 2 and 3
  are the two that would write); `open(.., path: &[byte], mode: int)` for `open_read` (mode 0);
  `read(.., file: int, into: &![byte])`; `read_at(.., file: int, from: int, into: &![byte])` for
  `file_pread`; `size(.., file: int)`; `close(.., handle: int)` for a file or a directory. What they are
  written into is `@cancho_dir_open`, `@cancho_dir_step`, `@cancho_open`, `@cancho_fd_read`,
  `@cancho_fd_pread`, `@cancho_fd_size` and `@cancho_fd_close`.
- **What a directory holds** (the listing `list` needs). Three more functions: `list(.., dir: int)` for
  `dir_list`, a handle of a listing; `next(.., listing: int, into: &![byte])` for `dir_next`, which writes
  the next name into the bytes and answers its length plus 65536 times its kind (1 a file, 2 a directory,
  3 a link, 0 not known), 0 at the end, or minus why not; `stat(.., dir: int, name: &[byte], which: int)`
  for `dir_stat`, asked three times for a name (`which` 0 its kind, 1 its length, 2 when it was changed,
  in seconds since 1970): the first that answers below zero is the failure. `dir_list_close` is `close`
  again. They are written into `@cancho_dir_list`, `@cancho_dir_next`, `@cancho_dir_list_close` and
  `@cancho_dir_stat`. A name given to `dir_stat` is held to being one component first, as `step`'s is.
- **Files that are written** (§9, step 4). Five more functions, and two more answers of `step`:
  `step(.., dir, name, which)` with `which` 2 for `dir_open_new` (a file that must not be there yet) and 3 for
  `dir_open_append` (a file to add to, made if it is not there); `write(.., file: int, bytes: &[byte])` for
  `file_write`, answering how many bytes it took, which may be fewer than it was given; `sync(.., handle: int)`
  for `file_sync` and, with a directory's handle, `dir_sync`; `lock(.., file: int)` for `file_lock`;
  `rename(.., dir: int, from: &[byte], to: &[byte], fresh: int)` for `dir_rename` (`fresh` 0) and
  `dir_rename_new` (`fresh` 1: a name that is taken is refused, 17); `remove(.., dir: int, name: &[byte])` for
  `dir_remove`. They are written into `@cancho_fd_write`, `@cancho_fd_sync`, `@cancho_fd_lock`,
  `@cancho_dir_rename`, `@cancho_dir_remove` and `@cancho_dir_sync`. Both names of a rename, and a name that is
  removed, are held to being one component first. A system with none of these is one that only reads: a
  program that reaches a writing builtin through it traps.
- **What stays the language's and is not the system's to get wrong.** Before the module is asked, a path
  is held to the prefix of the `Fs` it was opened through (`cancho_path_covers`, the same text as on a
  hosted target, now in `bare.ll`): one that is outside it, or has `..` in it, is a trap, as it is on a
  system. A name under a directory is held to being one component (`cancho_sys_component` in `bare.ll`,
  what `dirrt_component` of `dirs.c` checks) and is refused with 22 without the module being asked.
- **Effects are not discharged** (§10, step 1): the function that contains the block declares what its
  body performs, `io_write` and `args` among it, and so do its callers.

Not yet, and refused or stated rather than wrong:

- **One `system` block in a program.** A second is refused ("no code for what is here").
- **No `return` inside the block**, however deep: what was lent would stay lent. Refused the same way.
- **A function that contains the block and is entered again while the block runs** (it calls itself
  through the body) lends again, and the inner block's end switches the system off for the outer one: what
  the outer one then asks traps. Not checked.
- **`read_bytes`** is not answered: only `getchar`.
- **No `file_pwrite`, `file_truncate`, `dir_mkdir`, `dir_rmdir`, `dir_open_new_mode`, `dir_mode`, no links,
  and none of the path-taking `open_write`, `open_append`, `open_new`, `open_rw`, `fs_rename`, `fs_remove`**:
  those builtins are still refused on the bare target. What is served is what `write`, `replace` and `move`
  of cancho-tools reach (§2).
- **A length or a time of 2^63 or more** cannot be told from a failure: `stat` answers one whole number.
- **A trap in a hosted function is a trap of the whole program.** On a system a program that traps ends
  and the system goes on; here the program *is* the kernel. A path with `..` in it is the case a person
  can type: a function that takes a path from outside must look before it opens, as cancho-tools do.

Measured:

- The compiler builds itself with the change (three stages, the second and third equal).
- `tests/commands/freestanding.sh`: `tests/probes/bare_system.cho`, a function that writes its arguments
  to its `Io`, with a system that writes into a slice, builds for `aarch64-none-elf` into a module that
  calls nothing it does not define, defines the seven functions and declares none of them; lends at the
  block and not after it. The same two files for the machine the gate runs on print the command line's
  arguments and answer how many. With the block taken out, the bare build is refused where the builtin
  is written.
- The same test, for files: `tests/probes/bare_system_files.cho` opens a directory, a file under it by
  name and reads it to its end, with a system of one directory and one file kept in the module. It builds
  for `aarch64-none-elf` into a module that calls nothing it does not define and defines the seven
  functions; the two checks are called (twice and once); with no `step` in the module the function is
  there and traps; with a `size` whose parameter is a run of bytes the build is refused.
- The same test, for a directory: `tests/probes/bare_system_list.cho` lists one, asks `dir_stat` of each
  name, of one that is not there and of one that is not a component. It builds for `aarch64-none-elf` into
  a module that calls nothing it does not define and defines the four functions; `stat` is called three
  times, after the check of the name.
- The same test, for what writes: `tests/probes/bare_system_write.cho` holds a lock, makes a file that must not be
  there, writes eleven bytes to a system that takes four at a time, has it kept, and renames it over a name, first
  refusing a name that is taken. It builds for `aarch64-none-elf` into a module that calls nothing it does not
  define and defines the six functions; both names of a rename are checked; with a system that has nothing that
  writes, `@cancho_fd_write` is there and traps.
- **A file written, on a machine.** cancho-os with cancho-tools' `replace` built in as it is written, and a module
  whose `rename` is the kernel's one write of the disk. In `qemu-system-aarch64` as `virt` and as a Raspberry Pi 3
  with an SD card, on FAT16 and FAT32 volumes macOS made: nine commands answer the same bytes as the same tool
  built for the Mac, the four files it changed are the same bytes as the Mac's, and macOS's `fsck_msdos` finds
  nothing wrong with the volumes. Not yet tried on a real board.
- **A directory, on a machine.** cancho-os with a command `names [path]` written against `dir_list`,
  `dir_next` and `dir_stat`. In `qemu-system-aarch64` as `virt` and as a Raspberry Pi 3 with an SD card, on
  FAT16 and FAT32 volumes macOS made with four files of known lengths and times and a directory: each line
  is the length and the time that was put there (FAT's time read as UTC), with a long name whole, at the
  console and over SSH. Not yet tried on a real board.
- **Files, on a machine.** cancho-os with a command `count <path>` whose function takes an `Fs`, opens
  the file (`open_dir` and `dir_open_read` for a name, `open_read` for a path) and adds its bytes up, and
  a module that reads the kernel's FAT volume. In `qemu-system-aarch64` as `virt`, on a FAT32 image another
  system made: a file of 300,000 bytes is counted as 300,000 bytes adding up to 38,241,496, which is what
  Python counts in the same file; the same through two directories and a long name; a file of no bytes, one
  of exactly a cluster's; a name that is not there. Not yet tried on a real board.
- **On a machine.** cancho-os, built with this compiler, with a command whose function takes an `Io` and
  an `Args` and nothing of the kernel, and a module that answers with the shell's output and the words of
  the line. In `qemu-system-aarch64` as a Raspberry Pi 3 and as `virt`: `tell one two three` prints the
  three arguments and its last line, at the console and over SSH with OpenSSH's client; the shell answers
  afterwards. `cancho authority` of that kernel gains `io_write` and no longer says it never touches the
  console or the command line, which is what §10 said step 1 would cost.

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
library. Each tool was built to LLVM text for the host (`--emit ll`), and the functions that can be
reached from its `main` were followed through that text. These are all the runtime's functions any of the
nine reaches, apart from the regions, the heap and the trap, which the bare target has:

| the runtime's function, as it is defined today | reached by |
|---|---|
| `ptr @cancho_arg(i64 %n)`, with the globals `cancho_argc` and `cancho_argv` the C `main` stores | all nine |
| `i64 @cancho_write_bytes(ptr %p, i64 %n)` | all nine |
| `void @cancho_flush_out(ptr %out)` | all nine |
| `getchar` | `jsonq`, `tally`, `write` |
| `ptr @cancho_checked_path(ptr %src, i64 %len, ptr %prefix, i64 %plen)`, `i1 @cancho_path_covers(...)` | all nine |
| `void @cancho_dir_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, ptr %out)` | all nine |
| `void @cancho_dir_step(ptr %dir, ptr %name, i64 %len, i64 %which, ptr %out)` | all nine |
| `i64 @cancho_open(ptr %src, i64 %len, ptr %prefix, i64 %plen, i64 %mode)` | `seek`, `peek`, `jsonq`, `tally`, `hash`, `write` |
| `i64 @cancho_fd_read(i64 %fd, ptr %p, i64 %n)` | all but `list` |
| `i64 @cancho_fd_close(i64 %fd)` | all nine |
| `i64 @cancho_fd_pread(i64 %fd, ptr %p, i64 %n, i64 %at)`, `i64 @cancho_fd_size(i64 %fd)` | `peek` |
| `void @cancho_dir_list(ptr %dir, ptr %out)`, `void @cancho_dir_next(ptr %list, ptr %buf, i64 %room, ptr %out)`, `i64 @cancho_dir_list_close(i64 %stream)` | `list` |
| `void @cancho_dir_stat(ptr %dir, ptr %name, i64 %len, ptr %out)` | `list`, `move` |
| `i64 @cancho_fd_write(i64 %fd, ptr %p, i64 %n)`, `i64 @cancho_fd_sync(i64 %fd)` | `write`, `replace` |
| `i64 @cancho_fd_lock(i64 %fd)` | `write`, `replace`, `move` |
| `void @cancho_dir_rename(ptr %dir, ptr %from, i64 %flen, ptr %to, i64 %tlen, i64 %fresh, ptr %out)`, `void @cancho_dir_sync(ptr %dir, ptr %out)` | `write`, `replace`, `move` |
| `void @cancho_dir_remove(ptr %dir, ptr %name, i64 %len, ptr %out)` | `write`, `replace` |
| `i64 @cancho_errno()`, `ptr @cancho_errno_location()` | all nine |

Twenty-three functions, with plain signatures: integers, pointers with lengths, an `out` pointer for the
directory ones. Under them, on a hosted target, are about forty functions of the C library (`fopen`,
`openat`, `read`, `fstatat`, `renameat`, `flock`, `fsync`, ...). **A system, to these programs, is whoever
defines those twenty-three.** The first nine rows are everything the five tools that only read need.

(Followed, not proven: a function whose address is taken counts as reached, so the list can be a little
long and cannot be short. §9's first step makes the count a program in the repository and a test.)

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

1. **The exact list as a program.** The table of §2 was made by a throwaway script. It becomes a program
   in the repository (with `scripts/ll-census.cho` of the native-backend epic, #841), and the runtime's
   convention for each of the twenty-three is written down beside it: arguments, result, how an error is
   told. *Shows:* the table reproduced, and a test that fails when a tool starts to reach one more.
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
   (Built: `write`, `sync`, `lock`, rename, remove and the two of `step`; `replace` runs in cancho-os.)
   *Shows:* `write`, `replace`, `move` in cancho-os, each change a version and a line in its record;
   cancho-tools' own conformance suite, as far as it can be pointed at a remote board.
5. **§7 (b)**, if (a) was built first.

## 10. What was read in the compiler, and what it means for the order

Read at `ad5bbbb8`, before any code, to find where the work is.

- **How a function is called.** A cancho function is `@lexs_N` in the module, N its node. A parameter that
  is a slice (`&r [byte]`) is two arguments, a pointer and a length; any other reference, a capability's
  among them, is one pointer; a whole number is an `i64`. (A function `out[&s, &b](k: &!s [int], p: &b
  [byte], extra: int)` is `@lexs_28(ptr, i64, ptr, i64, i64)`.) So the functions of §5 are short: load what
  was lent from the global, pass it on with the runtime's own arguments, which already are a pointer and a
  length wherever the operation takes bytes.
- **A statement is known in ten places.** `borrow` is the nearest existing one: the parser (`ast.cho`),
  the kinds (`kinds.cho`), the checker and writer of bodies (`body.cho`, three places), the formatter
  (`printing.cho`), the two listings (`listing.cho`, `jsonprint.cho`), the reader of a stored program
  (`ingest.cho`) and its identity (`identity.cho`). A new statement is all ten.
- **No new reserved word is needed.** `system` then a name then `(` is two names in a row at the start of
  a statement, which is nothing today. The parser can take it there and nowhere else, so no program that
  has a variable called `system` changes meaning.
- **Effects are the hard part, and not where §6 put it.** What a function performs is a set of bits
  (`effects.cho`, `st[54]`), one for each label a builtin performs. A label that carries an argument
  (`fs_read("")`, `mmio("...")`) is *not* in the set: it raises flags that say "this function performed
  something the set does not follow", and what exactly is worked out elsewhere, for `authority`
  (`authority.cho`, `narrowing.cho`). So discharging `fs_read("")` at the block is not "clear some bits":
  the flags it raised are the same flags a `mmio("...")` in the same body raised, and clearing them would
  lose the kernel's own. Doing it soundly means following labels with arguments precisely at the block,
  in both places.

**So the order changes, and §6 and §7 are not the first thing built.**

1. *First, the mechanism and no change to effects.* The block lends, the driver's functions answer, a
   call with nothing lent traps. The hosted effects are **not** discharged: the function that contains
   the block declares them (`io_write`, `args`, `fs_read("")`, ...), and so do its callers up to `main`.
   The capabilities are the ones `split(world)` already gives a bare program (§7 (a)). `cancho authority`
   of the image then lists those labels beside its addresses. That is true (the image does read files,
   its own), and it is less sharp than today's report, which says the image never touches a filesystem.
   Nothing about effects can be wrong in this step, because nothing about effects is touched.
2. *Then the discharge of §6 and the capabilities of the block (§4 `as (...)`, §7 (b)), together*, as one
   change to the effect checker and to `authority`, with tests that can fail: a body that performs a
   `mmio` of its own inside the block must still have it in the row.

Step 1 is enough for cancho-os to run the tools. Step 2 is what makes the report as sharp as it is today.

## 11. Open, for the owner

- The word: `system ... as ... in { }`, or another.
- §7: (a) then (b), or (b) at once.
- Whether the names of the module's functions are fixed by this document or by a module of the library
  (`std.system`) that a program's module must mirror, which would let the checker's message point at one
  place.

## 12. The discharge of §6, designed against the checker

Written before the code, after reading `effects.cho`, `authority.cho`, `narrowing.cho` and the block in
`body.cho` at `f1fa983b`. It is step 2 of §10 as far as effects go. The capabilities of the block's own
(§4 `as (...)`, §7 (b)) are a separate change and are not part of it: the discharge needs to know what the
body performed and which functions the module has, and neither depends on where the body's `Io` came from.

### 12.1 Where, and on which target

The discharge is made at the block, in the function that contains it, and **only where there is no system
under the program** (`--target aarch64-none-elf`). On a hosted target the block is only its body (§"What is
built"): what the body writes goes to the real console, the module is checked and not used, and so nothing
is discharged. The row of the function that contains the block therefore depends on the target:

```
fn lend[&i, &g, &m, &u](io: &!i Io, args: &g Args, mem: &!m [byte], used: &!u [int]) -> [] int {
    var n = 0;
    system probe_io(mem, used) in {
        n = tell(io, args);        // tell: [io_write, args]
    }
    return n;
}
```

is exact for no system, and for a hosted target it is `effect-not-declared` (there the row is
`[io_write, args]`, and with that row it is `effect-declared-not-performed` for no system). A function that
owns the capabilities, as a `main` that owns the `World` does, declares neither and is the same source on
both, which is how `tests/probes/bare_system.cho` stays one file for two targets. A kernel is for one
target, and is checked for it: `cancho check --target aarch64-none-elf`, and `cancho authority --target
aarch64-none-elf`, which this change makes `authority` accept.

A command that checks a program without building it for a target (the ones that list identities) holds it
as a hosted one. That is stated here and not changed.

### 12.2 Which labels

A hosted label is discharged when the module has **every** function that answers a builtin performing it
and served by a lent system (`bare.served`). A label one of whose functions is missing is not discharged:
the body still performs it as far as the row goes, and reaching the builtin traps, as today.

| label | builtins served | the module's functions, all of them |
|---|---|---|
| `io_write` | `putchar`, `write_bytes`, `flush_out` | `out`, `flush` |
| `err_write` | `write_err` | `err` |
| `io_read` | `getchar` | `key` |
| `args` | `arg_count`, `arg` | `arguments`, `argument` |
| `fs_read("...")`, whatever its argument | `open_read`, `open_dir` | `open`, `folder` |
| `file_read` | `file_read`, `file_pread`, `file_size` | `read`, `read_at`, `size` |
| `dir_read` | `dir_enter`, `dir_open_read`, `dir_list`, `dir_next`, `dir_stat` | `step`, `list`, `next`, `stat` |
| `file_write` | `file_write`, `file_sync`, `file_lock` | `write`, `sync`, `lock` |
| `dir_write` | `dir_open_new`, `dir_open_append`, `dir_rename`, `dir_rename_new`, `dir_remove`, `dir_sync` | `step`, `rename`, `remove`, `sync` |

Nothing else is ever discharged: not `heap`, not `fs_write("...")` (no builtin that performs it is served),
not the network, the clock, a process, `ffi`, `mmio`, `cpu`. A builtin that performs a discharged label and
is not served (`read_bytes` for `io_read`, `file_pwrite` for `file_write`, `dir_mkdir` for `dir_write`) is
refused by the build if the program reaches it (`unsupported-on-target`), as today, so no image has one.

`fs_read` is discharged whatever its argument is. The argument is the prefix the path is held to, and that
is checked before the module is asked (`cancho_path_covers`, §"What is built"); the module's `open` and
`folder` answer for every path that passes.

### 12.3 How: the body is counted apart

What a function performs is one set (`st[54]`): a bit for each plain label, and flags that say "a label
with an argument was performed" (bits 61 and 62), "one was not followed" (57, 58), "a function value was
called" (56). The labels with an argument themselves are rows kept beside it (`instances.note_label`: the
function, the label, the literal). §10 said why clearing is wrong: the flag `fs_read("")` raised is the
flag a `mmio("...")` raised.

So nothing is cleared. At the block:

1. The module's functions are held to their places as today, and each one's row is performed by the
   containing function (`effects.perform_call`). This is before the body and outside the discharge: an
   `mmio("...")` of the module's `out`, or an `io_write` the module's own row declares, is in the
   containing function's row.
2. The set so far is put aside, and the body starts from the empty set. How many rows of labels with an
   argument there are is noted.
3. The body is walked as any block is. While it is, every label with an argument that is noted says
   whether it is one the block answers (`fs_read`, when the module has `open` and `folder`) or not; one
   that is not raises a bit of the body's set kept for this (55), and so does anything noted that has no
   row (a foreign function's symbol, a set of signals that was not narrowed).
4. After the body, the containing function's set becomes the set put aside and, of the body's:
   - every plain label that is not discharged;
   - the flags for a function value (56) and for a label that was not followed (57, 58), as they are;
   - the flags for a label with an argument (61, 62) if and only if the body performed one the block does
     not answer (bit 55), or one that was not followed, or called a function value. If none of those, every
     label with an argument the body performed was an `fs_read` the module answers, and the flags are not
     carried over.
   - The rows the body added for `fs_read` are taken out, when it is discharged. A row that was there
     before the block (the function reads a file outside the block too) stays. Every other row the body
     added stays: an `mmio("...")` or a `cpu("...")` the body performs itself, or a callee's row carries,
     is a row of the containing function as before, and is held to its row exactly (`mmio_inexact`).

One more place knows about paths: a call of a function whose row has `fs_read("p")`, from a function that
owns no `Fs` and declares no label with an argument, is refused (`effects.outside_owned`). Inside a block
that discharges `fs_read` that refusal is not made for `fs_read`; it still is for `fs_write` and `exec`.

The block's discharged labels are kept in one slot of the state, with a bit that says the body is being
walked. On a hosted target, and while nothing is built, the slot is never set and none of this runs.

### 12.4 Why the row is still true

A row says: running this function performs no more than these labels. For a hosted label, to perform it
is to reach the runtime's function for a builtin that carries it. In a module for no system that has a
`system` block, each of those functions is defined once, by §5, as: trap if nothing is lent, else call the
module's function. So while the body runs, everything that happens because of a label in the table of
12.2 is either a trap, which performs nothing, or a call of one of the module's functions named in that
row of the table; and the rows of those functions were performed by the containing function in step 1,
outside the discharge. A label is discharged only if all of its functions are in the module, so there is
no builtin under a discharged label whose answer is something the containing function's row does not
already hold. Everything the body does that is not in the table reaches the containing function's set
unchanged, because the body's set is added to it whole except for exactly those labels; and the flags that
make the checker careful are dropped only when the body's set shows that nothing but discharged `fs_read`
raised them. Where the checker cannot tell (a function value, a label it did not follow), the flags stay
and the function is judged as it is today.

It is the one place a row is not the union of what is called, and it is the place §6 named: within the
block those effects are implemented by functions whose own effects were just added.

What is not argued, because it is not true: that the row is right on a hosted target. There the runtime's
functions are the C library's, and the block discharges nothing.

### 12.5 What `cancho authority` prints

`cancho authority` is the union of what every function the program reaches performed. A tool's function is
reached, and it performed `io_write`: without more, the report of a kernel would list it although the
kernel's own rows no longer do.

For `--target aarch64-none-elf`, in a program that has a `system` block, the labels that block discharges
are left out of the report, for every function. The reason is the one of 12.4, and it is about the image
and not about the block: in that image there is one definition of each runtime function, and it is the
module's function or a trap, wherever the builtin is written. A function outside the block that calls
`putchar` declares `io_write` in its row (rows are checked one function at a time, without knowing who
calls); in the image it traps or is answered by `out`. What the module's functions perform is in the
report, since they are reached.

So for cancho-os the report is addresses, registers and `heap`, and "never touches the console, the
filesystem, the command line" again. `cancho authority` of a tool on its own, for its own target, is
unchanged: `fs_read("")` and the rest. `cancho authority` of the kernel without `--target` is that of a
hosted program, and lists the hosted labels (and the kernel's rows, written for no system, are refused
there first).

`authority-reached` (each function with its own labels) is unchanged on every target: `tell` performs
`io_write`, and says so.

### 12.6 Not in this change, and stated

- **A row that still lists a discharged label is refused only as far as rows are held exact today.** A
  function that performs no label with an argument is held both ways: `effect-declared-not-performed`. A
  function that does (a kernel's, with its `mmio`) is, while code is written, not held to "the row says
  more" for plain labels (`effects.check`, `both`); that is how it was, and a stale `io_write` there is
  accepted. It then travels to the callers' sets and to the report only if the report is not the one of
  12.5, so the report is still sharp; the rows are the writer's to clean.
- **§7 (a) stays**: a hosted builtin reached outside the block is a trap, found when it runs.
- **One block in a program**, and none inside another, as before.

### 12.7 Tests that can fail

In `tests/commands/freestanding.sh`, each a build or a check and the rule it answers:

- a body that writes to an `Mmio` of its own inside the block: the containing function's row has that
  `mmio("...")`; without it, `effect-not-declared`; with one more, `effect-declared-not-performed`;
- a module with no `key`: the body's `io_read` is not discharged, and a row without it is
  `effect-not-declared`;
- a module with no `open`: `fs_read("")` is not discharged;
- the same source for the machine the gate runs on: `effect-not-declared` with the row for no system, and
  built and run with the hosted row;
- a containing function that declares a discharged label: `effect-declared-not-performed`;
- `cancho authority --target aarch64-none-elf` of the probes: no discharged label, the module's `mmio` and
  the body's own, and the label that was not discharged.

And by hand, each shown to fail a case above: the discharge made to drop the body's whole set; made to
ignore which functions the module has; made on a hosted target too.
