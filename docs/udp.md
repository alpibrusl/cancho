# UDP: datagram sockets without `Ffi("libc")`

> **Status: built, both halves (§9 and §10); the round-trip measurement against C is not done.** Issue [#355](https://github.com/alpibrusl/cancho/issues/355).
> The asker is [cancho-dns#18](https://github.com/alpibrusl/cancho-dns/issues/18), a forwarding and caching
> DNS resolver. [`native-sockets.md`](native-sockets.md) §9 recorded "UDP and Unix sockets: no asker" and kept
> `Conn` free of a transport name in case one came; this is that second transport. Everything here is
> edition 5, on both backends, IPv4 (as `tcp_listen` and `tcp_connect` are today).

---

## 1. What is missing, and why a resolver needs all of it

cancho has no datagram socket. `std/` has none, `native-sockets.md` builds `Listener` and `Conn` over
`SOCK_STREAM` only, and `Poller` watches only those. DNS is UDP first (RFC 1035 §4.2.1); TCP is the fallback
for an answer that did not fit. A resolver that cannot speak UDP is not a drop-in for a stub resolver, and
its security story rests on things only UDP has: a random source port per upstream query, and a kernel that
drops a datagram from the wrong peer.

Three things a program must be able to do, each with an authority the report can name:

| the program wants to | authority | shape |
|---|---|---|
| ask one upstream a question and read its answer | `net_out("203.0.113.53:53")` | a **connected** datagram socket |
| answer clients on a port | `net_in("53")` | a **bound** datagram socket |
| reply to *this* client, later | none beyond the bound socket | a **peer** it was sent by |

## 2. The central decision: connected and bound, never "send to anything"

The issue's main question was what a `sendto` peer is checked against. The answer is that **there is no
`sendto` with a free destination**, because nothing could check one: the bound in a `Net` is a literal
(`net.md` §4), and a destination chosen at run time is not. So:

- **`udp_connect(net, host, port)`** checks `host` and `port` against the `Net`'s bound exactly as
  `tcp_connect` does (`connect.md` §10.1: a prefix on the host, equality on the port), resolves, and calls
  `connect(2)` on a `SOCK_DGRAM` socket. Its row is `net_out(bound)`, the same row `tcp_connect` has. From
  then on the kernel itself sends only to that peer and **drops datagrams from any other source**, so the
  report's claim ("this program talks to that upstream and nothing else") holds for sending *and* receiving.
- **`udp_bind(net, port)`** is `tcp_listen` for datagrams: `net_in(bound)`, a port, `SO_REUSEADDR`. It
  receives from anyone, as a server must, and it can send **only to a peer it received from** (§4). A bound
  socket therefore cannot be turned into a way to reach a host that has not first written to it.
- There is no `udp_send_to(&Udp, bytes, host, port)`. A program that wants a second destination connects a
  second socket and the report shows both.

This is the only shape in which cancho-dns's headline holds: reaching the upstreams in its generated set and
no others.

## 3. The handle and its verbs

One new `res` type, no literal form, one descriptor leaf, consumed by `udp_close` (`native-sockets.md` §3 and
`file-handles.md` §4.1: the handle is the capability). It is **not** `Conn`: a datagram socket has different
answers (an empty datagram is a datagram, and an oversize one is reported rather than cut), and a `Conn`
that sometimes had message boundaries would make every `conn_read` caller wonder.

```
enum UdpOpened  { Ok(Udp), Failed(int) }                     // errno; -1 = name did not resolve (as Dialed)
enum Datagram { Got(int), Truncated(int), Again, Failed(int) }
```

`Got(n)` is a datagram of `n` bytes, **`n` may be 0**. `Truncated(n)` means the datagram was `n` bytes and the
buffer held fewer; the buffer holds the first bytes and the rest are gone. A program that does not care reads
it like `Got`; a parser that must not act on a cut message refuses it, with its own rule tag. `Again` is
`EAGAIN`/`EWOULDBLOCK` on a non-blocking socket. `Sent` (existing) is reused: `Wrote(n)` is always the whole
datagram or the call fails, because a datagram is never partial, and `Again` is a full send buffer.

| builtin | signature | row |
|---|---|---|
| `udp_connect` | `(net, host, port) -> UdpOpened` | `net_out(bound)` |
| `udp_bind` | `(net, port, flags) -> UdpOpened` | `net_in(bound)` (`flags` bit 1 is `SO_REUSEPORT`, as `tcp_listen`'s) |
| `udp_recv` | `(&!Udp, &![byte]) -> Datagram` | `udp_recv` |
| `udp_send` | `(&!Udp, &[byte]) -> Sent` | `udp_send` |
| `udp_recv_from` | `(&!Udp, &![byte], &![int]) -> Datagram` | `udp_recv` (§4) |
| `udp_send_to` | `(&!Udp, &[byte], int) -> Sent` | `udp_send` (§4) |
| `udp_local_port` | `(&Udp) -> int` | `[]` (§5) |
| `udp_nonblocking` | `(&!Udp) -> int` | `[]` |
| `udp_close` | `(Udp) -> int` | `[]` |
| `poller_add_udp` | `(&!Poller, &Udp, token, events) -> int` | `poll` |

`udp_send` on a bound socket, and `udp_send_to` on a connected one, answer `Failed(EDESTADDRREQ)` /
`Failed(EISCONN)`-style errors from the kernel; the checker does not distinguish the two types (they are one
type, with a mode), and a test pins the answers. `poller_add_udp` registers for *readable*: a datagram socket
is writable nearly always, and a full send buffer is reported by `Again`.

A receive of a **zero-length buffer** answers `Failed(EINVAL)` without reaching the kernel, as `conn_read`
does (`native-sockets.md` §10 item 2): a datagram of any size is then `Truncated`, never silently empty.
`MSG_TRUNC` is what lets Linux report the real length; Darwin has no equivalent in `recv`, so there the
length is the buffer size and `Truncated` means "at least as big as the buffer" (§7 states it as unmeasured).

## 4. A peer is a ticket, not an address

A server replies to the client that wrote to it, often after waiting for something else (a resolver answers a
cache miss only after its upstream does). So the sender's address must outlive the next `udp_recv`. It cannot
be a number the program can write, or `udp_send_to(u, bytes, 0x7f000001_0035)` would be a free `sendto`.

`udp_recv_from` (a bound socket's receive; `udp_recv` stays the connected socket's and records nothing) writes
the sender into a **runtime ring** of 65,536 entries (a `sockaddr_in`, the ticket it was issued under and the
descriptor that heard it: 32 bytes each, 2 MiB of bss, one global in each backend, like `conn_detach`'s epochs in
`native-sockets.md` §10.3), and writes a **ticket** for it into the first cell of an `int` slice the caller
passes. (The first draft of this section had a separate `udp_peer(&!Udp)` that answered the last sender; an
out-parameter needs no per-socket state and cannot return another datagram's ticket, so it is gone.) A ticket
is the running count of datagrams received, so the entry it names is `ticket mod 65,536`. `udp_send_to` accepts a
ticket only if it is positive, the entry still holds that very ticket, **and** the entry was recorded by this
socket's descriptor, so:

- a number the program invented is refused (`Failed(EBADF)`), as `conn_attach(1)` is;
- a ticket from another socket is refused, so the reach of a bound socket stays "those who wrote to it";
- a ticket older than 65,536 received datagrams is refused. This is the one real limit, stated up front: a
  resolver holding a pending query across more than 65,536 other datagrams loses the reply, and the program
  gets `EBADF` it can count. Nothing is silently misdelivered.

The row for `udp_send_to` is `udp_send`, not `net_out`: the bound socket's authority is `net_in`, and
replying to a sender is not new reach. A test shows that a program with only `net_in("53")` cannot name any
other destination.

## 5. The source port

DNS needs an unpredictable source port per upstream query (the Kaminsky defence). Linux and Darwin choose
ephemeral ports for an unbound socket's `connect(2)`, Linux with randomisation. So the program's port for an
upstream query is the kernel's, and **the way to get a fresh one is a fresh socket**: `udp_connect`,
exchange, `udp_close`. That costs a `socket`, a `connect` and a `close` a query. `udp_local_port` answers the
port the kernel chose, for logging and for a test that two sockets differ.

What the design does *not* give is a program-chosen range or its own randomisation; that would need the
program to `bind` before `connect`, which needs a `net_in` row for a port it does not mean to listen on.
That is a real cost to the authority report and nothing asks for it yet.

## 6. Randomness stays out

The language has no randomness source (`tls-pure.md`: a ChaCha20 DRBG seeded from `/dev/urandom` through
`Fs`). Transaction IDs and 0x20 case randomisation reach it. That is cancho-dns's row to carry, not this
document's, and it is stated here so the two designs do not each assume the other solved it.

## 7. What building it must prove

1. **Mutation checks on every row**: `udp_recv` answering `Got` for a truncated datagram, dropping `Again`,
   ignoring the count; `udp_send` sending a prefix; a ticket check that skips the descriptor or the sequence.
   Each mutant is killed or explained.
2. **The forgery tests**: no builtin takes a descriptor or an address; `Udp { }` is rejected; a `udp_send_to`
   with an invented, a stale, a wrong-socket and a negative ticket is `Failed(EBADF)`.
3. **The authority tests**: a program that uses `udp_connect` over a narrowed `Net` reports `net_out` with its
   `host:port` and nothing foreign; one that uses `udp_bind` reports `net_in` with its port; `udp_connect` to a
   host outside the bound traps as `tcp_connect` does.
4. **Real sockets, both backends**: loopback echo; an empty datagram (`Got(0)`); an oversize datagram
   (`Truncated`); a datagram from a stranger to a connected socket is dropped (it never reaches the program);
   a peer that never answers (a non-blocking `udp_recv` is `Again`); readiness through a `Poller`.
5. **Measure**: a round trip against a C `recvfrom`/`sendto` loop on the same machine, reported plainly in
   this document whichever way it points. No claim of parity is written before the number is.
6. **Not verified, said so**: Darwin's `MSG_TRUNC` equivalent, `SO_NOSIGPIPE` irrelevance, and `kqueue` for
   the new handle are from platform headers and CI, not from a run here.

## 8. Slices

1. **This document.**
2. **The connected half**: `Udp`, `UdpOpened`, `Datagram`, `udp_connect`, `udp_send`, `udp_recv`,
   `udp_nonblocking`, `udp_close`, `udp_local_port`, `poller_add_udp`; tests and mutants.
3. **The bound half**: `udp_bind`, the peer ring, `udp_recv_from`, `udp_send_to`; the forgery tests (§10).
4. **`std.conns`-style table** for `Udp` tickets, if a program asks (a resolver holding many upstream sockets
   will); decided by the programs, not here. **A program asked** ([cancho-dns](https://github.com/alpibrusl/cancho-dns)
   D3: one connected socket per query in flight, a source port per query, the only form that keeps the
   defence of `docs/design.md` §6). Built as §11.

| Question | Settled |
|---|---|
| One type or two (bound and connected)? | **One**, with a mode; the kernel answers the wrong-mode call and a test pins it |
| `sendto` with a free destination | **No** (§2): unprovable against a literal bound |
| Peer as an address, an index or bytes | **A ticket** (§4), checked against the receiving socket |
| Source port | **The kernel's, a fresh socket per query** (§5); a program-chosen port needs a `net_in` it does not mean |
| Reuse `Conn` | **No**: empty datagrams and truncation are different answers |
| IPv6, multicast, broadcast, `recvmmsg`, Unix datagram, DTLS | **Out**, as the issue says |

## 9. What slice 2 built, and what is not done

Built, edition 5, both backends: `Udp`, `UdpOpened`, `Datagram`, `udp_connect`, `udp_send`, `udp_recv`,
`udp_local_port`, `udp_nonblocking`, `udp_close` and `poller_add_udp`. `udp_connect` is `tcp_connect`'s node
(`Expr::TcpConnect` with a `datagram` flag), so the bound check, the resolver walk and the `net_out(bound)` row
are the same code; `udp_send` is `conn_write`'s emitter and `udp_nonblocking` is `conn_nonblocking`'s.
`poller_add_udp` takes `events` as `poller_add_conn` does (§3's table is corrected to say so), and there is no
modify or remove: closing the socket removes it.

**Checked**, by `conformance/udp.rs` on both backends: a datagram exchanged with a peer; an empty datagram as
`Got(0)`; a 100-byte datagram into 16 bytes as `Truncated(100)` (Linux); a datagram from a stranger never
reaching a connected socket; `Again` on a non-blocking socket with nothing waiting, and `Failed(EINVAL)` for an
empty buffer; two sockets with two distinct ephemeral ports; a `Poller` reporting readability; `udp_connect` to a
host or a port outside the bound trapping; and the authority report (`net_out` with the `host:port`, `udp_send`
and `udp_recv`, no `ffi`, no `net_in`). Five reject fixtures: a forged `Udp { }`, a `Udp` taken apart, one left
open, `udp_recv` without its label, and the builtins at edition 4.

**Mutants**, all killed: truncation never reported (each backend); `MSG_TRUNC` dropped on Linux; the empty-buffer
guard moved; a datagram `connect` over a stream socket (each backend); the local port always 0; the local port's
high byte read from the wrong offset (LLVM); `udp_nonblocking` a no-op. Two survived the first version of the
tests and were fixed by strengthening them, not by excusing them: the port test only checked "positive and
different", which a port read from the wrong bytes also satisfies (it now requires an ephemeral port, above
1023), and a blocking receive hung the suite instead of failing it (the harness now kills a program after 20
seconds).

**Found.** Adding builtins and types moved four things that pin counts, each corrected in place: the generated
`examples/selfhost/tables.cho` (regenerated with `UPDATE_SELFHOST_TABLES=1`), the WASI refused set (48 to 55,
`docs/wasm.md`), the checker's label count (26 to 28: `udp_recv`, `udp_send`), and the type and builtin counts in
`docs/self-hosting.md`. The datagram builtins are refused on WASI as sockets are.

**Not done in slice 2.** The bound half (§10). The round-trip measurement against a C loop (§7 item 5): no
number is claimed. **Darwin:** the nine tests above passed in CI on the `darwin-aarch64` runner (the `Truncated`
rule, `getsockname` and the `kqueue` registration included); that is the CI result, not a run on a Mac here.

## 10. What slice 3 built, and what is not done

Built, edition 5, both backends: `udp_bind(net, port, flags)`, `udp_recv_from` and `udp_send_to`, and the peer
ring behind them. `udp_bind` is `tcp_listen`'s node (`Expr::TcpListen` with a `datagram` flag, and a backlog of
0 the lowering supplies), so the port check against the `Net`'s bound, `SO_REUSEADDR`, `SO_REUSEPORT` and the
`net_in(bound)` row are the same code; it simply does not call `listen`. The ring is one zeroed global,
`lexs_udp_peers`, in each backend.

**The decision that held.** A bound socket has no free destination: `udp_send_to` takes a ticket and nothing
else. A program holding only `net_in("PORT")` has no `net_out` in its report (checked), and cannot name a host
that has not written to it.

**Checked**, on both backends: a server answering the sender it heard from; two senders answered in the
*opposite* order of arrival, each getting its own reply (the ticket outlives the next receive); five forged
tickets refused with `Failed(EBADF)` and nothing sent, the client then seeing exactly the one real reply (a
ticket never issued, `0`, negative, and the same slot one ring's length ahead and behind); a ticket from a
different bound socket refused; a ticket older than the ring refused (the program feeds itself 65,536 datagrams
through a connected socket, so no client can drop any, then replies late); an empty ticket slice as
`Failed(EINVAL)` and a receive that delivered nothing leaving the cell untouched; `udp_bind` outside the bound
trapping; and the authority report (`net_in` with the port, `udp_send` and `udp_recv`, no `net_out`, no `ffi`).
One more reject fixture, `udp_send_to_not_declared`.

**Mutants**, all killed on each backend that has the code: the owner check dropped; the ticket not compared; the
ticket compared only modulo the ring (killed by the ticket one ring ahead); `listen` called on a datagram
socket.

**The limit, as designed.** A reply held across more than 65,536 other received datagrams is refused. At 30,000
datagrams a second that is about two seconds, which is longer than a resolver waits for an upstream but not by
an order of magnitude; a program that holds replies longer must answer or drop them sooner. Only
`udp_recv_from` fills the ring, so a forwarder's connected sockets (`udp_recv`) do not age its clients'
tickets.

**Not done.** The round-trip measurement against a C loop. A `std` table of `Udp` handles for a program that
holds many upstream sockets (slice 4: built, §11). The ring is a fixed size; a larger one is a
constant in `socket_os.rs`. Darwin: this slice's tests (the ring, the tickets, `udp_bind`) passed on the
`darwin-aarch64` CI job of #358; that is the CI result, not a run on a Mac here.

## 11. Slice 4: tickets for datagram sockets

**Why now.** cancho-dns D3 sends every upstream query from a fresh connected socket, so that its source port is
the kernel's and unpredictable (§5), and it has hundreds of queries in flight. `Udp` is a resource and `std.vec`
holds only copyable things (`native-sockets.md` §10.3), so there was nowhere to put them: found by reading the
forwarder's design against the language before writing it, not by a failure.

**What it is.** `conn_detach`/`conn_attach` again, for `Udp`:

- `udp_detach(Udp) -> int` ends the handle, leaves the descriptor open and answers a **ticket**; `-1` means the
  descriptor is past the epoch table and the socket was closed rather than leaked.
- `udp_attach(int) -> UdpOpened` redeems a ticket **once** (`Ok(Udp)`), or `Failed(EBADF)`: never issued, already
  redeemed, copied, forged, of the other kind, or for a descriptor since reused. `UdpOpened` is reused; no new type.
- `std.udps`: `put`, `send`, `recv`, `nonblocking`, `local_port`, `watch`, `close` and `drop` over **a `std.conns`
  `Table`** (`empty`, `live` and `slots` are `conns`'s). A table holds tickets, not sockets, and the kind bit below
  makes each verb refuse the other kind's, so one table may hold both; `conns` gained `store`, `replace`, `ticket_at`
  and `release_slot` as public so that the two modules share the slot bookkeeping instead of copying it (the
  repository refuses a duplicated function body).

**One epoch table, a kind bit.** Both kinds of ticket use the descriptor epochs of `native-sockets.md` §10.3, because
a descriptor is one thing whatever it carries. Left like that, a `Conn`'s ticket would redeem as a `Udp` and the
reverse: memory-safe, since both are descriptors, but the wrong verbs on the wrong socket. So a `Udp` ticket sets
**bit 31 of its descriptor half**. `conn_attach` reads all 32 bits, finds a descriptor beyond the 65,536-entry table
and refuses it; `udp_attach` requires the bit and reads the other 31. No second table, and the 31-bit epoch and
the odd/even rule are unchanged.

**Built and tested** on both backends (`tests/conformance/udp.rs`): a ticket from `udp_connect` redeems and the
socket still sends and receives; eight forged redemptions refused (zero, negative, one, `i64::MAX`, the real ticket
plus one, plus two epochs, with the kind bit cleared, and redeemed as a `Conn`); a second redemption refused; a
`Conn`'s ticket refused by `udp_attach` and not spent by the refusal; and a three-socket table (slots in order, each
send and receive finding its own peer, a closed slot refusing stale use and being reused, `drop` closing the rest).
The builtins are refused on WASI with the others (`Gap::Sockets`) and are edition 5.

**Not done.** Nothing here measures what a table costs per datagram (two extra builtin calls and two table accesses,
as `native-sockets.md` §10.3 says for `Conn`); cancho-dns D3 is the first program that will.
