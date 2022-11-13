//! One-shot async channel.

extern crate alloc;

use alloc::sync::{Arc, Weak};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::sync::Lock;

/// Sender end.
#[derive(Debug)]
pub struct Sender<T: Copy + Send>
{
    /// Channel state.
    state: Weak<Lock<State<T>>>,
}

/// Receiver end.
#[derive(Debug)]
pub struct Receiver<T: Copy + Send>
{
    /// Channel state.
    state: Arc<Lock<State<T>>>,
}

/// Channel state.
#[derive(Debug)]
struct State<T: Copy + Send>
{
    /// Value to deliver.
    val: Option<T>,
    /// Task waker.
    waker: Option<Waker>,
}

impl<T: Copy + Send> Sender<T>
{
    /// Creates and initializes a new sender.
    ///
    /// * `state`: Shared state between the sender and receiver.
    ///
    /// Returns the newly created sender.
    fn new(state: Weak<Lock<State<T>>>) -> Self
    {
        Self { state }
    }

    /// Consumes `self` and sends a value to the receiver.
    ///
    /// * `val`: Value to be sent.
    pub fn send(self, val: T)
    {
        let state = if let Some(state) = self.state.upgrade() {
            state
        } else {
            return;
        };
        let mut state = state.lock();
        state.val = Some(val);
        let waker = if let Some(waker) = state.waker.take() {
            waker
        } else {
            return;
        };
        waker.wake();
    }
}

impl<T: Copy + Send> Receiver<T>
{
    /// Creates and initializes a new receiver.
    ///
    /// * `state`: Shared state between sender and receiver.
    ///
    /// Returns the newly created receiver.
    fn new(state: Arc<Lock<State<T>>>) -> Self
    {
        Self { state }
    }
}

impl<T: Copy + Send> Future for Receiver<T>
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output>
    {
        let mut state = self.state.lock();
        if let Some(val) = state.val {
            return Poll::Ready(val);
        }
        state.waker = Some(ctx.waker().clone());
        Poll::Pending
    }
}

/// Creates a new one-shot channel.
///
/// Returns the sender and receiver ends of the newly created channel.
pub fn channel<T: Copy + Send>() -> (Sender<T>, Receiver<T>)
{
    let state = State { val: None, waker: None };
    let state = Arc::new(Lock::new(state));
    let tx = Sender::new(Arc::downgrade(&state));
    let rx = Receiver::new(state);
    (tx, rx)
}
