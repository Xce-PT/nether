//! Frame buffer rendering target.

extern crate alloc;

use alloc::alloc::GlobalAlloc;
use core::alloc::Layout;
use core::arch::aarch64::*;
use core::iter::Iterator;
use core::mem::size_of;

use crate::alloc::{Alloc, UNCACHED_REGION};
use crate::sync::Lock;
use crate::to_dma;

/// Maximum width or height of a tile.
const TILE_DIM_MAX: usize = 32;

/// Uncached memory allocator.
static UNCACHED: Alloc<0x40> = Alloc::with_region(&UNCACHED_REGION);

/// Frame buffer.
pub struct FrameBuffer
{
    /// First frame buffer.
    fb0: *mut u16,
    /// Second frame buffer.
    fb1: *mut u16,
    /// Image width.
    width: usize,
    /// Image height.
    height: usize,
    /// Tile width.
    twidth: usize,
    /// Tile height.
    theight: usize,
    /// Controls.
    ctrl: Lock<Control>,
}

/// Frame buffer iterator.
pub struct FrameBufferIterator<'a>
{
    /// Frame buffer being iterated.
    fb: &'a FrameBuffer,
    /// Frame being iterated.
    frame: u64,
}

/// Frame buffer tile.
pub struct Tile<'a>
{
    /// Frame buffer that this tile draws on.
    fb: &'a FrameBuffer,
    /// Origin column for this tile.
    col: usize,
    /// Origin row for this tile.
    row: usize,
    /// Tile's color buffer.
    cb: [u16; TILE_DIM_MAX * TILE_DIM_MAX],
    /// Tile's depth buffer.
    db: [u16; TILE_DIM_MAX * TILE_DIM_MAX],
}

/// Vertex.
#[derive(Clone, Copy, Debug)]
pub struct Vertex
{
    /// Projected position.
    pub proj: float32x4_t,
    /// RGBA color.
    pub color: float32x4_t,
}

/// Frame buffer control.
#[derive(Debug)]
struct Control
{
    /// Current frame ID.
    frame: u64,
    /// Next tile ID.
    tnext: usize,
    /// Count of finished tiles in the current frame.
    tfinished: usize,
}

impl FrameBuffer
{
    /// Creates and initializes a new frame buffer.
    ///
    /// * `width`: Image width.
    /// * `height`: Image height.
    ///
    /// Returns the newly created frame buffer.
    ///
    /// Panics if the resolution is not supported or the system runs out of
    /// uncached memory to allocate.
    #[track_caller]
    pub fn new(width: usize, height: usize) -> Self
    {
        let mut twidth = 0;
        let mut theight = 0;
        for sz in (8 ..= TILE_DIM_MAX).step_by(8) {
            if width % sz == 0 {
                twidth = sz;
            }
            if height % sz == 0 {
                theight = sz;
            }
        }
        assert!(twidth > 0 && theight > 0, "Invalid width or height");
        let layout = Layout::from_size_align(width * height * size_of::<u16>(), 64).unwrap();
        let fb0 = unsafe { UNCACHED.alloc_zeroed(layout).cast::<u16>() };
        let fb1 = unsafe { UNCACHED.alloc_zeroed(layout).cast::<u16>() };
        assert!(!fb0.is_null() && !fb1.is_null(),
                "Failed to allocate memory for the frame buffers");
        let ctrl = Control { frame: 0,
                             tnext: 0,
                             tfinished: 0 };
        Self { fb0,
               fb1,
               width,
               height,
               twidth,
               theight,
               ctrl: Lock::new(ctrl) }
    }

    /// Returns the current frame ID.
    pub fn frame(&self) -> u64
    {
        self.ctrl.lock().frame
    }

    /// Creates an iterator of tiles awaiting to be drawn.
    ///
    /// Returns the newly created iterator.
    pub fn tiles(&self) -> FrameBufferIterator
    {
        FrameBufferIterator::new(self)
    }

    /// Returns the DMA address of the frame buffer not currently being drawn,
    /// flipping them beforehand if drawing has finished.
    pub fn vsync(&self) -> u32
    {
        let mut ctrl = self.ctrl.lock();
        if ctrl.tfinished == self.width * self.height / (self.twidth * self.theight) {
            ctrl.tnext = 0;
            ctrl.tfinished = 0;
            ctrl.frame += 1;
        };
        if ctrl.frame & 0x1 == 0 {
            return to_dma(self.fb0 as _) as _;
        }
        to_dma(self.fb1 as _) as _
    }
}

impl Drop for FrameBuffer
{
    fn drop(&mut self)
    {
        let layout = Layout::from_size_align(self.width * self.height * size_of::<u16>(), 64).unwrap();
        unsafe {
            UNCACHED.dealloc(self.fb0.cast(), layout);
            UNCACHED.dealloc(self.fb1.cast(), layout);
        }
    }
}

unsafe impl Send for FrameBuffer {}

unsafe impl Sync for FrameBuffer {}

impl<'a> FrameBufferIterator<'a>
{
    /// Creates and initializes a new iterator over the tiles of a frame buffer.
    ///
    /// * `fb`: Frame buffer that this iterator borrows tiles from.
    ///
    /// Returns the newly created iterator.
    fn new(fb: &'a FrameBuffer) -> Self
    {
        Self { fb,
               frame: fb.ctrl.lock().frame }
    }
}

impl<'a> Iterator for FrameBufferIterator<'a>
{
    type Item = Tile<'a>;

    fn next(&mut self) -> Option<Tile<'a>>
    {
        let mut ctrl = self.fb.ctrl.lock();
        if self.frame != ctrl.frame {
            return None;
        }
        let id = ctrl.tnext;
        if id >= self.fb.width * self.fb.height / (self.fb.twidth * self.fb.theight) {
            return None;
        }
        ctrl.tnext += 1;
        Some(Tile::new(self.fb, id))
    }
}

impl<'a> Tile<'a>
{
    /// Creates and initializes a new tile.
    ///
    /// * `fb`: Frame buffer that this tile represents.
    /// * `id`: ID of the tile.
    ///
    /// Returns the newly created tile.
    ///
    /// Panics if the specified tile identifier is outside the valid range.
    #[track_caller]
    fn new(fb: &'a FrameBuffer, id: usize) -> Self
    {
        let col = id * fb.twidth % fb.width;
        let row = id * fb.twidth / fb.width * fb.theight;
        assert!(row < fb.height, "Invalid tile ID: {id}");
        let cb = [0; TILE_DIM_MAX * TILE_DIM_MAX];
        let db = [0; TILE_DIM_MAX * TILE_DIM_MAX];
        Self { fb, col, row, cb, db }
    }

    /// Draws a triangle to the tile.
    ///
    /// * `vert0`: First vertex.
    /// * `vert1`: Second vertex.
    /// * `vert2`: Third vertex.
    pub fn draw_triangle(&mut self, vert0: Vertex, vert1: Vertex, vert2: Vertex)
    {
        unsafe {
            // Skip if the bounding rectangle of the triangle is completely outside the
            // tile.
            let fcol = self.col as f32;
            let frow = self.row as f32;
            let min = vminq_f32(vert0.proj, vert1.proj);
            let min = vminq_f32(min, vert2.proj);
            let max = vmaxq_f32(vert0.proj, vert1.proj);
            let max = vmaxq_f32(max, vert2.proj);
            let tmin = vld1q_f32([fcol, frow, 0.0, 0.0].as_ptr());
            let tmax = vld1q_f32([fcol + self.fb.twidth as f32,
                                  frow + self.fb.theight as f32,
                                  1.0,
                                  f32::INFINITY].as_ptr());
            let cond0 = vcgtq_f32(tmax, min);
            let cond1 = vcltq_f32(tmin, max);
            let cond = vandq_u32(cond0, cond1);
            if vminvq_u32(cond) == 0 {
                return;
            }
            // Determine the linear barycentric coordinates of the four control points.
            let one = vdupq_n_f32(1.0);
            let x0 = vdupq_laneq_f32(vert0.proj, 0);
            let y0 = vdupq_laneq_f32(vert0.proj, 1);
            let x1 = vdupq_laneq_f32(vert1.proj, 0);
            let y1 = vdupq_laneq_f32(vert1.proj, 1);
            let x2 = vdupq_laneq_f32(vert2.proj, 0);
            let y2 = vdupq_laneq_f32(vert2.proj, 1);
            let vfcol = vdupq_n_f32(fcol);
            let vfrow = vdupq_n_f32(frow);
            let ftwidth = self.fb.twidth as f32 - 0.5;
            let ftheight = self.fb.theight as f32 - 0.5;
            let ptx = vld1q_f32([0.5, ftwidth, 0.5, ftwidth].as_ptr());
            let ptx = vaddq_f32(ptx, vfcol);
            let pty = vld1q_f32([0.5, 0.5, ftheight, ftheight].as_ptr());
            let pty = vaddq_f32(pty, vfrow);
            let x0 = vsubq_f32(x0, ptx);
            let y0 = vsubq_f32(y0, pty);
            let x1 = vsubq_f32(x1, ptx);
            let y1 = vsubq_f32(y1, pty);
            let x2 = vsubq_f32(x2, ptx);
            let y2 = vsubq_f32(y2, pty);
            let area0 = vmulq_f32(x1, y2);
            let area0 = vmlsq_f32(area0, x2, y1);
            let area1 = vmulq_f32(x2, y0);
            let area1 = vmlsq_f32(area1, x0, y2);
            let area2 = vmulq_f32(x0, y1);
            let area2 = vmlsq_f32(area2, x1, y0);
            if vmaxvq_f32(area0) < 0.0 || vmaxvq_f32(area1) < 0.0 || vmaxvq_f32(area2) < 0.0 {
                // Triangle is completely outside this tile.
                return;
            }
            let area = vaddq_f32(area0, area1);
            let area = vaddq_f32(area, area2);
            let invarea = vdivq_f32(one, area);
            let bary0 = vmulq_f32(area0, invarea);
            let bary1 = vmulq_f32(area1, invarea);
            let bary2 = vmulq_f32(area2, invarea);
            // Compute the linear barycentric coordinate increments.
            let invtwidth = 1.0 / (ftwidth - 0.5);
            let invtheight = 1.0 / (ftheight - 0.5);
            let ctl0 = vgetq_lane_f32(bary0, 0);
            let hdiff0 = vgetq_lane_f32(bary0, 1);
            let hdiff0 = hdiff0 - ctl0;
            let hdiff0 = hdiff0 * invtwidth;
            let vdiff0 = vgetq_lane_f32(bary0, 2);
            let vdiff0 = vdiff0 - ctl0;
            let vdiff0 = vdiff0 * invtheight;
            let ctl1 = vgetq_lane_f32(bary1, 0);
            let hdiff1 = vgetq_lane_f32(bary1, 1);
            let hdiff1 = hdiff1 - ctl1;
            let hdiff1 = hdiff1 * invtwidth;
            let vdiff1 = vgetq_lane_f32(bary1, 2);
            let vdiff1 = vdiff1 - ctl1;
            let vdiff1 = vdiff1 * invtheight;
            let ctl2 = vgetq_lane_f32(bary2, 0);
            let hdiff2 = vgetq_lane_f32(bary2, 1);
            let hdiff2 = hdiff2 - ctl2;
            let hdiff2 = hdiff2 * invtwidth;
            let vdiff2 = vgetq_lane_f32(bary2, 2);
            let vdiff2 = vdiff2 - ctl2;
            let vdiff2 = vdiff2 * invtheight;
            let hinc0 = vdupq_n_f32(hdiff0 * 4.0);
            let vinc0 = vdupq_n_f32(vdiff0);
            let hinc1 = vdupq_n_f32(hdiff1 * 4.0);
            let vinc1 = vdupq_n_f32(vdiff1);
            let hinc2 = vdupq_n_f32(hdiff2 * 4.0);
            let vinc2 = vdupq_n_f32(vdiff2);
            // Loop over all the fragments in the tile, drawing them in groups of 4.
            let mut vbary0 = vld1q_f32([ctl0, ctl0 + hdiff0, ctl0 + hdiff0 + hdiff0, ctl0 + hdiff0 * 3.0].as_ptr());
            let mut vbary1 = vld1q_f32([ctl1, ctl1 + hdiff1, ctl1 + hdiff1 + hdiff1, ctl1 + hdiff1 * 3.0].as_ptr());
            let mut vbary2 = vld1q_f32([ctl2, ctl2 + hdiff2, ctl2 + hdiff2 + hdiff2, ctl2 + hdiff2 * 3.0].as_ptr());
            for row in 0 .. self.fb.theight {
                let mut hbary0 = vbary0;
                let mut hbary1 = vbary1;
                let mut hbary2 = vbary2;
                for col in (0 .. self.fb.twidth).step_by(4) {
                    // Invalidate all the fragments outside the triangle.
                    let valid0 = vcgezq_f32(hbary0);
                    let valid1 = vcgezq_f32(hbary1);
                    let valid2 = vcgezq_f32(hbary2);
                    let valid = vandq_u32(valid0, valid1);
                    let valid = vandq_u32(valid, valid2);
                    // Also invalidate fragments at the bottom or left edges.
                    let cond0 = vcltzq_f32(hinc0);
                    let cond1 = vcltzq_f32(vinc0);
                    let cond2 = vceqzq_f32(hbary0);
                    let cond2 = vmvnq_u32(cond2);
                    let cond = vandq_u32(cond0, cond1);
                    let cond = vorrq_u32(cond, cond2);
                    let valid = vandq_u32(valid, cond);
                    let cond0 = vcltzq_f32(hinc1);
                    let cond1 = vcltzq_f32(vinc1);
                    let cond2 = vceqzq_f32(hbary1);
                    let cond2 = vmvnq_u32(cond2);
                    let cond = vandq_u32(cond0, cond1);
                    let cond = vorrq_u32(cond, cond2);
                    let valid = vandq_u32(valid, cond);
                    let cond0 = vcltzq_f32(hinc2);
                    let cond1 = vcltzq_f32(vinc2);
                    let cond2 = vceqzq_f32(hbary2);
                    let cond2 = vmvnq_u32(cond2);
                    let cond = vandq_u32(cond0, cond1);
                    let cond = vorrq_u32(cond, cond2);
                    let valid = vandq_u32(valid, cond);
                    if vmaxvq_u32(valid) > 0 {
                        // Compute the perspective-correct barycentric coordinates.
                        let w0 = vdupq_laneq_f32(vert0.proj, 3);
                        let w0 = vmulq_f32(w0, hbary0);
                        let w1 = vdupq_laneq_f32(vert1.proj, 3);
                        let w1 = vmulq_f32(w1, hbary1);
                        let w2 = vdupq_laneq_f32(vert2.proj, 3);
                        let w2 = vmulq_f32(w2, hbary2);
                        let wp = vaddq_f32(w0, w1);
                        let wp = vaddq_f32(wp, w2);
                        let wp = vdivq_f32(one, wp);
                        let bary0 = vmulq_f32(w0, wp);
                        let bary1 = vmulq_f32(w1, wp);
                        let bary2 = vmulq_f32(w2, wp);
                        // Compute the Z component, discarding any fragments outside the clip range.
                        let z = vmulq_laneq_f32(bary0, vert0.proj, 2);
                        let z = vmlaq_laneq_f32(z, bary1, vert1.proj, 2);
                        let z = vmlaq_laneq_f32(z, bary2, vert2.proj, 2);
                        let cond0 = vcgezq_f32(z);
                        let cond1 = vcleq_f32(z, one);
                        let valid = vandq_u32(valid, cond0);
                        let valid = vandq_u32(valid, cond1);
                        // Compute the color values.
                        let maxrb = vdupq_n_f32(31.5);
                        let maxg = vdupq_n_f32(63.5);
                        let red = vmulq_laneq_f32(bary0, vert0.color, 0);
                        let red = vmlaq_laneq_f32(red, bary1, vert1.color, 0);
                        let red = vmlaq_laneq_f32(red, bary2, vert2.color, 0);
                        let red = vmulq_f32(red, maxrb);
                        let red = vcvtq_u32_f32(red);
                        let red = vshlq_n_u32(red, 11);
                        let green = vmulq_laneq_f32(bary0, vert0.color, 1);
                        let green = vmlaq_laneq_f32(green, bary1, vert1.color, 1);
                        let green = vmlaq_laneq_f32(green, bary2, vert2.color, 1);
                        let green = vmulq_f32(green, maxg);
                        let green = vcvtq_u32_f32(green);
                        let green = vshlq_n_u32(green, 5);
                        let blue = vmulq_laneq_f32(bary0, vert0.color, 2);
                        let blue = vmlaq_laneq_f32(blue, bary1, vert1.color, 2);
                        let blue = vmlaq_laneq_f32(blue, bary2, vert2.color, 2);
                        let blue = vmulq_f32(blue, maxrb);
                        let blue = vcvtq_u32_f32(blue);
                        let rgb565 = vorrq_u32(red, green);
                        let rgb565 = vorrq_u32(rgb565, blue);
                        // Store the valid pixels in the tile.
                        let offset = row * self.fb.twidth + col;
                        let valid = vmovn_u32(valid);
                        let od = vld1_u16(self.db.as_ptr().add(offset));
                        let d = vreinterpretq_u32_f32(z);
                        let dx = vshrq_n_u32(d, 14);
                        let dm = vshrq_n_u32(d, 12);
                        let fm = vdupq_n_u32(0xF800);
                        let d = vbslq_u32(fm, dx, dm);
                        let d = vmovn_u32(d);
                        let cond = vcgt_u16(d, od);
                        let valid = vand_u16(valid, cond);
                        let d = vbsl_u16(valid, d, od);
                        vst1_u16(self.db.as_mut_ptr().add(offset), d);
                        let rgb565 = vmovn_u32(rgb565);
                        let orgb565 = vld1_u16(self.cb.as_ptr().add(offset));
                        let rgb565 = vbsl_u16(valid, rgb565, orgb565);
                        vst1_u16(self.cb.as_mut_ptr().add(offset), rgb565);
                    }
                    hbary0 = vaddq_f32(hbary0, hinc0);
                    hbary1 = vaddq_f32(hbary1, hinc1);
                    hbary2 = vaddq_f32(hbary2, hinc2);
                }
                vbary0 = vaddq_f32(vbary0, vinc0);
                vbary1 = vaddq_f32(vbary1, vinc1);
                vbary2 = vaddq_f32(vbary2, vinc2);
            }
        }
    }
}

impl<'a> Drop for Tile<'a>
{
    fn drop(&mut self)
    {
        let buf = if self.fb.ctrl.lock().frame & 0x1 == 0 {
            self.fb.fb1
        } else {
            self.fb.fb0
        };
        let buf = unsafe { buf.add(self.row * self.fb.width + self.col) };
        for trow in 0 .. self.fb.theight {
            unsafe {
                let buf = buf.add(trow * self.fb.width);
                let cb = self.cb.as_ptr().add(trow * self.fb.twidth);
                cb.copy_to_nonoverlapping(buf, self.fb.twidth);
            }
        }
        self.fb.ctrl.lock().tfinished += 1;
    }
}
