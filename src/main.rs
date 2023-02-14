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
use core::mem::size_of_val;
#[cfg(not(test))]
use core::ops::Range;
#[cfg(not(test))]
use core::panic::PanicInfo;
#[cfg(not(test))]
use core::sync::atomic::{compiler_fence, Ordering};
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
/// Logical CPU count.
#[cfg(not(test))]
const CPU_COUNT: usize = 4;
/// Size of a cache line.
#[cfg(not(test))]
const CACHELINE_SIZE: usize = 64;
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

/// Invalidates the cache associated to the specified data to point of
/// coherence, effectively purging the data object from cache without writing it
/// out to memory.  Other objects sharing the same initial or final cache lines
/// as the object being purged will have their contents restored at the end of
/// this operation.
///
/// * `data`: Data object to purge from cache.
#[cfg(not(test))]
fn invalidate_cache<T: Copy>(data: &mut T)
{
    let size = size_of_val(data);
    if size == 0 {
        return;
    }
    let start = data as *mut T as usize;
    let end = data as *mut T as usize + size;
    let algn_start = start & !(CACHELINE_SIZE - 1);
    let algn_end = (end + (CACHELINE_SIZE - 1)) & !(CACHELINE_SIZE - 1);
    // Save the first and last cache lines.
    let start_cl = unsafe { *(algn_start as *const [u8; CACHELINE_SIZE]) };
    let end_cl = unsafe { *((algn_end - CACHELINE_SIZE) as *const [u8; CACHELINE_SIZE]) };
    // Invalidate the cache.
    compiler_fence(Ordering::Release);
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
    for addr in (algn_start .. algn_end).step_by(CACHELINE_SIZE) {
        unsafe { asm!("dc ivac, {addr}", addr = in (reg) addr, options (preserves_flags)) };
    }
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
    compiler_fence(Ordering::Acquire);
    // Restore the parts of the first and last cachelines shared with this data
    // object.
    if algn_start != start {
        let count = start - algn_start;
        unsafe {
            (algn_start as *mut u8).copy_from_nonoverlapping(&start_cl[0], count);
        }
    }
    if algn_end != end {
        let count = algn_end - end;
        let idx = CACHELINE_SIZE - count;
        unsafe {
            (end as *mut u8).copy_from_nonoverlapping(&end_cl[idx], count);
        }
    }
}

/// Cleans up the cache associated to the specified data object, effectively
/// flushing its contents to main memory.
///
/// * `data`: Data object to flush.
#[cfg(not(test))]
fn cleanup_cache<T: Copy>(data: &T)
{
    let start = data as *const T as usize & !(CACHELINE_SIZE - 1);
    let end = (data as *const T as usize + size_of_val(data) + (CACHELINE_SIZE - 1)) & !(CACHELINE_SIZE - 1);
    compiler_fence(Ordering::Release);
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
    for addr in (start .. end).step_by(CACHELINE_SIZE) {
        unsafe { asm!("dc cvac, {addr}", addr = in (reg) addr, options (nomem, nostack, preserves_flags)) };
    }
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
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
