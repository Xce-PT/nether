//! Driver for the official touchscreen.
//!
//! There is no official documentation for this driver, so its implementation is my interpretation of the implementation in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/input/touchscreen/raspberrypi-ts.c).

extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;
use core::simd::f32x4;
use core::sync::atomic::{fence, Ordering};

use crate::alloc::{Alloc, UNCACHED_REGION};
use crate::math::{Angle, Quaternion};
use crate::pixvalve::PIXVALVE;
use crate::simd::*;
use crate::sync::{Lazy, Lock, RwLock};
use crate::{mbox, to_dma};

/// Maximum number of touch points tracked by the video core.
const MAX_POINTS: usize = 10;
/// Invalid points length used by the VC as a poor man's lock.
const INVALID_POINTS: u8 = 99;
/// Touch sensor's width.
const WIDTH: usize = 800;
/// Touch sensor's height.
const HEIGHT: usize = 480;
/// Set touch buffer property tag.
const SET_TOUCHBUF_TAG: u32 = 0x4801F;

/// Global touchscreen driver instance.
pub static TOUCH: Lazy<Touch> = Lazy::new(Touch::new);

/// Uncached memory allocator instance.
static UNCACHED: Alloc<0x10> = Alloc::with_region(&UNCACHED_REGION);

/// Touchscreen driver.
#[derive(Debug)]
pub struct Touch
{
    /// Touchscreen buffer.
    state: Lock<Box<State, Alloc<'static, 0x10>>>,
    /// Saved touch points for comparison.
    saved: RwLock<[Option<f32x4>; 2]>,
}

/// Input changes since the last poll.
#[derive(Clone, Copy, Debug)]
pub struct Recognizer
{
    /// Last saved sample.
    saved: [Option<f32x4>; 2],
    /// Amount moved since the last poll.
    pub trans: f32x4,
    /// Amount rotated since the last poll.
    pub rot: Quaternion,
    /// First finger's position.
    pos0: Option<f32x4>,
    /// Second finger's position.
    pos1: Option<f32x4>,
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
        state.points_len = INVALID_POINTS;
        let state = Box::new_in(state, UNCACHED);
        let addr_in = to_dma(state.as_ref() as *const State as usize) as u32;
        mbox! {SET_TOUCHBUF_TAG: addr_in => _};
        let saved = Default::default();
        PIXVALVE.register_vsync(Self::poll);
        Self { state: Lock::new(state),
               saved: RwLock::new(saved) }
    }

    /// Handler that polls the touchscreen buffer and updates the saved state
    /// when new information is available.
    fn poll()
    {
        fence(Ordering::Acquire);
        let mut hw_state = TOUCH.state.lock();
        let state = **hw_state;
        if state.points_len as usize > MAX_POINTS {
            return;
        }
        hw_state.points_len = INVALID_POINTS;
        fence(Ordering::Release);
        // We're only interested in information containing at most two touch points.
        if !(1 ..= 2).contains(&state.points_len) {
            *TOUCH.saved.wlock() = Default::default();
            return;
        }
        let mapper = |point: &Point| {
            let x = point.x_lsb as usize | (point.x_msb as usize & 0x3) << 8;
            let y = point.y_lsb as usize | (point.y_msb as usize & 0x3) << 8;
            let y = HEIGHT - y;
            f32x4::from_array([x as f32 + 0.5, y as f32 + 0.5, 0.0, 0.0])
        };
        let mut iter = state.points[.. state.points_len as usize].iter().map(mapper).fuse();
        let new = [iter.next(), iter.next()];
        *TOUCH.saved.wlock() = new;
    }
}

impl Recognizer
{
    /// Sensor height.
    pub const HEIGHT: f32 = HEIGHT as _;
    /// Sensor width.
    pub const WIDTH: f32 = WIDTH as _;

    /// Creates and initializes a new gesture recognizer.
    ///
    /// Returns the newly created recognizer.
    pub fn new() -> Self
    {
        Self { saved: [None, None],
               trans: f32x4::from_array([0.0; 4]),
               rot: Quaternion::default(),
               pos0: None,
               pos1: None }
    }

    /// Returns the amount translated since the last sample.
    pub fn translation_delta(&self) -> f32x4
    {
        self.trans
    }

    /// Returns the amount rotated since last sampled.
    pub fn rotation_delta(&self) -> Quaternion
    {
        self.rot
    }

    /// Returns the position of the first touch point.
    pub fn first_position(&self) -> Option<f32x4>
    {
        self.pos0
    }

    /// Returns the position of the second touch point.
    pub fn second_position(&self) -> Option<f32x4>
    {
        self.pos1
    }

    /// Samples the touch sensor and computes the deltas since the last sample.
    pub fn sample(&mut self)
    {
        let new = *TOUCH.saved.rlock();
        let old = self.saved;
        self.saved = new;
        self.pos0 = new[0];
        self.pos1 = new[1];
        match (old[0], old[1], new[0], new[1]) {
            (Some(old0), Some(old1), Some(new0), Some(new1)) => self.compute_rotation(old0, old1, new0, new1),
            (Some(old), None, Some(new), None) => self.compute_translation(old, new),
            _ => {
                self.rot = Quaternion::default();
                self.trans = f32x4::from_array([0.0; 4]);
            }
        }
    }

    /// Computes the translation given by the single-finger pan gesture.
    ///
    /// * `old`: Old sample.
    /// * `new`: New sample.
    fn compute_translation(&mut self, old: f32x4, new: f32x4)
    {
        self.trans = new - old;
    }

    /// Computes the rotation from a two-finger gesture.
    ///
    /// * `old0`: First old sample.
    /// * `old1`: Second old sample.
    /// * `new0`: First new sample.
    /// * `new1`: Second new sample.
    fn compute_rotation(&mut self, old0: f32x4, old1: f32x4, new0: f32x4, new1: f32x4)
    {
        // Make sure that the points are in the same order as in the last poll by
        // verifying which are closest to which.
        let sqdist0 = (old0 - new0).sq_len();
        let sqdist1 = (old0 - new1).sq_len();
        let (new0, new1) = if sqdist0 <= sqdist1 { (new0, new1) } else { (new1, new0) };
        // Compute the rotation by calculating the angle between the vectors created by
        // the difference between the two contacts in each sample.
        let old = old1 - old0;
        let new = new1 - new0;
        let (Some(old), Some(new)) = (old.normalize(), new.normalize()) else {
            self.rot = Quaternion::default();
            return;
        };
        let axis = old.cross_dot(new);
        let angle = Angle::from_cos(axis[3]);
        self.rot = Quaternion::from_axis_angle(axis, angle);
    }
}
