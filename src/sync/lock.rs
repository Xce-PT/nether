//! Locking primitives.

use core::cell::UnsafeCell;
#[cfg(not(test))]
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
#[cfg(test)]
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(test))]
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(test))]
use crate::{cpu_id, CPU_COUNT};

/// Lock guard whose lifetime determines how long the lock is held.
#[derive(Debug)]
pub struct Guard<'a, T: ?Sized>
{
    /// Lock to be released once this guard is dropped.
    lock: &'a Lock<T>,
    /// Zero-sized field to remove the Send trait.
    _data: PhantomData<*mut ()>,
}

/// Lock container.
#[derive(Debug)]
pub struct Lock<T: ?Sized>
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
}

/// Dummy lock advisor implementation for tests.
#[cfg(test)]
#[derive(Debug)]
pub struct Advisor
{
    is_locked: AtomicBool,
}

impl<'a, T: ?Sized> Guard<'a, T>
{
    /// Creates and initializes a new guard.
    ///
    /// * `lock`: Lock to be released when this guard is dropped.
    ///
    /// Returns the newly created guard.
    fn new(lock: &'a Lock<T>) -> Self
    {
        unsafe { lock.advisor.lock() };
        Self { lock,
               _data: PhantomData }
    }
}

impl<'a, T: ?Sized> Deref for Guard<'a, T>
{
    type Target = T;

    fn deref(&self) -> &'a Self::Target
    {
        unsafe { &*self.lock.content.get() }
    }
}

impl<'a, T: ?Sized> DerefMut for Guard<'a, T>
{
    fn deref_mut(&mut self) -> &'a mut Self::Target
    {
        unsafe { &mut *self.lock.content.get() }
    }
}

impl<'a, T: ?Sized> Drop for Guard<'a, T>
{
    fn drop(&mut self)
    {
        unsafe { self.lock.advisor.unlock() };
    }
}

impl<T: ?Sized> Lock<T>
{
    /// Creates and initializes a new lock.
    ///
    /// `content`: Content to protect.
    ///
    /// Returns the newly created lock.
    pub const fn new(content: T) -> Self
        where T: Sized
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
        Self { affinity: AtomicUsize::new(CPU_COUNT) }
    }

    /// Places a hold on the lock, blocking the logical CPU if another logical
    /// CPU is already holding it.
    ///
    /// The caller must ensure that this is called before a critical section.
    pub unsafe fn lock(&self)
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
    pub unsafe fn unlock(&self)
    {
        let affinity = cpu_id();
        assert!(affinity == self.affinity.load(Ordering::Relaxed),
                "Core #{affinity} attempted to relinquish a lock that it doesn't hold");
        self.affinity.store(CPU_COUNT, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl Advisor
{
    pub const fn new() -> Self
    {
        Self { is_locked: AtomicBool::new(false) }
    }

    pub unsafe fn lock(&self)
    {
        assert!(!self.is_locked.load(Ordering::Relaxed), "Potential deadlock detected");
        self.is_locked.store(true, Ordering::Relaxed);
    }

    pub unsafe fn unlock(&self)
    {
        assert!(self.is_locked.load(Ordering::Relaxed),
                "Attempted to relinquish the hold on a lock that is not held");
        self.is_locked.store(false, Ordering::Relaxed);
    }
}

unsafe impl<T: ?Sized> Send for Lock<T> {}

unsafe impl<T: ?Sized> Sync for Lock<T> {}
