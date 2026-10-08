#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <time.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
static double now(){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1e9+t.tv_nsec;}
static int peer(int fd){struct sockaddr_storage s;socklen_t l=sizeof s;int r=getpeername(fd,(void*)&s,&l);return r<0?-errno:0;}
int main(){setvbuf(stdout,0,_IONBF,0);
  int ls=socket(AF_INET,SOCK_STREAM,0);int one=1;setsockopt(ls,SOL_SOCKET,SO_REUSEADDR,&one,4);
  struct sockaddr_in a={0};a.sin_family=AF_INET;a.sin_addr.s_addr=htonl(INADDR_LOOPBACK);
  bind(ls,(void*)&a,sizeof a);listen(ls,128);socklen_t al=sizeof a;getsockname(ls,(void*)&a,&al);
  // 1. FIN then ask; 2. RST then ask
  for(int mode=0;mode<3;mode++){
    int c=socket(AF_INET,SOCK_STREAM,0);connect(c,(void*)&a,sizeof a);
    int s=accept(ls,0,0);
    if(mode==1){close(c);} 
    if(mode==2){struct linger lg={1,0};setsockopt(c,SOL_SOCKET,SO_LINGER,&lg,sizeof lg);close(c);}
    usleep(50000);
    printf("mode %s: getpeername => %d (errno name via %s)\n",mode==0?"open":mode==1?"peer-FIN":"peer-RST",peer(s),strerror(-peer(s)<0?0:-peer(s)));
    char b[8];if(mode){ssize_t n=read(s,b,8);(void)n;}
    printf("   after read: getpeername => %d\n",peer(s));
    close(s); if(mode==0)close(c);
  }
  // cost
  int N=200000;double t0=now();
  int c=socket(AF_INET,SOCK_STREAM,0);connect(c,(void*)&a,sizeof a);int s=accept(ls,0,0);
  t0=now();for(int i=0;i<N;i++)peer(s);double t1=now();
  printf("getpeername: %.0f ns/call\n",(t1-t0)/N);
  int M=3000;t0=now();
  for(int i=0;i<M;i++){int cc=socket(AF_INET,SOCK_STREAM,0);connect(cc,(void*)&a,sizeof a);int ss=accept(ls,0,0);close(cc);close(ss);}
  t1=now();printf("connect+accept+2 close: %.0f ns/conn\n",(t1-t0)/M);
  t0=now();
  for(int i=0;i<M;i++){int cc=socket(AF_INET,SOCK_STREAM,0);connect(cc,(void*)&a,sizeof a);int ss=accept(ls,0,0);peer(ss);close(cc);close(ss);}
  t1=now();printf("same + one getpeername: %.0f ns/conn\n",(t1-t0)/M);
  return 0;}
