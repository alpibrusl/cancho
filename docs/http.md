# `std.http` and `std.route`: reading a request and choosing a handler

> **Status: built.** `std/http.ls` (request parser, percent-decoding, query
> lookup, response head) and `std/route.ls` (method + path to a route id);
> tests `tests/lex/http_test.ls` (8) and `route_test.ls` (2), run by
> `lex-sys test`; `conformance/http.rs`; drivers under `tests/programs/`.
> No sockets, no allocation of their own beyond the router's tables. §6 is
> the evidence, §7 the numbers.

## 1. Why

`json.md` was the first piece of a FastAPI-shaped package; this is the
second and third. A request arrives as bytes and has to become a method, a
path, some headers and a length (`std.http`), and then a decision about
which code runs (`std.route`). `packages/http-request` could read a request
and stream its body to standard output, which is what the two programs that
needed it did, and was no help to a program that wants to *look at* one.

It is a parser and a router, **not a server**: nothing here touches a
socket, so it works the same on bytes from `net.sockets`, from a file, or
from a test. The server loop, body reading and an end-to-end benchmark
against a real framework are the next piece (§8).

## 2. `std.http`: the table

`parse(src, table)` fills a table of integers the caller provides --
offsets into `src`, nothing copied, the shape `std.json`'s tape has -- and
answers the offset where the body starts, or a negative error. `slots(n)`
is how big the table must be for `n` headers; more headers than that is a
refusal, not a truncation.

```
let table = box_slice(heap, http.slots(32), 0);
let n = http.parse(request, contents(w));
if n >= 0 { let id = route.find(router, http.method(request, t), http.path(request, t), p); }
```

`src` may hold more than the head (the start of a body, or the next request
of a pipelined connection): `parse` stops at the blank line and never looks
past it, and its answer is where to resume.

**An error is a value, and says where**: `0 - (position * 16 + code)`,
decoded by `error_code`, `error_position` and `error_message`, the encoding
`std.json` uses. One code is not a refusal: **1 means the head has not all
arrived**, `is_incomplete` says so, and a server reads more and calls
`parse` again. A head longer than `max_head()` (64 KiB) with no blank line
is code 10, not 1, so a peer cannot make a server buffer without bound.

## 3. What is refused, and why

The posture is `std.json`'s: strict, and a refusal says where. Most of what
HTTP/1.1 parsers disagree about is how a message ends, and disagreement
there is **request smuggling**: a proxy and a server that read the same
bytes as two different requests. So the ambiguous ones are refused.

| Code | Refused | Why |
|---|---|---|
| 1 | head not complete | not an error: read more |
| 2 | request line malformed; anything after the version but CRLF | one shape, one parse |
| 3 | method not a token | RFC 9110 §5.6.2 |
| 4 | target empty, with a `#`, or with a byte outside visible ASCII | a fragment is never sent to a server; raw high bytes must be percent-encoded |
| 5 | version other than `HTTP/1.0` and `HTTP/1.1` | nothing here speaks 2 or 0.9 |
| 6 | header with no colon, an empty name, **whitespace before the colon**, a bare line feed or NUL in a value | the classic parser-differential inputs |
| 7 | more headers than the table holds | a limit, said aloud |
| 8 | `Content-Length` that is not 1-15 digits, or repeated with different values | `+5`, `5, 5` and `0x5` all mean different things to different parsers |
| 9 | **obsolete line folding** (a header line starting with space or tab) | RFC 9112 §5.2 lets a server refuse it, and it is how a header is smuggled past a filter |
| 10 | head over 64 KiB with no blank line | bounded buffering |
| 11 | `Transfer-Encoding` other than `chunked`, **or `chunked` together with `Content-Length`** | the textbook CL.TE / TE.CL smuggling pair; RFC 9112 §6.3 says to refuse |
| 12 | HTTP/1.1 with no `Host`, or two | §7.2 |

Line endings are CRLF only; a bare line feed is refused (code 6). A request
with a repeated `Content-Length` that **agrees** is accepted, because RFC
9112 §6.3 allows collapsing it.

**One thing this does not do:** a malformed head is refused only once the
blank line has arrived, so garbage with no terminator gets code 1 until the
limit or the connection's own timeout, not an immediate 400. A server needs
a read timeout regardless, and parsing line by line as bytes arrive would
refuse sooner; nothing needs it yet.

After the table, the accessors: `method`, `target`, `path` (before any `?`,
still percent-encoded), `query`, `version`, `header_count`,
`header_name`/`header_value`, `find_header`/`header` (case-insensitive;
write the name lowercase), `content_length` (-1 for none *or* chunked),
`is_chunked`, `body_start`, and `keeps_alive` (HTTP/1.1 unless it said
`close`, HTTP/1.0 only if it said `keep-alive`; tokens in a
comma-separated `Connection` value are found).

`percent_decode(text, out, plus_is_space)` decodes into a buffer you
provide and answers the length, or -1 for a malformed or truncated escape or
no room -- never "pass it through". `query_value(query, key)` answers the
raw `(start, end)` of the first `key=value`, or `(-1, -1)`; a key with no
`=` has an empty value.

`respond_head(heap, out, status, content_type, length, keep_alive)` appends
the status line, `Content-Type`, `Content-Length`, `Connection` and the
blank line. **It traps** on a `content_type` containing a carriage return,
line feed or NUL, and on a status outside 100-999 or a negative length:
header injection is a program bug, and the place to find it is the first
response, not a client's cookie jar.

## 4. Decisions

| Question | Answer | Why |
|---|---|---|
| Header table: strings or offsets? | offsets, in the caller's table | the `std.json` answer: no allocation, and the other forty headers are never turned into anything |
| Lenient or strict? | strict | §3. A parser that accepts everything is the other side of a smuggling attack |
| Is `Host` required? | yes for 1.1, not for 1.0 | RFC 9112 §3.2 |
| Chunked bodies | recognised and flagged, **not decoded** | the body is the caller's; a decoder is a stream and belongs with the server loop |
| `Expect: 100-continue`, trailers, upgrades | not handled | nothing has asked |
| Methods | any token | a router decides what it serves |

## 5. `std.route`

A router answers one question: given `GET` and `/users/42/posts`, which of
the routes a program registered is it, and which parts of the path were
parameters? It answers with a route **id the program chose**, and the
program `match`es on it. There is no registry of function values, so
dispatch is a jump the compiler can see, and a handler no route reaches is
one reachability can drop.

```
r = route.add(heap, r, "GET", "/health", 1);
r = route.add(heap, r, "GET", "/users/:id/posts/:post", 2);
r = route.add(heap, r, "GET", "/files/*path", 3);
```

`:name` is one non-empty segment; `*name`, last only, is the rest of the
path, even empty. Parameters come back as `(start, end)` ranges of the path
in pattern order, in a table of `2 * most_params(router)` ints.

**All the rules**, because a router with a hidden rule is a router with a
security bug:

1. **Static first.** A pattern with no parameter is found by hash. Only if
   no static route has the path are the others tried.
2. **Of the parameterised, the one added first that fits wins.** Not the
   most specific; the first. Order is the one thing a reader can see.
3. **The path is matched as sent.** `%`-escapes are *not* decoded first:
   `/%75sers` does not match `/users`. The parameters are raw ranges;
   decode them with `http.percent_decode`. Exactly one spelling of a path
   reaches a route, which is what keeps a proxy's access rule and this
   router from disagreeing about which route it was.
4. **No normalisation.** `/a/` is not `/a`; `/a//b` is not `/a/b`; an empty
   segment matches no `:param`; no redirect is issued.
5. **Methods are exact and case-sensitive** (RFC 9110 §9.1). `get` is not
   `GET`.
6. **405 is not 404.** `find` answers `-2` for a path some route has under
   another method, `-1` for a path no route has. (It does not say *which*
   methods, so no `Allow` header; §8.)

A malformed pattern, an id below zero, an empty method, or a second static
route for the same method and path **traps at registration** -- the program
is wrong, and a table is written once, at start.

**How lookup scales.** Parameterised routes are indexed by their first
literal segment, so a table of a thousand `/svc17/items/:id` routes costs a
lookup like a table of ten; only routes sharing a first segment, and routes
that *begin* with a parameter, are compared one by one. The first version
scanned every parameterised route in order, and §7 has what that cost.

### 5.1 `http.dechunk`: a chunked request body

`dechunk(src, out) -> (consumed, decoded)` reads the chunked body at `src[0]`
(where `parse` said the body starts) into `out`. `consumed` is the bytes of `src`
the whole body took -- terminating chunk and blank line included, so a pipelined
request is found right after -- and `decoded` the bytes written; `consumed` is
negative when there is no body to return: `-1` not all here yet, `-2` a size that
is not 1-8 hex digits, `-3` framing that is not exactly CRLF, `-4` too large for
`out`, `-5` a chunk extension or trailer. `dechunk_incomplete` and
`dechunk_message` read the code.

**Strict on purpose**: extensions and trailers are where smuggling lives, nothing
here needs them, and refusing is one line where interpreting them is a page.
When more bytes arrive the decode starts again from the first chunk (cost
proportional to the body, bounded by `out`). Tested three ways: unit tests for a
good body, every prefix of one (never wrongly refused -- that is what lets a
server call it again after each read), and each refusal; a differential test
against a second decoder written independently in Rust over 600 generated bodies,
each mutated four ways and cut once, plus overflow sizes -- every one of the six
outcomes reached; and two mutations of the decoder (nine digits allowed; a
trailer treated as "wait") each failing it.

## 6. Evidence

| Check | Result |
|---|---|
| 8 parser tests: every refusal code, **with its exact position**; agreeing duplicate `Content-Length`; chunked; the keep-alive rules for 1.0 and 1.1; pipelining stops at the blank line; table too small; a head that never ends; percent-decoding and the query; the response bytes | pass, both backends |
| 2 router tests: static, parameterised, rest, 405, trailing/doubled slashes, escapes not decoded, priority | pass, both backends |
| **Parser vs `httparse`**: 600 generated valid requests (random methods, targets, 0-6 headers with arbitrary whitespace and values, optional length/chunked/close), each mutated four ways and cut once, 3,600 inputs | **1,089 accepted, every one with the same body offset, method, target, version, header count and every header name and value bytes as `httparse`**; 1,499 incomplete (and where `httparse` has a whole request, it is because of a bare line feed, which this refuses); none of the rest accepted by `httparse` unless counted in the next row |
| ...where this refuses what `httparse` accepts | 55 inputs, all in the categories §3 names: code 6 (21), 11 (17), 12 (9), 2 (4), 8 (4). The test fails for any other code |
| **Never looser**: this accepting what `httparse` refuses | 0 |
| **No trap on any bytes**: 1,500 random byte strings (a fifth fully random, the rest from the grammar's own alphabet, NULs and 0xff included) | driver exits 0 |
| **Router vs a reference matcher** written from §5 alone: 13 routes, 4,000 queries | every answer and every parameter range agrees; 282 matched, 374 wrong method, 3,344 no route |
| 12 misuse programs (bad pattern, negative id, empty method, duplicate static, header injection, bad status, negative length) | each dies of SIGILL |

Two things worth saying about how the first of those went. The parser
differential **passed the first time it ran**, which is a reason to read the
counts rather than the green: the statistics line is printed by the test and
shows 1,089 accepted, not a handful. And the router corpus first produced
**no "no route" answers at all** -- a random one-segment route came out as
`/*rest`, which matches everything -- so its first version agreed with the
reference on a table that could not fail on that axis. Fixed by never
generating a root rest route, and the test now asserts every answer occurs
more than a hundred times.

## 7. Measurements

One shared, noisy machine; each figure a spread over three runs, as a
difference between a short and a long run. The request is a browser-sized
head: 427 bytes, a path with two parameters and a query, 9 headers.

| | per request |
|---|---|
| `httparse` (Rust, tuned, SIMD where it can) | 0.26-0.28 µs |
| **`std.http.parse` + `std.route.find`**, 0 to 5,000 routes | **1.0-1.4 µs** |
| Python `http.client.parse_headers` (what `http.server` uses) | ~37 µs |

So **about four times slower than `httparse`** and about **thirty times
faster than Python's parser**. Parse and route together are one to two
microseconds, which is a million requests a second a core before a socket is
involved; the socket is where the time goes.

Before the router was indexed by first segment, parse and route together
cost 4.0 µs a request with 100 routes and 17.5 µs with 1,000, linear in the table. After it, the cost is
flat from none to 5,000. (The benchmark's routes have distinct first
segments, which is the case the index serves; a thousand routes under one
prefix would still be a thousand comparisons, and §8 says so.)

Two optimisations that were tried and measured: the blank-line search went
from a general substring scan to one that looks hard only at line feeds
(1.35 to 1.0 µs, the whole gain); a compile-time table of character classes
instead of comparison chains made **no difference**, and was kept only
because it reads more clearly.

**What is *not* measured here, and not claimed:** a request through a real
socket, a keep-alive connection, or anything against FastAPI. The first
measurement of that, an accept loop, was in the conversation that proposed
this work; a router-backed server is the next thing to build.

## 8. Not done

| Missing | Why it waits |
|---|---|
| ~~A server loop that reads, parses, routes and writes~~ | **Built**: `examples/api`, [`server.md`](server.md), with the requests-a-second figures. Chunked decoding and a configurable buffer are built (§5.1); still missing: streaming a body larger than the buffer to a handler |
| ~~Chunked decoding~~ | **Built** (§5.1): `http.dechunk`. Still not: `Expect: 100-continue`, trailers (refused), chunk extensions (refused) |
| Refusing a malformed head before its blank line arrives | §3, last paragraph |
| ~~`Allow` on a 405~~ | **Built**: `route.allowed(heap, router, path, params, out)` appends `GET, POST` -- registration order, each once -- and `http.respond_head_with` writes whole header lines it was handed (anything but `name: value` + CRLF traps, for the reason a header injection does). `examples/api` answers `Allow:` on its 405s |
| Typed parameters (`:id:int`), regex constraints | **Half built, without the grammar**: the handler asks for the type -- `route.param_nat(path, params, i)` (a non-negative decimal of at most 17 digits, else -1), `route.param(path, params, i)` (the matched text), `route.param_decoded(path, params, i, out)` (percent-decoded). A router that refuses a non-number itself still adds a grammar, and nothing has asked |
| Many routes under one prefix | linear within the prefix (§7); a trie would fix it, and nothing has that many |
| Query string into a map | `query_value` is a scan; a map is a few lines for a caller who wants one |
| JSON body binding and validation (what makes FastAPI feel like FastAPI) | no reflection and no macros here: a model is a hand-written function over the `std.json` tape |
