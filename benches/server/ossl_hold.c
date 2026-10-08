// A TLS 1.3 echo server on OpenSSL, one thread, one epoll, for `scripts/tls_memory.py` (docs/tls-memory.md): the
// per-connection resident memory of an ordinary OpenSSL server, to put beside `examples/tls_echo`'s. It does what
// `tls_echo` does and no more: accept, handshake, echo. No tickets are sent (SSL_OP_NO_TICKET), as `tls_echo` sends none.
//
//   ossl_hold <port> <chain.pem> <key.pem> [release]
//
// With `release`, SSL_MODE_RELEASE_BUFFERS: OpenSSL frees a connection's record buffers whenever they are empty, which is
// what a server with many idle connections sets. Without it the buffers stay once allocated. Prints `listening <port>`.
//
//   gcc -O2 -o ossl_hold benches/server/ossl_hold.c -lssl -lcrypto
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/socket.h>
#include <unistd.h>

typedef struct Conn { int fd; SSL *ssl; int handshaken; char *pend; int plen, poff; } Conn;

static void nonblock(int fd) { fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK); }

static void drop(int ep, Conn *c) {
  epoll_ctl(ep, EPOLL_CTL_DEL, c->fd, 0);
  SSL_free(c->ssl); close(c->fd); free(c->pend); free(c);
}

int main(int argc, char **argv) {
  if (argc < 4) { fprintf(stderr, "usage: ossl_hold <port> <chain.pem> <key.pem> [release]\n"); return 2; }
  int port = atoi(argv[1]), release = argc > 4 && !strcmp(argv[4], "release");
  SSL_CTX *ctx = SSL_CTX_new(TLS_server_method());
  SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION);
  SSL_CTX_set_options(ctx, SSL_OP_NO_TICKET);
  SSL_CTX_set_num_tickets(ctx, 0);
  SSL_CTX_set_session_cache_mode(ctx, SSL_SESS_CACHE_OFF);
  SSL_CTX_set_mode(ctx, SSL_MODE_ENABLE_PARTIAL_WRITE | SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER | (release ? SSL_MODE_RELEASE_BUFFERS : 0));
  if (SSL_CTX_use_certificate_chain_file(ctx, argv[2]) != 1 || SSL_CTX_use_PrivateKey_file(ctx, argv[3], SSL_FILETYPE_PEM) != 1) {
    ERR_print_errors_fp(stderr); return 1;
  }
  int ls = socket(AF_INET, SOCK_STREAM, 0), one = 1;
  setsockopt(ls, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
  struct sockaddr_in a = {0}; a.sin_family = AF_INET; a.sin_port = htons(port); a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  if (bind(ls, (struct sockaddr *)&a, sizeof a) || listen(ls, 4096)) { perror("bind"); return 1; }
  nonblock(ls);
  int ep = epoll_create1(0);
  struct epoll_event ev = {0}; ev.events = EPOLLIN; ev.data.ptr = 0; epoll_ctl(ep, EPOLL_CTL_ADD, ls, &ev);
  printf("listening %d\n", port); fflush(stdout);
  static char buf[16384];
  struct epoll_event evs[256];
  for (;;) {
    int n = epoll_wait(ep, evs, 256, -1);
    for (int i = 0; i < n; i++) {
      Conn *c = evs[i].data.ptr;
      if (!c) {
        for (;;) {
          int fd = accept(ls, 0, 0); if (fd < 0) break;
          nonblock(fd);
          int one = 1; setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
          Conn *k = calloc(1, sizeof *k); k->fd = fd; k->ssl = SSL_new(ctx); SSL_set_fd(k->ssl, fd); SSL_set_accept_state(k->ssl);
          struct epoll_event e = {0}; e.events = EPOLLIN | EPOLLRDHUP; e.data.ptr = k; epoll_ctl(ep, EPOLL_CTL_ADD, fd, &e);
        }
        continue;
      }
      if (!c->handshaken) {
        int r = SSL_accept(c->ssl);
        if (r == 1) c->handshaken = 1;
        else { int e = SSL_get_error(c->ssl, r); if (e != SSL_ERROR_WANT_READ && e != SSL_ERROR_WANT_WRITE) { drop(ep, c); continue; } }
        if (!c->handshaken) continue;
      }
      if (c->pend) {  // finish a short write first
        int w = SSL_write(c->ssl, c->pend + c->poff, c->plen - c->poff);
        if (w > 0) { c->poff += w; if (c->poff == c->plen) { free(c->pend); c->pend = 0; } }
        if (c->pend) continue;
      }
      for (;;) {
        int r = SSL_read(c->ssl, buf, sizeof buf);
        if (r <= 0) {
          int e = SSL_get_error(c->ssl, r);
          if (e == SSL_ERROR_WANT_READ || e == SSL_ERROR_WANT_WRITE) break;
          drop(ep, c); c = 0; break;
        }
        int w = SSL_write(c->ssl, buf, r);
        if (w < r) { int rest = r - (w > 0 ? w : 0); c->pend = malloc(rest); memcpy(c->pend, buf + (w > 0 ? w : 0), rest); c->plen = rest; c->poff = 0; break; }
      }
    }
  }
}
