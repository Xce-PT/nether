// Boot code for Nether Battles.

.bss

.balign 0x1000
// Stacks for both EL1 and EL2, one per core.
eln_stack_x4:
.zero 0x4000
// Translation tables initialized with invalid records.
root_tt:
.zero 0x1000
static_tt:
.zero 0x1000
heap_tt:
.zero 0x1000
perry_tt:
.zero 0x1000
stacks_tt:
.zero 0x1000
static_detail_tt:
.zero 0x1000

.text

.section .text.boot

// Boot code.
.globl boot
boot:
    // Set up the ELN stack.
    mrs x0, mpidr_el1
    and x0, x0, #0x3
    adrp fp, eln_stack_x4
    add fp, fp, x0, lsl 12
    add fp, fp, #1 << 12
    mov sp, fp
    // Execute boot code depending on the current exception level.
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
    mov x0, #0xc4
    msr spsr_el2, x0
    adr x0, start
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
    mov x0, #0xc4
    msr spsr_el1, x0
    adr x0, start
    msr elr_el1, x0
    // Core 0 tasks.
    mrs x0, mpidr_el1
    and x0, x0, #0x3
    cbnz x0, 0f
    // Clean up the BSS.
    adrp x0, bss_start
    adrp x1, bss_end
1:
    cmp x0, x1
    beq 1f
    stp xzr, xzr, [x0], #0x10
    b 1b
1:
    // Initialize the translation tables.
    adrp x0, root_tt
    adrp x1, static_tt
    adrp x2, heap_tt
    mov x3, #0x8000 << 48
    movk x3, #0x403
    orr x1, x1, x3
    orr x2, x2, x3
    stp x1, x2, [x0]
    adrp x1, perry_tt
    adrp x2, stacks_tt
    orr x1, x1, x3
    orr x2, x2, x3
    stp x1, x2, [x0, #0x10]
    adrp x0, static_tt
    adrp x1, static_detail_tt
    orr x1, x1, x3
    str x1, [x0]
    adrp x0, boot_start
    mov x1, x0
    adrp x2, boot_end
    sub x2, x2, x1
    mov x3, #0x4a3
    adrp x4, static_detail_tt
    mov x5, #1 << 12
    bl map
    adrp x0, text_start
    mov x1, x0
    adrp x2, text_end
    sub x2, x2, x1
    bl map
    adrp x0, rodata_start
    mov x1, x0
    adrp x2, rodata_end
    sub x2, x2, x1
    movk x3, #0x20, lsl 48
    bl map
    adrp x0, data_start
    mov x1, x0
    adrp x2, data_end
    sub x2, x2, x1
    movk x3, #0x723
    bl map
    adrp x0, bss_start
    mov x1, x0
    adrp x2, bss_end
    sub x2, x2, x1
    bl map
    mov x0, xzr
    mov x1, #32 << 20
    mov x2, #960 << 20 // 1024MB minus 32MB for the VC minus 32MB reserved for static code and data growth.
    mov x3, #0x30 << 48
    movk x3, #0x721
    adrp x4, heap_tt
    mov x5, #1 << 21
    bl map
    mov x0, xzr
    mov x1, #0xfc << 24
    mov x2, #64 << 20
    mov x3, #0x30 << 48
    movk x3, #0x429
    adrp x4, perry_tt
    bl map
    mov x0, #64 << 20
    adrp x1, dma_start
    adrp x2, dma_end
    sub x2, x2, x1
    mov x3, #0x20 << 48
    movk x3, #0x425
    bl map
    // Map the EL0 stacks.
    adrp x0, stacks_tt
    add x0, x0, #0xfc8
    adrp x1, stack_x4
    mov x2, #0x20 << 48
    movk x2, #0x421
    orr x3, x1, x2
    str x3, [x0], #0x10
    add x1, x1, #2 << 20
    orr x3, x1, x2
    str x3, [x0], #0x10
    add x1, x1, #2 << 20
    orr x3, x1, x2
    str x3, [x0], #0x10
    add x1, x1, #2 << 20
    orr x3, x1, x2
    str x3, [x0]
    // Unpark the secondary cores.
    adr x0, boot
    mov x1, #0xd8
    str x0, [x1, #0x8] // Core 1.
    str x0, [x1, #0x10] // Core 2.
    str x0, [x1, #0x18] // Core 3.
    dsb sy
    sev
0:
    // Configure and enable the MMu.
    adrp x0, root_tt
    msr ttbr0_el1, x0
    mov x0, #0x1 << 32
    movk x0, #0xa59d, lsl 16
    movk x0, #0x2520
    msr tcr_el1, x0
    mov x0, #0x44ff
    msr mair_el1, x0
    mov x0, #0x30d0 << 16
    movk x0, #0x1b9f
    msr sctlr_el1, x0
    isb
    // Jump to Rust code at EL1 with SP_EL0.
    mrs x0, mpidr_el1
    and x0, x0, #0x3
    mov fp, #1 << 32
    sub fp, fp, x0, lsl #22 // 2MB gap between stacks.
    msr sp_el0, fp
    mov fp, xzr
    eret

// Map function.
//
// x0: Start virtual offset (clobbered).
// x1: Start physical address (clobbered.
// x2: Length (clobbered).
// x3: Descriptor template.
// x4: Table.
// x5: Stride.
// x6: Temporary (clobbered).
map:
    // Compute the address of the first record.
    udiv x0, x0, x5
    lsl x0, x0, #3
    add x0, x0, x4
    // Compute the end physical address.
    add x2, x1, x2
    // Write the records.
0:
    cmp x1, x2
    beq 0f
    orr x6, x3, x1
    str x6, [x0], #0x8
    add x1, x1, x5
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
    mov fp, sp
    b fault
.balign 0x80
    stp x0, fp, [sp, #-0x10]!
    mov fp, sp
    mrs x0, currentel
    cmp x0, #0x4
    mov x0, #0x\kind + 1
    bne fault
    mrs x0, spsr_el1
    orr x0, x0, #0xc0
    msr spsr_el1, x0
    ldp x0, fp, [sp], #0x10
    eret
.balign 0x80
    stp x0, fp, [sp, #-0x10]!
    mov fp, sp
    mrs x0, currentel
    cmp x0, #0x4
    mov x0, #0x\kind + 2
    bne fault
    mrs x0, spsr_el1
    orr x0, x0, #0xc0
    msr spsr_el1, x0
    ldp x0, fp, [sp], #0x10
    eret
.balign 0x80
    mov x0, #0x\kind + 3
    mov fp, sp
    b fault
.balign 0x80
.endr

.section .text
