//! Synchronization primitives.

#[cfg(not(test))]
mod lazy;
mod lock;

#[cfg(not(test))]
pub use self::lazy::Lazy;
pub use self::lock::{Advisor as LockAdvisor, Guard as LockGuard, Lock};
