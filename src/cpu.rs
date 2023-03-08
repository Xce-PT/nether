//! CPU-related utilities.
//!
//! Contains several utilities to control CPU features used throughout the
//! project.

use core::arch::asm;
use core::cmp::min;
use core::mem::size_of_val;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::clock::now;
use crate::sync::Lock;

/// Number of logical CPUs in the system.
pub const COUNT: usize = 4;

/// Size of a cache line.
const CACHELINE_SIZE: usize = 64;

/// Global load monitor instance.
pub static LOAD: Load = Load::new();

/// Load monitor.
#[derive(Debug)]
pub struct Load
{
    vals: Lock<LoadValues>,
}

/// Load values.
#[derive(Debug)]
struct LoadValues
{
    /// Last reset time.
    ref_time: u64,
    /// Total idle time since last reset.
    idle_time: u64,
}

impl Load
{
    /// Creates and initializes a new load monitor.
    ///
    /// Returns the newly created load monitor.
    const fn new() -> Self
    {
        let vals = LoadValues { ref_time: 0,
                                idle_time: 0 };
        Self { vals: Lock::new(vals) }
    }

    /// Registers the duration of a logical CPU's last idle period, ignoring any
    /// idle time before the last reset.
    fn idle_since(&self, time: u64)
    {
        let mut vals = self.vals.lock();
        let now = now();
        let duration = min(now - time, now - vals.ref_time);
        vals.idle_time += duration;
    }

    /// Returns the amount of active and idle time of all logical CPUs.
    pub fn report(&self) -> (u64, u64)
    {
        let vals = self.vals.lock();
        let now = now();
        let duration = (now - vals.ref_time) * COUNT as u64;
        let active = duration - vals.idle_time;
        let idle = vals.idle_time;
        (active, idle)
    }

    /// Resets the monitor.
    pub fn reset(&self)
    {
        let mut vals = self.vals.lock();
        vals.ref_time = now();
        vals.idle_time = 0;
    }
}

/// Hints the calling CPU to idle in a low power state until an IRQ is
/// delivered.
pub fn sleep()
{
    let start = now();
    unsafe {
        asm!("msr daifclr, #0x3", "wfi", options(nomem, nostack, preserves_flags));
    }
    LOAD.idle_since(start);
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
