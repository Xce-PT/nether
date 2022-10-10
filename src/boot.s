// Boot code for Nether Battles.

.data

// High range root table initialized with 32x1GB hardware memory pages mapping the entire physical address space.
.balign 0x1000
l1_tt1:
.irp page,0,1,2,3,4,5,6,7,8,9,a,b,c,d,e,f,10,11,12,13,14,15,16,17,18,19,1a,1b,1c,1d,1e,1f
.quad 0x20 << 48 | 0x\page << 30 | 0x725
.endr
.zero 0xf00

.bss

.balign 0x1000
// Stack for both EL1 and EL2.
stack:
.zero 0x1000
// Low range L1, L2, and L3 tables initialized with invalid records.
l1_tt0:
.zero 0x1000
l2_tt0:
.zero 0x1000
l3_tt0:
.zero 0x1000

.text

.section .text.boot

// Boot code.
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
    adrp fp, stack
    add fp, fp, #1 << 12
    mov sp, fp
    // Execute boot code depending on which exception level we're booting in.
    mrs x0, currentel
    cmp x0, #0x8 // Booted in EL2.
    beq 0f
    cmp x0, #0x4 // Booted in EL1.
    beq 1f
    // Booting in EL0 or EL3 is not supported.
    bl halt
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
    // Set up the MMU with low range cached identity addresses for the code, data, and heap, and the entire physical address space in high range addresses.
    adrp x0, l1_tt0
    adrp x1, l2_tt0
    mov x2, #0x8000 << 48
    movk x2, #0x403
    orr x1, x1, x2
    str x1, [x0]
    adrp x0, l2_tt0
    adrp x1, l3_tt0
    orr x1, x1, x2
    str x1, [x0]
    adrp x0, boot_start
    adrp x1, boot_end
    mov x2, #0x7a3
    bl idmap
    adrp x0, text_start
    adrp x1, text_end
    bl idmap
    adrp x0, rodata_start
    adrp x1, rodata_end
    movk x2, #0x20, lsl 48
    bl idmap
    adrp x0, data_start
    adrp x1, data_end
    movk x2, #0x723
    bl idmap
    adrp x0, bss_start
    adrp x1, bss_end
    bl idmap
    adrp x0, l1_tt0
    msr ttbr0_el1, x0
    adrp x0, l1_tt1
    msr ttbr1_el1, x0
    mov x0, #0x1 << 32
    movk x0, #0xa51d, lsl 16
    movk x0, #0x2520
    msr tcr_el1, x0
    mov x0, #0xff
    msr mair_el1, x0
    mov x0, #0x30d0 << 16
    movk x0, #0x1b9f
    msr sctlr_el1, x0
    isb
    // Jump to main at EL1.
    eret

// Identity map function.
//
// x0: Start address (clobbered input).
// x1: End address (input).
// x2: Descriptor template (clobbered input).
idmap:
    adrp x3, l3_tt0
    add x3, x3, x0, lsr #9
0:
    cmp x0, x1
    beq 0f
    orr x4, x0, x2
    str x4, [x3], #0x8
    add x0, x0, #1 << 12
    b 0b
0:
    ret

// Interrupt vector.
//
// Panics on any EL2 interrupts, any Sync or SError EL1 interrupts, and does nothing for FIQs and IRQs
// since those are handled synchronously.
.balign 0x800
ivec:
.irp kind,0,4,8,c
    mov x0, #0x\kind
    b fault
.balign 0x80
    str x0, [sp, #-0x10]!
    mrs x0, currentel
    cmp x0, #0x4
    mov x0, #0x\kind + 1
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
    mov x0, #0x\kind + 2
    bne fault
    mrs x0, spsr_el1
    orr x0, x0, #0xc0
    msr spsr_el1, x0
    ldr x0, [sp], #0x10
    eret
.balign 0x80
    mov x0, #0x\kind + 3
    b fault
.balign 0x80
.endr

.section .text
