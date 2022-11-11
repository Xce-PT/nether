//! Synchronization primitives.

#[cfg(not(test))]
mod lazy;
mod lock;

#[cfg(not(test))]
pub use self::lazy::Lazy;
pub use self::lock::{Guard as LockGuard, Lock};
