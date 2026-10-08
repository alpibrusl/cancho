#!/usr/bin/env python3
"""The arithmetic of docs/gcm-wide.md §3.2, in Python, against SP 800-38D §6.3.

    python3 scripts/ghash_reflected_model.py

GHASH on blocks read as big-endian integers (the reflected order, no bit
reversal), products summed unreduced, and one shift and fold for the sum: the
steps `crates/cancho-codegen-llvm/src/crypto/ghash.rs` emits as LLVM IR. It
checks 200 random single products and an eight-block aggregation against the
bit-by-bit algorithm. Prints `ok`; an assertion fails otherwise.
"""
import random
M=(1<<128)-1
def bswap(b): return int.from_bytes(b,'big')  # reflected value of raw 16 bytes
def clmul(a,b):
    r=0
    while b:
        if b&1: r^=a
        a<<=1; b>>=1
    return r
def ref_mul(x,y):  # SP800-38D bit order; x,y 16-byte -> ints with bit0 = MSB
    X=int.from_bytes(x,'big'); Y=int.from_bytes(y,'big')
    R=0xE1<<120
    Z=0; V=Y
    for i in range(128):
        if (X>>(127-i))&1: Z^=V
        V = (V>>1)^R if V&1 else V>>1
    return Z
def reduce_(lo,mid,hi):
    m0=mid&((1<<64)-1); m1=mid>>64
    plo=lo^(m0<<64)
    phi=hi^m1
    plo&=M; phi&=M
    H=((phi<<1)|(plo>>127))&M
    L=(plo<<1)&M
    th=L^(L>>1)^(L>>2)^(L>>7)
    tl=((L<<127)^(L<<126)^(L<<121))&M
    u=tl^(tl>>1)^(tl>>2)^(tl>>7)
    return H^th^u
def split(v): return v&((1<<64)-1), v>>64
def agg(xs,hs):
    lo=mid=hi=0
    for x,h in zip(xs,hs):
        x0,x1=split(x); h0,h1=split(h)
        lo^=clmul(x0,h0); hi^=clmul(x1,h1); mid^=clmul(x0,h1)^clmul(x1,h0)
    return reduce_(lo,mid,hi)
random.seed(1)
for t in range(200):
    h=random.randbytes(16); x=random.randbytes(16)
    hv=bswap(h); xv=bswap(x)
    got=agg([xv],[hv])
    want=ref_mul(x,h)
    assert got==want,(hex(got),hex(want))
# powers and 8-block aggregation
h=random.randbytes(16); hv=bswap(h)
pw=[hv]
for i in range(7): pw.append(agg([pw[-1]],[hv]))
ys=random.randbytes(16); blocks=[random.randbytes(16) for _ in range(8)]
y=ref_mul(ys,h)  # dummy
y=bswap(ys)
yy=ys
for b in blocks:
    yy=ref_mul(bytes(a^c for a,c in zip(yy,b)),h).to_bytes(16,'big')
xs=[bswap(b) for b in blocks]; xs[0]^=y
got=agg(xs,[pw[7-j] for j in range(8)])
assert got.to_bytes(16,'big')==yy
print("ok")
