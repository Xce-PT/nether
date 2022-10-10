//! Video core driver.
//!
//! Documentation:
//!
//! * [Mailbox property interface](https://github.com/raspberrypi/firmware/wiki/Mailbox-property-interface)
//!
//! Since there's no documented support for double-buffering, the pan property
//! tag is being used to move the display to the top of the frame buffer every
//! even frame and to the bottom of the frame buffer every odd frame.

use core::arch::asm;
use core::sync::atomic::{fence, Ordering};

use crate::mbox::{Mailbox, Message, MBOX};
use crate::pgalloc::ALLOC as PGALLOC;
use crate::sync::{Lazy, Lock};
use crate::touch::Info as TouchInfo;

/// Frame buffer allocation property tag.
const ALLOC_TAG: u32 = 0x40001;
/// Frame buffer physical dimensions property tag.
const PHYS_DIM_TAG: u32 = 0x48003;
/// Frame buffer virtual dimensions property tag.
const VIRT_DIM_TAG: u32 = 0x48004;
/// Frame buffer pixel depth property tag.
const PIX_DEPTH_TAG: u32 = 0x48005;
/// Frame buffer panning property tag.
const OFFSET_TAG: u32 = 0x48009;

/// Global video driver instance.
pub static VIDEO: Lazy<Lock<Video>> = Lazy::new(Video::new);

/// Video driver.
#[derive(Debug)]
pub struct Video
{
    /// Frame buffer base.
    base: *mut u32,
    /// Frame buffer size in bytes.
    size: usize,
    /// Frame buffer width.
    width: usize,
    /// Frame buffer height.
    height: usize,
    /// Frame counter.
    count: u64,
}

/// Frame buffer dimensions.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Dimensions
{
    /// Width of the frame buffer.
    width: u32,
    /// Height of the frame buffer.
    height: u32,
}

/// Frame buffer allocation.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Alloc
{
    /// Base of the allocated memory, or alignment on request.
    base: u32,
    /// Size of the allocated memory.
    size: u32,
}

/// Frame buffer panning offset.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct Offset
{
    /// Horizontal coordinate of the display's top side in the frame buffer.
    x: u32,
    /// Vertical coordinate of the display's left side in the frame buffer.
    y: u32,
}

impl Video
{
    /// Creates and initializes a new video driver instance.
    ///
    /// Returns the newly created instance.
    fn new() -> Lock<Self>
    {
        let mut msg = Message::new_in(&PGALLOC).unwrap();
        let dim = Dimensions { width: 800,
                               height: 480 };
        msg.add_tag(PHYS_DIM_TAG, dim).unwrap();
        let dim = Dimensions { width: 800,
                               height: 480 * 2 };
        msg.add_tag(VIRT_DIM_TAG, dim).unwrap();
        let depth = 32u32;
        msg.add_tag(PIX_DEPTH_TAG, depth).unwrap();
        let alloc = Alloc { base: 16, // Alignment.
                            size: 0 };
        msg.add_tag(ALLOC_TAG, alloc).unwrap();
        let msg = MBOX.exchange(msg).unwrap();
        let mut base: *mut u32 = 0usize as _;
        let mut size = 0usize;
        let mut width = 0usize;
        let mut height = 0usize;
        for tag in &msg {
            match tag.id() {
                ALLOC_TAG => {
                    let alloc = unsafe { tag.interpret_as::<Alloc>().unwrap() };
                    base = Mailbox::map_from_vc(alloc.base);
                    size = alloc.size as _;
                }
                PHYS_DIM_TAG => {
                    let dim = unsafe { tag.interpret_as::<Dimensions>().unwrap() };
                    width = dim.width as _;
                    height = dim.height as _;
                }
                _ => continue,
            }
        }
        let this = Self { base,
                          size,
                          width,
                          height,
                          count: 0 };
        Lock::new(this)
    }

    /// Moves the display to the currently off-screen half of the frame buffer
    /// to show the latest frame.
    pub fn vsync(&mut self)
    {
        if self.count != 0 {
            let mut msg = Message::new_in(&PGALLOC).unwrap();
            let offset = Offset { x: 0,
                                  y: if self.count & 0x1 == 0 { 0 } else { self.height as _ } };
            msg.add_tag(OFFSET_TAG, offset).unwrap();
            MBOX.exchange(msg).unwrap();
        }
        self.count += 1;
    }

    /// Draws circles with a fixed radius around the specified.
    ///
    /// * `points`: The points around which to draw the circles.
    pub fn draw_circles(&mut self, points: &[TouchInfo])
    {
        // Draw to the off-screen frame buffer.
        let base = if self.count & 1 == 0 {
            unsafe { self.base.add(self.size / 4 / 2) }
        } else {
            self.base
        };
        unsafe {
            asm!(
                // Find the squares of the radius of the inner and outer circles and store them in every lane of their respective vectors..
                "mov {irad}.s[0], {rad:w}",
                "mov {irad}.s[1], {irad}.s[0]",
                "mov {irad}.d[1], {irad}.d[0]",
                "mov {orad}.16b, {irad}.16b",
                "mov {tmpl}.s[0], {thick:w}",
                "mov {tmpl}.s[1], {tmpl}.s[0]",
                "mov {tmpl}.d[1], {tmpl}.d[0]",
                "add {orad}.4s, {orad}.4s, {tmpl}.4s",
                "mul {irad}.4s, {irad}.4s, {irad}.4s",
                "mul {orad}.4s, {orad}.4s, {orad}.4s",
                // Create the template for the column offsets.
                "mov {tmp:w}, #0",
                "mov {tmpl}.s[0], {tmp:w}",
                "mov {tmp:w}, #1",
                "mov {tmpl}.s[1], {tmp:w}",
                "mov {tmp:w}, #2",
                "mov {tmpl}.s[2], {tmp:w}",
                "mov {tmp:w}, #3",
                "mov {tmpl}.s[3], {tmp:w}",
                // Create a mask to set the alpha channel to opaque on every pixel.
                "movi {alpha}.4s, #0xff, lsl 24",
                // Loop over all the rows of the screen.
                "mov {row:w}, #0",
                "0:",
                "cmp {row:w}, {end_row:w}",
                "beq 0f",
                // Copy the current row to every lane of a vector to allow computing distances later.
                "mov {rows}.s[0], {row:w}",
                "mov {rows}.s[1], {rows}.s[0]",
                "mov {rows}.d[1], {rows}.d[0]",
                // Loop over all the columns in each row.
                "mov {col:w}, #0",
                "1:",
                "cmp {col:w}, {end_col:w}",
                "beq 1f",
                // Copy the current column to a vector and add the offset template.
                "mov {cols}.s[0], {col:w}",
                "mov {cols}.s[1], {cols}.s[0]",
                "mov {cols}.d[1], {cols}.d[0]",
                "add {cols}.4s,{cols}.4s, {tmpl}.4s",
                // Loop over all the provided points.
                "mov {point}, {start_point}",
                "movi {pxs}.2d, #0x0",
                "2:",
                "cmp {point}, {end_point}",
                "beq 2f",
                // Load the point and find the distance to all the pixel coordinates in the vectors.
                "ld2r {{v0.4s, v1.4s}}, [{point}], #0x8",
                "sub v0.4s, v0.4s, {cols}.4s",
                "sub v1.4s, v1.4s, {rows}.4s",
                "mul {dist}.4s, v0.4s, v0.4s",
                "mla {dist}.4s, v1.4s, v1.4s",
                // Check whether the pixels are inside the outer or inner circles.
                "cmhi v0.4s, {orad}.4s, {dist}.4s",
                "cmhi v1.4s, {dist}.4s, {irad}.4s",
                "and v0.16b, v0.16b, v1.16b",
                // Draw the pixels without erasing previously drawn circles.
                "orr {pxs}.16b, v0.16b, {pxs}.16b",
                "b 2b",
                "2:",
                // Make the pixels opaque and draw them in the frame buffer.
                "orr {pxs}.16b, {pxs}.16b, {alpha}.16b",
                "str {pxs:q}, [{base}], #0x10",
                "add {col:w}, {col:w}, #4",
                "b 1b",
                "1:",
                "add {row:w}, {row:w}, #1",
                "b 0b",
                "0:",
                base = inout (reg) base => _,
                start_point = in (reg) points.as_ptr(),
                end_point = in (reg) points.as_ptr().add(points.len()),
                point = out (reg) _,
                end_col = in (reg) self.width,
                col = out (reg) _,
                cols = out (vreg) _,
                end_row = in (reg) self.height,
                row = out (reg) _,
                rows = out (vreg) _,
                tmp = out (reg) _,
                tmpl = out (vreg) _,
                rad = in (reg) 50i32,
                irad = out (vreg) _,
                orad = out (vreg) _,
                thick = in (reg) 4,
                dist = out (vreg) _,
                alpha = out (vreg) _,
                pxs = out (vreg) _,
                out ("v0") _,
                out ("v1") _,
                options (nostack)
            );
        }
        fence(Ordering::Release);
    }
}
