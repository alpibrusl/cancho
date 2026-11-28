// virt.S -- a boot page for QEMU's `virt` machine, in the subset compiler/native/a64asm.cho assembles:
//     qemu-system-aarch64 -M virt -cpu cortex-a72 -nographic -kernel IMAGE
// with the image linked at 0x40080000, where QEMU puts a kernel. The link gives __stack_top, __bss_start and __bss_end;
// the program gives main. QEMU starts a kernel that is not Linux's at EL1 with the MMU off.
.section .text.boot, "ax"
.global _start
_start:
    ldr     x0, =__stack_top
    mov     sp, x0
    mov     x0, #(3 << 20)            // CPACR_EL1.FPEN: floating point is not trapped
    msr     cpacr_el1, x0
    isb
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
1:  cmp     x0, x1
    b.hs    2f
    str     xzr, [x0], #8
    b       1b
2:  mov     x0, #0
    mov     x1, #0
    bl      main
park:
    wfe
    b       park

// probe_out(v): the foreign function of tests/probes/freestanding.cho, `declare i64 @probe_out(i64)`. It writes
// `probe `, the low byte of v as two hexadecimal digits and a new line to the PL011 at 0x09000000, and answers 0.
.global probe_out
probe_out:
    ldr     x1, =0x09000000           // the PL011's data register: QEMU's takes a byte whenever one is written
    mov     w2, #112                  // "probe "
    strb    w2, [x1]
    mov     w2, #114
    strb    w2, [x1]
    mov     w2, #111
    strb    w2, [x1]
    mov     w2, #98
    strb    w2, [x1]
    mov     w2, #101
    strb    w2, [x1]
    mov     w2, #32
    strb    w2, [x1]
    lsr     w2, w0, #4                // the number's low byte, as two hexadecimal digits
    and     w2, w2, #15
    cmp     w2, #10
    b.lo    1f
    add     w2, w2, #39
1:  add     w2, w2, #48
    strb    w2, [x1]
    and     w2, w0, #15
    cmp     w2, #10
    b.lo    2f
    add     w2, w2, #39
2:  add     w2, w2, #48
    strb    w2, [x1]
    mov     w2, #10
    strb    w2, [x1]
    mov     x0, #0
    br      x30
