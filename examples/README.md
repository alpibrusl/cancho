# Examples

Programs meant to be **read**. Every one declares what it prints in its
own header, and `cargo test` walks this directory and checks it — so an
example that stops matching the language fails CI rather than quietly
rotting.

All of them build with `--std` available, which costs nothing: a program
that imports none of the library emits a byte-identical object either
way ([`standard-library.md`](../docs/standard-library.md) §5.2).

```sh
cargo run -p cancho -- run examples/tour.cho --std
```

| Example | What it is |
|---|---|
| [`hello.cho`](hello.cho) | The smallest program that says hello |
| [`tour.cho`](tour.cho) | One section per feature, in the order the milestones added them |
| [`pipeline.cho`](pipeline.cho) | M2 as *one* system rather than four features side by side |
| [`tree.cho`](tree.cho) | A binary search tree — why a language needs a heap at all |
| [`lines.cho`](lines.cho) | The M3 acceptance criterion: a real command-line tool |
| [`tally.cho`](tally.cho) | `wc` over standard input |
| [`wordcount.cho`](wordcount.cho) | `wc` over an embedded document, and the before-and-after for `std` |
| [`rational.cho`](rational.cho) | 250 lines of exact rational arithmetic; the M1 language still compiling unchanged |
| [`queue.cho`](queue.cho) | A work queue whose jobs **own** memory, held in a collection |
| [`pipeline_checks.cho`](pipeline_checks.cho) | A fallible pipeline: four exits, one `defer` |
| [`buffer/`](buffer/) | A growable byte buffer, written as a library |
| [`slab/`](slab/) | Shared ownership, as far as this language reaches |
| [`modular/`](modular/) | Two modules and a root |
| [`selfhost/`](selfhost/) | The cancho lexer and parser, written in cancho, checked against the Rust ones |
| [`cut/`](cut/) | `cut -d -f`, ported to **ask** what a string library needs |
| [`wordfreq/`](wordfreq/) | The capstone: three files, every capability doing real work |

---

## The tour

`tour.cho` is the shortest honest answer to "what can this language do".

```sh
cargo run -p cancho -- run examples/tour.cho --std
# M0: 7 5 3 1
# M1 bool: 1010010
# M1 struct: (3, 4) -> 25
# M1 enum: 0 12 20
# M1 generic: 5 3 z
# M2 linear: 4 7 9 5 6
# M2 borrow: 4 8 12
# M2 unique: 3 5 5
# M2 effects: 42
# M2 capability: 88
# M2 foreign: 7 9
# M2 arena: 1 4 9 -> 14
# M3 arithmetic: 6 1
# M3 slice: 3 1 4 1 5 -> 14
# M3 string: hello (5 bytes, e is 101)
# M3 file: on disk (7)
# M3 heap: 3 1 4 -> 8 (freed)
# M3 reading: 8 3 8 (kept)
# M3 args: 1 (named)
```

## Programs rather than a tour

### `pipeline.cho` — the thesis in one program

```sh
cargo run -p cancho -- run examples/pipeline.cho
# jobs: 4 done, 2 cancelled
# spent: 49 of 50
# headroom: 1
```

Admits a run of jobs against a budget. A job is a linear resource
finished exactly once — completed or cancelled, never both and never
neither; deciding which needs its cost, and reading a field of something
you own without spending it is a borrow; the running tally lives in an
arena; the overrun is computed by libc through a capability that names
libc and nothing else; and printing needs the console capability `main`
was handed. **Delete any one of those and it stops compiling.**

### `tree.cho` — why a heap exists

```sh
cargo run -p cancho -- run examples/tree.cho
# 1 3 4 5 7 8 9
# sum 37 count 7 depth 3
```

A binary search tree: its shape is decided at run time, its nodes
outlive the calls that made them, and the type is defined in terms of
itself. None of that fits in a block.

There is **no `free` in the file**, and every node is freed. `unbox` is
the only thing that ends a box, a box is `res`, and a node the program
forgot would be a compile error rather than a leak found next month.

### `lines.cho` — the M3 acceptance criterion

```sh
cargo run -p cancho -- run examples/lines.cho
# lines 6
# errors 2
# longest 15

lines /var/log/app.log        # or a path you name
```

Given a path it reads that file, counts and filters it, writes a report
and reads the report back; given nothing it lays down a sample and reads
that, which is how a tool with no input behaves anyway.

It is also where the design shows a sharp edge honestly. A capability is
narrowed to a **literal**, checked where it is written — so a tool that
reads a path the *user* chose cannot narrow to it, because there is no
literal to narrow to. `lines` therefore passes its `Fs` on unnarrowed,
and narrowing it to `/tmp` would not make a safer tool, it would make a
broken one.

What the types still buy at that width: `report`'s row says
`fs_read("")` and `fs_write("")`, so a reader knows it touches arbitrary
paths without opening the body. A tool that *did* know its directory
would narrow, and its row would say so instead. **The type tells the
truth either way.**

### `tally.cho` — `wc` over a pipe

```sh
printf 'the quick brown fox\njumps over\nthe lazy dog\n' \
    | cargo run -p cancho -- run examples/tally.cho
#      3      9     44
```

Worth reading for `count`'s row: `[io_read, io_write]` says in the
signature that the function consumes the program's input as well as
writing to the console, and a caller learns both without opening the
body.

### `wordcount.cho` — the before-and-after for `std`

```sh
cargo run -p cancho -- run examples/wordcount.cho --std
```

It used to open with `space`, `newline`, `write_all`, `print_nat` and
`is_blank` — five helpers that are not what the program is about, and
that 24 other files here each wrote out again. They are `std.io` and
`std.bytes` now.

`is_blank` moving out is the part worth noticing. `wordcount` counted
space and newline; `tally` counted six bytes; neither knew the other
disagreed. One definition, in one place, is most of what a standard
library is for.

### `queue.cho` — a collection of resources

```sh
cargo run -p cancho -- run examples/queue.cho --std
# compile 7
# link 4
# test 4
# rejected: 1 blank
# 3 jobs, 15 bytes
```

Jobs that **own** their storage, held in a `std.list`, ended exactly
once each. Three rules are visible and each is load-bearing: the list
moves jobs and never ends one, because a generic function does not know
what ending a `T` means; a job never finished does not compile; and the
tally beside it is a `Vec[int]` rather than a list, because an array
cannot hold a resource at all. [`collections.md`](../docs/collections.md).

### `pipeline_checks.cho` — four exits, one `defer`

```sh
cargo run -p cancho -- run examples/pipeline_checks.cho --std
# ok: fine
# empty
# bad prefix
# 3 checked
```

The program `docs/defer.md` §4 is about, and the reason it is an example
rather than a fixture is that the case for `defer` is not the code that
exists — it is the code nobody wrote. Without it, each of the four
bail-out paths repeats the buffer's drop, tangled into its own return
expression.

Both halves are checked rather than asserted: take the `defer` out and
the function stops compiling at the first early return, and with it in,
valgrind reports 4 allocs and 4 frees.

### `rational.cho` — the M1 language, still standing

250 lines of exact rational arithmetic, with a generic `Result[T]`
threaded through every fallible operation. It predates M2 and stays that
way on purpose.

## Libraries

### `newton.cho` — a method fixed point could not carry

Newton's method for √2, five steps, reporting the residual `|x² - 2|`
after each.

Read the numbers rather than the code. Q16.16 — the fixed point this
language forced before `float` — resolves 1.526e-5, and **step 3's
residual is already below that**, so the last three steps are invisible
to the representation the language used to have.

The honest detail is that the residual stops at 4.44e-16 rather than
zero: √2 is not representable in binary64, so the iteration settles on
the nearest value that is. A method that *converged* and a method that
reached the answer are different things, and this is where the
difference becomes visible.

Every number here is printed by `std.fmt.float_into` — the shortest
decimal that reads back to the same bits, written in cancho rather than
in the compiler (`float-printing.md`). An earlier version of this file
reported everything through `truncate` and a scale factor of 1e18,
because printing a float was still open; the difference is visible in
step 3, whose residual was `6007304882427` there and is
`6.007304882427178e-6` here. Three digits the scale factor was eating.

### `base64/` — a program that did not start here

GNU coreutils' `base64`, ported and checked against it: the conformance
suite pipes the same bytes through both binaries and compares, twelve
sizes, both directions, three malformed inputs and a megabyte.

The decode table is a `static` — built by a loop that runs **during
compilation** and ends up in the binary's read-only data
(`docs/compile-time-data.md`). It used to be that loop run once per
decoded character, scanning 64 entries to find one; replacing it made
this program **6.1× faster** on 5.4 MB, which is the largest single
change to it since the port.

It is the one program in this directory that had opinions before it
arrived — the 76-column wrapping, the padding, the exit status on bad
input — and it is worth reading for what it *did not* need. No new
capability, no library, no change to linearity or effect rows. What it
needed was the bit operators, which did not exist until it asked
(`docs/bitwise.md`).

It streams, and that is not style: an arena is one 64 KiB chunk and
standard input is not, so three-bytes-in-four-characters-out was the only
way to write it — which is also how the C writes it.

The encoder buffers now. `emit` used to be a `putchar` per character;
it fills a 4 KiB slice and flushes it with `io.write_all`, because
`docs/bulk-io.md` measured a libc call per byte at **12.8×** a bulk
write. That is worth **1.6×** on this program rather than 12.8×, and §4
of that document is honest about where the rest went — the cost moved
from calls into byte-at-a-time stores that `cc -O2` vectorises and
Cranelift does not. The visible price in the source is that `emit`
threads `(at, column)` through and answers a pair: there is no object to
keep them in, which is the same shape the rest of this program already
had.

`docs/porting.md` is the report, including §6 on what one small port does
not establish.

### `selfhost/` — the front end, written in itself

```sh
cargo build -p cancho-syntax --example dump_tokens --example dump_ast
cargo run -p cancho -- build examples/selfhost/parser.cho $(for m in driver listing pass1 ast kinds lexcore tables; do echo examples/selfhost/$m.cho; done) --std -o /tmp/parser
sh examples/selfhost/diff.sh target/debug/examples/dump_ast /tmp/parser examples/hello.cho tests/accept/*.cho
# identical: 105  different: 0
```

`lexcore.cho` is the cancho tokeniser, `lexer.cho` prints its tokens, `ast.cho` parses them into a
syntax tree, `parser.cho` prints the tree and `check.cho` runs the first half of the checker
(`pass1.cho`) over it, all in cancho, as stages 1 and 2 of the staged port in
`docs/self-hosting.md` §6. Each reads a source file on standard input and prints what
the Rust front end makes of it (`dump_tokens.rs`, `dump_ast.rs` in `cancho-syntax`
are the oracles), so the check is `diff`. `fuzz.py` adds hostile edge cases and
mutants; `tests/conformance/selfhost.rs` runs both ports over every program in the
repository in CI.

### `cut/` — the port written to find out what was missing

GNU `cut -d<delim> -f<list>`, checked against it on eight field specs
including the ones a hand-rolled splitter gets wrong: empty leading and
trailing fields, a line with no delimiter at all, a field past the end.

It is here as a **probe**. `docs/utf8.md` §1 said the rest of a string
library is "code, not design", and the way to learn *which* code is to
write a program that needs it — the route `vec.set` and `vec.swap` took.
This one asked for two functions, `bytes.count_byte` and `bytes.field`,
and neither existed before it was written.

The other thing it found is the **arena as a line-length limit**. A
region is one 64 KiB chunk, the field bitmap is already in it, and a
65 000-byte line buffer beside a 1 025-byte bitmap traps on exhaustion.
So the 60 000 in the source is what fits, not what was wanted, and the
comment says so. GNU has no such limit because it grows; so would this,
on the heap with `std.buffer`, the way `sort/` does. The arena version
is the one that shows what an arena costs.

### `sort/` — the port with resources in it

`LC_ALL=C sort`: the files named on the command line, or standard input
when none are, sorted by byte order. Checked against GNU `sort` the same
way `base64/` is checked against GNU `base64` — seven input shapes, named
files, several at once, a missing file, and 1.2 MB past the first read.

This is the one to read for **linearity in a program that has some**.
Five owned resources, all on the heap: the text, two parallel runs saying
where each line is, and the permutation being sorted with its scratch.
`main` creates all five, lends them down, and destroys all five.

The shape that repeats is `out = buffer.push(heap, out, byte)` — the
library is move-based, so a loop that fills a buffer moves it round and
round. `docs/porting.md` §9.2 is honest about what that costs: nothing
hard, and three tokens every time.

Also worth reading for the rows. Four of the eight functions declare
`[]`, and they are the four doing the sorting — the effects live at the
edges, which is §9.3.

### `serve/` — a REST endpoint over a real socket

```sh
cargo run -p cancho -- vcs fetch --lock examples/serve/net.lock \
    --store packages/net-sockets/.cancho-vcs -o /tmp/net-sockets
cargo run -p cancho -- build --std examples/serve/serve.cho /tmp/net-sockets/*.cho -o serve
./serve 8080
```

The answer to *can this language do X?*, where X is the one everybody
asks. It binds a TCP port named on the command line, accepts one
connection, routes the request line and answers with JSON — `GET /health`
gets a 200, anything else gets a 404.

Eight `extern fn` declarations against libc and nothing else, plus two
byte-writing helpers — no longer written here, though: they moved to
`packages/net-sockets/sockets.cho`, this repository's first real
`cancho-vcs` package (`docs/package-system.md` §6), because
`results_stub/` below declared the exact same eight and two,
independently. `serve.cho` locks the names it needs in `net.lock` and
`import`s the fetched result rather than duplicating them. No socket
type, no `Net` capability, no HTTP module. The test suite makes the
request from Rust over loopback, both routes, and checks that the
`Content-Length` it declares is the body it sends — which it is by
construction, because the header and the body are assembled into the same
slice.

Read `main` first: three capabilities destroyed on three lines, so a
server that cannot touch a file, allocate on the heap or print to the
console. Then read `serve`, which is the whole socket lifecycle with the
capability *lent* in — it can call libc and it can do nothing else, and
its row says so.

`docs/reach.md` is the argument the program is evidence for, including
what this **cannot** reach and why it is one sentence rather than a list.

### `api/` — a JSON API server: keep-alive, routed, one thread

```sh
cargo run -p cancho -- vcs fetch --lock examples/api/server.lock \
    --store packages/http-server/.cancho-vcs -o /tmp/api-deps
cargo run -p cancho -- build --std examples/api/api.cho /tmp/api-deps/*.cho -o api
./api 8080                       # or: ./api 8080 reuseport 30   (share the port; 30 s without progress)
curl localhost:8080/users/42     # {"id":42,"name":"user-42"}
```

`serve/` answers one request and exits; this is the server the last four
library pieces (`std.json`, `std.map`, `std.http`, `std.route`) were for. The
loop itself is the `http.server` package (`packages/http-server/`,
[`docs/http-server.md`](../docs/http-server.md)) -- the first package that
imports `std` -- and `api.cho` is its routes, its handlers and a `main`. It
keeps every connection open behind a `Poller` (`epoll` on Linux, `kqueue` on
macOS) -- and holds no `Ffi` and declares no `extern fn`, which
`docs/native-sockets.md` is the account of -- answers pipelined requests
in order, reads requests that arrive in pieces, refuses what the parser
refuses (and closes), and routes four JSON endpoints. A silent client costs a
slot and nothing else, and so does one that stops reading its answers:
a `send` never waits for room (`MSG_DONTWAIT`, plus a one-millisecond send timeout because macOS ignores the flag), what the kernel does not take waits in the
connection's own buffer, and the connection is not read until it has gone.
On one core it does about 120,000 requests a second
against about 3,000 for FastAPI on the same request
([`docs/server.md`](../docs/server.md) §5 has the method and the caveats).

The sockets are the `net.sockets` package, as in `serve/`; `poll`, `signal`
and `time` are declared in the file. Read `drain` first (one connection's
bytes into answers), then `serve` (the loop and the dense connection array).

### `tls_echo/` — a TLS 1.3 server to copy

```sh
cargo run -p cancho -- build --std examples/tls_echo/*.cho \
    packages/tls/{tls,record,message,slot,client12,client,hello,identity,ticket,server}.cho \
    packages/x509/{verify,names,x509,key}.cho -o tls_echo
mkdir -p /tmp/certs && cp tests/vectors/tls/echo/first/* /tmp/certs/   # chain.pem, key.pem, names
./tls_echo --port 8443 --dir /tmp/certs --alpn echo
openssl s_client -connect localhost:8443 -servername echo.lex-sys.test \
    -CAfile tests/vectors/tls/echo/ca.pem -alpn echo -quiet
kill -HUP <pid>                                         # read the certificate again
```

The example `cancho-mqtt` and `cancho-gateway` copy
([`docs/tls-server.md`](../docs/tls-server.md) §11): `packages/tls`'s engine
between each socket and an echo, many connections on one thread and one
`Poller`, and **no foreign code**, so its authority report is bounded. Read
`front.cho`'s header first: the phases a connection goes through, how the two
handshake bounds (in progress, and started a second) delay a burst instead
of refusing it, the timeouts, the log line per connection with the suite,
group, SNI and ALPN. Then `identity.cho`, which is why the loop holds a
directory handle and never the filesystem capability: the reload reads
beneath one directory, following no link. `SIGHUP` reloads, `SIGTERM` sends
close_notify to everyone and exits. With `--tickets 2` it sends each client that can resume two session
tickets ([`docs/tls-server.md`](../docs/tls-server.md) §12; `--ticket-lifetime`, `--ticket-keys <file>` for keys shared by
several processes, read again on `SIGHUP`), and a client that comes back resumes with one key exchange and no
signature.


### `https_hello/` — HTTPS/1.1 over that engine

```sh
cargo run -p cancho -- build --std examples/https_hello/{hello,loop,app}.cho \
    examples/tls_echo/{front,identity}.cho packages/http-server/server.cho \
    packages/tls/{tls,record,message,slot,client12,client,hello,identity,ticket,server}.cho \
    packages/x509/{verify,names,x509,key}.cho -o https_hello
./https_hello --port 8443 --dir /tmp/certs
curl --cacert tests/vectors/tls/echo/ca.pem --resolve echo.lex-sys.test:8443:127.0.0.1 \
    https://echo.lex-sys.test:8443/hello/world
```

`tls_echo`'s program with [`packages/http-server`](../docs/http-server.md) where the echo was, fed bytes
instead of sockets (`http-server.md` §11): keep-alive, pipelining, a request answered in the order it came, a
`/big/<n>` answer streamed as the client takes it, and a client that stops reading stops being read, all in
one thread. Same options, same bounds, same reload, same authority report as `tls_echo`. Read `loop.cho`'s header
(where the bytes go and who waits for whom), then `app.cho` (what a request is turned into, and `hold` + `stream`). `POST /upload`
streams a body of any size through the 16 KiB buffer and answers its byte count and SHA-256 (`hold` + `proceed` + `body_part`/`body_take`, `http-server.md` §12);
`POST /upload/refuse` is refused with a 413 before a `100 Continue`; `--read-timeout` and `--max-body` bound a head and a body.
### `http_fetch_nb/` — many requests at once, on one thread

```sh
cargo run -p cancho -- build --std examples/http_fetch_nb/{fetch,fetch_io}.cho \
    packages/http-client/{wire,client}.cho \
    packages/tls/{tls,record,message,slot,client12,client,hello,identity,ticket,server}.cho \
    packages/x509/{verify,names,x509,key}.cho -o http_fetch_nb
./http_fetch_nb --repeat 100 http://127.0.0.1:8080/a http://127.0.0.1:8080/b
./http_fetch_nb --resolve echo.lex-sys.test=127.0.0.1 --repeat 3 \
    https://echo.lex-sys.test:8443/hello/world < tests/vectors/tls/echo/ca.pem
```

[`packages/http-client`](../docs/http-client.md) (a non-blocking HTTP/1.1 client with no socket in it) driven by a loop that owns the
sockets, the TLS engine and the poller: each URL is a lane that is requested again when it is done, over a connection the pool keeps,
so `--repeat 100` of two URLs is two connections. Plain TCP and TLS 1.3 (the chain and the **name** checked against the roots on
standard input; nothing is built in), streamed request bodies (`--post N`, `--chunked-post N`, `--expect`), `HEAD`, the client's
timers, and a replay of a request on a connection the upstream had closed (`--retry`). **A host is an IPv4 address or a name given
to `--resolve`**: resolving is the caller's (`http-client.md` §3.8). Read `fetch_io.cho`'s header (where the bytes go and who waits
for whom, the same rule `https_hello` states), then `fetch.cho` (what a caller does with the client's events).

### `fetch/` — the other direction

```sh
cargo run -p cancho -- vcs fetch --lock examples/fetch/connect.lock \
    --store packages/net-connect/.cancho-vcs -o /tmp/fetch-deps
cargo run -p cancho -- vcs fetch --lock examples/fetch/response.lock \
    --store packages/http-response/.cancho-vcs -o /tmp/fetch-deps
cargo run -p cancho -- build --std examples/fetch/fetch.cho \
    /tmp/fetch-deps/*.cho -o fetch
./fetch 127.0.0.1 8080 /health
```

An HTTP client, and the first program here that connects. It sends
`GET <path>` over HTTP/1.0 and streams the body to standard output:
header bytes are held until the blank line, and every later byte is
written straight through, so it needs no `Heap`. The test suite points
it at `serve/`, so a cancho client fetches from a cancho server.

The first program here with two real dependencies at once: `net.connect`
(`packages/net-connect/connect.cho`) for `octets_of`/`port_of`/
`connect_to` -- everything needed to turn `argv` into a connected
socket, generic to any outbound program -- and `http.response`
(`packages/http-response/response.cho`) for `send_all`/`status_of`, the
HTTP-specific, client-side mirror of `http.request`
(`docs/package-system.md` §6). Both now transitively require
`net.sockets` too (for `socket`/`close`/`write` respectively), so both
fetches land in the *same* output directory -- a store is always
exactly one file, so `net.sockets` materializes once no matter which of
the two closures reaches it first, rather than colliding as a duplicate
declaration the way fetching it into two separate directories would.

Its comments mark the three places where it works around the language,
and `docs/connect.md` is the report. The address has to be four octets
because nothing can resolve a name. `struct sockaddr_in` is written in
the Linux layout, which works on macOS only because BSD forgives it.
And *could not connect* is all it can say, because `errno` is behind a
pointer.

### `report/` — the second outbound program

```sh
cargo run -p cancho -- vcs fetch --lock examples/report/connect.lock \
    --store packages/net-connect/.cancho-vcs -o /tmp/report-deps
cargo run -p cancho -- vcs fetch --lock examples/report/response.lock \
    --store packages/http-response/.cancho-vcs -o /tmp/report-deps
cargo run -p cancho -- build --std examples/report/report.cho \
    /tmp/report-deps/*.cho -o report
./report 127.0.0.1 8080 /result "42"
```

Locks and fetches `net.connect` and `http.response` into one shared
directory, the same shape `fetch/`'s own section above uses.

`docs/net.md` §5 and `docs/connect.md` §6 put the bar at two askers per
half of the network before `Net` gets built, and `fetch/` was the only
outbound one. This is the second: an agent that posts a result rather
than a generic client, which is the shape a program that runs to a
number and has to tell someone actually takes. It sends `POST <path>`
with a body, so it needs a `Content-Length` going out the same way
`serve/`'s responses need one coming back — the one thing that differs
from `fetch/`, which never sends a body. Everything else about
connecting is copied from `fetch/` unchanged, because `docs/connect.md`
already settled it and a second program asking the same question gets
the same answer.

Sending a body large enough to matter, which `fetch/` never had to,
found two of its own bugs rather than any new design question: a
100,000-byte message copied into one scratch buffer overran a region's
64 KiB arena, and the fix for that still left the header buffer five
bytes short. Both are `docs/connect.md` §8.

### `collect/` — the second inbound program

```sh
cargo run -p cancho -- vcs fetch --lock examples/collect/request.lock \
    --store packages/http-request/.cancho-vcs -o /tmp/http-request
cargo run -p cancho -- build --std examples/collect/collect.cho /tmp/http-request/*.cho -o collect
./collect 8080 3
```

Locks and fetches `http.request` (`packages/http-request/request.cho`,
`docs/package-system.md` §4.6), the fourth real package and the first
that itself depends on a package (`net.sockets`). One fetch is enough --
a store is always exactly one file (`vcs publish` takes one input), so
fetching any of `http.request`'s own declarations transitively writes
the *whole* `net-sockets.cho` file too, closure-resolved and verified the
same way a direct dependency's own file is; every direct `sockets.*`
call `collect.cho` still makes resolves from that same fetched file, with
no separate `net.sockets` lock needed.

`report/`'s inbound counterpart, and `docs/listen.md` is its report.
`serve/` accepts one connection and never reads a body; `collect`
accepts a count of connections in a loop and reads each `POST`'s body
in full, by `Content-Length`, streamed to standard output the same way
`fetch/` and `report/` stream a response rather than materialising it
— the reason `docs/connect.md` §8's arena bug does not repeat here.
Found nothing new in the program itself, but found a real deadlock in
its own test: a 100,000-byte body overflows a pipe's kernel buffer,
and a test that only reads the child's stdout after the exchange
finishes blocks forever the moment that pipe fills. Draining it
concurrently, on its own thread, is the fix.

### `vsock/` — the third outbound program, and the first slice of `lex-os`

```sh
cargo run -p cancho -- vcs fetch --lock examples/vsock/net.lock \
    --store packages/net-sockets/.cancho-vcs -o /tmp/vsock-deps
cargo run -p cancho -- vcs fetch --lock examples/vsock/connect.lock \
    --store packages/net-connect/.cancho-vcs -o /tmp/vsock-deps
cargo run -p cancho -- vcs fetch --lock examples/vsock/wire.lock \
    --store packages/agent-wire/.cancho-vcs -o /tmp/vsock-deps
cargo run -p cancho -- build --std examples/vsock/vsock.cho \
    /tmp/vsock-deps/*.cho -o vsock
./vsock <cid> <port>
```

Locks and fetches `net.sockets`, `net.connect` and `agent.wire`
(`packages/agent-wire/wire.cho`) into one shared directory, the same
`fetch/`'s and `report/`'s own sections use -- `net.connect` now
transitively requires `net.sockets` too (`docs/package-system.md` §6),
so a separate output directory per package would fetch `net.sockets`
twice, under two different paths, which `build` refuses as a duplicate
declaration.

Connects over `AF_VSOCK`, the channel `lex-os-guest` uses to reach its
host supervisor, and — once connected — speaks one round of the real
`lex-os-proto` wire protocol: reads one newline-delimited `AgentViewMsg`
line, prints the goal and step it carries, and answers with a `Done`
action, encoded by hand rather than pulled in as a JSON library (the
decoder is `agent.wire`; the encoder differs too much between this file
and `agent_supervisor/`'s own to share — `docs/package-system.md` §6).
`docs/reach.md` §3.4 is the bug this program found scoping it:
a foreign *return*, not just a parameter, can cross at the wrong width.
The exchange itself was verified against real `serde_json` output and,
end to end, over a real `AF_UNIX` `socketpair` standing in for
`AF_VSOCK`'s byte-stream half — this sandbox has no `vhost_vsock`, so an
actual `AF_VSOCK` round trip against `lex-os-guest` stays untested here,
honestly, in the program's own comments.

### `results_stub/` — a real lex-os component, not a stand-in

```sh
cargo run -p cancho -- vcs fetch --lock examples/results_stub/net.lock \
    --store packages/net-sockets/.cancho-vcs -o /tmp/net-sockets
cargo run -p cancho -- build --std examples/results_stub/results_stub.cho /tmp/net-sockets/*.cho -o results_stub
./results_stub --listen 127.0.0.1:8443
```

Locks and fetches the same `net.sockets` package `serve/` does above --
`serve/`'s own section says why it exists.

`vsock/` and `agent_supervisor/`/`agent_guest/` are cancho *analogues*
of lex-os's own guest/supervisor exchange, checked against its wire
format but not built from lex-os's own source. This one is a real
lex-os component, ported: `lex-os/crates/results-stub`, the single
allowed-egress target the demo's manifest narrows to
(`lex-os` issue #10) — the epic issue's own long-unchecked box, "a
lex-os component ported/written in cancho (first production use)."

Checked directly against the Rust original's behaviour, not just its
intent: same HTTP/1.1 stub over a raw socket, same fixed `200`, same
per-request log line shape. Two places this port is honestly narrower,
both because of what this language's foreign-call boundary can and
cannot cross (`docs/reach.md` §3) rather than by oversight, and both
recorded in the file's own header comment — no peer address (`accept`'s
own two `NULL`s in `serve.cho` already made the identical call for the
identical reason), and a request body beyond its 200-byte preview
buffer is drained, not kept, since a whole unbounded body has nowhere
to live in a 64 KiB arena. `crates/cancho/tests/conformance/
backends.rs`'s `the_two_backends_answer_the_results_stub_port` builds
it on both backends, sends it a real HTTP request over loopback, and
checks the response and the log line both.

### `agent_supervisor/` and `agent_guest/` — the same exchange, over HTTP

```sh
cargo run -p cancho -- vcs fetch --lock examples/agent_supervisor/request.lock \
    --store packages/http-request/.cancho-vcs -o /tmp/http-request
cargo run -p cancho -- build --std examples/agent_supervisor/agent_supervisor.cho \
    /tmp/http-request/*.cho -o agent_supervisor

cargo run -p cancho -- vcs fetch --lock examples/agent_guest/connect.lock \
    --store packages/net-connect/.cancho-vcs -o /tmp/agent-guest-deps
cargo run -p cancho -- vcs fetch --lock examples/agent_guest/response.lock \
    --store packages/http-response/.cancho-vcs -o /tmp/agent-guest-deps
cargo run -p cancho -- vcs fetch --lock examples/agent_guest/wire.lock \
    --store packages/agent-wire/.cancho-vcs -o /tmp/agent-guest-deps
cargo run -p cancho -- build --std examples/agent_guest/agent_guest.cho \
    /tmp/agent-guest-deps/*.cho -o agent_guest

./agent_supervisor 8080 "write the report" 3 &
./agent_guest 127.0.0.1 8080
# goal: write the report
# step: 3
```

`agent_supervisor` locks and fetches `http.request` alone, the same
shape `collect/`'s own section above uses; `agent_guest` locks and
fetches `net.connect` and `http.response` into one shared directory,
the same shape `fetch/`'s own section uses, plus `agent.wire` for the
same `AgentViewMsg` decoder `vsock/`'s own section above uses.

The same guest/supervisor exchange `vsock/` plays over `AF_VSOCK`,
played over plain HTTP/1.0 instead — not a second transport for
`lex-os` (`lex-os-proto` names no HTTP channel, and none is proposed
here), but a channel this sandbox *can* round-trip end to end, over a
real socket, in real CI, where `vsock/`'s own round trip cannot be.
`agent_supervisor` is `serve/`'s and `collect/`'s shape (bind, accept
one connection, read a request's body by `Content-Length`); `agent_guest`
is `fetch/`'s and `report/`'s (connect, send a request with a body, read
the response). The exchange is inverted from `vsock/`'s own shape only
because HTTP's request is guest-initiated where a vsock stream lets the
supervisor push first: `agent_guest` `POST`s the action it would
otherwise have sent last, and `agent_supervisor` answers with the view
it would otherwise have sent next — a real `AgentViewMsg`, checked byte
for byte against real `serde_json` output for exactly that shape.

### `buffer/` — growing, written out

```sh
cargo run -p cancho -- run examples/buffer/main.cho examples/buffer/buffer.cho
# counting: 1 4 9 16 25 36 49 64
```

Worth reading for `reserve`, which is the *entire* implementation of
"growing": a bigger box, a copy, and the old one ended. The language has
no `realloc` — which would also hide which of two very different things
happened — so the doubling policy is in this repository rather than in
the compiler, and the cost is countable: building 31 bytes from a
one-byte buffer is three allocations, which is what valgrind reports.

`std.buffer` is this, promoted.

### `slab/` — shared ownership, as far as it reaches

```sh
cargo run -p cancho -- run examples/slab/main.cho examples/slab/slab.cho
# live handle:  7
# after remove: missing
# new handle:   9
# old handle:   missing
```

A slab owns every value and hands out `Gen { index, generation }`
handles, which are two plain `int`s and copy like any other `val`. The
line worth watching is the last: a handle whose slot was removed comes
back `Missing`, which is a **value the program decides what to do about**
rather than a dangling pointer it gets no say in.

It is also the before-and-after for tuples and shadowing — writing it is
what found both gaps. `insert` and `look` each had to answer with a slab
*and* something else, so each declared a `res struct` that was not a
concept in the library; one function had to be split in two because a
struct pattern could not rename what it binds; and `run` had eight names
for one slab because a name could not be rebound. All three are gone, no
rule was weakened, and the object file did not change.

### `modular/` — two modules and a root

```sh
cargo run -p cancho -- run examples/modular/main.cho \
    examples/modular/counts.cho examples/modular/text.cho
# seen 3, total 60
# 60
```

`fmt.text` holds the console helpers, `fmt.counts` holds a tally and
imports `fmt.text`, and `main` imports both — one under its own name and
one renamed with `as`, because a qualifier is a name the importing file
chose rather than a path. It is also where `pub` earns its place:
`counts.step` is private, so it is an implementation detail rather than a
promise.

### `wordfreq/` — the capstone

```sh
cargo run -p cancho -- run examples/wordfreq/main.cho \
    examples/wordfreq/text.cho examples/wordfreq/counts.cho
# dog 1
# lazy 1
# over 1
# jumps 1
# fox 2
# brown 1
# quick 1
# the 3
```

Three files: `text.cho` holds byte helpers, `counts.cho` holds the tally,
`main.cho` is the program. Every capability is in it doing real work —
arguments, file IO, the heap, matching through references, slices — and
`bump` is worth reading in particular: it walks the tally through a
*unique* reference and increments a count in place, which is what
matching `&!c` is for.
