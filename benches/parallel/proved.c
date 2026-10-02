// checked sum with a proof pass, in C: per block, max and min (comparisons only), then an unchecked sum when it cannot overflow.
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>
#define N 16384
#define ROUNDS 12207
static double now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1e3+t.tv_nsec/1e6;}
static int64_t v[N];
static int64_t sum_checked(const int64_t*p,long from,long to,int64_t total){for(long i=from;i<to;i++){if(__builtin_add_overflow(total,p[i],&total))abort();}return total;}
int main(int argc,char**argv){
  int mode=argc>1?atoi(argv[1]):0;
  for(long i=0;i<N;i++)v[i]=i%3-1;
  double t=now(); int64_t total=0;
  for(int r=0;r<ROUNDS;r++){
    if(mode==0){ total=sum_checked(v,0,N,total); }                       // checked, per element
    else if(mode==1){ uint64_t s=0; for(long i=0;i<N;i++)s+=(uint64_t)v[i]; total+=(int64_t)s; }   // wrapping
    else {                                                                  // proof pass per block, then wrapping
      for(long from=0;from<N;from+=4096){
        long to=from+4096<N?from+4096:N; int64_t hi=v[from],lo=v[from];
        for(long i=from;i<to;i++){ if(v[i]>hi)hi=v[i]; if(v[i]<lo)lo=v[i]; }
        if(hi<(1LL<<50)&&lo>-(1LL<<50)){ uint64_t s=0; for(long i=from;i<to;i++)s+=(uint64_t)v[i]; if(__builtin_add_overflow(total,(int64_t)s,&total))abort(); }
        else total=sum_checked(v,from,to,total);
      }
    }
  }
  printf("%ld %.0f ms\n",(long)total,now()-t);
  return total==-12207?0:1;
}
