/* net.c -- the sockets and the clock, for the code of the compiler written in cancho.
 *
 * SOURCE of `net_linux.ll` and `net_darwin.ll`, which `selfhost_tables.rs` embeds in `runtime.cho` and `driver.cho`
 * prints into every module (the one of the target it builds for). Made with `make.sh` in this directory:
 *
 *     clang -S -emit-llvm -O0 -Xclang -disable-O0-optnone -DDARWIN=0 net.c   (and -DDARWIN=1)
 *
 * with no system header: every constant that differs between the two systems is below, the same ones the Rust
 * backend uses (`cancho_ir::SocketOs`), and the one function that reads `errno` is the module's own
 * `cancho_errno_location`. A function that answers more than one value writes them to `out`.
 */
typedef long i64;
typedef unsigned char u8;

/* Every trap is the Rust backend's instruction (SIGILL), written once in the module by `driver.cho`. */
void cancho_trap(void) __attribute__((noreturn));

#if DARWIN
#define SOL_SOCKET 0xffff
#define SO_REUSEADDR 4
#define SO_REUSEPORT 0x200
#define SO_NOSIGPIPE 0x1022
#define MSG_NOSIGNAL 0
#define O_NONBLOCK 4
#define EAGAIN 35
#define EINPROGRESS 36
#define SOCK_CLOEXEC 0
#define AI_ADDR 32
#define AI_CANONNAME 24
#define CLOCK_MONO 6
#define SO_ERROR 0x1007
#define AF_INET6 30
#define FAMILY_AT 1
#else
#define SOL_SOCKET 1
#define SO_REUSEADDR 2
#define SO_REUSEPORT 15
#define SO_NOSIGPIPE 0
#define MSG_NOSIGNAL 0x4000
#define O_NONBLOCK 0x800
#define EAGAIN 11
#define EINPROGRESS 115
#define SOCK_CLOEXEC 0x80000
#define AI_ADDR 24
#define AI_CANONNAME 32
#define CLOCK_MONO 1
#define SO_ERROR 4
#define AF_INET6 10
#define FAMILY_AT 0
#endif
#define F_GETFL 3
#define F_SETFL 4
#define F_SETFD 2
#define FD_CLOEXEC 1
#define EINVAL 22
#define NONE (-2)

extern int socket(int, int, int);
extern int setsockopt(int, int, int, const void *, int);
extern int bind(int, const void *, int);
extern int listen(int, int);
extern int accept(int, void *, void *);
extern int accept4(int, void *, void *, int);
extern int connect(int, const void *, int);
extern int socketpair(int, int, int, int *);
extern int getsockopt(int, int, int, void *, unsigned *);
extern int getpeername(int, void *, unsigned *);
extern int fflush(void *);
extern int ferror(void *);
extern void clearerr(void *);
extern i64 fread(void *, i64, i64, void *);
#if DARWIN
extern void *__stdoutp;
extern void *__stdinp;
#define STDOUT __stdoutp
#define STDIN __stdinp
#else
extern void *stdout;
extern void *stdin;
#define STDOUT stdout
#define STDIN stdin
#endif
#if DARWIN
extern int kqueue(void);
extern int kevent(int, const void *, int, void *, int, const void *);
#else
extern int epoll_create1(int);
extern int epoll_ctl(int, int, int, void *);
extern int epoll_wait(int, void *, int, int);
#endif
extern int close(int);
extern i64 recv(int, void *, i64, int);
extern i64 send(int, const void *, i64, int);
extern int fcntl(int, int, ...);
extern int getaddrinfo(const char *, const char *, const void *, void **);
extern void freeaddrinfo(void *);
extern int clock_gettime(int, void *);
extern i64 recvfrom(int, void *, i64, int, void *, void *);
extern i64 sendto(int, const void *, i64, int, const void *, int);
extern int getsockname(int, void *, void *);
extern int *cancho_errno_location(void);
#define ERRNO (*cancho_errno_location())

static int netrt_tcp_socket(int kind) {
  int fd = socket(2, kind | SOCK_CLOEXEC, 0);
#if DARWIN
  if (fd >= 0) fcntl(fd, F_SETFD, FD_CLOEXEC);
#endif
  return fd;
}

static void netrt_int_option(int fd, int level, int name) {
  int one = 1;
  setsockopt(fd, level, name, &one, 4);
}

static int netrt_accept_cloexec(int fd) {
#if DARWIN
  int c = accept(fd, 0, 0);
  if (c >= 0) fcntl(c, F_SETFD, FD_CLOEXEC);
  return c;
#else
  return accept4(fd, 0, 0, SOCK_CLOEXEC);
#endif
}

/* The port a bound names, or NONE: the whole bound for `bind` and `tcp_listen`, what follows its last `:` for a
 * dial. A bound that is not a number is -1, a port nothing can ask for. */
static i64 netrt_port_of(const u8 *text, i64 from, i64 to) {
  if (to <= from) return NONE;
  i64 v = 0;
  for (i64 i = from; i < to; i++) {
    if (text[i] < 48 || text[i] > 57) return -1;
    v = v * 10 + (text[i] - 48);
  }
  return v;
}

static i64 netrt_dial_port(const u8 *bound, i64 len) {
  for (i64 i = len; i > 0; i--)
    if (bound[i - 1] == 58) return netrt_port_of(bound, i, len);
  return NONE;
}

static i64 netrt_host_length(const u8 *bound, i64 len) {
  for (i64 i = len; i > 0; i--)
    if (bound[i - 1] == 58) return i - 1;
  return len;
}

/* The port a bound names, or NONE. */
static void netrt_check_port(i64 port, i64 bound) {
  if (bound != NONE && port != bound) cancho_trap();
}

static void netrt_sockaddr_any(u8 *a, i64 port) {
  for (int i = 0; i < 16; i++) a[i] = 0;
  a[0] = 2;
  a[2] = (u8)(port >> 8);
  a[3] = (u8)port;
}

/* `bind(net, port)`: a listening-ready socket bound to every address, or -1. */
i64 cancho_net_bind(i64 port, const u8 *bound, i64 bound_len) {
  netrt_check_port(port, netrt_port_of(bound, 0, bound_len));
  u8 a[16];
  netrt_sockaddr_any(a, port);
  int fd = netrt_tcp_socket(1);
  if (fd < 0) return -1;
  netrt_int_option(fd, SOL_SOCKET, SO_REUSEADDR);
  if (bind(fd, a, 16) != 0) {
    close(fd);
    return -1;
  }
  return fd;
}

i64 cancho_net_listen(i64 fd, i64 backlog) { return listen((int)fd, (int)backlog); }
i64 cancho_net_accept(i64 fd) { return netrt_accept_cloexec((int)fd); }

/* The port a bound names for a listener: a bare port, or the one after the last `:` of `host:port`. */
static i64 netrt_listen_port(const u8 *bound, i64 len) {
  for (i64 i = len; i > 0; i--)
    if (bound[i - 1] == 58) return netrt_port_of(bound, i, len);
  return netrt_port_of(bound, 0, len);
}

/* `tcp_listen(net, port, backlog, flags)` and `tcp_listen_on(net, host, port, backlog, flags)`: `host` is the IPv4
 * address as a whole number (0 for every interface). out = tag (0 Ok, 1 Failed), the descriptor, why. */
void cancho_net_tcp_listen(i64 port, i64 backlog, i64 flags, const u8 *bound, i64 bound_len, i64 host, i64 *out) {
  netrt_check_port(port, netrt_listen_port(bound, bound_len));
  u8 a[16];
  netrt_sockaddr_any(a, port);
  a[4] = (u8)(host >> 24);
  a[5] = (u8)(host >> 16);
  a[6] = (u8)(host >> 8);
  a[7] = (u8)host;
  out[0] = 1;
  out[1] = -1;
  out[2] = 0;
  int fd = netrt_tcp_socket(1);
  if (fd < 0) {
    out[2] = ERRNO;
    return;
  }
  netrt_int_option(fd, SOL_SOCKET, SO_REUSEADDR);
  if (flags & 1) netrt_int_option(fd, SOL_SOCKET, SO_REUSEPORT);
  if (bind(fd, a, 16) != 0) {
    out[2] = ERRNO;
    close(fd);
    return;
  }
  if (listen(fd, (int)backlog) != 0) {
    out[2] = ERRNO;
    close(fd);
    return;
  }
  out[0] = 0;
  out[1] = fd;
}

/* `tcp_accept(&!Listener)`: out = tag (0 Ok, 1 Again, 2 Failed), the connection, why. */
void cancho_net_tcp_accept(const i64 *listener, i64 *out) {
  int c = netrt_accept_cloexec((int)*listener);
  i64 why = ERRNO;
  if (c >= 0) {
#if DARWIN
    netrt_int_option(c, SOL_SOCKET, SO_NOSIGPIPE);
    int flags = fcntl(c, F_GETFL);
    fcntl(c, F_SETFL, flags & ~O_NONBLOCK);
#endif
    out[0] = 0;
  } else {
    out[0] = why == EAGAIN ? 1 : 2;
  }
  out[1] = c;
  out[2] = why;
}

/* `conn_read(&!Conn, &![byte])`: out = tag (0 Data, 1 End, 2 Again, 3 Failed), the count, why. */
void cancho_net_conn_read(const i64 *conn, u8 *buf, i64 len, i64 *out) {
  i64 got, why;
  if (len == 0) {
    got = -1;
    why = EINVAL;
  } else {
    got = recv((int)*conn, buf, len, 0);
    why = ERRNO;
  }
  out[0] = got < 0 ? (why == EAGAIN ? 2 : 3) : (got == 0 ? 1 : 0);
  out[1] = got;
  out[2] = why;
}

/* `conn_write(&!Conn, &[byte])`: out = tag (0 Wrote, 1 Again, 2 Failed), the count, why. */
void cancho_net_conn_write(const i64 *conn, const u8 *buf, i64 len, i64 *out) {
  i64 moved = send((int)*conn, buf, len, MSG_NOSIGNAL);
  i64 why = ERRNO;
  out[0] = moved < 0 ? (why == EAGAIN ? 1 : 2) : 0;
  out[1] = moved;
  out[2] = why;
}

i64 cancho_net_nonblocking(const i64 *handle) {
  int fd = (int)*handle;
  int flags = fcntl(fd, F_GETFL);
  int r = fcntl(fd, F_SETFL, flags | O_NONBLOCK);
  return r < 0 ? ERRNO : 0;
}

i64 cancho_net_nodelay(const i64 *handle) {
  int one = 1;
  int r = setsockopt((int)*handle, 6, 1, &one, 4);
  return r < 0 ? ERRNO : 0;
}

/* `pipe_open()`: a socket pair, close-on-exec. out = tag (0 Ok, 1 Failed), the parent's end, the child's, why. */
void cancho_net_pipe_open(i64 *out) {
  int fds[2];
  fds[0] = -1;
  fds[1] = -1;
  int r = socketpair(1, 1 | SOCK_CLOEXEC, 0, fds);
  i64 why = ERRNO;
  if (r >= 0) {
#if DARWIN
    for (int i = 0; i < 2; i++) {
      fcntl(fds[i], F_SETFD, FD_CLOEXEC);
      netrt_int_option(fds[i], SOL_SOCKET, SO_NOSIGPIPE);
    }
#endif
  }
  out[0] = r < 0;
  out[1] = fds[0];
  out[2] = fds[1];
  out[3] = why;
}

i64 cancho_net_close(i64 fd) { return close((int)fd); }

/* The host of a dial, checked against the bound's host and made a C string; traps when it is not the bound's.
 * Before edition 10 the bound's host is a prefix of the name. With `exact` (a bound the compiler marked, edition 10,
 * `docs/net-bounds.md` 2.3) it is the whole name, or, written `*.suffix`, exactly one label ahead of `.suffix`; an empty
 * host half is the unnarrowed root and admits every name either way. */
static void netrt_checked_host(const u8 *name, i64 len, const u8 *bound, i64 host_len, u8 *buffer, int exact) {
  if (len >= 256) cancho_trap();
  if (exact && host_len > 0) {
    if (host_len >= 2 && bound[0] == '*' && bound[1] == '.') {
      const u8 *tail = bound + 1;
      i64 tail_len = host_len - 1;
      if (len <= tail_len) cancho_trap();
      i64 label_end = len - tail_len;
      for (i64 i = 0; i < len; i++) {
        buffer[i] = name[i];
        if (i < label_end ? name[i] == '.' : name[i] != tail[i - label_end]) cancho_trap();
      }
    } else {
      if (len != host_len) cancho_trap();
      for (i64 i = 0; i < len; i++) {
        buffer[i] = name[i];
        if (name[i] != bound[i]) cancho_trap();
      }
    }
    buffer[len] = 0;
    return;
  }
  if (len < host_len) cancho_trap();
  for (i64 i = 0; i < len; i++) {
    buffer[i] = name[i];
    if (i < host_len && name[i] != bound[i]) cancho_trap();
  }
  buffer[len] = 0;
}

/* The descriptor of a connection to `name`:`port`, or -1 with `*why` the reason (-1 when the name did not
 * resolve). `conn` makes it a `Conn` (no `SIGPIPE` on Darwin), `start` leaves it non-blocking and in progress. */
static i64 netrt_dial(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 conn, i64 start, int kind, i64 *why) {
  int exact = 0;
  if (bound_len > 0 && bound[0] == 1) {
    exact = 1;
    bound++;
    bound_len--;
  }
  netrt_check_port(port, netrt_dial_port(bound, bound_len));
  u8 host[256];
  netrt_checked_host(name, len, bound, netrt_host_length(bound, bound_len), host, exact);
  u8 hints[48];
  for (int i = 0; i < 48; i++) hints[i] = 0;
  *(int *)(hints + 4) = 2;
  *(int *)(hints + 8) = kind;
  void *res = 0;
  *why = 0;
  if (getaddrinfo((const char *)host, 0, hints, &res) != 0) {
    *why = -1;
    return -1;
  }
  u8 *addr = *(u8 **)((u8 *)res + AI_ADDR);
  int addrlen = *(int *)((u8 *)res + 16);
  addr[2] = (u8)(port >> 8);
  addr[3] = (u8)port;
  int fd = netrt_tcp_socket(kind);
  if (fd < 0) {
    *why = ERRNO;
    freeaddrinfo(res);
    return -1;
  }
  if (start) {
    int flags = fcntl(fd, F_GETFL);
    fcntl(fd, F_SETFL, flags | O_NONBLOCK);
  }
  int r = connect(fd, addr, addrlen);
  i64 reason = ERRNO;
  int ok = r == 0 || (start && r < 0 && reason == EINPROGRESS);
  freeaddrinfo(res);
  if (ok) {
#if DARWIN
    if (conn) netrt_int_option(fd, SOL_SOCKET, SO_NOSIGPIPE);
#endif
    return fd;
  }
  *why = reason;
  close(fd);
  return -1;
}

/* `connect(net, host, port)`: the descriptor or -1. */
i64 cancho_net_connect(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len) {
  i64 why;
  return netrt_dial(name, len, port, bound, bound_len, 0, 0, 1, &why);
}

/* `tcp_connect(net, host, port)`: out = tag (0 Ok, 1 Failed), the connection, why. */
void cancho_net_tcp_connect(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 *out) {
  i64 why;
  i64 fd = netrt_dial(name, len, port, bound, bound_len, 1, 0, 1, &why);
  out[0] = fd < 0 ? 1 : 0;
  out[1] = fd;
  out[2] = why;
}

/* ---------------------------------------------------------------------------------------------------- UDP ---
 * Datagram sockets (`docs/udp.md`). `lexs_udp_peers` is the ring of senders a bound socket has heard from, 65,536
 * entries of 32 bytes (the address at 0, the ticket at 16, the descriptor it was heard on at 24), then the count of
 * tickets issued. A ticket names an entry by its low sixteen bits, and is good while the entry still holds it and for
 * the socket that heard it. */
#define PEER_SLOTS 65536
#define PEER_STRIDE 32
#define PEER_TICKET_AT 16
#define PEER_FD_AT 24
#define EBADF 9
#if DARWIN
#define MSG_TRUNC_FLAG 0
#else
#define MSG_TRUNC_FLAG 0x20
#endif
u8 lexs_udp_peers[PEER_SLOTS * PEER_STRIDE + 8];

/* `udp_connect(net, host, port)`: out = tag (0 Ok, 1 Failed), the socket, why. */
void cancho_net_udp_connect(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 *out) {
  i64 why;
  i64 fd = netrt_dial(name, len, port, bound, bound_len, 0, 0, 2, &why);
  out[0] = fd < 0 ? 1 : 0;
  out[1] = fd;
  out[2] = why;
}

/* `udp_bind(net, port, flags)`: a datagram socket bound to every interface; `flags` bit 1 is `SO_REUSEPORT`. */
void cancho_net_udp_bind(i64 port, i64 flags, const u8 *bound, i64 bound_len, i64 *out) {
  netrt_check_port(port, netrt_listen_port(bound, bound_len));
  u8 a[16];
  netrt_sockaddr_any(a, port);
  out[0] = 1;
  out[1] = -1;
  out[2] = 0;
  int fd = netrt_tcp_socket(2);
  if (fd < 0) {
    out[2] = ERRNO;
    return;
  }
  netrt_int_option(fd, SOL_SOCKET, SO_REUSEADDR);
  if (flags & 1) netrt_int_option(fd, SOL_SOCKET, SO_REUSEPORT);
  if (bind(fd, a, 16) != 0) {
    out[2] = ERRNO;
    close(fd);
    return;
  }
  out[0] = 0;
  out[1] = fd;
}

/* `Datagram`'s leaves: Got 0, Truncated 1, Again 2, Failed 3, then what `recv` answered twice and why. */
static void netrt_datagram(i64 moved, i64 reason, i64 room, i64 *out) {
  i64 tag;
  if (moved < 0)
    tag = reason == EAGAIN ? 2 : 3;
  else
#if DARWIN
    tag = moved >= room ? 1 : 0;
#else
    tag = moved > room ? 1 : 0;
#endif
  out[0] = tag;
  out[1] = moved;
  out[2] = moved;
  out[3] = reason;
}

/* `udp_recv(&!Udp, &![byte])`: an empty buffer is `Failed(EINVAL)` and never reaches the kernel. */
void cancho_udp_recv(const i64 *udp, u8 *p, i64 room, i64 *out) {
  i64 moved = -1;
  i64 reason = EINVAL;
  if (room != 0) {
    moved = recv((int)*udp, p, room, MSG_TRUNC_FLAG);
    reason = ERRNO;
  }
  netrt_datagram(moved, reason, room, out);
}

/* `udp_recv_from(&!Udp, &![byte], &![int])`: the datagram, and a ticket for its sender in the first cell. */
void cancho_udp_recv_from(const i64 *udp, u8 *p, i64 room, i64 *cells, i64 ncells, i64 *out) {
  int fd = (int)*udp;
  i64 ticket = *(i64 *)(lexs_udp_peers + PEER_SLOTS * PEER_STRIDE) + 1;
  u8 *entry = lexs_udp_peers + (ticket & (PEER_SLOTS - 1)) * PEER_STRIDE;
  i64 moved = -1;
  i64 reason = EINVAL;
  if (room != 0 && ncells != 0) {
    int len = 16;
    moved = recvfrom(fd, p, room, MSG_TRUNC_FLAG, entry, &len);
    reason = ERRNO;
  }
  if (moved >= 0) {
    *(i64 *)(entry + PEER_TICKET_AT) = ticket;
    *(int *)(entry + PEER_FD_AT) = fd;
    *(i64 *)(lexs_udp_peers + PEER_SLOTS * PEER_STRIDE) = ticket;
    cells[0] = ticket;
  }
  netrt_datagram(moved, reason, room, out);
}

/* Does the ticket name an entry that still holds it, heard on this socket? */
static int netrt_ticket_ok(int fd, i64 ticket, u8 *entry) {
  return ticket > 0 && *(i64 *)(entry + PEER_TICKET_AT) == ticket && *(int *)(entry + PEER_FD_AT) == fd;
}

/* `udp_send_to(&!Udp, &[byte], ticket)`: out = Sent's tag (Wrote 0, Again 1, Failed 2), the count, why. */
void cancho_udp_send_to(const i64 *udp, const u8 *p, i64 n, i64 ticket, i64 *out) {
  int fd = (int)*udp;
  u8 *entry = lexs_udp_peers + (ticket & (PEER_SLOTS - 1)) * PEER_STRIDE;
  i64 moved = -1;
  i64 reason = EBADF;
  if (netrt_ticket_ok(fd, ticket, entry)) {
    moved = sendto(fd, p, n, MSG_NOSIGNAL, entry, 16);
    reason = ERRNO;
  }
  out[0] = moved < 0 ? (reason == EAGAIN ? 1 : 2) : 0;
  out[1] = moved;
  out[2] = reason;
}

/* `udp_peer(&Udp, ticket, &![byte])`: who a ticket names, in the 19 bytes `conn_peer` writes; 0, `EBADF` for a ticket
 * that fails the checks, `EINVAL` for a buffer under 19 bytes. */
i64 cancho_udp_peer(const i64 *udp, i64 ticket, u8 *out, i64 room) {
  u8 *entry = lexs_udp_peers + (ticket & (PEER_SLOTS - 1)) * PEER_STRIDE;
  if (!netrt_ticket_ok((int)*udp, ticket, entry)) return EBADF;
  if (room < 19) return EINVAL;
  out[0] = 4;
  for (int i = 0; i < 4; i++) out[1 + i] = entry[4 + i];
  for (int i = 5; i < 17; i++) out[i] = 0;
  out[17] = entry[2];
  out[18] = entry[3];
  return 0;
}

/* `udp_local_port(&Udp)`: the port the kernel chose, or minus the errno. */
i64 cancho_udp_local_port(const i64 *udp) {
  u8 a[16];
  int len = 16;
  int r = getsockname((int)*udp, a, &len);
  i64 why = ERRNO;
  if (r < 0) return -why;
  return ((i64)a[2] << 8) | a[3];
}

/* ---------------------------------------------------------------------------------------------------- poller ---
 * `epoll` on Linux, `kqueue` on Darwin, behind one surface of (token, events) pairs, level-triggered. `packed` says
 * the `epoll_event` is the 12-byte packed one (x86-64); elsewhere it is 16 bytes. */
#define READABLE 1
#define WRITABLE 2
#define BATCH 64

/* `poller_new()`: out = tag (0 Ok, 1 Failed), the descriptor, why. */
void cancho_poller_new(i64 *out) {
#if DARWIN
  int fd = kqueue();
  i64 why = ERRNO;
  if (fd >= 0) fcntl(fd, F_SETFD, FD_CLOEXEC);
#else
  int fd = epoll_create1(0x80000);
  i64 why = ERRNO;
#endif
  out[0] = fd < 0;
  out[1] = fd;
  out[2] = why;
}

#if DARWIN
static int netrt_kevent_change(int kq, int fd, i64 filter, int add, i64 token) {
  u8 kev[32];
  for (int i = 0; i < 32; i++) kev[i] = 0;
  *(i64 *)kev = fd;
  *(short *)(kev + 8) = (short)filter;
  *(unsigned short *)(kev + 10) = add ? 0x0001 : 0x0002;
  *(i64 *)(kev + 24) = token;
  return kevent(kq, kev, 1, 0, 0, 0);
}
#else
static void netrt_ev_put(u8 *ev, i64 packed, int mask, i64 token) {
  *(unsigned *)ev = (unsigned)mask;
  *(i64 *)(ev + (packed ? 4 : 8)) = token;
}
#endif

/* `poller_add_conn`, `poller_add_listener` and the others like them; `modify` changes what is watched. 0 or the errno. */
i64 cancho_poller_ctl(const i64 *poller, const i64 *handle, i64 token, i64 events, i64 modify, i64 packed) {
  int p = (int)*poller, fd = (int)*handle;
  int wants_read = (events & READABLE) != 0, wants_write = (events & WRITABLE) != 0;
#if DARWIN
  int r = netrt_kevent_change(p, fd, -1, wants_read, token);
  i64 read_why = ERRNO;
  int w = netrt_kevent_change(p, fd, -2, wants_write, token);
  i64 write_why = ERRNO;
  if (r < 0 && wants_read) return read_why;
  if (w < 0 && wants_write) return write_why;
  return 0;
#else
  u8 ev[16];
  netrt_ev_put(ev, packed, (wants_read ? 1 : 0) | (wants_write ? 4 : 0), token);
  int r = epoll_ctl(p, modify ? 3 : 1, fd, ev);
  return r < 0 ? ERRNO : 0;
#endif
}

/* `poller_add_child(&!Poller, &Child, token)`: the child's exit, as readable. */
i64 cancho_poller_add_child(const i64 *poller, const i64 *child, i64 token, i64 packed) {
  int p = (int)*poller;
  i64 word = *child;
#if DARWIN
  int pid = (int)word;
  u8 kev[32];
  for (int i = 0; i < 32; i++) kev[i] = 0;
  *(i64 *)kev = pid;
  *(short *)(kev + 8) = -5;
  *(unsigned short *)(kev + 10) = 0x0001 | 0x0010;
  *(unsigned *)(kev + 12) = 0x80000000u;
  *(i64 *)(kev + 24) = token;
  int r = kevent(p, kev, 1, 0, 0, 0);
  i64 answer = r < 0 ? ERRNO : 0;
  if (answer == 3) {
    /* The child has exited already, and kqueue will not watch a zombie: a user event, triggered as it is added. */
    *(short *)(kev + 8) = -10;
    *(unsigned *)(kev + 12) = 0x01000000u;
    r = kevent(p, kev, 1, 0, 0, 0);
    answer = r < 0 ? ERRNO : 0;
  }
  return answer;
#else
  int pidfd = (int)(word >> 32);
  u8 ev[16];
  netrt_ev_put(ev, packed, 1, token);
  int r = epoll_ctl(p, 1, pidfd, ev);
  i64 answer = r < 0 ? ERRNO : 0;
  return pidfd >= 0 ? answer : -(i64)pidfd;
#endif
}

i64 cancho_poller_remove(const i64 *poller, const i64 *handle, i64 packed) {
  int p = (int)*poller, fd = (int)*handle;
#if DARWIN
  netrt_kevent_change(p, fd, -1, 0, 0);
  netrt_kevent_change(p, fd, -2, 0, 0);
  return 0;
#else
  u8 ev[16];
  int r = epoll_ctl(p, 2, fd, ev);
  return r < 0 ? ERRNO : 0;
#endif
}

/* `poller_wait(&!Poller, &![int], timeout_ms)`: (token, events) pairs into the slice; how many, or -errno. */
i64 cancho_poller_wait(const i64 *poller, i64 *pairs, i64 length, i64 timeout, i64 packed) {
  int p = (int)*poller;
  i64 max = length / 2;
  if (max > BATCH) max = BATCH;
  if (max == 0) return -22;
  u8 buf[32 * BATCH];
#if DARWIN
  i64 ts[2];
  ts[0] = timeout / 1000;
  ts[1] = (timeout % 1000) * 1000000;
  int got = kevent(p, 0, 0, buf, (int)max, timeout < 0 ? 0 : ts);
#else
  int got = epoll_wait(p, buf, (int)max, (int)timeout);
#endif
  if (got < 0) return -(i64)ERRNO;
  for (int i = 0; i < got; i++) {
    i64 token, events;
#if DARWIN
    u8 *e = buf + 32 * i;
    short filter = *(short *)(e + 8);
    unsigned short flags = *(unsigned short *)(e + 10);
    token = *(i64 *)(e + 24);
    int reads = filter == -1 || filter == -5 || filter == -10 || (flags & 0x4000) != 0;
    events = (reads ? READABLE : 0) | (filter == -2 ? WRITABLE : 0);
#else
    u8 *e = buf + (packed ? 12 : 16) * i;
    unsigned mask = *(unsigned *)e;
    token = *(i64 *)(e + (packed ? 4 : 8));
    events = ((mask & (1 | 8 | 16)) ? READABLE : 0) | ((mask & 4) ? WRITABLE : 0);
#endif
    pairs[2 * i] = token;
    pairs[2 * i + 1] = events;
  }
  return got;
}

/* ------------------------------------------------------------------------------------------------ hand-over ---
 * A connection handed from one owner to another is a ticket: the descriptor's epoch (odd) over its number. */
#define EPOCH_SLOTS 65536
int lexs_fd_epoch[EPOCH_SLOTS];

/* `conn_detach(Conn)` and `udp_detach`: the descriptor stays open and the handle ends; the ticket, or -1. */
i64 cancho_conn_detach(i64 fd, i64 udp) {
  if ((unsigned long)fd >= EPOCH_SLOTS) {
    close((int)fd);
    return -1;
  }
  int next = lexs_fd_epoch[fd] + 1;
  lexs_fd_epoch[fd] = next;
  return ((i64)(next & 2147483647) << 32) | fd | (udp ? (i64)1 << 31 : 0);
}

/* `conn_attach(ticket)` and `udp_attach`: out = tag (0 Attached, 1 Failed), the descriptor, EBADF. */
void cancho_conn_attach(i64 ticket, i64 udp, i64 *out) {
  i64 fd = ticket & (udp ? 2147483647 : 4294967295);
  i64 epoch = (unsigned long)ticket >> 32;
  int in_range = (unsigned long)fd < EPOCH_SLOTS;
  i64 index = in_range ? fd : 0;
  int current = lexs_fd_epoch[index];
  int valid = in_range && ((i64)(current & 2147483647) == epoch) && (epoch & 1) && ticket >= 0;
  if (udp) valid = valid && (((unsigned long)ticket >> 31) & 1);
  if (valid) lexs_fd_epoch[index] = current + 1;
  out[0] = valid ? 0 : 1;
  out[1] = fd;
  out[2] = 9;
}

/* `tcp_connect_start(net, host, port)`: as `tcp_connect`, the socket left non-blocking and the connection in progress. */
void cancho_net_tcp_connect_start(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 *out) {
  i64 why;
  i64 fd = netrt_dial(name, len, port, bound, bound_len, 1, 1, 1, &why);
  out[0] = fd < 0 ? 1 : 0;
  out[1] = fd;
  out[2] = why;
}

/* `conn_connect_status`: 0 once a non-blocking connect has succeeded, the errno it failed with otherwise. */
i64 cancho_conn_connect_status(const i64 *conn) {
  int value = 0;
  unsigned length = 4;
  int r = getsockopt((int)*conn, SOL_SOCKET, SO_ERROR, &value, &length);
  i64 why = ERRNO;
  return r < 0 ? why : value;
}

/* `conn_peer(&Conn, &![byte])`: 19 bytes: 4 or 6, sixteen address bytes, the port big-endian. 0, or the errno. */
i64 cancho_conn_peer(const i64 *conn, u8 *out, i64 room) {
  if (room < 19) return EINVAL;
  u8 sa[32];
  unsigned length = 32;
  int r = getpeername((int)*conn, sa, &length);
  if (r < 0) return ERRNO;
  u8 family = sa[FAMILY_AT];
  if (family == 2) {
    out[0] = 4;
    for (int i = 0; i < 16; i++) out[1 + i] = i < 4 ? sa[4 + i] : 0;
  } else if (family == AF_INET6) {
    out[0] = 6;
    for (int i = 0; i < 16; i++) out[1 + i] = sa[8 + i];
  } else {
    return EINVAL;
  }
  out[17] = sa[2];
  out[18] = sa[3];
  return 0;
}

/* `flush_out(io)`: out = tag (0 Ok, 1 Failed), 0, why; an earlier failed write the flush did not see is EIO. */
void cancho_flush_out(i64 *out) {
  int flushed = fflush(STDOUT);
  i64 why = ERRNO;
  int indicator = ferror(STDOUT);
  int failed = flushed != 0 || indicator != 0;
  out[0] = failed;
  out[1] = 0;
  out[2] = flushed != 0 ? why : 5;
}

/* `read_bytes(io, into)`: out = tag (0 Got, 1 End, 2 Failed), the count, why. */
void cancho_read_stdin(u8 *buf, i64 length, i64 *out) {
  i64 moved = fread(buf, 1, length, STDIN);
  i64 why = ERRNO;
  int failed = ferror(STDIN) != 0;
  clearerr(STDIN);
  i64 ended = length != 0 ? 1 : 0;
  out[0] = moved != 0 ? 0 : (failed ? 2 : ended);
  out[1] = moved;
  out[2] = why;
}

/* `clock_ms` (monotonic) and `clock_unix_ms` (wall). */
i64 cancho_clock_ms(i64 wall) {
  i64 ts[2];
  clock_gettime(wall ? 0 : CLOCK_MONO, ts);
  return ts[0] * 1000 + (unsigned long)ts[1] / 1000000;
}
