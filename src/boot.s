/*

Boot code for Nether Battles.

*/

.section .text.boot

.globl boot
boot:
    // Clean up the BSS and set up the stack.
    adrp x0, bss_start
    adrp x1, bss_end
0:
    cmp x0, x1
    beq 0f
    stp xzr, xzr, [x0], #0x10
    b 0b
0:
    adrp fp, bss_end
    add fp, fp, #1 << 12
    mov sp, fp
    // Execute boot code depending on which exception level we're booting in.
    mrs x0, currentel
    cmp x0, #0x8 // Booted in EL2.
    beq 0f
    cmp x0, #0x4 // Booted in EL1.
    beq 1f
    // Booting in EL0 or EL3 is not supported.
    wfe
0:
    // Set up EL2 registers.
    adr x0, ivec
    msr vbar_el2, x0
    mov x0, #0x8000 << 16
    msr hcr_el2, x0
    mov x0, #0x30cd << 16
    movk x0, #0x830
    msr sctlr_el2, x0
    mov x0, #0x30 << 16
    msr cptr_el2, x0
    mrs x0, midr_el1
    msr vpidr_el2, x0
    mrs x0, mpidr_el1
    msr vmpidr_el2, x0
    mov x0, #0xc5
    msr spsr_el2, x0
    adr x0, main
    msr elr_el2, x0
    msr sp_el1, fp
1:
    // Set up EL1 registers.
    mov x0, #0x30 << 16
    msr cpacr_el1, x0
    isb
    msr fpcr, xzr
    msr fpsr, xzr
    adr x0, ivec
    msr vbar_el1, x0
    mov x0, #0xc5
    msr spsr_el1, x0
    adr x0, main
    msr elr_el1, x0
    // Set up the MMU with identity addresses and cache for the first 2MB of memory.
    mov x0, fp
    msr ttbr0_el1, x0
    add x1, x0, #1 << 12
    mov x2, #0x8000 << 48
    movk x2, #0x403
    orr x1, x1, x2
    str x1, [x0]
    mov x2, #0x725
    mov x1, #1 << 30
    orr x1, x1, x2
    str x1, [x0, #0x8]
    mov x1, #2 << 30
    orr x1, x1, x2
    str x1, [x0, #0x10]
    mov x1, #3 << 30
    orr x1, x1, x2
    str x1, [x0, #0x18]
    add x0, x0, #1 << 12
    add x1, x0, #1 << 12
    mov x2, #0x721
    str x2, [x0], #0x8
    mov x2, #2 << 20
    mov x3, #0x725
0:
    cmp x0, x1
    beq 0f
    orr x4, x2, x3
    str x4, [x0], #0x8
    add x2, x2, #2 << 20
    b 0b
0:
    mov x0, #0x1 << 32
    movk x0, #0x8020, lsl 16
    movk x0, #0x3520
    msr tcr_el1, x0
    mov x0, #0xff
    msr mair_el1, x0
    mov x0, #0x30d5 << 16
    movk x0, #0x1b9f
    msr sctlr_el1, x0
    isb
    // Jump to main at EL1.
    eret

.balign 0x800
.global ivec
ivec:
.irp kind,0,1,2,3
    b fault
.balign 0x80
    str x0, [sp, #-0x10]!
    mrs x0, currentel
    cmp x0, #0x4
    bne fault
    mrs x0, spsr_el1
    orr x0, x0, #0xc0
    msr spsr_el1, x0
    ldr x0, [sp], #0x10
    eret
.balign 0x80
    str x0, [sp, #-0x10]!
    mrs x0, currentel
    cmp x0, #0x4
    bne fault
    mrs x0, spsr_el1
    orr x0, x0, #0xc0
    msr spsr_el1, x0
    ldr x0, [sp], #0x10
    eret
.balign 0x80
    b fault
.balign 0x80
.endr

.section .text
