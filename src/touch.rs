//! Driver for the official touchscreen.
//!
//! There is no official documentation for this driver, so its implementation is my interpretation of the implementation in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/input/touchscreen/raspberrypi-ts.c).

extern crate alloc;

use alloc::boxed::Box;
use core::mem::MaybeUninit;
use core::sync::atomic::{fence, Ordering};

use crate::alloc::{Engine as AllocatorEngine, DMA};
use crate::mbox::{Mailbox, Message, MBOX};
use crate::sync::{Lazy, Lock};

/// Tag to tell the video core about the location of the touchscreen buffer.
const TOUCHBUF_TAG: u32 = 0x4801F;
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
    regs: Box<MaybeUninit<Registers>, AllocatorEngine<'static>>,
    /// Cached touch point information.
    info: [Info; MAX_POINTS],
    /// Length of the cache.
    info_len: usize,
}

/// Touch point information.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Info
{
    /// Horizontal coordinate.
    pub x: i32,
    /// Vertical coordinate.
    pub y: i32,
}

/// Registers mapped in the touchscreen buffer.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Registers
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
        let mut regs = Box::<Registers, AllocatorEngine>::new_uninit_in(DMA);
        unsafe { MaybeUninit::assume_init(*regs).points_len = INVALID_POINTS };
        let mut msg = Message::new().unwrap();
        let data = unsafe { Mailbox::map_to_vc(&mut *regs) };
        msg.add_tag(TOUCHBUF_TAG, data).unwrap();
        MBOX.exchange(msg).unwrap();
        let this = Self { regs,
                          info: [Info { x: 0, y: 0 }; MAX_POINTS],
                          info_len: 0 };
        Lock::new(this)
    }

    /// Polls the touchscreen buffer looking for new touch point information.
    ///
    /// Returns either new or cached touch information depending on
    /// availability.
    pub fn poll(&mut self) -> &[Info]
    {
        fence(Ordering::Acquire);
        let regs = unsafe { (*self.regs).assume_init() };
        if regs.points_len == 0 {
            self.info_len = 0;
            return &self.info[0 .. 0];
        }
        if regs.points_len == INVALID_POINTS {
            return &self.info[0 .. self.info_len];
        }
        for idx in 0 .. regs.points_len as usize {
            let x = regs.points[idx].x_lsb as i32 | (regs.points[idx].x_msb as i32 & 0x3) << 8;
            let y = regs.points[idx].y_lsb as i32 | (regs.points[idx].y_msb as i32 & 0x3) << 8;
            self.info[idx] = Info { x, y };
        }
        self.info_len = regs.points_len as _;
        unsafe { MaybeUninit::assume_init(*self.regs).points_len = INVALID_POINTS };
        fence(Ordering::Release);
        &self.info[0 .. self.info_len]
    }
}
