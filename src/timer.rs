//! Timer scheduler.
//!
//! Provides timer scheduling functionality piggyhopped on the VSync interrupt
//! handled by the pixel valve driver since that's the main ticker used by this
//! project.  This is a best effort implementation that will try to respect the
//! periodicity of scheduled timers as much as possible, but might delay or even
//! skip handler calls depending on system load.

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::Reverse;

use crate::clock::now;
use crate::pixvalve::PIXVALVE;
use crate::sync::{Lazy, Lock};

/// Global timer scheduler instance.
pub static TIMER: Lazy<Timer> = Lazy::new(Timer::new);

/// Timer scheduler.
pub struct Timer
{
    /// Timers waiting to be scheduled.
    new_timers: Lock<Vec<Event>>,
    /// Scheduled timers.
    timers: Lock<Vec<Event>>,
}

/// Timer event.
struct Event
{
    /// Event deadline.
    deadline: u64,
    /// Recurring period.
    period: u64,
    /// Event handler.
    handler: fn() -> bool,
}

impl Timer
{
    /// Creates and initializes a new timer scheduler.
    ///
    /// Returns the newly  created scheduler.
    fn new() -> Self
    {
        PIXVALVE.register_vsync(Self::tick);
        Self { new_timers: Lock::new(Vec::new()),
               timers: Lock::new(Vec::new()) }
    }

    /// Registers a handler to be called after a time interval.
    ///
    /// * `interval`: Minimum time interval in milliseconds between the
    ///   registration and the first handler call.
    /// * `handler`: Handler to be called after the timer expires.  Returns a
    ///   boolean indicating whether the timer should be rescheduled.
    pub fn schedule(&self, interval: u64, handler: fn() -> bool)
    {
        let now = now();
        let deadline = now + interval;
        let event = Event { deadline,
                            period: interval,
                            handler };
        self.new_timers.lock().push(event);
    }

    /// Tick handler.
    fn tick()
    {
        let now = now();
        // Required to prevent deadlocks if a handler attempts to schedule a new timer.
        let mut new_timers = TIMER.new_timers.lock();
        let needs_sorting = !new_timers.is_empty();
        let mut timers = TIMER.timers.lock();
        timers.append(&mut *new_timers);
        drop(new_timers);
        if needs_sorting {
            timers.sort_unstable_by_key(|event| Reverse(event.deadline));
        }
        drop(timers);
        // Call the handlers of all the expired timers.
        loop {
            let mut timers = TIMER.timers.lock();
            if timers.last().map(|event| event.deadline > now).unwrap_or(true) {
                return;
            }
            let event = timers.pop().unwrap();
            drop(timers);
            let should_resched = (event.handler)();
            if should_resched {
                let deadline = now - now % event.period + event.deadline % event.period + event.period;
                let event = Event { deadline,
                                    period: event.period,
                                    handler: event.handler };
                TIMER.new_timers.lock().push(event);
            }
        }
    }
}
