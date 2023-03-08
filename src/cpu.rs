//! CPU-related utilities.
//!
//! Contains several utilities to control CPU features used throughout the
//! project.

use core::arch::asm;
use core::mem::size_of_val;
use core::sync::atomic::{compiler_fence, Ordering};

/// Number of logical CPUs in the system.
pub const COUNT: usize = 4;

/// Size of a cache line.
const CACHELINE_SIZE: usize = 64;

/// Hints the calling CPU to idle in a low power state until an IRQ is
/// delivered.
pub fn sleep()
{
    unsafe {
        asm!("msr daifclr, #0x3", "wfi", options(nomem, nostack, preserves_flags));
    }
}

/// Invalidates the cache associated with the specified data to point of
/// coherence, effectively purging the data object from cache without writing it
/// out to memory.  Other objects sharing the same initial or final cache lines
/// as the object being purged will have their contents restored at the end of
/// this operation.
///
/// * `data`: Data object to purge from cache.
pub fn invalidate_cache<T: Copy>(data: &mut T)
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

/// Cleans up the cache associated with the specified data object, effectively
/// flushing its contents to main memory.
///
/// * `data`: Data object to flush.
pub fn cleanup_cache<T: Copy>(data: &T)
{
    let size = size_of_val(data);
    if size == 0 {
        return;
    }
    let start = data as *const T as usize & !(CACHELINE_SIZE - 1);
    let end = (data as *const T as usize + size + (CACHELINE_SIZE - 1)) & !(CACHELINE_SIZE - 1);
    compiler_fence(Ordering::Release);
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
    for addr in (start .. end).step_by(CACHELINE_SIZE) {
        unsafe { asm!("dc cvac, {addr}", addr = in (reg) addr, options (nomem, nostack, preserves_flags)) };
    }
    unsafe { asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
}

/// Returns the ID of the calling logical CPU.
pub fn id() -> usize
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
