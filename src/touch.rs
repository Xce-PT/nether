//! Driver for the official touchscreen.
//!
//! There is no official documentation for this driver, so its implementation is my interpretation of the implementation in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/input/touchscreen/raspberrypi-ts.c).

extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;
use core::simd::u32x2;
use core::sync::atomic::{fence, Ordering};

use crate::alloc::{Engine as AllocatorEngine, DMA};
use crate::mbox::{Request, RequestProperty, MBOX};
use crate::sync::{Lazy, Lock};

/// Maximum number of touch points tracked by the video core.
const MAX_POINTS: usize = 10;
/// Invalid dummy value used to verify whether the buffer has been updated since
/// last read.
const INVALID_POINTS: u8 = 99;

/// Global touchscreen driver instance.
pub static TOUCH: Lazy<Lock<Touch>> = Lazy::new(Touch::new);

/// Touchscreen driver.
#[derive(Debug)]
pub struct Touch
{
    /// Touchscreen buffer.
    state: Box<State, AllocatorEngine<'static>>,
    /// Cached touch point information.
    cache: Cache,
}

/// Touch point information.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Cache
{
    /// Touch point count.
    len: usize,
    /// List of touch points.
    points: [u32x2; MAX_POINTS],
}

/// Registers mapped in the touchscreen buffer.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct State
{
    /// Not sure about the purpose of this field.
    _mode: u8,
    /// Not sure about the purpose of this field.
    _gesture: u8,
    /// Number of touch points in the buffer.
    points_len: u8,
    /// Information about individual touch points.
    points: [Point; MAX_POINTS],
}

/// Information about an individual point.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Point
{
    /// Most significant byte of the horizontal coordinate.
    x_msb: u8,
    /// Least significant byte of the horizontal coordinate.
    x_lsb: u8,
    /// Most significant byte of the vertical coordinate.
    y_msb: u8,
    /// Least significant byte of the vertical coordinate.
    y_lsb: u8,
    /// Touch force (unused).
    _force: u8,
    /// Touch area (unused).
    _area: u8,
}

impl Touch
{
    /// Creates and initializes a new touchscreen driver.
    ///
    /// Returns the initialized touchscreen driver.
    fn new() -> Lock<Self>
    {
        #[allow(invalid_value)] // Filled by the hardware.
        #[allow(clippy::uninit_assumed_init)] // Same as above.
        let mut state = unsafe { MaybeUninit::<State>::uninit().assume_init() };
        state.points_len = INVALID_POINTS;
        let mut state = Box::new_in(state, DMA);
        let mut req = Request::new();
        req.push(RequestProperty::SetTouchBuffer { buf: state.as_mut() as *mut State as _ });
        MBOX.exchange(req);
        let this = Self { state,
                          cache: Cache::new() };
        Lock::new(this)
    }

    /// Polls the touchscreen buffer looking for new touch point information,
    /// filling the cache if new data is found.
    ///
    /// Returns the cached information.
    pub fn poll(&mut self) -> Cache
    {
        fence(Ordering::Acquire);
        let state = *self.state;
        if state.points_len == INVALID_POINTS {
            return self.cache;
        }
        self.state.points_len = INVALID_POINTS;
        fence(Ordering::Release);
        if state.points_len == 0 {
            self.cache.len = 0;
            return self.cache;
        }
        for idx in 0 .. state.points_len as usize {
            let x = state.points[idx].x_lsb as i32 | (state.points[idx].x_msb as i32 & 0x3) << 8;
            let y = state.points[idx].y_lsb as i32 | (state.points[idx].y_msb as i32 & 0x3) << 8;
            self.cache.points[idx] = u32x2::from_array([x as _, y as _]);
        }
        self.cache.len = state.points_len as _;
        self.cache
    }
}

impl Cache
{
    /// Creates and initializes a new cache container.
    ///
    /// Returns the created cache container.
    fn new() -> Self
    {
        Self { len: 0,
               points: [u32x2::from_array([0 as _, 0 as _]); MAX_POINTS] }
    }

    pub fn as_slice(&self) -> &[u32x2]
    {
        &self.points[0 .. self.len]
    }
}
