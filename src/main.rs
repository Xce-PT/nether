//! Nether Battles intends to one day be a Dungeon Keeper clone with primitive
//! assets running on a bare metal Raspberry Pi 4.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(panic_info_message)]
#![feature(pointer_byte_offsets)]
#![feature(allocator_api)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(strict_provenance)]
#![feature(slice_ptr_get)]
#![feature(portable_simd)]

mod alloc;
#[cfg(not(test))]
mod irq;
mod math;
#[cfg(not(test))]
mod mbox;
#[cfg(not(test))]
mod pixvalve;
#[cfg(not(test))]
mod sched;
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
use core::f32::consts::FRAC_PI_2;
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
use self::math::{Matrix, Projector, Quaternion, Scalar, Vector};
#[cfg(not(test))]
use self::sched::SCHED;
#[cfg(not(test))]
use self::touch::Recognizer;
#[cfg(not(test))]
use self::uart::UART;
#[cfg(not(test))]
use self::video::{Triangle, VIDEO};

/// Cached range.
#[cfg(not(test))]
const CACHED_RANGE: Range<usize> = 0x40000000 .. 0x80000000 - (32 << 20);
/// DMA RANGE.
#[cfg(not(test))]
const DMA_RANGE: Range<usize> = 0x1000 .. 0x80000;
/// Peripherals range.
#[cfg(not(test))]
const PERRY_RANGE: Range<usize> = 0x80000000 .. 0x84000000;
/// Video core reserved range.
#[cfg(not(test))]
const VC_RANGE: Range<usize> = 0x84000000 .. 0x86000000;
/// Logical CPU count.
#[cfg(not(test))]
const CPU_COUNT: usize = 4;
/// Software generated IRQ that halts the system.
#[cfg(not(test))]
const HALT_IRQ: u32 = 0;

#[cfg(not(test))]
global_asm!(include_str!("boot.s"));

/// Entry point.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn start() -> !
{
    let affinity = cpu_id();
    debug!("Booted core #{affinity}");
    if affinity == 0 {
        IRQ.register(HALT_IRQ, || halt());
        SCHED.spawn(ticker());
    }
    IRQ.dispatch()
}

/// Actions to perform in an infinite loop.
#[cfg(not(test))]
async fn ticker() -> !
{
    let proj = Projector::perspective(FRAC_PI_2, 0.5, 4.0);
    let tri = Triangle::new();
    let cam = Matrix::default();
    let pos = Vector::from_components(0.0, 0.0, -2.0);
    let mut rot = Quaternion::default();
    let scale = Scalar::default();
    let mut recog = Recognizer::new();
    loop {
        recog.sample();
        let vec0 = Vector::from_components(0.0, 0.0, 1.0);
        let vec1 = recog.translated();
        let axis = vec0.cross(vec0 + vec1);
        let angle = vec1.length().to_angle();
        rot = Quaternion::from_axis_angle(axis, angle) * rot;
        rot = recog.rotated() * rot;
        let mdl = Matrix::from_components(pos, rot, scale);
        VIDEO.enqueue(tri.geom(), mdl, cam, proj);
        VIDEO.commit().await;
    }
}

/// Panics with diagnostic information about a fault.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn fault(kind: usize) -> !
{
    let affinity = cpu_id();
    let level: usize;
    let syndrome: usize;
    let addr: usize;
    let ret: usize;
    let state: usize;
    unsafe {
        asm!(
            "mrs {el}, currentel",
            "lsr {el}, {el}, #2",
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
    let affinity = cpu_id();
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

/// Halts the system with a diagnostic error message.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> !
{
    let mut uart = UART.lock();
    let affinity = cpu_id();
    if let Some(location) = info.location() {
        write!(uart,
               "Core #{affinity} panicked at {}:{}: ",
               location.file(),
               location.line()).unwrap()
    } else {
        write!(uart, "Core #{affinity} panic: ").unwrap()
    }
    if let Some(args) = info.message() {
        uart.write_fmt(*args).unwrap()
    } else {
        uart.write_str("Unknown reason").unwrap()
    }
    uart.write_char('\n').unwrap();
    drop(uart);
    backtrace();
    IRQ.trigger(HALT_IRQ);
    halt();
}

/// Returns the ID of the current CPU core.
#[cfg(not(test))]
fn cpu_id() -> usize
{
    let id: usize;
    unsafe {
        asm!(
            "mrs {id}, mpidr_el1",
            "and {id}, {id}, #0xff",
            id = out (reg) id,
            options (nomem, nostack, preserves_flags));
    }
    id
}

/// Sends the return addresses of all the function calls from this function all
/// the way to the boot code through the UART.
#[cfg(not(test))]
fn backtrace()
{
    let mut uart = UART.lock();
    let mut fp: usize;
    let mut lr: usize;
    unsafe {
        asm!("mov {fp}, fp", "mov {lr}, lr", fp = out (reg) fp, lr = out (reg) lr, options (nomem, nostack, preserves_flags))
    };
    let mut frame = 0usize;
    writeln!(uart, "Backtrace:").unwrap();
    while fp != 0x0 {
        writeln!(uart, "#{frame}: 0x{lr:X}").unwrap();
        unsafe { asm!("ldp {fp}, {lr}, [{fp}]", fp = inout (reg) fp, lr = out (reg) lr, options (preserves_flags)) };
        frame += 1;
    }
}
