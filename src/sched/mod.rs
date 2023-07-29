//! Cooperative task scheduler.

extern crate alloc;

mod chan;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::task::Wake;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    scheduled: Lock<VecDeque<Arc<dyn Task>>>,
    /// All running tasks.
    running: Lock<BTreeMap<u64, Arc<dyn Task>>>,
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

/// Future that returns pending on the first poll and ready on subsequent polls.
#[derive(Debug)]
pub struct Relent
{
    /// Whether this future has been polled.
    is_ready: bool,
}

/// Task state.
#[derive(Debug)]
struct State<T: Copy + Send, F: Future<Output = T> + Send + 'static>
{
    /// Task identifier.
    id: u64,
    /// Whether the task is active.
    is_active: AtomicBool,
    /// Future polled by this task.
    fut: Lock<Pin<Box<F>>>,
    /// Join handler notification channel sender end.
    tx: Lock<Option<Sender<T>>>,
}

/// Task waker.
#[derive(Debug)]
struct Alarm
{
    /// Task identifier.
    id: u64,
}

/// Type-erased task state.
trait Task: Send + Sync
{
    /// Returns the task's unique identifier.
    fn id(&self) -> u64;

    /// Sets the task to active and returns its previous status.
    fn activate(&self) -> bool;

    /// Resumes executing the task, notifying its join handler on completion.
    ///
    /// Returns whether the task has finished.
    fn resume(&self) -> bool;
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
        let state = Arc::new(state);
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

    /// Returns a future that, when awaited on, yields execution to the other
    /// tasks in the active queue once.
    pub fn relent() -> Relent
    {
        Relent::new()
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
        if !task.activate() {
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

impl Relent
{
    /// Creates and initializes a new relent future.
    ///
    /// Returns the newly created future.
    pub fn new() -> Self
    {
        Self { is_ready: false }
    }
}

impl Future for Relent
{
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<()>
    {
        if self.is_ready {
            return Poll::Ready(());
        }
        self.as_mut().is_ready = true;
        ctx.waker().wake_by_ref();
        Poll::Pending
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
               is_active: AtomicBool::new(true),
               fut: Lock::new(Box::pin(fut)),
               tx: Lock::new(Some(tx)) }
    }
}

impl<T: Copy + Send, F: Future<Output = T> + Send + 'static> Task for State<T, F>
{
    fn id(&self) -> u64
    {
        self.id
    }

    fn activate(&self) -> bool
    {
        self.is_active.swap(true, Ordering::SeqCst)
    }

    fn resume(&self) -> bool
    {
        let alarm = Arc::new(Alarm::new(self.id));
        let waker = Waker::from(alarm);
        let mut ctx = Context::from_waker(&waker);
        self.is_active.swap(false, Ordering::SeqCst);
        if let Poll::Ready(val) = self.fut.lock().as_mut().poll(&mut ctx) {
            self.tx
                .lock()
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
