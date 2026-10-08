// `oserv`: a one-thread keep-alive HTTP/1.1 server over OpenSSL (or plain), the OpenSSL figure for
// `docs/tls-performance.md` §3.5: what a request costs when the library under the server is OpenSSL and everything else
// is as small as it can be. It answers every request with the same fixed body, so what is measured is the record layer
// and the socket, not an application.
//
//   oserv <port> <chain.pem> <key.pem> [plain]
//   gcc -O2 -o oserv benches/server/oserv.c -lssl -lcrypto
//
// TLS 1.3 only, AES-128-GCM and X25519 as `tls_echo` negotiates them; no session tickets (`-num_tickets 0`'s effect), so
// a full handshake is the whole cost of a new connection, as it is for the engine under comparison. The handshake is done
// blocking at accept; the established connection is non-blocking under epoll. One request is a head ending in a blank
// line; the response is `Content-Length: 9` and a nine-byte body, the size of `https_hello`'s `GET /hello/42` answer.
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/tcp.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <unistd.h>

static const char answer[] = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 9\r\n\r\nhello, 42";

struct conn { int fd; SSL *ssl; int have; char buf[4096]; };

static int serve(struct conn *c) {                  // 0 to keep the connection, -1 to drop it
  for (;;) {
    int n = c->ssl ? SSL_read(c->ssl, c->buf + c->have, sizeof c->buf - c->have - 1)
                   : (int)read(c->fd, c->buf + c->have, sizeof c->buf - c->have - 1);
    if (n <= 0) {
      if (c->ssl) { int e = SSL_get_error(c->ssl, n); return (e == SSL_ERROR_WANT_READ || e == SSL_ERROR_WANT_WRITE) ? 0 : -1; }
      return (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) ? 0 : -1;
    }
    c->have += n; c->buf[c->have] = 0;
    char *end;
    while ((end = strstr(c->buf, "\r\n\r\n"))) {
      int used = (int)(end - c->buf) + 4;
      int w = c->ssl ? SSL_write(c->ssl, answer, sizeof answer - 1) : (int)write(c->fd, answer, sizeof answer - 1);
      if (w != (int)sizeof answer - 1) return -1;
      memmove(c->buf, c->buf + used, c->have - used + 1); c->have -= used;
    }
  }
}

int main(int argc, char **argv) {
  if (argc < 4) { fprintf(stderr, "usage: oserv <port> <chain.pem> <key.pem> [plain]\n"); return 2; }
  int plain = argc > 4 && !strcmp(argv[4], "plain");
  SSL_CTX *ctx = SSL_CTX_new(TLS_server_method());
  SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION);
  SSL_CTX_set_ciphersuites(ctx, "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256");
  SSL_CTX_set1_groups_list(ctx, "X25519:P-256");
  SSL_CTX_set_num_tickets(ctx, 0);
  SSL_CTX_set_options(ctx, SSL_OP_NO_TICKET);
  if (SSL_CTX_use_certificate_chain_file(ctx, argv[2]) != 1 || SSL_CTX_use_PrivateKey_file(ctx, argv[3], SSL_FILETYPE_PEM) != 1) {
    ERR_print_errors_fp(stderr); return 1;
  }
  int ls = socket(AF_INET, SOCK_STREAM, 0), one = 1;
  setsockopt(ls, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
  struct sockaddr_in sa = {0}; sa.sin_family = AF_INET; sa.sin_port = htons(atoi(argv[1])); inet_pton(AF_INET, "127.0.0.1", &sa.sin_addr);
  if (bind(ls, (struct sockaddr *)&sa, sizeof sa) < 0 || listen(ls, 1024) < 0) { perror("bind"); return 1; }
  int ep = epoll_create1(0);
  struct epoll_event ev = {.events = EPOLLIN, .data.ptr = NULL};
  epoll_ctl(ep, EPOLL_CTL_ADD, ls, &ev);
  fprintf(stderr, "listening %s\n", argv[1]);
  struct epoll_event evs[64];
  for (;;) {
    int n = epoll_wait(ep, evs, 64, -1);
    for (int i = 0; i < n; i++) {
      struct conn *c = evs[i].data.ptr;
      if (!c) {
        int fd = accept(ls, 0, 0); if (fd < 0) continue;
        setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
        struct conn *k = calloc(1, sizeof *k); k->fd = fd;
        if (!plain) {
          k->ssl = SSL_new(ctx); SSL_set_fd(k->ssl, fd);
          if (SSL_accept(k->ssl) != 1) { SSL_free(k->ssl); close(fd); free(k); continue; }
        }
        fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK);
        struct epoll_event e = {.events = EPOLLIN, .data.ptr = k};
        epoll_ctl(ep, EPOLL_CTL_ADD, fd, &e);
      } else if (serve(c) < 0) {
        epoll_ctl(ep, EPOLL_CTL_DEL, c->fd, 0);
        if (c->ssl) SSL_free(c->ssl);
        close(c->fd); free(c);
      }
    }
  }
}
