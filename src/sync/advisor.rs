//! Advisory spin-lock primitive.
//!
//! The core of all other locks, only acts as an advisor and doesn't actually
//! own any content.

use core::hint::spin_loop;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::cpu::{id as cpu_id, COUNT as CPU_COUNT};

/// Lock advisor.
#[repr(align(64))] // Take up an entire cache line.
#[derive(Debug)]
pub struct Advisor
{
    /// The Logical CPU that currently holds the lock.
    affinity: AtomicUsize,
}

#[cfg(not(test))]
impl Advisor
{
    /// Creates and initializes a new lock advisor.
    ///
    /// Returns the newly created lock advisor.
    pub const fn new() -> Self
    {
        Self { affinity: AtomicUsize::new(CPU_COUNT) }
    }

    /// Places a hold on the lock, blocking the logical CPU if another logical
    /// CPU is already holding it.
    ///
    /// Panics if a deadlock is detected.
    ///
    /// The caller must ensure that this is called before a critical section.
    #[track_caller]
    pub fn lock(&self)
    {
        let affinity = cpu_id();
        assert!(self.affinity.load(Ordering::Relaxed) != affinity,
                "Deadlock detected on core #{affinity}");
        while self.affinity
                  .compare_exchange_weak(CPU_COUNT, affinity, Ordering::SeqCst, Ordering::Relaxed)
                  .is_err()
        {
            spin_loop()
        }
    }

    /// Relinquishes the hold on a lock, unblocking another logical CPU that
    /// intends to hold it.
    ///
    /// Panics if the lock is not held by this logical CPU.
    ///
    /// The caller must make sure to call this after being done with a critical
    /// section.
    #[track_caller]
    pub fn unlock(&self)
    {
        let affinity = cpu_id();
        assert!(affinity == self.affinity.load(Ordering::Relaxed),
                "Logical CPU #{affinity} attempted to relinquish a lock that it doesn't hold");
        self.affinity.store(CPU_COUNT, Ordering::SeqCst);
    }
}
