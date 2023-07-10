//! Frame buffer rendering target.
//!
//! Expects triangles with X and Y in screen coordinates with an inverted Z
//! where the near clipping plane is at 1 and the far clipping plane is at 0,
//! and draws them to cached tiles of up to 32x32 pixels. Color pixels are
//! stored in the 16 bit little endian integer RGB565 format, whereas depth
//! pixels are stored in a custom 16-bit floating point format with just a 5-bit
//! exponent and 11-bit mantissa.

extern crate alloc;

use alloc::alloc::GlobalAlloc;
use core::alloc::Layout;
use core::iter::Iterator;
use core::mem::size_of;
use core::simd::{f32x4, u16x4, u32x4, SimdFloat, SimdPartialEq, SimdPartialOrd, SimdUint};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::alloc::{Alloc, UNCACHED_REGION};
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
    /// Tile count.
    tcount: usize,
    /// Id of the next tile to draw.
    tnext: AtomicU64,
    /// Finished tile counter.
    tfinished: AtomicU64,
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
    /// Frame buffer that this tile draws to.
    fb: &'a FrameBuffer,
    /// Origin column for this tile.
    col: usize,
    /// Origin row for this tile.
    row: usize,
    /// X control point coordinates.
    xctl: f32x4,
    /// Y control point coordinates.
    yctl: f32x4,
    /// Tile's color buffer.
    cb: Buffer,
    /// Tile's depth buffer.
    db: Buffer,
}

/// Vertex.
#[derive(Clone, Copy, Debug)]
pub struct Vertex
{
    /// Projected position.
    pub proj: f32x4,
    /// RGBA color.
    pub color: f32x4,
}

/// Tile buffer.
#[repr(align(0x40), C)]
#[derive(Debug)]
struct Buffer([u16; TILE_DIM_MAX * TILE_DIM_MAX]);

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
        Self { fb0,
               fb1,
               width,
               height,
               twidth,
               theight,
               tcount: width * height / (twidth * theight),
               tnext: AtomicU64::new(0),
               tfinished: AtomicU64::new(0) }
    }

    /// Returns the current frame ID.
    pub fn frame(&self) -> u64
    {
        self.tfinished.load(Ordering::Relaxed) / self.tcount as u64
    }

    /// Creates an iterator of tiles awaiting to be drawn.
    ///
    /// Returns the newly created iterator.
    pub fn tiles(&self) -> FrameBufferIterator
    {
        FrameBufferIterator::new(self)
    }

    /// Returns the DMA address of the frame buffer not currently being drawn.
    pub fn vsync(&self) -> u32
    {
        let frame = self.frame();
        if frame & 0x1 == 0 {
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
        Self { fb, frame: fb.frame() }
    }
}

impl<'a> Iterator for FrameBufferIterator<'a>
{
    type Item = Tile<'a>;

    fn next(&mut self) -> Option<Tile<'a>>
    {
        let tnext = loop {
            let tnext = self.fb.tnext.load(Ordering::Relaxed);
            if tnext / self.fb.tcount as u64 != self.frame {
                return None;
            };
            if self.fb
                   .tnext
                   .compare_exchange(tnext, tnext + 1, Ordering::Relaxed, Ordering::Relaxed)
                   .is_ok()
            {
                break tnext;
            }
        };
        Some(Tile::new(self.fb, tnext))
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
    fn new(fb: &'a FrameBuffer, id: u64) -> Self
    {
        let pos = id as usize % fb.tcount;
        let col = pos * fb.twidth % fb.width;
        let row = pos * fb.twidth / fb.width * fb.theight;
        let xdist = fb.twidth as f32;
        let ydist = fb.theight as f32;
        let xmin = col as f32;
        let ymin = row as f32;
        let xmax = xmin + xdist;
        let ymax = ymin + ydist;
        let xctl = f32x4::from([xmin, xmax, xmin, xmax]);
        let yctl = f32x4::from([ymin, ymin, ymax, ymax]);
        let cb = Buffer([0; TILE_DIM_MAX * TILE_DIM_MAX]);
        let db = Buffer([0; TILE_DIM_MAX * TILE_DIM_MAX]);
        Self { fb,
               col,
               row,
               xctl,
               yctl,
               cb,
               db }
    }

    /// Draws a triangle to the tile.
    ///
    /// * `vert0`: First vertex.
    /// * `vert1`: Second vertex.
    /// * `vert2`: Third vertex.
    pub fn draw_triangle(&mut self, vert0: Vertex, vert1: Vertex, vert2: Vertex)
    {
        let ptx = self.xctl;
        let pty = self.yctl;
        // Check whether the triangle's axis aligned bounding box overlaps this tile.
        let tri_max = vert0.proj.simd_max(vert1.proj).simd_max(vert2.proj);
        let tile_min = f32x4::from([ptx[0], pty[0], 0.0, 0.0]);
        if tri_max.simd_lt(tile_min).any() {
            // Triangle is guaranteed to be completely outside this tile.
            return;
        }
        let tri_min = vert0.proj.simd_min(vert1.proj).simd_min(vert2.proj);
        let tile_max = f32x4::from([ptx[3], pty[3], 1.0, f32::INFINITY]);
        if tri_min.simd_gt(tile_max).any() {
            // Triangle is completely outside this tile.
            return;
        }
        // Compute the linear barycentric coordinates of the fragments at the tile's
        // corners.
        let x0 = f32x4::splat(vert0.proj[0]) - ptx;
        let y0 = f32x4::splat(vert0.proj[1]) - pty;
        let x1 = f32x4::splat(vert1.proj[0]) - ptx;
        let y1 = f32x4::splat(vert1.proj[1]) - pty;
        let x2 = f32x4::splat(vert2.proj[0]) - ptx;
        let y2 = f32x4::splat(vert2.proj[1]) - pty;
        let area0 = x1 * y2 - x2 * y1;
        if area0.reduce_max() < 0.0 {
            // The whole triangle is outside the tile.
            return;
        }
        let area1 = x2 * y0 - x0 * y2;
        if area1.reduce_max() < 0.0 {
            // The whole triangle is outside the tile.
            return;
        }
        let area2 = x0 * y1 - x1 * y0;
        if area2.reduce_max() < 0.0 {
            // The whole triangle is outside the tile.
            return;
        }
        let total_recip = (area0 + area1 + area2).recip();
        let mut bary0 = area0 * total_recip;
        let mut bary1 = area1 * total_recip;
        let mut bary2 = area2 * total_recip;
        // Compute the linear barycentric coordinate increments.
        let twidth = self.fb.twidth;
        let theight = self.fb.theight;
        let xdiv = (twidth as f32).recip();
        let ydiv = (theight as f32).recip();
        let hinc0 = (bary0[1] - bary0[0]) * xdiv;
        let vinc0 = (bary0[2] - bary0[0]) * ydiv;
        bary0[0] += hinc0 * 0.5 + vinc0 * 0.5;
        bary0[1] = bary0[0] + hinc0;
        bary0[2] = bary0[1] + hinc0;
        bary0[3] = bary0[2] + hinc0;
        let hinc0 = f32x4::splat(hinc0 * 4.0);
        let vinc0 = f32x4::splat(vinc0);
        let hinc1 = (bary1[1] - bary1[0]) * xdiv;
        let vinc1 = (bary1[2] - bary1[0]) * ydiv;
        bary1[0] += hinc1 * 0.5 + vinc1 * 0.5;
        bary1[1] = bary1[0] + hinc1;
        bary1[2] = bary1[1] + hinc1;
        bary1[3] = bary1[2] + hinc1;
        let hinc1 = f32x4::splat(hinc1 * 4.0);
        let vinc1 = f32x4::splat(vinc1);
        let hinc2 = (bary2[1] - bary2[0]) * xdiv;
        let vinc2 = (bary2[2] - bary2[0]) * ydiv;
        bary2[0] += hinc2 * 0.5 + vinc2 * 0.5;
        bary2[1] = bary2[0] + hinc2;
        bary2[2] = bary2[1] + hinc2;
        bary2[3] = bary2[2] + hinc2;
        let hinc2 = f32x4::splat(hinc2 * 4.0);
        let vinc2 = f32x4::splat(vinc2);
        // Loop over all the pixels in the tile in groups of 4, and shade those that
        // belong to the triangle.
        let mut vbary0 = bary0;
        let mut vbary1 = bary1;
        let mut vbary2 = bary2;
        let zero = f32x4::splat(0.0);
        let one = f32x4::splat(1.0);
        let dxm = u32x4::splat(0x3F800000);
        let dxb = u32x4::splat(0x30000000);
        let dmm = u32x4::splat(0x7FF000);
        let ds = u32x4::splat(12);
        let rbmul = f32x4::splat(31.5);
        let gmul = f32x4::splat(63.5);
        let rshift = u32x4::splat(11);
        let gshift = u32x4::splat(5);
        for trow in 0 .. theight {
            let mut hbary0 = vbary0;
            let mut hbary1 = vbary1;
            let mut hbary2 = vbary2;
            for tcol in (0 .. twidth).step_by(4) {
                let offset = trow * twidth + tcol;
                // Fill the triangle.
                let mut valid = hbary0.simd_gt(zero) & hbary1.simd_gt(zero) & hbary2.simd_gt(zero);
                // Also draw the top, top-right, right, and bottom-right edges.
                if (hbary0.simd_eq(zero) | hbary1.simd_eq(zero) | hbary2.simd_eq(zero)).any() {
                    valid |= hbary0.simd_eq(zero) & (hinc0.simd_lt(zero) | hinc0.simd_eq(zero) & vinc0.simd_lt(zero));
                    valid |= hbary1.simd_eq(zero) & (hinc1.simd_lt(zero) | hinc1.simd_eq(zero) & vinc1.simd_lt(zero));
                    valid |= hbary2.simd_eq(zero) & (hinc2.simd_lt(zero) | hinc2.simd_eq(zero) & vinc2.simd_lt(zero));
                }
                // Compute the perspective-correct barycentric coordinates.
                let w0 = f32x4::splat(vert0.proj[3]) * hbary0;
                let w1 = f32x4::splat(vert1.proj[3]) * hbary1;
                let w2 = f32x4::splat(vert2.proj[3]) * hbary2;
                let wp = (w0 + w1 + w2).recip();
                let bary0 = w0 * wp;
                let bary1 = w1 * wp;
                let bary2 = w2 * wp;
                // Compute the depth and exclude all fragments outside the range between the
                // values in the depth buffer and the near clipping plane.
                let db = unsafe { self.db.0.as_mut_ptr().add(offset).cast::<u16x4>() };
                let odepth = unsafe { db.read() };
                let z0 = f32x4::splat(vert0.proj[2]) * bary0;
                let z1 = f32x4::splat(vert1.proj[2]) * bary1;
                let z2 = f32x4::splat(vert2.proj[2]) * bary2;
                let z = z0 + z1 + z2;
                valid &= z.simd_le(one);
                let zb = z.to_bits().saturating_sub(dxb);
                let zx = (zb & dxm) >> ds;
                let zm = (zb & dmm) >> ds;
                let depth = (zx | zm).cast::<u16>();
                let mut valid = valid.cast::<i16>();
                valid &= depth.simd_gt(odepth);
                if valid.any() {
                    // Store the new depth values.
                    let depth = valid.select(depth, odepth);
                    unsafe { db.write(depth) };
                    // Apply shading.
                    let cb = unsafe { self.cb.0.as_mut_ptr().add(offset).cast::<u16x4>() };
                    let ocolor = unsafe { cb.read() };
                    let red0 = f32x4::splat(vert0.color[0]) * bary0;
                    let red1 = f32x4::splat(vert1.color[0]) * bary1;
                    let red2 = f32x4::splat(vert2.color[0]) * bary2;
                    let red = red0 + red1 + red2;
                    let red = red.simd_max(zero).simd_min(one);
                    let green0 = f32x4::splat(vert0.color[1]) * bary0;
                    let green1 = f32x4::splat(vert1.color[1]) * bary1;
                    let green2 = f32x4::splat(vert2.color[1]) * bary2;
                    let green = green0 + green1 + green2;
                    let green = green.simd_max(zero).simd_min(one);
                    let blue0 = f32x4::splat(vert0.color[2]) * bary0;
                    let blue1 = f32x4::splat(vert1.color[2]) * bary1;
                    let blue2 = f32x4::splat(vert2.color[2]) * bary2;
                    let blue = blue0 + blue1 + blue2;
                    let blue = blue.simd_max(zero).simd_min(one);
                    // Compute the RGB565 color values.
                    let red = (red * rbmul).cast::<u32>() << rshift;
                    let green = (green * gmul).cast::<u32>() << gshift;
                    let blue = (blue * rbmul).cast::<u32>();
                    let color = (red | green | blue).cast::<u16>();
                    let color = valid.select(color, ocolor);
                    unsafe { cb.write(color) };
                }
                hbary0 += hinc0;
                hbary1 += hinc1;
                hbary2 += hinc2;
            }
            vbary0 += vinc0;
            vbary1 += vinc1;
            vbary2 += vinc2;
        }
    }
}

impl<'a> Drop for Tile<'a>
{
    fn drop(&mut self)
    {
        let buf = if self.fb.frame() & 0x1 == 0 {
            self.fb.fb1
        } else {
            self.fb.fb0
        };
        let buf = unsafe { buf.add(self.row * self.fb.width + self.col) };
        for trow in 0 .. self.fb.theight {
            unsafe {
                let buf = buf.add(trow * self.fb.width);
                let cb = self.cb.0.as_ptr().add(trow * self.fb.twidth);
                cb.copy_to_nonoverlapping(buf, self.fb.twidth);
            }
        }
        self.fb.tfinished.fetch_add(1, Ordering::Relaxed);
    }
}
