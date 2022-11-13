//! Lazy initialization for static objects.
//!
//! [`Lazy`] enables the unexplicit lazy initialization of static objects by
//! delaying calls to their initializers until they are accessed for the first
//! time, which is useful to deal with non-const initializers as well as to
//! avoid explicit initializations which are error prone.

use core::cell::UnsafeCell;
use core::ops::Deref;

use super::lock::Advisor as LockAdvisor;

/// Lazy initializer for static values.
pub struct Lazy<T: Send + Sync + 'static>
{
    /// Lock advisor to prevent simultaneous initialization.
    advisor: LockAdvisor,
    /// Initialization function.
    init: fn() -> T,
    /// Actual object to be lazily initialized.
    content: UnsafeCell<Option<T>>,
}

impl<T: Send + Sync + 'static> Lazy<T>
{
    /// Creates and initializes a lazy initializer.
    ///
    /// `init`: Initialization function to be called at first access.
    ///
    /// Returns the newly created lazy initializer.
    pub const fn new(init: fn() -> T) -> Self
    {
        Self { advisor: LockAdvisor::new(),
               init,
               content: UnsafeCell::new(None) }
    }
}

impl<T: Send + Sync + 'static> Deref for Lazy<T>
{
    type Target = T;

    fn deref(&self) -> &T
    {
        unsafe { self.advisor.lock() };
        let content = unsafe { (*self.content.get()).get_or_insert_with(self.init) };
        unsafe { self.advisor.unlock() };
        content
    }
}

unsafe impl<T: Send + Sync + 'static> Send for Lazy<T> {}

unsafe impl<T: Send + Sync + 'static> Sync for Lazy<T> {}
