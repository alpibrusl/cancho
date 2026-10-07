// Reference server for docs/server.md §5, the ceiling for this loop design: a
// poll(2) keep-alive server in C that does not parse -- it finds the blank line
// and sends one fixed answer. Build: gcc -O2 -o cpoll cpoll.c. Same
// contract as api.cho (answers GET /users/42's body for any request).
#include <arpa/inet.h>
#include <poll.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#define MAXC 1024
int main(int c, char **v) {
  signal(SIGPIPE, SIG_IGN);
  int lfd = socket(AF_INET, SOCK_STREAM, 0), one = 1; setsockopt(lfd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one); setsockopt(lfd, SOL_SOCKET, SO_REUSEPORT, &one, sizeof one);
  struct sockaddr_in sa = {0}; sa.sin_family = AF_INET; sa.sin_port = htons(atoi(v[1])); bind(lfd, (struct sockaddr*)&sa, sizeof sa); listen(lfd, 1024);
  const char *reply = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 26\r\nConnection: keep-alive\r\n\r\n{\"id\":42,\"name\":\"user-42\"}";
  static struct pollfd p[MAXC + 1]; static char buf[MAXC][4096]; static int fill[MAXC]; int n = 0;
  p[0].fd = lfd; p[0].events = POLLIN;
  for (;;) {
    poll(p, n + 1, 1000);
    if (p[0].revents) { int cfd = accept(lfd, 0, 0); if (cfd >= 0) { if (n >= MAXC) close(cfd); else { p[n + 1].fd = cfd; p[n + 1].events = POLLIN; fill[n] = 0; n++; } } }
    for (int k = n - 1; k >= 0; k--) {
      if (!p[k + 1].revents) continue;
      int got = read(p[k + 1].fd, buf[k] + fill[k], 4096 - fill[k]); int drop = got <= 0;
      if (!drop) { fill[k] += got; char *e; int used = 0;
        while ((e = memmem(buf[k] + used, fill[k] - used, "\r\n\r\n", 4))) { write(p[k + 1].fd, reply, strlen(reply)); used = e + 4 - buf[k]; }
        memmove(buf[k], buf[k] + used, fill[k] - used); fill[k] -= used; if (fill[k] >= 4096) drop = 1; }
      if (drop) { close(p[k + 1].fd); int l = n - 1; if (k != l) { memcpy(buf[k], buf[l], fill[l]); fill[k] = fill[l]; p[k + 1].fd = p[l + 1].fd; } n--; }
    }
  }
}
