# `std.flate`: DEFLATE, gzip and zlib

Status: **built** (`std/flate.cho`, tests in `tests/lex/flate_test.cho`). Epic #491 / port plan #680; the audit is `docs/vcs-native.md`.

## 1. Why it is in `std`

`std/` had no compression. cancho-tools' `archive` wrote one (`tools/archive/flate.cho`, its `docs/archive.md` §2) and said it would move into `std`
"when a second program wants it". The second is `cancho vcs`: a git loose object and every pack entry is a zlib stream, so the native
`cancho vcs` cannot read or write a git repository without it (`docs/vcs-native.md` §3). Two askers, as `docs/standard-library.md` §5 asks, found by
reading both programs. A first inflater of the same family (`puff.c`, bit at a time) was written for `cancho vcs` in #690 and measured at about half
the speed of this one (215 against 410 MB/s on one stream); it is not kept.

The file is `archive.flate` as it was, with these changes: the module is `std.flate`; its own CRC-32 is gone (`std.crc.crc32_of`/`crc32_update` are
the same CRC-32/IEEE, and `std.crc` has no heap parameter); and the zlib section is new.

## 2. What it has

| function | does |
|---|---|
| `inflate(heap, src, start, end, limit)` | a raw DEFLATE stream; answers `(output, status, offset after the stream)`; a table for the short codes |
| `deflate(heap, src)` | a raw stream: LZ77 with a hash chain of 16 and lazy matching, blocks of up to 32,768 tokens, each stored, fixed or dynamic, whichever is smallest; the same input gives the same bytes |
| `gunzip`, `gzip`, `gunzip_parts`, `is_gzip` | RFC 1952, one member; the CRC-32 and the length of the trailer checked |
| `unzlib(heap, src, start, end, limit)` | RFC 1950: the header checked, `inflate`, the Adler-32 compared; the offset is after the four bytes of the trailer |
| `zlib(heap, src)` | the header `78 9c`, `deflate`, the Adler-32 |
| `adler32_start`, `adler32_update`, `adler32_of` | the checksum, incremental |
| `take`, `decode`, `construct`, ... | the bit reader and the code tables, public because cancho-tools' streaming inflater is written on them |

Statuses (an `int` the caller maps to a rule; nothing here traps on any input): `inflate` 1 the stream is invalid, 2 the input ended, 3 the output would
pass `limit`; `gunzip` 10 to 15 (`flate.cho`'s header); `unzlib` adds **20** not a zlib header (method, window over 32 KiB, or not a multiple of
31), **21** a preset dictionary, **22** the Adler-32 is not the trailer's.

## 3. Measured

macOS aarch64, the Rust compiler's build, `-O2`, best of 3 to 7 runs. **A run through standard input and output is dominated by the I/O**:
`lexcore.slurp` reads 20 MB in 0.29 s and `buffer.write` writes 20 MB in about 0.27 s (about 70 MB/s each), so the compute is the total
minus those, and is an upper bound.

| what | measured |
|---|---|
| `unzlib` of a 6.0 MB level-6 stream to 20 MB of text, no output written | **0.049 s, about 410 MB/s of output** (the inflater alone, `archive.flate` in the same harness) |
| `unzlib` of the same stream, output written to standard output | 0.41 s in all: 0.09 s to read 6 MB and about 0.27 s to write 20 MB |
| `zlib` (deflate) of 20 MB of text, output 6.09 MB | 0.91 s in all: 0.29 s to read, about 0.08 s to write; **about 0.5 s of compression**, about 38 MB/s. Python's `zlib.compress(level 6)` takes 0.57 s on the same data |
| size of that output | 6,091,243 bytes: **100.9 %** of zlib level 6 (6,035,277); 100.0 to 104.3 % on the other inputs of 100 KB and 1 MB below |
| the embedded library | `std/flate.cho` is 55.5 KB (1,597 lines), +7 % on `std` (789,906 bytes); the standalone `cancho`'s `stdsource.cho` grows from 822,399 to 879,744 bytes; a program that does not call it costs nothing (§5 of `standard-library.md`) |

**Against the independent implementation (Python's `zlib`):** 120 cases (empty, 1, 100, 5,000, 100,000 and 1,000,000 bytes of text, random bytes,
zeros and cancho source, each compressed at levels 0, 1, 6 and 9 and read by `unzlib`, and each compressed by `zlib` and read by Python): no difference.
`tests/lex/flate_test.cho` has the 12 tests that run in `cancho test` (the 7 of cancho-tools, the CRC-32 check value, and for zlib: seven streams written
by Python (stored, fixed and dynamic blocks) rebuilt byte for byte, Adler-32, round trips up to 300,000 bytes, every refusal with its status, the limit,
and every prefix and one changed byte of four streams: a status, never a trap).

Not measured here: the compressor against zlib on inputs larger than 20 MB; the streaming inflater of cancho-tools on this build.

## 4. What is next

1. cancho-tools: `import std.flate`, delete `tools/archive/flate.cho`, use `std.crc` for the CRC (its `stream.cho` and `zip.cho` call
   `flate.crc_fill`, `crc_update` and `crc32`), and move its pin to a cancho that has this module. Not done: that is a change in another repository.
2. `cancho vcs` (#680, `examples/selfhost`): the object store on `std.flate`, in place of the `zlib.cho` of #690.
3. The slow I/O (`slurp`, `buffer.write`): about 70 MB/s on this machine, measured here, not investigated.
