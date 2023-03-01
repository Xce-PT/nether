//! Cooperative task scheduler.

extern crate alloc;

mod chan;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll, Waker};

use self::chan::{channel, Receiver, Sender};
use crate::irq::IRQ;
use crate::sync::{Lazy, Lock};

/// Scheduler alarm IRQ.
const SCHED_IRQ: u32 = 1;

/// Global scheduler instance.
pub static SCHED: Lazy<Scheduler> = Lazy::new(Scheduler::new);

/// Task scheduler.
pub struct Scheduler
{
    /// Tasks scheduled for polling.
    scheduled: Lock<VecDeque<Arc<Lock<dyn Task>>>>,
    /// All running tasks.
    running: Lock<BTreeMap<u64, Arc<Lock<dyn Task>>>>,
    /// Spawned task counter.
    count: AtomicU64,
}

/// Future that can be awaited on until its corresponding task terminates.
#[derive(Debug)]
pub struct JoinHandle<T: Copy + Send>
{
    /// Receiving end of the notification channel.
    rx: Receiver<T>,
}

/// Task state.
#[derive(Debug)]
struct State<T: Copy + Send, F: Future<Output = T> + Send + 'static>
{
    /// Task identifier.
    id: u64,
    /// Whether the task is active.
    is_active: bool,
    /// Future polled by this task.
    fut: Pin<Box<F>>,
    /// Join handler notification channel sender end.
    tx: Option<Sender<T>>,
}

/// Task waker.
#[derive(Debug)]
struct Alarm
{
    /// Task identifier.
    id: u64,
}

/// Type-erased task state.
trait Task: Send
{
    /// Returns the task's unique identifier.
    fn id(&self) -> u64;

    /// Sets the task to active and returns its previous status.
    fn activate(&mut self) -> bool;

    /// Resumes executing the task, notifying its join handler on completion.
    ///
    /// Returns whether the task has finished.
    fn resume(&mut self) -> bool;
}

impl Scheduler
{
    /// Creates and initializes a new scheduler.
    ///
    /// Returns the created scheduler.
    fn new() -> Self
    {
        IRQ.register(SCHED_IRQ, Self::poll);
        Self { scheduled: Lock::new(VecDeque::new()),
               running: Lock::new(BTreeMap::new()),
               count: AtomicU64::new(1) /* Zero means no task. */ }
    }

    /// Spawns a new task.
    ///
    /// * `fut`: Future to poll to completion.
    ///
    /// Returns a join handle that can be used to await for the termination of
    /// the new task and obtain the result of the future.
    pub fn spawn<T: Send + Copy + 'static>(&self, fut: impl Future<Output = T> + Send + 'static) -> JoinHandle<T>
    {
        let id = self.count.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel::<T>();
        let state = State::new(id, fut, tx);
        let state = Arc::new(Lock::new(state));
        self.running.lock().insert(id, state.clone());
        let mut scheduled = self.scheduled.lock();
        scheduled.push_back(state);
        let count = scheduled.len();
        drop(scheduled);
        if count == 1 {
            IRQ.notify_self(SCHED_IRQ);
        } else {
            IRQ.notify_all(SCHED_IRQ);
        }
        JoinHandle::new(rx)
    }

    /// Schedules a task to be polled.
    ///
    /// * `id`: Task identifier.
    fn wake(&self, id: u64)
    {
        let task = self.running
                       .lock()
                       .get(&id)
                       .expect("Attempted to wake  up a non-existing task")
                       .clone();
        if !task.lock().activate() {
            let mut scheduled = self.scheduled.lock();
            scheduled.push_back(task);
            let count = scheduled.len();
            drop(scheduled);
            if count == 1 {
                IRQ.notify_self(SCHED_IRQ);
            } else {
                IRQ.notify_all(SCHED_IRQ);
            }
        }
    }

    /// IRQ handler that polls all active tasks.
    fn poll()
    {
        let mut scheduled = SCHED.scheduled.lock();
        let task = scheduled.pop_front();
        let count = scheduled.len();
        drop(scheduled);
        if let Some(task) = task {
            let mut task = task.lock();
            let finished = task.resume();
            if finished {
                SCHED.running.lock().remove(&task.id());
            }
            match count {
                0 => (),
                1 => IRQ.notify_self(SCHED_IRQ),
                _ => IRQ.notify_all(SCHED_IRQ),
            }
        }
    }
}

impl<T: Copy + Send> JoinHandle<T>
{
    /// Creates and initializes a new join handler.
    ///
    /// * `rx`: Task termination notification channel receiver.
    ///
    /// Returns the newly created join handler.
    fn new(rx: Receiver<T>) -> Self
    {
        Self { rx }
    }
}

impl<T: Copy + Send> Future for JoinHandle<T>
{
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output>
    {
        Pin::new(&mut self.rx).poll(ctx)
    }
}

impl<T: Copy + Send, F: Future<Output = T> + Send + 'static> State<T, F>
{
    /// Creates and initializes a new task state.
    ///
    /// * `id`: Task identifier.
    /// * `fut`: Future for this task to poll.
    /// * `tx`: Join handler notification channel sender.
    ///
    /// Returns the newly created task state.
    fn new(id: u64, fut: F, tx: Sender<T>) -> Self
    {
        Self { id,
               is_active: true,
               fut: Box::pin(fut),
               tx: Some(tx) }
    }
}

impl<T: Copy + Send, F: Future<Output = T> + Send + 'static> Task for State<T, F>
{
    fn id(&self) -> u64
    {
        self.id
    }

    fn activate(&mut self) -> bool
    {
        let is_active = self.is_active;
        self.is_active = true;
        is_active
    }

    fn resume(&mut self) -> bool
    {
        let alarm = Arc::new(Alarm::new(self.id));
        let waker = Waker::from(alarm);
        let mut ctx = Context::from_waker(&waker);
        self.is_active = false;
        if let Poll::Ready(val) = self.fut.as_mut().poll(&mut ctx) {
            self.tx
                .take()
                .expect("Missing channel sender end to notify the join handle of a finished task")
                .send(val);
            return true;
        }
        false
    }
}

impl Alarm
{
    /// Creates and initializes a new alarm.
    ///
    /// Returns the newly created alarm.
    fn new(id: u64) -> Self
    {
        Self { id }
    }
}

impl Wake for Alarm
{
    fn wake(self: Arc<Self>)
    {
        SCHED.wake(self.id);
    }

    fn wake_by_ref(self: &Arc<Self>)
    {
        SCHED.wake(self.id);
    }
}
