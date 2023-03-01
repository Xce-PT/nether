//! Read-write locking primitives.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

use super::Advisor;

/// Read grant on the lock.
#[derive(Debug)]
pub struct ReadGuard<'a, T: Send + Sync + ?Sized>
{
    /// Lock to which this guard grants shared access to.
    lock: &'a RwLock<T>,
    /// Zero-sized field to remove the Send trait.
    _data: PhantomData<*mut ()>,
}

/// Write grant on the lock.
#[derive(Debug)]
pub struct WriteGuard<'a, T: ?Sized>
{
    /// Lock which this guard grants exclusive access to.
    lock: &'a RwLock<T>,
    /// Zero-sized field to remove the Send trait.
    _data: PhantomData<*mut ()>,
}

/// Read-write lock.
#[derive(Debug)]
pub struct RwLock<T: ?Sized>
{
    /// Spin-lock.
    advisor: Advisor,
    /// Reader count.
    share_count: AtomicUsize,
    /// Protected content.
    content: UnsafeCell<T>,
}

impl<'a, T: Send + Sync + ?Sized> ReadGuard<'a, T>
{
    /// Creates and initializes a new read guard.
    ///
    /// * `lock`: Lock to grant shared access to.
    ///
    /// Returns the newly created guard.
    fn new(lock: &'a RwLock<T>) -> Self
    {
        lock.advisor.lock();
        lock.share_count.fetch_add(1, Ordering::Relaxed);
        lock.advisor.unlock();
        Self { lock,
               _data: PhantomData }
    }
}

impl<'a, T: Send + Sync + ?Sized> Deref for ReadGuard<'a, T>
{
    type Target = T;

    fn deref(&self) -> &'a Self::Target
    {
        unsafe { &*self.lock.content.get() }
    }
}

impl<'a, T: Send + Sync + ?Sized> Drop for ReadGuard<'a, T>
{
    fn drop(&mut self)
    {
        self.lock.share_count.fetch_sub(1, Ordering::SeqCst);
    }
}

impl<'a, T: ?Sized> WriteGuard<'a, T>
{
    /// Creates and initializes a new write guard.
    ///
    /// * `lock`: Lock to grant exclusive access to.
    ///
    /// Returns the newly created guard.
    ///
    /// Panics if a deadlock condition is detected.
    #[track_caller]
    fn new(lock: &'a RwLock<T>) -> Self
    {
        while lock.share_count.load(Ordering::Relaxed) != 0 {
            spin_loop();
        }
        lock.advisor.lock();
        Self { lock,
               _data: PhantomData }
    }
}

impl<'a, T: ?Sized> Deref for WriteGuard<'a, T>
{
    type Target = T;

    fn deref(&self) -> &'a Self::Target
    {
        unsafe { &*self.lock.content.get() }
    }
}

impl<'a, T: ?Sized> DerefMut for WriteGuard<'a, T>
{
    fn deref_mut(&mut self) -> &'a mut Self::Target
    {
        unsafe { &mut *self.lock.content.get() }
    }
}

impl<'a, T: ?Sized> Drop for WriteGuard<'a, T>
{
    fn drop(&mut self)
    {
        self.lock.advisor.unlock();
    }
}

impl<T: ?Sized> RwLock<T>
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
               share_count: AtomicUsize::new(0),
               content: UnsafeCell::new(content) }
    }

    /// Non-exclusively locks access to the content, blocking execution if
    /// another logical CPU is already exclusively accessing it.
    ///
    /// Returns a [`ReadGuard`] which allows shared immutable access to the
    /// content and holds the lock until dropped.
    pub fn rlock(&self) -> ReadGuard<T>
        where T: Send + Sync
    {
        ReadGuard::new(self)
    }

    /// Exclusively locks access to the content, blocking execution if another
    /// logical CPU is already accessing it.
    ///
    /// Returns a [`WriteGuard`] which allows exclusive mutable access to the
    /// content and holds the lock until dropped.
    ///
    /// Panics if a deadlock condition is detected.
    #[track_caller]
    pub fn wlock(&self) -> WriteGuard<T>
    {
        WriteGuard::new(self)
    }
}

unsafe impl<T: Send + ?Sized> Send for RwLock<T> {}

unsafe impl<T: Send + ?Sized> Sync for RwLock<T> {}
