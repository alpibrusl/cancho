# UDP: datagram sockets without `Ffi("libc")`

> **Status: design; nothing built.** Issue [#355](https://github.com/alpibrusl/cancho/issues/355).
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
| `udp_bind` | `(net, port) -> UdpOpened` | `net_in(bound)` |
| `udp_recv` | `(&!Udp, &![byte]) -> Datagram` | `udp_recv` |
| `udp_send` | `(&!Udp, &[byte]) -> Sent` | `udp_send` |
| `udp_peer` | `(&!Udp) -> int` | `[]` (§4) |
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

`udp_recv` on a **bound** socket records the sender in a **runtime ring** of 65,536 entries (a `sockaddr_in`
and a sequence number each, about 1.5 MiB of bss, one global in each backend, like `conn_detach`'s epochs in
`native-sockets.md` §10.3). `udp_peer(&!Udp)` answers a ticket for the sender of the **last datagram this
socket received**: `sequence << 16 | slot`. `udp_send_to` accepts a ticket only if the slot still holds that
sequence **and** was recorded by this socket's descriptor, so:

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
3. **The bound half**: `udp_bind`, the peer ring, `udp_peer`, `udp_send_to`; the forgery tests.
4. **`std.conns`-style table** for `Udp` tickets, if a program asks (a resolver holding many upstream sockets
   will); decided by the programs, not here.

| Question | Settled |
|---|---|
| One type or two (bound and connected)? | **One**, with a mode; the kernel answers the wrong-mode call and a test pins it |
| `sendto` with a free destination | **No** (§2): unprovable against a literal bound |
| Peer as an address, an index or bytes | **A ticket** (§4), checked against the receiving socket |
| Source port | **The kernel's, a fresh socket per query** (§5); a program-chosen port needs a `net_in` it does not mean |
| Reuse `Conn` | **No**: empty datagrams and truncation are different answers |
| IPv6, multicast, broadcast, `recvmmsg`, Unix datagram, DTLS | **Out**, as the issue says |
