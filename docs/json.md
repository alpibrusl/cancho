# `std.json`: parsing and writing JSON with nothing the caller did not hand it

> **Status: built.** `std/json.ls`; tests `tests/lex/json_*_test.ls`
> (29, run by `lex-sys test`), `conformance/json.rs`; drivers under
> `tests/programs/`. Strict RFC 8259; no tree, no allocation of its own.
> §5 has the numbers, and §6 has the two compiler bugs it found.

## 1. Why

The question that started this was whether a FastAPI-shaped package is
buildable here, and the first thing it needs is the thing every request
carries. There was no JSON in the language at all: the VCS op log is JSON
on the Rust side, `--output json` is JSON out of the CLI, and
`examples/serve` writes `{"ok":true}` as a string literal.

## 2. The design: a tape, not a tree

A value tree is a heap, a free list and a drop order for every document,
and a request handler wants none of them. `std.json` parses into a **tape**
the caller provides: one flat `[int]`, three ints a node, that points back
into the source bytes instead of copying them.

```
let tape = alloc_slice[a](json.tape_len(body), 0);     // or a heap slice
let nodes = json.parse(body, tape);
let age = json.to_int(body, tape, json.get(body, tape, 0, "age"));
```

* A node is named by its **index**; `-1` is "no node", and every accessor
  answers its zero value for it, so `get(get(get(..)))` needs no check
  between steps.
* A **string is not decoded** until asked for: the node holds the byte range
  inside the quotes and a flag saying whether it has an escape. A string
  with none (`string_plain`) *is* its bytes in the source, borrowed with no
  copy (`string_view`). A **number is not converted** until asked for: an
  object is walked for the key and the one value, and the other forty are
  never turned into anything.
* An object's children are its pairs in order; a key at node `k` has its
  value at `k + 1` and the next key at `skip(k + 1)`. `at` and `get` are
  linear in the position, which is what a flat list costs and what a
  request body of a few dozen fields makes irrelevant.

This is simdjson's shape, and it is the shape here for a different reason:
it is the one that needs no ownership. Everything lives in a slice the
caller already knows how to size (`tape_len` is always enough: a node is at
least a byte) and free.

Writing goes the other way, through a `Writer` that owns a `std.buffer`,
puts the commas, colons and quotes in itself, and is moved through every
call like a `Buffer` is, because growing it replaces the allocation under
it.

## 3. What it decides

| | |
|---|---|
| **Strict** | RFC 8259 and nothing else: no comments, trailing commas, leading zeros, `+1`, `.5`, `1.`, `NaN`, `'single'`; no unescaped control character in a string; no lone surrogate (`\ud800`) |
| **UTF-8 is validated** | Every byte ≥ 0x80 in a string is checked with `std.utf8` (overlongs, surrogates, > U+10FFFF refused). A parser that accepts what another refuses is where request-smuggling bugs live |
| **A parse error is a value** | `parse` answers the node count, or `0 - (position * 16 + code)`; `error_code`, `error_position`, `error_message` take it apart. Nine codes |
| **Nesting is limited to 128** | Not recursed into until the stack runs out: the stack is the one resource a parser cannot be handed |
| **A full tape is an error** | Code 8, not an overrun |
| **Integers saturate** | `to_int` clamps at the ends of `int` (a 20-digit number is not an overflow trap in a server); `fits_int` says whether it was exact. A float node truncates toward zero, saturating |
| **Floats are correctly rounded** | `to_float` is the nearest float to the decimal text, a tie to the even one (§4). A number past the range is ±infinity and one below it ±0.0: JSON has no such values but the number it spells does |
| **Duplicate keys: the first wins** | The document's order, not an arbitrary one |
| **Keys compare as decoded text** | `"café"` is the key `"café"` |
| **The writer cannot write bad JSON** | A bare value in an object, a key outside one, two keys in a row, a close that does not match, nesting past 60, finishing with something open: a **trap**. They are the caller's bugs, and the alternative is a service that sends `{"a":}` |
| **Invalid UTF-8 going out becomes U+FFFD** | The output is always a valid document, whatever was handed in |
| **A float that is not finite is `null`** | What most encoders do, and what keeps the document valid |
| **Floats keep their kind** | `3.0` is written `3.0`, so it reads back as a float; positional from 1e-6 to 1e21, scientific outside, shortest digits that round-trip |

## 4. Why it can be trusted

* **Numbers: 13,000 of them, bit for bit.** `conformance/json.rs` reads a
  corpus with Rust's `str::parse::<f64>` (correctly rounded) as the
  reference: random doubles over every exponent in their shortest form,
  17-digit decimals, **the exact decimal midpoint between two adjacent
  floats and one digit either side of it** (the case a floating-point
  shortcut gets wrong, built from `m * 5^s / 10^s` in 128-bit integers),
  integers on both sides of 2^53 and of `int`'s ends, subnormals,
  the overflow edge, and exponents of a million digits. Zero mismatches.
  The same run checks `to_int` against an `i128` parse clamped to `i64`.
* **Accept and refuse what `serde_json` does, over damaged input.** Ten
  valid documents and 1,500 mutations of them (one to two bytes replaced,
  deleted, inserted, swapped or truncated from a pool of structural and
  non-ASCII bytes): 291 accepted by both, 1,218 refused by both, **no
  disagreement**. Every accepted one is written back out by `std.json`'s
  own writer and read by `serde_json` to the same value. One policy
  difference, found by it and declared in §3: `1.5e22100` is a number
  `serde_json` refuses as out of range and this accepts as an infinity.
* **29 library tests**, in the language, on both backends
  (`tests/lex/`): every error code with its position, UTF-8 corner cases
  built byte by byte in a buffer, escape decoding including a surrogate
  pair, saturation at both ends of `int`, writer output exact to the byte,
  and **1,500 floats written and read back to the same bits**.
* **Misuse traps** are checked by building eight misusing programs and
  confirming each dies by signal.

### 4.1 The part that was hard: reading a float

A decimal with 15 or fewer digits and a power of ten within 10^±22 is two
exact floats and one correctly rounded operation (Clinger's fast path).
Everything else -- and a random double printed in 17 digits is the common
case -- needs exact arithmetic, because `m * 10^e` in floating point is
off by an ulp often enough to break a round trip. `slow_decimal` does it
with `std.bignum`: the digits (up to 767, the most a halfway point can
have, plus a sticky digit for the rest) and the power of ten become
bignums, scaled so the quotient has 56–57 bits, divided bit by bit, and
rounded on those bits with the remainder as the sticky bit. Limbs are
sized to the number (a 17-digit one needs a dozen where a worst-case one
needs hundreds), which is what took it from **18 µs to under 1 µs**.

## 5. Measured

`tests/programs/json_bench.ls` and `json_write_bench.ls`; timing from
outside as the difference between one round and twenty-one, so the
byte-at-a-time stdin read is left out. A 4-core sandbox; Python 3.11's C
`json`, which **builds every Python object** where the tape builds none --
the comparison is real for a handler that touches a few fields and unfair
to the tape for one that needs every value, so both columns are given.

**Reading**

| document | size | tape only | tape + every float converted | Python `json.loads` |
|---|---|---|---|---|
| 30k API objects (strings, ints, bools, 2-decimal floats, nested, 17-digit lat/lon) | 7.2 MB | 20 ms, **355 MB/s** | 93 ms, 77 MB/s | 119 ms, 60 MB/s |
| 300k floats, 17 digits | 5.9 MB | 13 ms, 461 MB/s | **277 ms, 21 MB/s** | 85 ms, 70 MB/s |
| 300k floats, 3 decimals | 2.8 MB | 10 ms, 287 MB/s | 26 ms, 107 MB/s | 37 ms, 75 MB/s |
| 500k integers | 5.7 MB | 26 ms, 217 MB/s | 25 ms, 229 MB/s | 40 ms, 142 MB/s |
| 100k strings with escapes | 5.3 MB | 9 ms, 601 MB/s | 6 ms, 820 MB/s | 25 ms, 209 MB/s |

**Writing** 200,000 small objects (`id`, `name`, a `score` of the form
`n/8`, a bool, a two-element array), 15.7 MB, byte-for-byte the same
bytes as `json.dumps(separators=(",", ":"))`:

| | time | throughput |
|---|---|---|
| `std.json` Writer | 106 ms | **148 MB/s** |
| Python `json.dumps` | 266 ms | 59 MB/s |

**The honest reading.** Scanning is fast and conversion is not uniformly:
17-digit floats are the weak case (about 0.9 µs each, 3.3× slower than
Python's `float()`), and writing a 17-digit float is slower still (§7). A
handler reading a few fields and writing a short reply is at the top of
both tables; one that converts 300,000 random doubles is not.

## 6. Found along the way

Writing a module that is the third to declare a `pub fn drop` found two
compiler bugs, both with their own regression test
(`conformance/modules.rs`, both backends), and `modules.md` §3.1.1 records
them:

* **Functions**: every function was emitted as `lexs_<name>`. The type
  checker accepted two modules each declaring `drop`; the assembler refused
  with `invalid redefinition of function 'lexs_drop'`. Symbols are
  `module.name` now.
* **Statics** (silent, and worse): a `static` was looked up among *every*
  module's and the first taken, so two modules each with `static table`
  read one table. `a.first() * 10 + b.first()` answered 55 where the tables
  held 5 and 7. Looked up in its own module now.

And one change outside this module's files that it needed: `std.fmt`'s
`float_into` prints a number with a short decimal form (`3.5`, `19.99`,
`0.000125`) without the exact algorithm (a candidate `m / 10^k` is checked
by one exact division; `10^-k` wider than a float's spacing makes the
candidate unique, so it is the shortest and the nearest). That took float
writing from **6 µs to under 0.5 µs** where it applies, and the existing
oracle test (against Rust's `{:e}`) now includes 2,000 short decimals and
their one-ulp neighbours, which must fall back correctly.

## 7. What it does not do

| | |
|---|---|
| Eisel–Lemire / Ryu | The fast float paths cover short decimals; a 17-digit double is ~0.9 µs to read and ~6 µs to write. Both want 128-bit multiplies and a 650-entry table of powers of five -- generated, as `std.math`'s 2/π table was, not copied |
| A mutable document | The tape is read-only. Build a different document with the Writer |
| Streaming | A whole document in memory. A chunked reader would be a second parser |
| Integers past 64 bits | They saturate in `to_int` and are `to_float`-able; there is no big integer |
| Pretty printing | `{"a":1}`, never indented |
| Comments, JSON5, trailing commas | Refused, by design (§3) |
| Schema validation, typed decoding | No reflection, no macros: a model is a hand-written function over the tape. Generating those from a declaration is a tool, not a library |
| Newline-delimited JSON | Split on `\n` with `std.bytes` and parse each; nothing needed here |

## 8. Open

| Question | Why it waits |
|---|---|
| A hash map, for objects too wide for a linear `get` | `std.map` exists now ([`map.md`](map.md)) with byte-string keys; indexing a parsed object into one is a few lines a caller can write, and nothing here does it for them |
| Float reading and writing at Python speed | §7's first row |
| ~~An HTTP layer that uses it~~ | **Built**: [`http.md`](http.md) (parser, router) and [`server.md`](server.md) (`examples/api`, which reads and writes JSON bodies with this module) |
