// Closed-loop keep-alive load generator for docs/server.md §5; see bench.sh.
// Closed-loop keep-alive load: T threads, each owning K connections. A round is
// "send one request on every connection, then read every response", so K
// requests are in flight per thread. Counts completed responses for SECS seconds.
#include <arpa/inet.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
static int port, secs, K; static volatile int stop; static long counts[64]; static const char *path;
static int readresp(int fd, char *buf) {          // one response, by Content-Length; returns bytes or -1
  int have = 0, need = -1, head = -1;
  for (;;) {
    int n = read(fd, buf + have, 4096 - have); if (n <= 0) return -1; have += n; buf[have] = 0;
    if (head < 0) { char *e = strstr(buf, "\r\n\r\n"); if (!e) continue; head = e - buf + 4;
      char *c = strstr(buf, "Content-Length: "); need = head + (c ? atoi(c + 16) : 0); }
    if (have >= need) return have;
  }
}
static void *run(void *a) {
  long id = (long)a; char req[512]; snprintf(req, sizeof req, "GET %s HTTP/1.1\r\nHost: x\r\n\r\n", path);
  int fds[256]; char buf[4096]; struct sockaddr_in sa = {0}; sa.sin_family = AF_INET; sa.sin_port = htons(port); inet_pton(AF_INET, "127.0.0.1", &sa.sin_addr);
  for (int i = 0; i < K; i++) { fds[i] = socket(AF_INET, SOCK_STREAM, 0); int one = 1; setsockopt(fds[i], IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    if (connect(fds[i], (struct sockaddr*)&sa, sizeof sa) < 0) { perror("connect"); exit(1); } }
  while (!stop) {
    for (int i = 0; i < K; i++) if (write(fds[i], req, strlen(req)) < 0) { perror("write"); return 0; }
    for (int i = 0; i < K; i++) { if (readresp(fds[i], buf) < 0) { fprintf(stderr, "short read\n"); return 0; } counts[id]++; }
  }
  return 0;
}
int main(int c, char **v) {
  port = atoi(v[1]); int threads = atoi(v[2]); K = atoi(v[3]); secs = atoi(v[4]); path = v[5];
  pthread_t t[64]; for (long i = 0; i < threads; i++) pthread_create(&t[i], 0, run, (void*)i);
  sleep(secs); stop = 1; long sum = 0; for (int i = 0; i < threads; i++) { pthread_join(t[i], 0); sum += counts[i]; }
  printf("%ld\n", sum / secs); return 0;
}
