//! Nether Battles intends to one day be a Dungeon Keeper clone with primitive
//! assets running on a bare metal Raspberry Pi 4.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(panic_info_message)]
#![feature(pointer_byte_offsets)]
#![feature(allocator_api)]
#![feature(default_alloc_error_handler)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(strict_provenance)]
#![feature(slice_ptr_get)]

mod alloc;
#[cfg(not(test))]
mod irq;
mod mbox;
mod sync;
#[cfg(not(test))]
mod touch;
#[cfg(not(test))]
mod uart;
#[cfg(not(test))]
mod video;

#[cfg(not(test))]
use core::arch::{asm, global_asm};
#[cfg(not(test))]
use core::fmt::Write;
#[cfg(not(test))]
use core::ops::Range;
#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use core::write;

#[cfg(not(test))]
use self::irq::IRQ;
#[cfg(not(test))]
use self::touch::TOUCH;
#[cfg(not(test))]
use self::uart::UART;
#[cfg(not(test))]
use self::video::VIDEO;

/// Heap range.
#[cfg(not(test))]
const HEAP_RANGE: Range<usize> = 0x40000000 .. 0x80000000 - (64 << 20);
/// DMA RANGE.
#[cfg(not(test))]
const DMA_RANGE: Range<usize> = 0x1000 .. 0x80000;
/// Peripherals range.
#[cfg(not(test))]
const PERRY_RANGE: Range<usize> = 0x80000000 .. 0x84000000;
/// Video core reserved range.
#[cfg(not(test))]
const VC_RANGE: Range<usize> = 0x84000000 .. 0x86000000;
/// Pixel valve 1 base address.
#[cfg(not(test))]
const PV1_BASE: usize = 0x2207000 + PERRY_RANGE.start;
/// PV1 interrupt enable register.
#[cfg(not(test))]
const PV1_INTEN: *mut u32 = (PV1_BASE + 0x24) as _;
/// PV1 status and acknowledgement register.
#[cfg(not(test))]
const PV1_STAT: *mut u32 = (PV1_BASE + 0x28) as _;
/// PV1 IRQ.
#[cfg(not(test))]
const PV1_IRQ: usize = 142;
/// PV VSync interrupt enable flag.
#[cfg(not(test))]
const PV_VSYNC: u32 = 0x10;

#[cfg(not(test))]
global_asm!(include_str!("boot.s"));

/// Entry point.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn main(affinity: u8) -> !
{
    debug!("Booted core #{affinity}");
    if affinity != 0 {
        halt()
    }
    IRQ.lock().listen(PV1_IRQ, tick);
    VIDEO.lock().clear();
    unsafe {
        PV1_STAT.write_volatile(PV_VSYNC);
        PV1_INTEN.write_volatile(PV_VSYNC);
    };
    loop {
        IRQ.lock().handle();
        sleep();
    }
}

/// Panics with diagnostic information about a fault.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn fault(kind: usize) -> !
{
    let affinity: usize;
    let level: usize;
    let syndrome: usize;
    let addr: usize;
    let ret: usize;
    let state: usize;
    unsafe {
        asm!(
            "mrs {aff}, mpidr_el1",
            "and {aff}, {aff}, #0x3",
            "mrs {el}, currentel",
            "lsr {el}, {el}, #2",
            aff = out (reg) affinity,
            el = out (reg) level,
            options (nomem, nostack, preserves_flags));
        match level {
            2 => asm!(
                    "mrs {synd}, esr_el2",
                    "mrs {addr}, far_el2",
                    "mrs {ret}, elr_el2",
                    "mrs {state}, spsr_el2",
                    synd = out (reg) syndrome,
                    addr = out (reg) addr,
                    ret = out (reg) ret,
                    state = out (reg) state,
                    options (nomem, nostack, preserves_flags)),
            1 => asm!(
                        "mrs {synd}, esr_el1",
                        "mrs {addr}, far_el1",
                        "mrs {ret}, elr_el1",
                        "mrs {state}, spsr_el1",
                        synd = out (reg) syndrome,
                        addr = out (reg) addr,
                        ret = out (reg) ret,
                        state = out (reg) state,
                        options (nomem, nostack, preserves_flags)),
            _ => panic!("Exception caught at unsupported level {level}"),
        }
    };
    panic!("Core #{affinity} triggered an exception at level {level}: Kind: 0x{kind:x}, Syndrome: 0x{syndrome:x}, Address: 0x{addr:x}, Location: 0x{ret:x}, State: 0x{state:x}");
}

/// Halts the system.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn halt() -> !
{
    let affinity: usize;
    unsafe {
        asm!("mrs {affinity}, mpidr_el1", "and {affinity}, {affinity}, #0x3", affinity = out (reg) affinity, options (nomem, nostack, preserves_flags))
    };
    debug!("Halted core #{affinity}");
    unsafe {
        asm!("msr daifset, #0x3",
             "0:",
             "wfe",
             "b 0b",
             options(nomem, nostack, preserves_flags, noreturn))
    }
}

/// Dummy function just to run tests.
#[cfg(test)]
fn main() {}

/// Performs a single iteration of the main loop..
#[cfg(not(test))]
fn tick(irq: usize)
{
    if irq != PV1_IRQ {
        return;
    }
    let mut video = VIDEO.lock();
    let mut touch = TOUCH.lock();
    let touches = touch.poll();
    video.draw_circles(touches);
    video.vsync();
    unsafe { PV1_STAT.write_volatile(PV_VSYNC) };
}

/// Halts the system with a diagnostic error message.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> !
{
    let mut uart = UART.lock();
    if let Some(location) = info.location() {
        write!(uart, "Panicked at {}:{}: ", location.file(), location.line()).unwrap()
    } else {
        uart.write_str("Panic: ").unwrap()
    }
    if let Some(args) = info.message() {
        uart.write_fmt(*args).unwrap()
    } else {
        uart.write_str("Unknown reason").unwrap()
    }
    uart.write_char('\n').unwrap();
    drop(uart);
    halt();
}

/// Puts the system to sleep until the next interrupt.
#[cfg(not(test))]
fn sleep()
{
    unsafe {
        asm!("msr daifclr, #0x3",
             "wfi",
             "msr daifset, #0x3",
             options(nomem, nostack, preserves_flags))
    };
}
