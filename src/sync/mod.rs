//! Synchronization primitives.

#[cfg(not(test))]
mod lazy;
mod lock;
#[cfg(not(test))]
mod rwlock;

#[cfg(not(test))]
pub use self::lazy::Lazy;
pub use self::lock::{Guard as LockGuard, Lock};
#[cfg(not(test))]
pub use self::rwlock::{ReadGuard as ReadLockGuard, RwLock, WriteGuard as WriteLockGuard};
