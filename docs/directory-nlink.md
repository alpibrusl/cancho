# `dir_nlink`: how many names a file has, so a program can refuse a hard link

> **Status: built, in the compiler in cancho only.** Edition 12. `dir_nlink` and `dirs.confined` exist in `examples/selfhost` and `packages/confined`; the Rust compiler does not
> know them (no Rust was written for this; `builtin-tables.md` says how the builtin was added). Measured on macOS aarch64: the command tests
> `directory_nlink` and `confined` pass, and the standalone `cancho` built from these sources reproduces itself (stage 2 equals stage 3). **Not measured: Linux.**
> The `st_nlink` offsets for Linux x86-64 and aarch64 come from the system headers' layouts, as the `st_mode` ones do, and are exercised by CI.

## 1. The gap

`std.dirs.open_file` follows no symbolic link: it opens a path one component at a time with `O_NOFOLLOW`. A **hard link** is not a symbolic link. It is another name
for a file that may lie anywhere on the same filesystem, so an open flag cannot tell it from the file it names. A program that reads a name that came from outside (an
argument, a request) can therefore be led out of its directory by a hard link that sits inside it. Measured with the `Dir` form before this change
([cancho#762](https://github.com/alpibrusl/cancho/issues/762)): with `data/hl.txt` a hard link to `secret/token.txt`, the program read the secret, and the authority report
said `dir_read` and `fs_read("…/data")`.

## 2. The builtin

```
dir_nlink(dir: &Dir, name: &[byte]) -> [dir_read] Done      // Done::Ok(count) | Done::Failed(errno)
```

The same shape as `dir_mode` (`directory-listing.md` §3.5): one component (`.`, `..`, an empty name, a `/` and a NUL are `EINVAL`, no call is made), `fstatat` with
`AT_SYMLINK_NOFOLLOW`, so a symbolic link answers its own count (1), never its target's. `count` is `st_nlink`. A regular file with no other name answers 1. It performs
`dir_read`, like `dir_stat` and `dir_mode`. Edition 12. There is no `dir_own_nlink`: the number for a directory itself means different things on different
filesystems, and nothing asks for it.

Where `st_nlink` is, per target (the array `cancho_plat` that `platform.cho` writes, `runtime/dirs.c` reads):

| target | offset | width |
|---|---|---|
| Linux x86-64 | 16 | 64 bits |
| Linux aarch64 | 20 | 32 bits |
| Darwin aarch64 | 6 | 16 bits |

## 3. `dirs.confined`

```
dirs.confined.open_file(dir: &Dir, path: &[byte]) -> [dir_read] Opened
```

`std.dirs.open_file`'s walk, with `dir_nlink` asked about the last component before it is opened. A file with one name is opened as before. A file with more is refused:
`Opened::Failed(31)`, `EMLINK` (`confined.emlink()`). It is a package, not a `std` module: `--std` is read by the Rust front end as well, which cannot parse `dir_nlink`, so
the file is named on the command line beside the program (`cancho build main.cho packages/confined/confined.cho`). `AGENTS.md` §3.7 describes it; its checked program uses
`std.dirs.open_file`, which the Rust front end accepts.

## 4. What it does not do

- **A race.** The count and the open are two calls on one name; a hard link made between them is not seen. Keep untrusted writers out of the directory. A single
  call that opened and answered `st_nlink` of the opened descriptor would close it; that is a second builtin, and nothing needs it yet.
- **Files with two names for a reason.** A package manager's store, a backup tool's tree. `dirs.confined` refuses them; `std.dirs.open_file` does not.
- **The `Fs` form.** A program that holds `Fs("/data")` and not a `Dir` still follows every link (`authority.md` §2.4). `dir_nlink` is a `Dir` verb.
- **The report.** `cancho authority` says `dir_read` for a program that asks for link counts and does not say it refuses them: it is policy in a library function, as
  `std.dirs`'s walk is.

## 5. Measured

`tests/commands/directory_nlink.sh` (macOS aarch64): a plain file 1, two names of one file 2 and 2, a file with three names 3, a symbolic link 1, a missing name `err 2`, `..` and
`sub/x` `err 22`. `tests/commands/confined.sh`: `plain` opened, `twin` (two names) `err 31`, the same one directory down, a missing name `err 2`, `..` and the empty name `err 22`.
`tests/reject/dir_nlink_is_edition_twelve.cho` holds the edition gate. The probes live in `tests/probes`, not `tests/programs`: the Rust parity tests walk that directory and the Rust front end cannot parse
`dir_nlink`. For the same reason there is no `not_declared` reject fixture: the Rust front end would refuse it with another rule. The row is held by the probe building at all (a row is exact), and the effect
list of `dir_nlink` is `dir_mode`'s (`scripts/builtin_tables.py show dir_nlink`).
