//! Video core driver.
//!
//! Documentation:
//!
//! * [Mailbox property interface](https://github.com/raspberrypi/firmware/wiki/Mailbox-property-interface)
//!
//! Since there's no documented support for double-buffering, the pan property
//! tag is being used to move the display to the top of the frame buffer every
//! even frame and to the bottom of the frame buffer every odd frame.

use core::mem::align_of;
use core::ptr::null_mut;
use core::simd::{mask32x4, u32x2, u32x4, SimdPartialOrd};
use core::sync::atomic::{fence, AtomicU64, Ordering};

use crate::irq::IRQ;
use crate::mbox::{Request, RequestProperty, ResponseProperty, MBOX};
use crate::sync::{Lazy, Lock};
use crate::PERRY_RANGE;

/// Pixel valve 1 base address.
const PV1_BASE: usize = 0x2207000 + PERRY_RANGE.start;
/// PV1 interrupt enable register.
const PV1_INTEN: *mut u32 = (PV1_BASE + 0x24) as _;
/// PV1 status and acknowledgement register.
const PV1_STAT: *mut u32 = (PV1_BASE + 0x28) as _;
/// PV1 IRQ.
const PV1_IRQ: u32 = 142;
/// PV VSync interrupt enable flag.
const PV_VSYNC: u32 = 0x10;

/// Global video driver instance.
pub static VIDEO: Lazy<Video> = Lazy::new(Video::new);

/// Video driver.
#[derive(Debug)]
pub struct Video
{
    /// Frame buffer base.
    base: Lock<*mut u32x4>,
    /// Frame buffer size in bytes.
    size: usize,
    /// Display width.
    width: usize,
    /// Display height.
    height: usize,
    /// Frame counter.
    count: AtomicU64,
}

impl Video
{
    /// Creates and initializes a new video driver instance.
    ///
    /// Returns the newly created instance.
    fn new() -> Self
    {
        let mut req = Request::new();
        req.push(RequestProperty::SetPhysicalSize { width: 800,
                                                    height: 480 });
        req.push(RequestProperty::SetVirtualSize { width: 800,
                                                   height: 480 * 2 });
        req.push(RequestProperty::SetDepth { bits: 32 });
        req.push(RequestProperty::Allocate { align: align_of::<u32x4>() });
        let resp = MBOX.exchange(req);
        let mut this = Self { base: Lock::new(null_mut()),
                              size: 0,
                              width: 0,
                              height: 0,
                              count: AtomicU64::new(0) };
        for prop in resp {
            match prop {
                ResponseProperty::Allocate { base, size } => {
                    *this.base.lock() = base.cast();
                    this.size = size;
                }
                ResponseProperty::SetPhysicalSize { width, height } => {
                    this.width = width;
                    this.height = height;
                }
                _ => continue,
            }
        }
        IRQ.register(PV1_IRQ, Self::vsync);
        unsafe {
            PV1_STAT.write_volatile(PV_VSYNC);
            PV1_INTEN.write_volatile(PV_VSYNC);
        }
        this
    }

    /// Displays rings with a fixed radius and thickness centered at the
    /// specified points on the screen.
    pub fn draw_rings(&self, rings: &[u32x2])
    {
        let sqouter = u32x4::splat(50 * 50);
        let sqinner = u32x4::splat(46 * 46);
        let black = u32x4::splat(0xFF000000);
        let white = u32x4::splat(0xFFFFFFFF);
        let idxs = u32x4::from_array([0, 1, 2, 3]);
        let count = self.count.load(Ordering::Relaxed);
        let mut offset = if count & 1 == 0 {
            self.width * self.height / 4
        } else {
            0
        };
        let base = self.base.lock();
        for row in 0 .. self.height {
            let row = u32x4::splat(row as _);
            for col in (0 .. self.width).step_by(4) {
                let col = u32x4::splat(col as _) + idxs;
                let mut mask = mask32x4::splat(false);
                for ring in rings {
                    let x = u32x4::splat(ring[0]);
                    let y = u32x4::splat(ring[1]);
                    let sqdistx = x - col;
                    let sqdisty = y - row;
                    let sqdist = sqdistx * sqdistx + sqdisty * sqdisty;
                    mask |= sqdist.simd_ge(sqinner) & sqdist.simd_lt(sqouter);
                }
                let color = mask.select(white, black);
                unsafe { base.add(offset).write(color) };
                offset += 1;
            }
        }
        fence(Ordering::Release);
    }

    /// Flips the frame buffers.
    fn vsync()
    {
        let count = VIDEO.count.fetch_add(1, Ordering::Relaxed);
        if count != 0 {
            let mut req = Request::new();
            req.push(RequestProperty::SetPosition { x: 0,
                                                    y: VIDEO.height * (count & 1) as usize });
            MBOX.exchange(req);
        }
        unsafe { PV1_STAT.write_volatile(PV_VSYNC) };
    }
}
