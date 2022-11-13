//! Driver for the official touchscreen.
//!
//! There is no official documentation for this driver, so its implementation is my interpretation of the implementation in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/input/touchscreen/raspberrypi-ts.c).

extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;
use core::simd::u32x2;
use core::sync::atomic::{fence, Ordering};

use crate::alloc::{Shell as Allocator, DMA};
use crate::mbox::{Request, RequestProperty, MBOX};
use crate::sync::Lazy;

/// Maximum number of touch points tracked by the video core.
const MAX_POINTS: usize = 10;

/// Global touchscreen driver instance.
pub static TOUCH: Lazy<Touch> = Lazy::new(Touch::new);

/// Touchscreen driver.
#[derive(Debug)]
pub struct Touch
{
    /// Touchscreen buffer.
    state: Box<State, Allocator<'static>>,
}

/// Touch point information.
#[derive(Debug)]
pub struct Info
{
    points: [u32x2; MAX_POINTS],
    len: usize,
}

/// Touchscreen state information from the video core.
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
    fn new() -> Self
    {
        #[allow(invalid_value)] // Filled by the hardware.
        #[allow(clippy::uninit_assumed_init)] // Same as above.
        let mut state = unsafe { MaybeUninit::<State>::uninit().assume_init() };
        state.points_len = 0;
        let mut state = Box::new_in(state, DMA);
        let mut req = Request::new();
        req.push(RequestProperty::SetTouchBuffer { buf: state.as_mut() as *mut State as _ });
        MBOX.exchange(req);
        Self { state }
    }

    /// Polls the touchscreen buffer looking for new touch point information.
    ///
    /// Returns whatever information is available.
    pub fn poll(&self) -> Info
    {
        fence(Ordering::Acquire);
        let state = *self.state;
        let mapper = |point: Point| {
            u32x2::from_array([point.x_lsb as u32 | (point.x_msb as u32 & 0x3) << 8,
                               point.y_lsb as u32 | (point.y_msb as u32 & 0x3) << 8])
        };
        let points = self.state.points.map(mapper);
        let len = if state.points_len <= MAX_POINTS as _ {
            state.points_len as usize
        } else {
            0
        };
        Info { points, len }
    }
}

impl Info
{
    pub fn as_slice(&self) -> &[u32x2]
    {
        &self.points[0 .. self.len]
    }
}
