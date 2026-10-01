// epoll twin of cpoll.c (docs/server.md §8): the same loop, contract and buffers with epoll(7) in place of poll(2), to separate what the kernel costs from what the program does. Build: gcc -O2 -o cepoll cepoll.c.
#include <arpa/inet.h>
#include <sys/epoll.h>
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
  static char buf[MAXC + 8][4096]; static int fill[MAXC + 8];
  int ep = epoll_create1(0); struct epoll_event ev, evs[64];
  ev.events = EPOLLIN; ev.data.u64 = 0; epoll_ctl(ep, EPOLL_CTL_ADD, lfd, &ev);
  for (;;) {
    int n = epoll_wait(ep, evs, 64, 1000);
    for (int i = 0; i < n; i++) {
      unsigned long t = evs[i].data.u64;
      if (t == 0) { int cfd = accept(lfd, 0, 0); if (cfd >= 0 && cfd < MAXC) { ev.events = EPOLLIN; ev.data.u64 = cfd; fill[cfd] = 0; epoll_ctl(ep, EPOLL_CTL_ADD, cfd, &ev); } else if (cfd >= 0) close(cfd); continue; }
      int fd = (int)t;
      int got = read(fd, buf[fd] + fill[fd], 4096 - fill[fd]); int drop = got <= 0;
      if (!drop) { fill[fd] += got; char *e; int used = 0;
        while ((e = memmem(buf[fd] + used, fill[fd] - used, "\r\n\r\n", 4))) { write(fd, reply, strlen(reply)); used = e + 4 - buf[fd]; }
        memmove(buf[fd], buf[fd] + used, fill[fd] - used); fill[fd] -= used; if (fill[fd] >= 4096) drop = 1; }
      if (drop) close(fd);
    }
  }
}
