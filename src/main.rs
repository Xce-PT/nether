//! Nether Battles intends to one day be a Dungeon Keeper clone with primitive
//! assets running on a bare metal Raspberry Pi 4.

#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]
#![feature(panic_info_message)]
#![feature(pointer_byte_offsets)]
#![feature(allocator_api)]
#![feature(strict_provenance)]
#![feature(slice_ptr_get)]
#![feature(portable_simd)]
#![feature(iter_array_chunks)]

mod alloc;
#[cfg(not(test))]
mod clock;
#[cfg(not(test))]
mod cpu;
#[cfg(not(test))]
mod irq;
mod math;
#[cfg(not(test))]
mod mbox;
#[cfg(not(test))]
mod pixvalve;
#[cfg(not(test))]
mod sched;
#[cfg(not(test))]
mod sync;
#[cfg(not(test))]
mod timer;
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
#[cfg(not(test))]
use core::ops::Range;
#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use core::write;

#[cfg(not(test))]
use self::cpu::{id as cpu_id, COUNT as CPU_COUNT, LOAD as CPU_LOAD};
#[cfg(not(test))]
use self::irq::IRQ;
#[cfg(not(test))]
use self::math::{Angle, Quaternion, Transform, Vector};
#[cfg(not(test))]
use self::sched::SCHED;
#[cfg(not(test))]
use self::timer::TIMER;
#[cfg(not(test))]
use self::touch::Recognizer;
#[cfg(not(test))]
use self::uart::UART;
#[cfg(not(test))]
use self::video::{Square, Triangle, VIDEO};

/// uncached RANGE.
#[cfg(not(test))]
const UNCACHED_RANGE: Range<usize> = 0x84000000 .. 0x85600000;
/// Cached range.
#[cfg(not(test))]
const CACHED_RANGE: Range<usize> = 0x40000000 .. 0x7C000000;
/// Peripherals range.
#[cfg(not(test))]
const PERRY_RANGE: Range<usize> = 0x80000000 .. 0x84000000;
/// Stack ranges.
#[cfg(not(test))]
const STACK_RANGES: [Range<usize>; CPU_COUNT] = [0xFFE00000 .. 0x100000000,
                                                 0xFFA00000 .. 0xFFC00000,
                                                 0xFF600000 .. 0xFF800000,
                                                 0xFF200000 .. 0xFF400000];
/// Uncached range from the perspective of the DMA controller.
#[cfg(not(test))]
const DMA_UNCACHED_RANGE: Range<usize> = 0xC0200000 .. 0xC1800000;
/// Cached range from the perspective of the DMA controller.
#[cfg(not(test))]
const DMA_CACHED_RANGE: Range<usize> = 0xC2000000 .. 0xCE000000;
/// Peripherals range from the perspective of the DMA controller.
#[cfg(not(test))]
const DMA_PERRY_RANGE: Range<usize> = 0x7C000000 .. 0x80000000;
/// Stack ranges from the perspective of the DMA controller.
#[cfg(not(test))]
const DMA_STACK_RANGES: [Range<usize>; CPU_COUNT] = [0xC1E00000 .. 0xC2000000,
                                                     0xC1C00000 .. 0xC1E00000,
                                                     0xC1A00000 .. 0xC1C00000,
                                                     0xC1800000 .. 0xC1A00000];
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
        let load = || {
            let (active, idle) = CPU_LOAD.report();
            let load = active * 100 / (active + idle);
            debug!("Load average: {load}%");
            CPU_LOAD.reset();
            true
        };
        CPU_LOAD.reset();
        TIMER.schedule(10000, load);
        SCHED.spawn(ticker());
    }
    IRQ.dispatch()
}

/// Actions to perform in an infinite loop.
#[cfg(not(test))]
async fn ticker() -> !
{
    let fov = Angle::from(FRAC_PI_2);
    let cam = Transform::default();
    let square = Square::new();
    let pos = Vector::from([0.0, 0.0, -4.0, 1.0]);
    let rot = Quaternion::default();
    let scale = 3.0;
    let sqmdl = Transform::from_components(pos, rot, scale);
    let tri = Triangle::new();
    let pos = Vector::from([0.0, 0.0, -2.0, 1.0]);
    let mut rot = Quaternion::default();
    let scale = 1.0;
    let mut recog = Recognizer::new();
    loop {
        recog.sample();
        let vec0 = Vector::from([0.0, 0.0, 1.0, 0.0]);
        let vec1 = recog.translated();
        let axis = vec0.cross_dot(vec0 + vec1);
        let angle = Angle::from(vec1.length());
        rot *= Quaternion::from_axis_angle(axis, angle);
        rot *= recog.rotated();
        let mdl = Transform::from_components(pos, rot, scale);
        VIDEO.draw_triangles(tri.geom(), mdl, cam, fov);
        VIDEO.draw_triangles(square.geom(), sqmdl, cam, fov);
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
    IRQ.notify_others(HALT_IRQ);
    halt();
}

/// Converts the specified virtual address to a physical address from the
/// perspective of the DMA controller.
///
/// * `addr`: Address to convert.
///
/// Returns the converted address.
///
/// Panics if the requested address is not accessible by the DMA controller.
#[cfg(not(test))]
#[track_caller]
fn to_dma(addr: usize) -> usize
{
    if UNCACHED_RANGE.contains(&addr) {
        return addr - UNCACHED_RANGE.start + DMA_UNCACHED_RANGE.start;
    }
    if CACHED_RANGE.contains(&addr) {
        return addr - CACHED_RANGE.start + DMA_CACHED_RANGE.start;
    }
    if PERRY_RANGE.contains(&addr) {
        return addr - PERRY_RANGE.start + DMA_PERRY_RANGE.start;
    }
    for cpu in 0 .. CPU_COUNT {
        if STACK_RANGES[cpu].contains(&addr) {
            return addr - STACK_RANGES[cpu].start + DMA_STACK_RANGES[cpu].start;
        }
    }
    panic!("Requested address is either not mapped or not accessible by the DMA controller: 0x{addr:X}");
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
