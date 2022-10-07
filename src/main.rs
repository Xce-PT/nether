//! Nether Battles intends to one day be a Dungeon Keeper clone with primitive
//! assets running on a bare metal Raspberry Pi 4.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(panic_info_message)]
#![feature(pointer_byte_offsets)]

#[cfg(not(test))]
mod irq;
mod mbox;
mod pgalloc;
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
use self::pgalloc::ALLOC as PGALLOC;
#[cfg(not(test))]
use self::touch::TOUCH;
#[cfg(not(test))]
use self::uart::UART;
#[cfg(not(test))]
use self::video::VIDEO;

/// Pixel valve 1 IRQ.
#[cfg(not(test))]
const PV_IRQ: usize = 142;
/// Pixel valve 1 base address.
#[cfg(not(test))]
const PV_BASE: usize = 0xFE207000;
/// Pixel valve 1 interrupt enabler register.
#[cfg(not(test))]
const PV_INT: *mut u32 = (PV_BASE + 0x24) as _;
/// Pixel valve 1 interrupt status and acknowledgement register.
#[cfg(not(test))]
const PV_STATUS: *mut u32 = (PV_BASE + 0x28) as _;
/// VSync interrupt.
#[cfg(not(test))]
const VSYNC_PV_INT: u32 = 0x10;
/// Total amount of physical RAM.
#[cfg(not(test))]
const TOTAL_RAM: usize = 1 << 30;
/// Free RAM sections.
#[cfg(not(test))]
const FREE_RAM: [Range<usize>; 4] = [0x0 .. 0x1160000,
                                     0x1510000 .. 0x1AC00000,
                                     0x2EC00000 .. 0x2EFF2000,
                                     0x2F000000 .. 0x37400000];
/// Smallest size of a memory page.
#[cfg(not(test))]
const PAGE_GRANULE: usize = 0x1000;

#[cfg(not(test))]
global_asm!(include_str!("boot.s"));

/// Entry point.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn main() -> !
{
    let dynamic: usize;
    unsafe { asm!("adrp {dynamic}, dynamic", dynamic = out (reg) dynamic) };
    let mut regions = FREE_RAM;
    regions[0].start = dynamic;
    unsafe { PGALLOC.track(&regions) };
    unsafe { PV_STATUS.write_volatile(VSYNC_PV_INT) };
    unsafe { PV_INT.write_volatile(VSYNC_PV_INT) };
    let mut irq = IRQ.lock();
    irq.listen(PV_IRQ, tick);
    drop(irq);
    debug!("Boot complete");
    loop {
        IRQ.lock().handle();
        sleep();
    }
}

/// Panics with diagnostic information about a fault.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn fault() -> !
{
    let level: usize;
    let syndrome: usize;
    let addr: usize;
    let ret: usize;
    let state: usize;
    unsafe {
        asm!("mrs {el}, currentel", "lsr {el}, {el}, #2", el = out (reg) level, options (nomem, nostack, preserves_flags))
    };
    match level {
        3 => unsafe {
            asm!("mrs {synd}, esr_el3", "mrs {addr}, far_el3", "mrs {ret}, elr_el3", "mrs {state}, spsr_el3", synd = out (reg) syndrome, addr = out (reg) addr, ret = out (reg) ret, state = out (reg) state, options (nomem, nostack, preserves_flags))
        },
        2 => unsafe {
            asm!("mrs {synd}, esr_el2", "mrs {addr}, far_el2", "mrs {ret}, elr_el2", "mrs {state}, spsr_el2", synd = out (reg) syndrome, addr = out (reg) addr, ret = out (reg) ret, state = out (reg) state, options (nomem, nostack, preserves_flags))
        },
        1 => unsafe {
            asm!("mrs {synd}, esr_el1", "mrs {addr}, far_el1", "mrs {ret}, elr_el1", "mrs {state}, spsr_el1", synd = out (reg) syndrome, addr = out (reg) addr, ret = out (reg) ret, state = out (reg) state, options (nomem, nostack, preserves_flags))
        },
        _ => panic!("Unknown exception caught at level {level}"),
    }
    panic!("Fault at level {level}: Syndrome: 0x{syndrome:x}, Address: 0x{addr:x}, Location: 0x{ret:x}, State: 0x{state:x}");
}

/// Dummy function just to run tests.
#[cfg(test)]
fn main() {}

/// Performs a single iteration of the main loop..
#[cfg(not(test))]
fn tick()
{
    if unsafe { PV_STATUS.read_volatile() & VSYNC_PV_INT } == 0 {
        return;
    }
    let mut video = VIDEO.lock();
    let mut touch = TOUCH.lock();
    let touches = touch.poll();
    video.draw_circles(touches);
    video.vsync();
    unsafe { PV_STATUS.write_volatile(VSYNC_PV_INT) };
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

/// Halts the system.
#[cfg(not(test))]
fn halt() -> !
{
    unsafe {
        asm!("msr daifset, #0x3",
             "0:",
             "wfe",
             "b 0b",
             options(nomem, nostack, preserves_flags, noreturn))
    }
}
