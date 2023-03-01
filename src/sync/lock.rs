//! Locking primitives.

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use super::Advisor;

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

impl<'a, T: ?Sized> Guard<'a, T>
{
    /// Creates and initializes a new guard.
    ///
    /// * `lock`: Lock to be released when this guard is dropped.
    ///
    /// Returns the newly created guard.
    ///
    /// Panics if a deadlock condition is detected.
    #[track_caller]
    fn new(lock: &'a Lock<T>) -> Self
    {
        lock.advisor.lock();
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
        self.lock.advisor.unlock();
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

    /// Locks access to the content, blocking execution if another logical CPU
    /// is already accessing it.
    ///
    /// Returns a [`Guard`] which allows access to the content and holds the
    /// lock until dropped.
    ///
    /// Panics if a deadlock condition is detected.
    #[track_caller]
    pub fn lock(&self) -> Guard<T>
    {
        Guard::new(self)
    }
}

unsafe impl<T: ?Sized + Send> Send for Lock<T> {}

unsafe impl<T: ?Sized + Send> Sync for Lock<T> {}
