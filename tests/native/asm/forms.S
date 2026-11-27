// forms.S -- every instruction and directive the assembler of compiler/native/a64asm.cho has, in each form it has it, for
// tests/commands/native_asm.sh: forms.expected is what clang's assembler made of this file (tests/native/asm_vectors.py).
// It is not a program: nothing here is meant to run.

// Before any directive the section is .text.
first:
    wfe
    b       first
    b       entry                 // another section's label: a relocation

.section .text.forms, "ax"
.global entry
.global outside
entry:
    b       moves
    .balign 16
    .quad   0x1122334455667788, -1, (1 << 40) | 3 + 4 - 1
    .quad   entry, moves, first, outside

// ---- mov ----
moves:
    mov     x0, x1
    mov     w2, w3
    mov     x4, xzr
    mov     sp, x0
    mov     x29, sp
    mov     x0, #0
    mov     x1, #-1
    mov     w1, #-1
    mov     x2, #0x33ff
    mov     x3, #(1 << 31)
    mov     x4, #(3 << 20)
    mov     x5, #0xffff000000000000
    mov     x6, #-0x10000
    mov     x7, #0xffffffff
    mov     w7, #0xffffffff
    mov     w8, #0x80000000
    mov     x9, #0xffffffffffff1234
    mov     w9, #0xffff1234
    mov     x10, #0x5555555555555555
    mov     w10, #0xaaaaaaaa
    mov     x11, #0x00ff00ff00ff00ff
    mov     x12, #0xfffffffffffffff0
    mov     xzr, #7
    mov     x13, 65536

// ---- loads and stores ----
memory:
    ldr     x0, [x1]
    ldr     w0, [x1]
    str     x2, [sp]
    str     w2, [x3]
    ldr     x4, [x5, #8]
    ldr     x4, [x5, #32760]
    ldr     w4, [x5, #16380]
    ldr     x4, [x5, #-8]             // what the scaled form cannot say is ldur
    ldr     x4, [x5, #1]
    ldr     w4, [x5, #255]
    str     x6, [sp, #16]
    str     w6, [x7, #-256]
    ldr     x8, [x9], #8
    ldr     w8, [x9], #4
    str     xzr, [x0], #8
    str     wzr, [x0], #-4
    ldrb    w10, [x11]
    ldrb    w10, [x11, #4095]
    ldrb    w10, [x11, #-1]
    ldrb    w10, [x11], #1
    strb    w12, [x13]
    strb    w12, [x13, #7]
    strb    w12, [x13], #1
    strb    wzr, [sp], #-1

// ---- the literal pool: one mov when one says it, else a literal after the section ----
literals:
    ldr     x0, =0
    ldr     x0, =0xffff
    ldr     x0, =0x10000
    ldr     x0, =0xffff000000000000
    ldr     w0, =0xffff0000
    ldr     x1, =0x30d00800
    ldr     x2, =0x6f6863616e6163
    ldr     x3, =0x30d00800           // again: the same literal
    ldr     w4, =0x12345
    ldr     w5, =0x12345
    ldr     w6, =-2
    ldr     x7, =-2000000
    ldr     x8, =(1 << 20) | 5
    ldr     x9, =outside
    ldr     x10, =entry
    ldr     x11, =moves
    ldr     x12, =outside
    ldr     x13, =first
    ldr     x14, moves                // a load from a label
    ldr     w15, moves
    ldr     x16, outside
    ldr     x17, entry

// ---- arithmetic ----
arithmetic:
    add     x0, x1, #0
    add     x0, x1, #4095
    add     x0, x1, #0x1000           // shifted left twelve
    add     x0, x1, #0xfff000
    add     x0, x1, #-4               // sub
    add     w2, w2, #4
    add     sp, sp, #16
    add     x3, sp, #64
    add     x4, x5, x6
    add     w2, w10, w10, lsl #1
    add     x4, x5, x6, lsr #63
    add     w4, w5, w6, asr #31
    sub     x0, x1, #1
    sub     x0, x1, #-1               // add
    sub     sp, sp, #0x2000
    sub     w7, w8, w9
    sub     x7, x8, x9, lsl #12
    subs    w7, w7, #1
    subs    x7, x7, #1
    subs    x7, x8, #-7               // adds
    subs    x7, x8, x9
    subs    wzr, w8, w9, lsr #3
    cmp     x0, #2
    cmp     w1, #2
    cmp     x0, #0x3000
    cmp     x0, #-1                   // cmn
    cmp     sp, #1
    cmp     x0, x1
    cmp     w10, w3
    cmp     x0, x1, lsl #2
    cmn     x5, #1
    cmn     w5, #-1                   // cmp
    cmn     x5, x6
    cmn     w5, w6, asr #1

// ---- logic ----
logic:
    and     x0, x0, #3
    and     w1, w1, #7
    and     w1, w1, #-2
    and     x0, x0, #0xfffffffffffffffe
    and     sp, x0, #0xfffffffffffffff0
    and     x0, x1, x2
    and     w0, w1, w2, lsl #4
    and     x0, x1, x2, ror #17
    orr     x0, x0, #0xff00
    orr     w0, w0, #0x80000000
    orr     w8, w8, w11
    orr     x8, x8, x9, lsr #1
    orr     x8, xzr, x9
    bic     x0, x0, #(1 << 0)
    bic     x0, x0, #(1 << 2)
    bic     x0, x0, #(1 << 12)
    bic     w0, w0, #1
    bic     x0, x1, x2
    bic     w0, w1, w2, asr #5

// ---- shifts and bits ----
bits:
    lsl     w11, w10, #1
    lsl     x11, x10, #63
    lsl     x11, x10, #0
    lsl     w8, w4, w5
    lsl     x8, x4, x5
    lsr     x0, x0, #2
    lsr     w0, w0, #31
    lsr     w1, w0, w2
    lsr     x1, x0, x2
    ubfx    w3, w0, #24, #3
    ubfx    w4, w1, #3, #10
    ubfx    w6, w1, #13, #15
    ubfx    x6, x1, #0, #64
    ubfx    x6, x1, #63, #1
    clz     w5, w4
    clz     x5, x4

// ---- branches, with numbered labels both ways ----
branches:
1:  cbz     w3, 9f
    cbnz    x0, 1b
    cbz     x0, outside
    cbnz    w0, first
5:  b.eq    5b
    b.ne    1b
    b.hs    2f
    b.lo    8f
    b.ge    7f
    b.lt    6f
    b.hi    5b
    b.ls    9f
6:
7:  b.gt    6b
    b.le    7b
    b.mi    8f
    b.pl    8f
    b.cs    9f
    b.cc    1b
8:  b.eq    outside
    b.ne    entry                     // a global label, even of this section, is whoever links's
    b.lo    first
2:  b       2b
    b       9f
    b       outside
    bl      moves
    bl      outside
    bl      entry
    br      x2
    br      x30
9:  adr     x0, 9b
    adr     x1, branches
    adr     x2, literals
    adr     x3, outside
    adr     x4, first
    adr     x5, entry
    adr     x6, 1f
1:  wfe

// ---- the system ----
system:
    mrs     x0, mpidr_el1
    mrs     x0, CurrentEL
    mrs     x1, ccsidr_el1
    mrs     x2, SCTLR_EL1
    msr     hcr_el2, x0
    msr     cntvoff_el2, xzr
    msr     csselr_el1, x11
    msr     VBAR_EL1, x30
    isb
    dsb     sy
    ic      iallu
    dc      isw, x8
    dc      cisw, x9
    dc      civac, x0
    tlbi    vmalle1
    wfe
    eret

// ---- a macro, used after the section it is used in has changed ----
.macro slot kind, to
    .balign 0x80
    mov     x0, #\kind
    mov     w1, #(\kind << 4) | 1
3:  b       \to
    b       3b
.endm

.macro pause
    wfe
.endm

.section .text.table, "ax"
.balign 0x100
table:
    slot 0, landing
    slot 1 , entry
    slot (2 + 3), outside
    pause
    slot 0x7f, 3f
3:  pause
    .balign 0x80
landing:
    ldr     x4, =outside
    ldr     x5, =0x6f6863616e6163
    ldr     w6, =0x7654321
    ldr     x7, =table
    ldr     x8, =landing
    ldr     x9, =-1
    cmp     x5, x6
    b.eq    moves
    b       table
    bl      landing

// Back in a section that was left: what follows goes after what it had, and its literals after that.
.section .text.forms, "ax"
again:
    ldr     x0, =0x123456789
    ldr     x1, =again
    b       system
    b       again
    .balign 8
    .quad   again
.section .text, "ax"
    b       first
    ldr     w0, =0x89abcdef
