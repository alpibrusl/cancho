/* virt.ld -- where tests/probes/bare_runs.cho goes in QEMU's `virt` machine (memory from 1 GB; `-kernel` puts an ELF where it is
   linked): the boot page of tests/native/asm/virt.S first, then the program; a stack of 64 KiB; the heap the kernel's allocator
   hands out, up to 0x44000000; the program's block is at 0x48000000, which nothing here reaches. */
ENTRY(_start)
SECTIONS
{
    . = 0x40080000;
    .text : { *(.text.boot) *(.text*) }
    .rodata : ALIGN(16) { *(.rodata*) }
    .data : ALIGN(16) { *(.data*) }
    .bss : ALIGN(16) { __bss_start = .; *(.bss*) *(COMMON) . = ALIGN(16); __bss_end = .; }
    . = ALIGN(16);
    . += 65536;
    __stack_top = .;
    __cancho_heap_start = ALIGN(4096);
    __cancho_heap_end = 0x44000000;
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) }
}
