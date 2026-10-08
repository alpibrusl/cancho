// `tconn`: new TLS 1.3 connections one after another over OpenSSL, each one verified (the chain against a CA file and the
// host name), the OpenSSL figure for a client's full handshake in `docs/tls-performance.md` §3.4. `openssl s_time` does not
// fail on a verification error, so it cannot say the check ran; this one aborts on the first handshake that does not verify.
//
//   tconn <port> <seconds> <ca.pem> <server name>
//   gcc -O2 -o tconn benches/server/tconn.c -lssl -lcrypto
//
// Prints the number of handshakes made. The client's CPU is read from outside (`/proc`, `getrusage`, `perf stat`) and
// divided by it. One thread; AES-128-GCM and X25519 are what the server (`oserv` or `tls_echo`) picks for this ClientHello.
#include <arpa/inet.h>
#include <netinet/tcp.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv) {
  if (argc < 5) { fprintf(stderr, "usage: tconn <port> <seconds> <ca.pem> <server name>\n"); return 2; }
  int port = atoi(argv[1]); double secs = atof(argv[2]); const char *name = argv[4];
  SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
  SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION);
  SSL_CTX_set_ciphersuites(ctx, "TLS_AES_128_GCM_SHA256");
  SSL_CTX_set1_groups_list(ctx, "X25519");
  SSL_CTX_set_session_cache_mode(ctx, SSL_SESS_CACHE_OFF);
  SSL_CTX_set_options(ctx, SSL_OP_NO_TICKET);
  if (!SSL_CTX_load_verify_locations(ctx, argv[3], 0)) { fprintf(stderr, "cannot load %s\n", argv[3]); return 1; }
  SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, 0);
  struct sockaddr_in sa = {0}; sa.sin_family = AF_INET; sa.sin_port = htons(port); inet_pton(AF_INET, "127.0.0.1", &sa.sin_addr);
  struct timespec t0, t1; clock_gettime(CLOCK_MONOTONIC, &t0);
  long n = 0;
  for (;;) {
    int fd = socket(AF_INET, SOCK_STREAM, 0), one = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    if (connect(fd, (struct sockaddr *)&sa, sizeof sa) < 0) { perror("connect"); return 1; }
    SSL *ssl = SSL_new(ctx); SSL_set_fd(ssl, fd); SSL_set_tlsext_host_name(ssl, name); SSL_set1_host(ssl, name);
    if (SSL_connect(ssl) != 1) { ERR_print_errors_fp(stderr); fprintf(stderr, "handshake %ld failed\n", n); return 1; }
    SSL_free(ssl); close(fd); n++;
    clock_gettime(CLOCK_MONOTONIC, &t1);
    if ((t1.tv_sec - t0.tv_sec) + (t1.tv_nsec - t0.tv_nsec) / 1e9 >= secs) break;
  }
  printf("%ld\n", n);
  return 0;
}
