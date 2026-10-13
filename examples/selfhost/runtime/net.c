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
extern int close(int);
extern i64 recv(int, void *, i64, int);
extern i64 send(int, const void *, i64, int);
extern int fcntl(int, int, ...);
extern int getaddrinfo(const char *, const char *, const void *, void **);
extern void freeaddrinfo(void *);
extern int clock_gettime(int, void *);
extern i64 *cancho_errno_location(void);
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
  if (bound != NONE && port != bound) __builtin_trap();
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

/* `tcp_listen(net, port, backlog, flags)`: out = tag (0 Ok, 1 Failed), the descriptor, why. */
void cancho_net_tcp_listen(i64 port, i64 backlog, i64 flags, const u8 *bound, i64 bound_len, i64 *out) {
  netrt_check_port(port, netrt_port_of(bound, 0, bound_len));
  u8 a[16];
  netrt_sockaddr_any(a, port);
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

i64 cancho_net_close(i64 fd) { return close((int)fd); }

/* The host of a dial, checked against the bound's host and made a C string; traps when it is not the bound's. */
static void netrt_checked_host(const u8 *name, i64 len, const u8 *bound, i64 host_len, u8 *buffer) {
  if (len >= 256 || len < host_len) __builtin_trap();
  for (i64 i = 0; i < len; i++) {
    buffer[i] = name[i];
    if (i < host_len && name[i] != bound[i]) __builtin_trap();
  }
  buffer[len] = 0;
}

/* The descriptor of a connection to `name`:`port`, or -1 with `*why` the reason (-1 when the name did not
 * resolve). `conn` makes it a `Conn` (no `SIGPIPE` on Darwin), `start` leaves it non-blocking and in progress. */
static i64 netrt_dial(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 conn, i64 start, i64 *why) {
  netrt_check_port(port, netrt_dial_port(bound, bound_len));
  u8 host[256];
  netrt_checked_host(name, len, bound, netrt_host_length(bound, bound_len), host);
  u8 hints[48];
  for (int i = 0; i < 48; i++) hints[i] = 0;
  *(int *)(hints + 4) = 2;
  *(int *)(hints + 8) = 1;
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
  int fd = netrt_tcp_socket(1);
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
  return netrt_dial(name, len, port, bound, bound_len, 0, 0, &why);
}

/* `tcp_connect(net, host, port)`: out = tag (0 Ok, 1 Failed), the connection, why. */
void cancho_net_tcp_connect(const u8 *name, i64 len, i64 port, const u8 *bound, i64 bound_len, i64 *out) {
  i64 why;
  i64 fd = netrt_dial(name, len, port, bound, bound_len, 1, 0, &why);
  out[0] = fd < 0 ? 1 : 0;
  out[1] = fd;
  out[2] = why;
}

/* `clock_ms` (monotonic) and `clock_unix_ms` (wall). */
i64 cancho_clock_ms(i64 wall) {
  i64 ts[2];
  clock_gettime(wall ? 0 : CLOCK_MONO, ts);
  return ts[0] * 1000 + (unsigned long)ts[1] / 1000000;
}
