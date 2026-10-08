#!/usr/bin/env python3
"""The arithmetic of docs/gcm-wide.md §3.2, in Python, against SP 800-38D §6.3.

    python3 scripts/ghash_reflected_model.py

GHASH on blocks read as big-endian integers (the reflected order, no bit
reversal), with the powers of H each times x^-1 so that no product needs a
shift, products summed unreduced, and one reduction by shifts of 64-bit lanes
for the sum (the Linux kernel's `ghash-clmulni-intel` sequence): the steps
`crates/cancho-codegen-llvm/src/crypto/ghash.rs` emits as LLVM IR. It checks
300 random single products and an eight-block aggregation against the
bit-by-bit algorithm. Prints `ok`; an assertion fails otherwise.
"""
import random
M64=(1<<64)-1; M=(1<<128)-1
def bswap(b): return int.from_bytes(b,'big')
def clmul(a,b):
    r=0
    while b:
        if b&1: r^=a
        a<<=1; b>>=1
    return r
def ref_mul(x,y):
    X=int.from_bytes(x,'big'); Y=int.from_bytes(y,'big')
    R=0xE1<<120; Z=0; V=Y
    for i in range(128):
        if (X>>(127-i))&1: Z^=V
        V = (V>>1)^R if V&1 else V>>1
    return Z
def lanes(v): return v&M64, v>>64
def mk(lo,hi): return (lo&M64)|((hi&M64)<<64)
def shl64(v,k): lo,hi=lanes(v); return mk(lo<<k,hi<<k)
def shr64(v,k): lo,hi=lanes(v); return mk(lo>>k,hi>>k)
def reduce2(data,t1):
    t3=data
    t3=shl64(t3,1)^data; t3=shl64(t3,5)^data; t3=shl64(t3,57)
    l,h=lanes(t3)
    t2=mk(0,l)        # pslldq 8
    t3=mk(h,0)        # psrldq 8
    data^=t2; t1^=t3
    t2=data
    t2=shr64(t2,5)^data; t2=shr64(t2,1)^data; t2=shr64(t2,1)
    t1^=t2; t1^=data
    return t1
POLY=0xC2000000000000000000000000000001
def twist(v):
    c=v>>127
    r=(v<<1)&M
    return r^POLY if c else r
def agg(xs,ts):
    lo=mid=hi=0
    for x,t in zip(xs,ts):
        x0,x1=lanes(x); t0,t1=lanes(t)
        lo^=clmul(x0,t0); hi^=clmul(x1,t1); mid^=clmul(x0,t1)^clmul(x1,t0)
    data=lo^((mid&M64)<<64); t1=hi^(mid>>64)
    return reduce2(data&M,t1&M)
random.seed(3)
for t in range(300):
    h=random.randbytes(16); x=random.randbytes(16)
    got=agg([bswap(x)],[twist(bswap(h))])
    assert got==ref_mul(x,h)
# powers via fast mul
h=random.randbytes(16); hv=bswap(h); T1=twist(hv)
pw=[hv]
for i in range(7): pw.append(agg([pw[-1]],[T1]))
T=[twist(p) for p in pw]
ys=random.randbytes(16); blocks=[random.randbytes(16) for _ in range(8)]
yy=ys
for b in blocks: yy=ref_mul(bytes(a^c for a,c in zip(yy,b)),h).to_bytes(16,'big')
xs=[bswap(b) for b in blocks]; xs[0]^=bswap(ys)
got=agg(xs,[T[7-j] for j in range(8)])
assert got.to_bytes(16,'big')==yy
print("ok")
