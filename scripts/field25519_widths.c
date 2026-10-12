// Phase 1 of docs/wide-multiply.md: field multiplication and a whole X25519 on 16 limbs of 16 bits (the shape of
// std/field25519.cho) against 5 limbs of 51 bits with a 64x64->128 multiply (curve25519-donna-c64 and the shape of
// std/field25519_51.cho). Plain C, no assembly. Both ladders are checked against RFC 7748 section 5.2 before they are timed.
//     cc -O2 -o field25519_widths scripts/field25519_widths.c && ./field25519_widths
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
typedef uint64_t u64; typedef int64_t i64; typedef unsigned __int128 u128;

/* ---------- 16 limbs of 16 bits, signed int64 (TweetNaCl, as std.field25519) ---------- */
typedef i64 f16[16];
static void c16(i64 *o){ for(int i=0;i<15;i++){ i64 v=o[i]+65536; i64 c=v>>16; o[i+1]+=c-1; o[i]=v-(c<<16);} i64 v=o[15]+65536; i64 c=v>>16; o[0]+=38*(c-1); o[15]=v-(c<<16);}
static void m16(i64 *o,const i64 *a,const i64 *b){ i64 t[31]={0}; for(int i=0;i<16;i++)for(int j=0;j<16;j++)t[i+j]+=a[i]*b[j]; for(int i=0;i<15;i++)t[i]+=38*t[i+16]; for(int i=0;i<16;i++)o[i]=t[i]; c16(o); c16(o);}
static void a16(i64*o,const i64*a,const i64*b){for(int i=0;i<16;i++)o[i]=a[i]+b[i];}
static void s16(i64*o,const i64*a,const i64*b){for(int i=0;i<16;i++)o[i]=a[i]-b[i];}
static void sw16(i64*p,i64*q,i64 bit){ i64 m=-bit; for(int i=0;i<16;i++){i64 x=m&(p[i]^q[i]);p[i]^=x;q[i]^=x;}}
static void un16(i64*o,const uint8_t*s){for(int i=0;i<16;i++)o[i]=s[2*i]|((i64)s[2*i+1]<<8);o[15]&=0x7fff;}
static void pk16(uint8_t*out,const i64*a){ i64 t[16],m[16]; memcpy(t,a,sizeof t); c16(t);c16(t);c16(t);
 for(int pass=0;pass<2;pass++){ m[0]=t[0]-0xffed; for(int i=1;i<15;i++){m[i]=t[i]-0xffff-((m[i-1]>>16)&1); m[i-1]&=0xffff;} m[15]=t[15]-0x7fff-((m[14]>>16)&1); i64 b=(m[15]>>16)&1; m[14]&=0xffff; sw16(t,m,1-b);}
 for(int i=0;i<16;i++){out[2*i]=t[i]&0xff;out[2*i+1]=(t[i]>>8)&0xff;}}
static void inv16(i64*o,const i64*a){ i64 c[16]; memcpy(c,a,sizeof c); for(int bit=253;bit>=0;bit--){ m16(c,c,c); if(bit!=2&&bit!=4) m16(c,c,a);} memcpy(o,c,sizeof c);}
static void x25519_16(uint8_t*out,const uint8_t*k,const uint8_t*u){
 uint8_t z[32]; memcpy(z,k,32); z[0]&=248; z[31]&=127; z[31]|=64;
 i64 x1[16],x2[16]={1},z2[16]={0},x3[16],z3[16]={1},a[16],b[16],c[16],d[16],e[16],da[16],cb[16],a24[16]={0xdb41,1};
 un16(x1,u); memcpy(x3,x1,sizeof x3); i64 swap=0;
 for(int i=254;i>=0;i--){ i64 bit=(z[i>>3]>>(i&7))&1; swap^=bit; sw16(x2,x3,swap); sw16(z2,z3,swap); swap=bit;
  a16(a,x2,z2); m16(e,a,a); s16(b,x2,z2); m16(d,b,b); s16(c,e,d); a16(x2,x3,z3); s16(z2,x3,z3); m16(da,z2,a); m16(cb,x2,b);
  a16(x3,da,cb); m16(x3,x3,x3); s16(z3,da,cb); m16(z3,z3,z3); m16(z3,z3,x1); m16(x2,e,d); m16(z2,c,a24); a16(z2,z2,e); m16(z2,c,z2);}
 sw16(x2,x3,swap); sw16(z2,z3,swap); inv16(z2,z2); m16(x2,x2,z2); pk16(out,x2);}

/* ---------- 5 limbs of 51 bits, u64, unsigned __int128 products (donna-c64) ---------- */

#define M51 ((1ULL<<51)-1)
typedef u64 f51[5];
static void a51(u64*o,const u64*a,const u64*b){for(int i=0;i<5;i++)o[i]=a[i]+b[i];}
static void s51(u64*o,const u64*a,const u64*b){ o[0]=a[0]+0xFFFFFFFFFFFDAULL-b[0]; for(int i=1;i<5;i++)o[i]=a[i]+0xFFFFFFFFFFFFEULL-b[i];}
static void m51(u64*out,const u64*in2,const u64*in){
 u64 r0=in[0],r1=in[1],r2=in[2],r3=in[3],r4=in[4],s0=in2[0],s1=in2[1],s2=in2[2],s3=in2[3],s4=in2[4];
 u128 t0,t1,t2,t3,t4; u64 c;
 t0=(u128)r0*s0; t1=(u128)r0*s1+(u128)r1*s0; t2=(u128)r0*s2+(u128)r2*s0+(u128)r1*s1;
 t3=(u128)r0*s3+(u128)r3*s0+(u128)r1*s2+(u128)r2*s1;
 t4=(u128)r0*s4+(u128)r4*s0+(u128)r3*s1+(u128)r1*s3+(u128)r2*s2;
 r4*=19;r1*=19;r2*=19;r3*=19;
 t0+=(u128)r4*s1+(u128)r1*s4+(u128)r2*s3+(u128)r3*s2;
 t1+=(u128)r4*s2+(u128)r2*s4+(u128)r3*s3;
 t2+=(u128)r4*s3+(u128)r3*s4;
 t3+=(u128)r4*s4;
 r0=(u64)t0&M51; c=(u64)(t0>>51);
 t1+=c; r1=(u64)t1&M51; c=(u64)(t1>>51);
 t2+=c; r2=(u64)t2&M51; c=(u64)(t2>>51);
 t3+=c; r3=(u64)t3&M51; c=(u64)(t3>>51);
 t4+=c; r4=(u64)t4&M51; c=(u64)(t4>>51);
 r0+=c*19; c=r0>>51; r0&=M51; r1+=c; c=r1>>51; r1&=M51; r2+=c;
 out[0]=r0;out[1]=r1;out[2]=r2;out[3]=r3;out[4]=r4;}

/* The same multiplication written the way cancho can write it with the three builtins: a (hi, lo) pair per
   product, explicit (sum, carry) additions into a two-word accumulator. No __int128 variable appears. */
static inline void mulw(u64 a,u64 b,u64*hi,u64*lo){u128 p=(u128)a*b;*lo=(u64)p;*hi=(u64)(p>>64);}
typedef struct{u64 hi,lo;}acc_t;
static inline void macc(acc_t*t,u64 a,u64 b){u64 h,l;mulw(a,b,&h,&l);u64 s=t->lo+l;u64 c=s<l;t->lo=s;t->hi=t->hi+h+c;}
static void m51b(u64*out,const u64*in2,const u64*in){
 u64 r0=in[0],r1=in[1],r2=in[2],r3=in[3],r4=in[4],s0=in2[0],s1=in2[1],s2=in2[2],s3=in2[3],s4=in2[4];
 acc_t t0={0,0},t1={0,0},t2={0,0},t3={0,0},t4={0,0};u64 c;
 macc(&t0,r0,s0);
 macc(&t1,r0,s1);macc(&t1,r1,s0);
 macc(&t2,r0,s2);macc(&t2,r2,s0);macc(&t2,r1,s1);
 macc(&t3,r0,s3);macc(&t3,r3,s0);macc(&t3,r1,s2);macc(&t3,r2,s1);
 macc(&t4,r0,s4);macc(&t4,r4,s0);macc(&t4,r3,s1);macc(&t4,r1,s3);macc(&t4,r2,s2);
 r4*=19;r1*=19;r2*=19;r3*=19;
 macc(&t0,r4,s1);macc(&t0,r1,s4);macc(&t0,r2,s3);macc(&t0,r3,s2);
 macc(&t1,r4,s2);macc(&t1,r2,s4);macc(&t1,r3,s3);
 macc(&t2,r4,s3);macc(&t2,r3,s4);
 macc(&t3,r4,s4);
 r0=t0.lo&M51; c=(t0.lo>>51)|(t0.hi<<13);
 {u64 s=t1.lo+c; t1.hi+=s<c; t1.lo=s;} r1=t1.lo&M51; c=(t1.lo>>51)|(t1.hi<<13);
 {u64 s=t2.lo+c; t2.hi+=s<c; t2.lo=s;} r2=t2.lo&M51; c=(t2.lo>>51)|(t2.hi<<13);
 {u64 s=t3.lo+c; t3.hi+=s<c; t3.lo=s;} r3=t3.lo&M51; c=(t3.lo>>51)|(t3.hi<<13);
 {u64 s=t4.lo+c; t4.hi+=s<c; t4.lo=s;} r4=t4.lo&M51; c=(t4.lo>>51)|(t4.hi<<13);
 r0+=c*19; c=r0>>51; r0&=M51; r1+=c; c=r1>>51; r1&=M51; r2+=c;
 out[0]=r0;out[1]=r1;out[2]=r2;out[3]=r3;out[4]=r4;}
static void sw51(u64*p,u64*q,u64 bit){u64 m=-bit;for(int i=0;i<5;i++){u64 x=m&(p[i]^q[i]);p[i]^=x;q[i]^=x;}}
static u64 ld(const uint8_t*s){u64 v=0;for(int i=7;i>=0;i--)v=v<<8|s[i];return v;}
static void un51(u64*o,const uint8_t*s){u64 w0=ld(s),w1=ld(s+8),w2=ld(s+16),w3=ld(s+24);
 o[0]=w0&M51;o[1]=(w0>>51|w1<<13)&M51;o[2]=(w1>>38|w2<<26)&M51;o[3]=(w2>>25|w3<<39)&M51;o[4]=(w3>>12)&M51;}
static void pk51(uint8_t*out,const u64*a){ u64 t[5],u[5]; memcpy(t,a,sizeof t);
 for(int r=0;r<2;r++){u64 c=t[4]>>51;t[4]&=M51;t[0]+=c*19;for(int i=0;i<4;i++){t[i+1]+=t[i]>>51;t[i]&=M51;}}
 memcpy(u,t,sizeof u);u[0]+=19;for(int i=0;i<4;i++){u[i+1]+=u[i]>>51;u[i]&=M51;} u64 q=u[4]>>51;
 t[0]+=19*q;for(int i=0;i<4;i++){t[i+1]+=t[i]>>51;t[i]&=M51;}t[4]&=M51;
 u64 w[4]={t[0]|t[1]<<51,t[1]>>13|t[2]<<38,t[2]>>26|t[3]<<25,t[3]>>39|t[4]<<12};
 for(int i=0;i<32;i++)out[i]=w[i>>3]>>(8*(i&7));}
static void inv51(u64*o,const u64*a){u64 c[5];memcpy(c,a,sizeof c);for(int bit=253;bit>=0;bit--){m51(c,c,c);if(bit!=2&&bit!=4)m51(c,c,a);}memcpy(o,c,sizeof c);}
static void x25519_51(uint8_t*out,const uint8_t*k,const uint8_t*u){
 uint8_t z[32];memcpy(z,k,32);z[0]&=248;z[31]&=127;z[31]|=64;
 u64 x1[5],x2[5]={1},z2[5]={0},x3[5],z3[5]={1},a[5],b[5],c[5],d[5],e[5],da[5],cb[5],a24[5]={121665};
 un51(x1,u);memcpy(x3,x1,sizeof x3);u64 swap=0;
 for(int i=254;i>=0;i--){u64 bit=(z[i>>3]>>(i&7))&1;swap^=bit;sw51(x2,x3,swap);sw51(z2,z3,swap);swap=bit;
  a51(a,x2,z2);m51(e,a,a);s51(b,x2,z2);m51(d,b,b);s51(c,e,d);a51(x2,x3,z3);s51(z2,x3,z3);m51(da,z2,a);m51(cb,x2,b);
  a51(x3,da,cb);m51(x3,x3,x3);s51(z3,da,cb);m51(z3,z3,z3);m51(z3,z3,x1);m51(x2,e,d);m51(z2,c,a24);a51(z2,z2,e);m51(z2,c,z2);}
 sw51(x2,x3,swap);sw51(z2,z3,swap);inv51(z2,z2);m51(x2,x2,z2);pk51(out,x2);}

static double now(){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;}
static void hex(const char*h,uint8_t*o){for(int i=0;i<32;i++){unsigned v;sscanf(h+2*i,"%2x",&v);o[i]=v;}}
int main(){
 uint8_t k[32],u[32],o16[32],o51[32];
 hex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4",k);
 hex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c",u);
 x25519_16(o16,k,u);x25519_51(o51,k,u);
 printf("rfc7748 16-bit: ");for(int i=0;i<32;i++)printf("%02x",o16[i]);puts("");
 printf("rfc7748 51-bit: ");for(int i=0;i<32;i++)printf("%02x",o51[i]);puts("");
 puts("expected      : c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");
 int N=2000;double t;
 t=now();for(int i=0;i<N;i++){x25519_16(o16,k,u);u[0]^=o16[0];}double d16=(now()-t)/N;
 t=now();for(int i=0;i<N;i++){x25519_51(o51,k,u);u[0]^=o51[0];}double d51=(now()-t)/N;
 printf("X25519 16-bit limbs: %.1f us   51-bit limbs: %.1f us   ratio %.1f\n",d16*1e6,d51*1e6,d16/d51);
 i64 A[16],B[16]; u64 a[5],b[5]; for(int i=0;i<16;i++){A[i]=1000+i*37;B[i]=2000+i*91;} for(int i=0;i<5;i++){a[i]=(1ULL<<50)+i*12345;b[i]=(1ULL<<50)+i*54321;}
 int M=20000000; u64 a2[5],b2[5]; memcpy(a2,a,sizeof a);memcpy(b2,b,sizeof b); t=now();for(int i=0;i<M;i++){m16(A,A,B);}double m16t=(now()-t)/M;
 t=now();for(int i=0;i<M;i++){m51(a,a,b);}double m51t=(now()-t)/M;
 t=now();for(int i=0;i<M;i++){m51b(a2,a2,b2);}double m51bt=(now()-t)/M;
 printf("field mul 51-bit, builtin-shaped (hi,lo) accumulators: %.1f ns (%llu)\n",m51bt*1e9,(unsigned long long)a2[0]);
 printf("field mul 16-bit: %.1f ns   51-bit: %.1f ns   ratio %.1f  (%lld %llu)\n",m16t*1e9,m51t*1e9,m16t/m51t,(long long)A[0],(unsigned long long)a[0]);
}
