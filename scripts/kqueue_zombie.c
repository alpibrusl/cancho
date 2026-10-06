/* kqueue and an exited child (docs/processes.md 4.8), macOS only:
 *
 *     cc -o /tmp/kq scripts/kqueue_zombie.c && /tmp/kq
 *
 * EVFILT_PROC/NOTE_EXIT on a live child; on 200 children confirmed zombies
 * with waitid(WNOWAIT), counting ESRCH; and EVFILT_USER added with
 * NOTE_TRIGGER in one change (reported, once, with its udata) against two
 * changes (the second rewrites udata). */
#include <time.h>
#include <sys/event.h>
#include <sys/wait.h>
#include <spawn.h>
#include <stdio.h>
#include <errno.h>
#include <unistd.h>
#include <string.h>
extern char **environ;
static pid_t spawn(const char *s){ pid_t p; char *a[]={"/bin/sh","-c",(char*)s,0};
  posix_spawn(&p,"/bin/sh",0,0,a,environ); return p; }
int main(void){
  int kq=kqueue(); struct kevent ev, out; struct timespec z={0,0}, s1={1,0};
  /* 1: a live child */
  pid_t p=spawn("sleep 0.2");
  EV_SET(&ev,p,EVFILT_PROC,EV_ADD|EV_ONESHOT,NOTE_EXIT,0,(void*)2);
  int r=kevent(kq,&ev,1,0,0,0); printf("live: kevent %d errno %d\n",r,r<0?errno:0);
  r=kevent(kq,0,0,&out,1,&s1); printf("live: woke %d filter %d udata %ld\n",r,out.filter,(long)out.udata);
  int st; waitpid(p,&st,0);
  /* 2: a zombie child, confirmed with waitid WNOWAIT */
  int esrch=0, n=200;
  for(int i=0;i<n;i++){
    p=spawn("exit 7"); siginfo_t si; memset(&si,0,sizeof si);
    int w=waitid(P_PID,p,&si,WEXITED|WNOWAIT); /* blocks until zombie, does not reap */
    EV_SET(&ev,p,EVFILT_PROC,EV_ADD|EV_ONESHOT,NOTE_EXIT,0,(void*)2);
    r=kevent(kq,&ev,1,0,0,0);
    if(r<0&&errno==ESRCH) esrch++;
    if(i==0) printf("zombie: waitid %d si_pid==p %d code %d; kevent %d errno %d\n",w,si.si_pid==p,si.si_status,r,r<0?errno:0);
    /* 3: EVFILT_USER triggered at add */
    if(i==0){
      EV_SET(&ev,p,EVFILT_USER,EV_ADD|EV_ONESHOT,NOTE_TRIGGER,0,(void*)2);
      r=kevent(kq,&ev,1,0,0,0); printf("user: add %d errno %d\n",r,r<0?errno:0);
      r=kevent(kq,0,0,&out,1,&z); printf("user: woke %d filter %d udata %ld\n",r,out.filter,(long)out.udata);
      r=kevent(kq,0,0,&out,1,&z); printf("user: again %d (oneshot)\n",r);
    }
    waitpid(p,&st,0);
  }
  printf("zombie: ESRCH %d of %d\n",esrch,n);
  /* 4: the add and the trigger as two changes: the second's udata wins */
  struct kevent ch[2];
  EV_SET(&ch[0],4242,EVFILT_USER,EV_ADD|EV_ONESHOT,0,0,(void*)2);
  EV_SET(&ch[1],4242,EVFILT_USER,0,NOTE_TRIGGER,0,(void*)0);
  r=kevent(kq,ch,2,0,0,0); int got=kevent(kq,0,0,&out,1,&z);
  printf("two changes: %d, woke %d udata %ld\n",r,got,(long)out.udata);
  return 0;
}
