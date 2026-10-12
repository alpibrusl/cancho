# The peer of a connection: `conn_peer`, `std.addr`, `conns.peer`

> **Status: built on both backends (§10); WASI refuses it with the other socket builtins; macOS arm64 and Linux arm64 run, Linux x86-64 is CI's.**
> Two programs ask. `cancho-gateway` (an HTTP/1.1 reverse proxy terminating HTTPS) needs the client's address for
> `X-Forwarded-For` and for per-address bounds; `cancho-mqtt` (the broker; its design section 7d reads "no per-source
> limit (`std.conns` gives no peer address)") needs it for a per-source limit on handshakes and connections.
> [`http-server.md`](http-server.md) §11.8 item 3 and [`tls-server.md`](tls-server.md) §11 name it, and
> [`native-sockets.md`](native-sockets.md) §9 recorded it as "additive and costs nothing now; no asker until an access
> log exists". There are now two askers, which is the bar ([`standard-library.md`](standard-library.md)).

---

## 1. What is missing

`tcp_accept` answers `Accepted::Ok(Conn)` and nothing about who connected. The kernel knows: `accept(2)` can fill a
`sockaddr`, and `getpeername(2)` answers it for any connected socket. A program written against `Ffi("libc")` could ask;
a program written against handles (everything since `native-sockets.md`) cannot, and `conn_raw_fd` is the escape hatch
that §6 of that document exists to make unnecessary. What the programs do with the answer:

| use | needs |
|---|---|
| a log line per connection (`examples/tls_echo`, an access log) | text of the address and port |
| `X-Forwarded-For: <client>` | text of the address, no port, no brackets |
| at most N connections and M handshakes a second from one source | a **key** that is equal for the same source and cheap to compare |
| the same bound surviving the client's own address rotation | a key coarser than one IPv6 address |

## 2. Measured before designing

Three things decided the design, each measured on this repository's compiler and on the two kernels it targets
(macOS 26 arm64 and Linux arm64 in Docker; the probes are `scripts/conn_peer_probe.c` and `scripts/conn_peer_dual.py`).

| question | Linux | macOS |
|---|---|---|
| `getpeername` on an open connection | answers | answers |
| ...after the peer sent FIN and closed | **answers** | **answers** |
| ...after the peer reset (`SO_LINGER` 0, close) | **`ENOTCONN` (107)** | **`EINVAL` (22)** |
| cost of one `getpeername` call | 237 ns | 235 ns |
| connect + accept + two closes on loopback | 25.4 µs | (stalled under load; not measured) |
| the same with one `getpeername` after accept | 21.2 µs (within noise of the line above) | |
| IPv6 listener (`::`), client dials `127.0.0.1` | peer is `::ffff:127.0.0.1` | peer is `::ffff:127.0.0.1` |
| `IPV6_V6ONLY` default | 0 | 0 |

And one fact about the compiler: **every cancho socket is IPv4 today.** `tcp_listen` and `tcp_connect` use
`AF_INET` (`connect.md` §10: "no IPv6"; `udp.md` header), so a client on `::1` cannot reach a cancho listener at all.
That does not make the IPv6 half of this design speculative: the address type must not need to change the day dual-stack
listening lands, and a program that is handed a `Conn` by a future listener must already be able to read its peer.
It does change how the IPv6 half is *tested* (§10).

## 3. The API

Three layers, each the smallest that is useful, none with an effect row.

### 3.1 The builtin: `conn_peer(&Conn, &![byte]) -> [] int`

Edition 5, both backends, refused on WASI with the other socket builtins (`wasm.md`; the refused count moves 58 to 59).
It calls `getpeername(2)` on the connection's descriptor and writes **19 bytes** into the buffer:

```
byte 0        family: 4 or 6
bytes 1..17   the address, 16 bytes, network order; an IPv4 address in bytes 1..5, bytes 5..17 zero
bytes 17..19  the port, big-endian
```

It answers `0`, or the `errno`: whatever `getpeername` said (`ENOTCONN` after a reset on Linux, `EINVAL` on macOS: §2),
and `EINVAL` (22) for a buffer under 19 bytes or a socket that is not IP (`AF_UNIX`, nothing else exists today),
**before anything is written to the buffer**. The family byte is `4` and `6`, not `AF_INET6`, because that is 10 on Linux
and 30 on macOS and a program must not carry a table of them (`native-sockets.md` §3 on `EAGAIN`, same reason). The kernel's
`sockaddr` layout differences (a `u16` family on Linux, a length byte then a `u8` on Darwin) stop in the backend.

The connection is borrowed shared (`&Conn`): `getpeername` moves no stream. The buffer is borrowed unique (it is written).

### 3.2 `std.addr`: the value, its text, its key

Pure, no heap, no effect. A **`Peer`** is `{ family, w0, w1, w2, w3, port }`: the address as four big-endian 32-bit
words (an IPv4 address is `w3`; the rest zero) and the port. It is a `val` struct: copied freely, compared by `same`
and `same_address`, held in a thousand-slot table without ceremony.

| function | |
|---|---|
| `v4(a, b, c, d, port)`, `v6(w0, w1, w2, w3, port)` | constructors; each normalises (§4) |
| `decode(raw) -> Parsed` | a `Peer` from the 19 bytes `conn_peer` wrote; `Bad` for another length or family |
| `text(p, out) -> int`, `text_port(p, out) -> int` | §6; the length, or -1 for a buffer under `max_text()` = 47 bytes |
| `parse(s) -> Parsed` | the inverse of `text` (port 0); `Bad` for anything that is not an address |
| `key(p) -> Key`, `same_key(a, b)`, `key_family`, `key_bits`, `key_from` | §5 |
| `family`, `port`, `word`, `with_port`, `same`, `same_address` | accessors |

### 3.3 `std.conns.peer(table, slot) -> Peered`

```
pub enum Peered { Known(addr.Peer), Unavailable(int) }     // the errno
pub fn peer[&t](table: &!t Table, slot: int) -> [] Peered
```

It is `attach`, `conn_peer`, `detach` like every other operation in the module, so the linearity the checker enforces does
not stop at the table's edge. An empty or out-of-range slot is `Unavailable(9)` (`EBADF`), as `read` and `write` answer.

## 4. IPv4-mapped IPv6 addresses are IPv4

A dual-stack listener reports an IPv4 client as `::ffff:a.b.c.d` (§2, measured on both kernels). A bound that counts
`1.2.3.4` and `::ffff:1.2.3.4` as two sources is a bound a client halves by choosing which socket family to dial with.
So **every constructor in `std.addr` normalises `::ffff:0:0/96` to the IPv4 address**, `decode` and `parse` included,
and a `Peer` made by this module never has `family 6` with `w0 = w1 = 0` and `w2 = 0xffff`. Nothing else is rewritten:
`::a.b.c.d` (IPv4-compatible, deprecated since RFC 4291), `64:ff9b::/96` (NAT64) and `2002::/16` (6to4) are different
addresses with different owners, and guessing which IPv4 host they "really" are is not this module's business.

## 5. The key for a per-address bound

`key(p)` is `{ family, bits }`: for IPv4 the whole 32-bit address; for IPv6 the first 64 bits, **the /64**. A /64 is
what one subscriber is handed, so a client rotating the low 64 bits (privacy extensions do, on purpose) does not obtain a
fresh allowance with each address. IPv4 is a /32 because that is what one address is.

It is a **pair**, not one integer, because the two spaces do not fit in 63 bits and a packing with a collision is worse
than none for a security bound (`::/64` would equal IPv4 `0.0.0.0`; a /64 key as `w0<<32|w1` is any `int`, including
negative ones). `same_key` compares; a program stores the two ints (`key_family`, `key_bits`) wherever it keeps per-slot
state and rebuilds with `key_from`. The prefix lengths are fixed (§11, question 2): the first asker that needs a /56 or a
/48 for a hostile network says so.

## 6. Text

`text` writes `1.2.3.4`, or an IPv6 address in the canonical form of RFC 5952: lower-case hex, no leading zeros, the
longest run of **two or more** zero groups written `::` (the first on a tie), a lone zero group written `0`. This is the
form for a log and for `X-Forwarded-For` (RFC 7239's `for=` additionally wants brackets and quotes for IPv6 and for a
port, which is the proxy's concern, not the address's). `text_port` writes `1.2.3.4:80` and `[::1]:80`.

`parse` accepts exactly what RFC 4291 §2.2 allows (full form, `::`, a trailing dotted quad) with either case of digits,
and refuses what it does not: brackets, a zone (`%eth0`), a prefix length, space, a dotted-quad octet with a leading zero
(`010` is eight in some parsers and ten in others, so it is neither), more than four hex digits in a group, two `::`.
`parse(text(p))` is `p` with port 0, for every `Peer` this module makes; checked (§10) against Rust's `std::net` as an oracle on a
generated corpus rather than on the author's examples.

## 7. When the address is available, and what it costs

**`conn_peer` asks the kernel when called (`getpeername`); `accept` stores nothing.** The alternative, storing the
`sockaddr` `accept` already returns in a per-descriptor table beside the epoch counters (`native-sockets.md` §10.3), was
rejected for these reasons:

- **Cost.** One call is 237 ns; a whole loopback connection is 21 to 25 µs (§2), so the call is about 1% of the cheapest
  possible connection and a real TLS connection (4 ms of handshake CPU, `tls-server.md` §6) makes it negligible. The stored
  design costs a 64K-entry table (about 1.5 MiB of bss at 24 bytes an entry, against the epoch table's 256 KiB) and a change
  to `accept_cloexec` on both backends, for a call a program makes once per connection.
- **No new state to go stale.** A stored address would have to be cleared by `conn_close`, `conn_detach` and every path that
  ends a descriptor; the epoch table exists because forgetting one is how a stale value reaches the wrong connection.
- **`tcp_connect` conns work too**, with the same builtin: the peer is the host that was dialled.

What the choice costs the program: **a connection the other end has reset has no peer to ask about** (§2: `ENOTCONN` on
Linux, `EINVAL` on macOS), where a stored address would still be there. The rule for a caller is therefore: **ask once,
immediately after `put`, and keep the answer** if it is needed later (a per-address count must be decremented under the
key it was incremented under, and `close` is when the key can no longer be asked for). A peer that resets between `accept`
and the call is a connection that is already dead; `Unavailable` says so and the program closes it. `examples/tls_echo`
does exactly this and §10 records the test of the race.

## 8. Transports other than TCP

`std.conns` holds `Conn`, and a `Conn` is a connected `SOCK_STREAM` socket from `tcp_accept`, `tcp_connect` or
`tcp_connect_start`. There is no Unix-domain socket in the language (the `AF_UNIX` in the process runtime is the channel
pair of `processes.md`, whose verbs are `Pipe`'s, not `Conn`'s). The datagram sockets of `udp.md` are a different handle with
a different, deliberate design (a sender is a ticket, never an address, so that a received address cannot become the
destination of a `sendto`); this change does not touch them, and §11 asks whether it should. If a `Conn` ever is not IP,
`conn_peer` answers `EINVAL` and `peer` answers `Unavailable(22)`; nothing here guesses a family.

## 9. Authority

The address is **data the program receives, not authority to dial it.** `tcp_connect` still checks its destination against
the `Net` capability's bound at the call site (`net_out("host:port")`, `connect.md` §4), and a `Peer` is four integers: a
program holding one for `203.0.113.7` and a `Net("api.internal:443")` still traps if it dials `203.0.113.7`. `conn_peer`
has **no effect row** (like `conn_nodelay`, `conn_nonblocking`, `conn_connect_status`: it names no resource, it asks about a
handle already held), `std.addr` is pure, and `conns.peer` is `[]`. So no label is added to `cancho authority` for a program
that calls it, no capability widens, and `narrow` is untouched. A conformance test compares the authority report of a server
with and without the call, and of `examples/tls_echo` before and after this change.

What the address is **not**: it is the *socket peer*, the other end of the TCP connection as the kernel sees it. It is not
an identity claim and is not authenticated by anything above TCP. Behind a reverse proxy or a load balancer it is the proxy's
address, and the real client's is in a header or a PROXY-protocol preface that this change does not parse (**PROXY
protocol is out of scope**; a program that sits behind one wants `std.addr.parse` for the address it carries, which exists).
Behind carrier-grade NAT many subscribers share one IPv4 address, so a per-address bound is a bound on a NAT, not on a person;
that is the bound's honest meaning and the reason an operator sets N above the number of people they expect behind one address.
A completed TCP handshake cannot be forged blind, so a *connection's* address is not spoofable by an off-path attacker, as a
datagram's is.

## 10. What was built, and what building it found

**Built.** `conn_peer(&Conn, &![byte]) -> [] int` (edition 5, both backends, no row; refused on WASI, so the builtin count is 134
and the refused count 59, `os_tables.rs` and `wasm.md` moved with it), `std/addr.cho` (new; pure), `conns.peer` in
`std/conns.cho`, `crates/cancho/tests/conformance/conn_peer.rs`, and in `examples/tls_echo/front.cho`: `peer=` on every
connection line, `--per-address`, `--per-address-rate`. Nothing in `packages/` changed (no package store to republish; `python3
scripts/publish_packages.py --check` says so). The IPv4-mapped normalisation, the `/32` and `/64` key, the canonical text and the
parser are in `std/addr.cho`; none of it needs the compiler.

**Tests**, each on both backends where it builds a program (`cargo test -p cancho --test conformance conn_peer`, 8 tests, all of
them also run on Linux arm64 in Docker):

| test | what it shows |
|---|---|
| `every_slot_answers_its_own_clients_address_and_port` | 11 clients at once from 11 distinct source ports; each slot's peer is its own client's `127.0.0.1:<port>`; slot 99, slot -1 and a closed slot are `Unavailable(9)`; the slot freed by `close` and taken by a later client answers *that* client's port, and slot 1 is unchanged |
| `a_connection_that_ended_has_a_peer_only_if_it_was_not_reset` | a client that closed (FIN) still has its address; one that reset (`SO_LINGER` 0) is `Unavailable(107)` on Linux and `Unavailable(22)` on macOS, as §2 measured |
| `the_builtin_fills_nineteen_bytes_and_refuses_fewer_without_writing` | an 18-byte and an empty buffer answer 22 and are left as they were (filled with 170); 19 bytes are filled in the §3.1 layout with the tail of an IPv4 address zero; a 25-byte buffer is written 19 bytes deep and no further |
| `an_ipv6_peer_is_read_and_an_ipv4_one_on_the_same_socket_is_normalised` | an `AF_INET6` dual-stack socket made by the test is put under the program's listener (`dup2`, through `Ffi("libc")`: §2 says no cancho socket is IPv6, and this is the only way to run the IPv6 half). A client from `::1` is `[::1]:<port>` with key `6 0`; a client from `127.0.0.1` on the same socket, which the kernel reports as `::ffff:127.0.0.1`, is `127.0.0.1:<port>`, key `4 2130706433` |
| `the_address_type_agrees_with_rust_on_a_generated_corpus` | `parse`, `text`, `text_port`, `decode` and `key` against Rust's `std::net` parser and an independent RFC 5952 formatter over 9,000-odd lines: IPv4 and IPv6 in six spellings (canonical, upper case, zero-padded, `::` over any run, a dotted tail), the mapped range and its neighbours, one- to three-character mutations of all of those, random strings of the alphabet, and 1,500 raw buffers with families 0, 4, 5, 6 and 255. Zero disagreements; every line answered (a trap would be a signal, not exit 0). Each parsed address also round-trips through `text` and through a mapped raw buffer inside the program |
| `the_address_api_does_what_the_document_says` | masking, the mapped boundaries (`::fffe:`, a nonzero second or first word), `same` against `same_address`, the key (a /64 shares, one bit of the /64 differs, the two families never collide even for `::` and `0.0.0.0`, a key rebuilt from its parts), the buffer limits, `decode` refusals |
| `asking_for_the_peer_adds_nothing_to_the_authority_report` | the authority report's effects and labels are byte-identical for a server with and without the call; no label mentions a peer |
| `knowing_an_address_does_not_let_a_program_dial_it` | with `Net("127.0.0.1:9")` and `addr.v4(203, 0, 113, 7, 80)` in hand, `tcp_connect` to that host traps, as it did before |

The existing `the_example_reports_a_bounded_authority_and_no_foreign_code` (`tls_echo.rs`), which pins every label of
`examples/tls_echo`, passes unchanged: the example now asks for each peer and reports the same fifteen labels.
`scripts/tls_echo_test.py` has three more cases: `peer`, `per-address` and `addr-rate` (§ of `tls-server.md` 11); on Linux all
three run in full, including a client from `127.0.0.2` that is not affected; on macOS the second-address step says it is skipped.

**What building it found.**

- *Every cancho socket is IPv4* (§2). The IPv6 half of the type exists for the day a listener is dual-stack, and is tested
  through `dup2`; it is not reachable by a program that does not hold `Ffi`. A gateway that wants IPv6 clients needs
  dual-stack `tcp_listen` first (question 4).
- *macOS and Linux disagree about a reset connection* (`EINVAL` against `ENOTCONN`), which the first draft of the library
  comment had wrong (it said `ENOTCONN` for both); corrected in `std/conns.cho` and here.
- *The first draft of `hex_value` and `hex_digit` in `std/addr.cho` were copies of `std/json.cho`'s and an example's*; the
  repository's duplication check (`conformance/duplication.rs`) refused them, and they were rewritten rather than shared,
  because `std.addr` importing `std.json` for a nibble would put a parser in every program that logs an address.
- *`tls_echo`'s log lines are matched by substrings in four test files*, so `peer=` goes after the connection id
  (`conn 3 peer=... established ...`, `refused 7 peer=... full`) and every existing match still holds. `case_full` matches
  `endswith(" full")`, which is why the refusal reason stays last.
- *The per-address handshake bound needs a table that outlives the connection*, because the attack it answers is handshake
  spam by connections that end at once. The example keeps 1,024 keys per one-second window in the loop's global array
  (`front.cho`, `g_ak`); the state is shared by `https_hello`, which therefore accepts the same two flags (not tested there).

**Mutants** (`scripts/conn_peer_mutants.py`; the compiler is rebuilt for each; 54 single edits to `std/addr.cho`, `std/conns.cho` and
each backend's `conn_peer`, run against `conformance conn_peer`): **51 killed on macOS and the other 3 killed on Linux, 54 of 54**,
and one equivalent mutant with its argument. The first run found five survivors, which is what the run is for: `from_parts` was
untested (the example was its only user), `v4` did not mask its octets in the word (the text hid it, since it masks again), a dotted
quad in the middle of an address was never in the corpus, and the errno `conns.peer` and each backend pass up was
indistinguishable from `EINVAL` on macOS, which is what a reset connection answers there; the first three got tests and the errno
three are killed on Linux by the `ENOTCONN` expectation (`LINUX_ONLY` in the script says so, and the script run in Docker kills
them). The equivalent one is the check for a leading `:` in `parse`, which a few lines later is refused anyway as an empty group.

**Measured**, on macOS 26 arm64 under load from other jobs, `examples/tls_echo` with `--per-address-rate 2` against six
simultaneous clients: they complete in 2.02 s with the last having waited 1.99 s (3 windows of 2), and on Linux a client from
`127.0.0.2` took 48 ms meanwhile.

**Not verified.** Linux x86-64 (CI is; here Linux was arm64 in Docker); the Darwin `sockaddr` layout is exercised on this Mac
(arm64) and not on x86-64 macOS; WASI (the builtin is refused there, and `wasm.md` records the count); `https_hello` with the new
flags (it builds, serves, and logs the peer through the shared `front.cho`, but no per-address case runs against it); a flood from
more than 1,024 distinct addresses against the key table; the `X-Forwarded-For` header (the gateway's change).

## 11. Open questions, with proposed answers

1. **Should `accept` store the address so a reset connection still has one?** Proposed: no (§7). Revisit if a program
   needs the address of a connection it never got to ask about; the builtin is additive either way.
2. **Configurable prefix lengths for the key** (a /56 or /48 for IPv6, a /24 for IPv4 behind a hostile network)?
   Proposed: not yet. `key` stays a pure function of a `Peer`; a program that needs another prefix can build its own from
   `word` in a few lines, and the first asker that does decides the signature.
3. **The peer of a UDP datagram.** `udp_recv_from` deliberately hides it behind a ticket (§8). A DNS server wants a per-source
   rate limit exactly as the broker does. Proposed: a separate change that adds `udp_peer(&Udp, ticket, &![byte]) -> int`
   (it grants nothing: the ticket was already the way to reply), after cancho-dns asks.
4. **Dual-stack listening** so an IPv6 client can connect at all (§2). Proposed: a separate change (`tcp_listen` flags), as
   it touches the bound's meaning (`net_in("8080")` today means IPv4 port 8080). This change's address type does not wait for it.
5. **The local address** (`getsockname`: which of its addresses a client dialled). No asker; `udp_local_port` exists for
   the one case that did. Proposed: not now.
6. **`X-Forwarded-For` formatting helpers** (appending to an existing header, RFC 7239 `for=`). Proposed: `packages/http-server`
   owns it, as its own change after this one; `std.addr.text` is the primitive.
