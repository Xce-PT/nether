//! Locking primitives.
//!
//! [`Lock`] is a type-safe lock implementation that remains locked for as long
//! as its returned [`Guard`] lives.
//!
//! [`Advisor`] is a spin-lock advisor that offers no type-safe guarantees but
//! is useful to implement other types that do such as [`Lock`].
//!
//! Recursive locking is supported, so locking on the same logical CPU more than
//! once won't cause a deadlock.

#[cfg(not(test))]
use core::arch::asm;
use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};

/// Lock guard whose lifetime determines how long the lock is held.
#[derive(Debug)]
pub struct Guard<'a, T>
{
    /// Lock to be released once this guard is dropped.
    lock: &'a Lock<T>,
}

/// Lock container.
#[derive(Debug)]
pub struct Lock<T>
{
    /// Actual spin-lock.
    advisor: Advisor,
    /// Protected content.
    content: UnsafeCell<T>,
}

/// Lock advisor.
#[cfg(not(test))]
#[derive(Debug)]
#[repr(align(64))] // Take up an entire cache line.
pub struct Advisor
{
    /// The Logical CPU that currently holds the lock.
    affinity: AtomicUsize,
    /// Recursion depth.
    count: AtomicUsize,
}

/// Dummy lock advisor implementation for tests.
#[cfg(test)]
#[derive(Debug)]
pub struct Advisor;

impl<'a, T> Guard<'a, T>
{
    /// Creates and initializes a new guard.
    ///
    /// * `lock`: Lock to be released when this guard is dropped.
    ///
    /// Returns the newly created guard.
    fn new(lock: &'a Lock<T>) -> Self
    {
        Self { lock }
    }
}

impl<'a, T> Deref for Guard<'a, T>
{
    type Target = T;

    fn deref(&self) -> &'a Self::Target
    {
        unsafe { &*self.lock.content.get() }
    }
}

impl<'a, T> DerefMut for Guard<'a, T>
{
    fn deref_mut(&mut self) -> &'a mut Self::Target
    {
        unsafe { &mut *self.lock.content.get() }
    }
}

impl<'a, T> Drop for Guard<'a, T>
{
    fn drop(&mut self)
    {
        unsafe { self.lock.advisor.unlock() };
    }
}

impl<T> Lock<T>
{
    /// Creates and initializes a new lock.
    ///
    /// `content`: Content to protect.
    ///
    /// Returns the newly created lock.
    pub const fn new(content: T) -> Self
    {
        Self { advisor: Advisor::new(),
               content: UnsafeCell::new(content) }
    }

    /// Locks access to the content, blocking execution if another core is
    /// already accessing it.
    ///
    /// Returns a [`Guard`] which allows access to the content and holds the
    /// lock until dropped.
    pub fn lock(&self) -> Guard<T>
    {
        unsafe { self.advisor.lock() };
        Guard::new(self)
    }
}

#[cfg(not(test))]
impl Advisor
{
    /// Creates and initializes a new lock advisor.
    ///
    /// Returns the newly created lock advisor.
    pub const fn new() -> Self
    {
        Self { affinity: AtomicUsize::new(0x0),
               count: AtomicUsize::new(0) }
    }

    /// Places a hold on the lock, blocking the logical CPU if another logical
    /// CPU is already holding it.
    ///
    /// The caller must ensure that this is called before a critical section.
    pub unsafe fn lock(&self)
    {
        // Affinity encoding takes advantage of the fact that bit 31 of MPIDR_EL1 is
        // always set, which allows using 0x0 as a special value.
        let affinity: usize;
        asm!("mrs {aff}, mpidr_el1", aff = out (reg) affinity, options (nomem, nostack, preserves_flags));
        if self.affinity.load(Ordering::Relaxed) == affinity {
            self.count.fetch_add(1, Ordering::Relaxed);
            return;
        }
        while self.affinity
                  .compare_exchange_weak(0x0, affinity, Ordering::SeqCst, Ordering::Relaxed)
                  .is_err()
        {
            spin_loop()
        }
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Relinquishes the hold on a lock, unblocking another logical CPU that
    /// intends to hold it.
    ///
    /// Panics if the lock is not held by this core.
    ///
    /// The caller must ensure to call this after being done with a critical
    /// section.
    pub unsafe fn unlock(&self)
    {
        let affinity: usize;
        asm!("mrs {aff}, mpidr_el1", aff = out (reg) affinity, options (nomem, nostack, preserves_flags));
        assert_eq!(affinity,
                   self.affinity.load(Ordering::Relaxed),
                   "Attempted to relinquish a lock that is not held by this core");
        if self.count.fetch_sub(1, Ordering::Relaxed) <= 1 {
            return;
        }
        self.affinity.store(0x0, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Advisor
{
    const fn new() -> Self
    {
        Self
    }

    unsafe fn lock(&self) {}

    unsafe fn unlock(&self) {}
}

unsafe impl<T> Sync for Lock<T> {}
