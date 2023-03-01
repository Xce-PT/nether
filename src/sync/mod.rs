//! Synchronization primitives.

mod advisor;
mod lazy;
mod lock;
mod rwlock;

use self::advisor::Advisor;
pub use self::lazy::Lazy;
pub use self::lock::{Guard as LockGuard, Lock};
pub use self::rwlock::{ReadGuard as ReadLockGuard, RwLock, WriteGuard as WriteLockGuard};
