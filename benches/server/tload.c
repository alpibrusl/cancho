// `kload` over TLS 1.3 (OpenSSL): the closed-loop keep-alive load generator of docs/http-server.md §11.7.
//
//   tload <port> <threads> <connections-per-thread> <seconds> <path> <ca.pem> <server name>
//
// The same round as `kload`: every connection sends one request, then every response is read, so K requests are in flight
// per thread. Each connection does its TLS 1.3 handshake first (the chain is checked against ca.pem and the name), outside
// the timed seconds' start only by the few milliseconds it takes; the printed figure is responses completed / seconds.
//
//   gcc -O2 -o tload benches/server/tload.c -lssl -lcrypto -lpthread      (Homebrew: -I/opt/homebrew/opt/openssl@3/include -L...)
#include <arpa/inet.h>
#include <netinet/tcp.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static int port, secs, K; static volatile int stop; static long counts[64]; static const char *path, *cafile, *name;
static int readresp(SSL *ssl, char *buf) {          // one response, by Content-Length; returns bytes or -1
  int have = 0, need = -1, head = -1;
  for (;;) {
    int n = SSL_read(ssl, buf + have, 4096 - have); if (n <= 0) return -1; have += n; buf[have] = 0;
    if (head < 0) { char *e = strstr(buf, "\r\n\r\n"); if (!e) continue; head = e - buf + 4;
      char *c = strstr(buf, "Content-Length: "); need = head + (c ? atoi(c + 16) : 0); }
    if (have >= need) return have;
  }
}
static void *run(void *a) {
  long id = (long)a; char req[512]; snprintf(req, sizeof req, "GET %s HTTP/1.1\r\nHost: %s\r\n\r\n", path, name);
  SSL_CTX *ctx = SSL_CTX_new(TLS_client_method()); SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION);
  if (!SSL_CTX_load_verify_locations(ctx, cafile, 0)) { fprintf(stderr, "cannot load %s\n", cafile); exit(1); }
  SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, 0);
  SSL **ss = malloc(sizeof(SSL *) * K); char buf[4096]; struct sockaddr_in sa = {0}; sa.sin_family = AF_INET; sa.sin_port = htons(port); inet_pton(AF_INET, "127.0.0.1", &sa.sin_addr);
  for (int i = 0; i < K; i++) {
    int fd = socket(AF_INET, SOCK_STREAM, 0); int one = 1; setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    if (connect(fd, (struct sockaddr *)&sa, sizeof sa) < 0) { perror("connect"); exit(1); }
    ss[i] = SSL_new(ctx); SSL_set_fd(ss[i], fd); SSL_set_tlsext_host_name(ss[i], name); SSL_set1_host(ss[i], name);
    if (SSL_connect(ss[i]) != 1) { ERR_print_errors_fp(stderr); exit(1); }
  }
  while (!stop) {
    for (int i = 0; i < K; i++) if (SSL_write(ss[i], req, strlen(req)) <= 0) { fprintf(stderr, "write failed\n"); return 0; }
    for (int i = 0; i < K; i++) { if (readresp(ss[i], buf) < 0) { fprintf(stderr, "short read\n"); return 0; } counts[id]++; }
  }
  return 0;
}
int main(int c, char **v) {
  if (c < 8) { fprintf(stderr, "usage: tload <port> <threads> <connections-per-thread> <seconds> <path> <ca.pem> <server name>\n"); return 2; }
  port = atoi(v[1]); int threads = atoi(v[2]); K = atoi(v[3]); secs = atoi(v[4]); path = v[5]; cafile = v[6]; name = v[7];
  pthread_t t[64]; for (long i = 0; i < threads; i++) pthread_create(&t[i], 0, run, (void *)i);
  sleep(secs); stop = 1; long sum = 0; for (int i = 0; i < threads; i++) { pthread_join(t[i], 0); sum += counts[i]; }
  printf("%ld\n", sum / secs);
  return 0;
}
