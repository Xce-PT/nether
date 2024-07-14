//! Synchronization primitives.

mod advisor;
mod lazy;
mod lock;
mod rwlock;

use self::advisor::Advisor;
pub use self::lazy::Lazy;
pub use self::lock::Lock;
pub use self::rwlock::RwLock;
