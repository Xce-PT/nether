//! Frame buffer rendering target.
//!
//! Expects triangles with X and Y in screen coordinates with a reverse Z
//! where the near clipping plane is at 1 and the far clipping plane is at 0,
//! and draws them to cached tiles of up to 32x32 pixels. Color pixels are
//! stored in the 16 bit native endian integer RGB565 format, whereas depth
//! pixels are stored in a custom 16-bit native endian floating point format
//! with just a 5-bit exponent and 11-bit mantissa.

extern crate alloc;

use alloc::alloc::GlobalAlloc;
use core::alloc::Layout;
use core::iter::Iterator;
use core::mem::size_of;
use core::simd::{f32x4, mask32x4, u16x4, u16x8, u32x4, usizex8, SimdFloat, SimdPartialOrd, SimdUint};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::alloc::{Alloc, UNCACHED_REGION};
use crate::simd::{SimdFloatExtra, SimdPartialEqExtra, SimdPartialOrdExtra};
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
    /// X control points.
    ptx: f32x4,
    /// Y control points.
    pty: f32x4,
    // Axis aligned bounding box minimum values.
    min: f32x4,
    // Axis aligned bounding box maximum values.
    max: f32x4,
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
        let origx = col as f32 + 0.5;
        let origy = row as f32 + 0.5;
        let sizex = (fb.twidth - 1) as f32;
        let sizey = (fb.theight - 1) as f32;
        let ptx = f32x4::from_array([origx, origx + sizex, origx, origx + sizex]);
        let pty = f32x4::from_array([origy, origy, origy + sizey, origy + sizey]);
        let min = f32x4::from_array([origx, origy, 0.0, 0.0]);
        let max = f32x4::from_array([origx + sizex, origy + sizey, 1.0, f32::INFINITY]);
        let cb = Buffer([0; TILE_DIM_MAX * TILE_DIM_MAX]);
        let db = Buffer([0; TILE_DIM_MAX * TILE_DIM_MAX]);
        Self { fb,
               col,
               row,
               ptx,
               pty,
               min,
               max,
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
        // Check whether the axis-aligned bounding boxes of the triangle and tile
        // overlap.
        let tmax = self.max;
        let min = vert0.proj.simd_min(vert1.proj).simd_min(vert2.proj);
        if tmax.simd_lt(min).any() {
            // The triangle is completely outside this tile.
            return;
        }
        let tmin = self.min;
        let max = vert0.proj.simd_max(vert1.proj).simd_max(vert2.proj);
        if tmin.simd_gt(max).any() {
            // The triangle is completely outside this tile.
            return;
        }
        // Compute the linear barycentric coordinates at the corner control points.
        let ptx = self.ptx;
        let pty = self.pty;
        let x0 = f32x4::splat(vert0.proj[0]) - ptx;
        let y0 = f32x4::splat(vert0.proj[1]) - pty;
        let x1 = f32x4::splat(vert1.proj[0]) - ptx;
        let y1 = f32x4::splat(vert1.proj[1]) - pty;
        let x2 = f32x4::splat(vert2.proj[0]) - ptx;
        let y2 = f32x4::splat(vert2.proj[1]) - pty;
        let area0 = x1 * y2 - x2 * y1;
        if area0.reduce_max() < 0.0 {
            // The triangle is completely outside this tile.
            return;
        }
        let area1 = x2 * y0 - x0 * y2;
        if area1.reduce_max() < 0.0 {
            // The triangle is completely outside this tile.
            return;
        }
        let area2 = x0 * y1 - x1 * y0;
        if area2.reduce_max() < 0.0 {
            // The triangle is completely outside this tile.
            return;
        }
        let itotal = (area0 + area1 + area2).fast_recip();
        let bary0 = area0 * itotal;
        let bary1 = area1 * itotal;
        let bary2 = area2 * itotal;
        // Compute the barycentric coordinate increments.
        let sizexi = (ptx[1] - ptx[0]).recip();
        let sizeyi = (pty[2] - pty[0]).recip();
        let hinc0 = (bary0[1] - bary0[0]) * sizexi;
        let vinc0 = (bary0[2] - bary0[0]) * sizeyi;
        let hinc1 = (bary1[1] - bary1[0]) * sizexi;
        let vinc1 = (bary1[2] - bary1[0]) * sizeyi;
        let hinc2 = (bary2[1] - bary2[0]) * sizexi;
        let vinc2 = (bary2[2] - bary2[0]) * sizeyi;
        // Try to reduce the number of tests to the smallest possible axis-aligned
        // bounding box.
        let twidth = self.fb.twidth;
        let (tcol, trow, tcolmax, trowmax) =
            if bary0.reduce_min() >= 0.0 && bary1.reduce_min() >= 0.0 && bary2.reduce_min() >= 0.0 {
                // The triangle overlaps the whole tile.
                (0, 0, self.fb.twidth, self.fb.theight)
            } else if (tmin.simd_le(min) & tmax.simd_ge(max) | mask32x4::from_array([false, false, true, true])).all() {
                // The whole triangle fits inside the tile.
                let col = self.col;
                let row = self.row;
                let tcol = (min[0] - tmin[0]) as usize & !0x1;
                let trow = (min[1] - tmin[1]) as usize & !0x1;
                let tcolmax = (max[0] as usize - col + 2) & !0x1;
                let trowmax = (max[1] as usize - row + 2) & !0x1;
                (tcol, trow, tcolmax, trowmax)
            } else {
                // The tile and the triangle may overlap partially.
                let col = self.col;
                let row = self.row;
                let intc = bary0.simd_gez() & bary1.simd_gez() & bary2.simd_gez();
                let bary0 = f32x4::from_array([bary0[0], bary0[0], bary0[2], bary0[1]]);
                let bary1 = f32x4::from_array([bary1[0], bary1[0], bary1[2], bary1[1]]);
                let bary2 = f32x4::from_array([bary2[0], bary2[0], bary2[2], bary2[1]]);
                let inc0 = f32x4::from_array([hinc0, vinc0, hinc0, vinc0]);
                let inc1 = f32x4::from_array([hinc1, vinc1, hinc1, vinc1]);
                let inc2 = f32x4::from_array([hinc2, vinc2, hinc2, vinc2]);
                let offset0 = bary0 * -inc0.fast_recip();
                let offset1 = bary1 * -inc1.fast_recip();
                let offset2 = bary2 * -inc2.fast_recip();
                let ptl = f32x4::from_array([ptx[0], pty[0], ptx[2], pty[1]]);
                let ptr = f32x4::from_array([ptx[0], pty[0], ptx[1], pty[2]]);
                let int0 = ptl + offset0;
                let int1 = ptl + offset1;
                let int2 = ptl + offset2;
                let (x0, y0) = int0.deinterleave(ptr);
                let y0 = y0.rotate_lanes_right::<2>();
                let (x1, y1) = int1.deinterleave(ptr);
                let y1 = y1.rotate_lanes_right::<2>();
                let (x2, y2) = int2.deinterleave(ptr);
                let y2 = y2.rotate_lanes_right::<2>();
                let tminx = f32x4::splat(tmin[0]);
                let tminy = f32x4::splat(tmin[1]);
                let tmaxx = f32x4::splat(tmax[0]);
                let tmaxy = f32x4::splat(tmax[1]);
                let vx0 = f32x4::splat(vert0.proj[0]);
                let vy0 = f32x4::splat(vert0.proj[1]);
                let vx1 = f32x4::splat(vert1.proj[0]);
                let vy1 = f32x4::splat(vert1.proj[1]);
                let vx2 = f32x4::splat(vert2.proj[0]);
                let vy2 = f32x4::splat(vert2.proj[1]);
                let valid0 = x0.simd_ge(tminx) & x0.simd_le(tmaxx) & y0.simd_ge(tminy) & y0.simd_le(tmaxy);
                let valid0 = valid0 & x0.simd_ge(vx1.simd_min(vx2)) & x0.simd_le(vx1.simd_max(vx2));
                let valid0 = valid0 & y0.simd_ge(vy1.simd_min(vy2)) & y0.simd_le(vy1.simd_max(vy2));
                let valid1 = x1.simd_ge(tminx) & x1.simd_le(tmaxx) & y1.simd_ge(tminy) & y1.simd_le(tmaxy);
                let valid1 = valid1 & x1.simd_ge(vx2.simd_min(vx0)) & x1.simd_le(vx2.simd_max(vx0));
                let valid1 = valid1 & y1.simd_ge(vy2.simd_min(vy0)) & y1.simd_le(vy2.simd_max(vy0));
                let valid2 = x2.simd_ge(tminx) & x2.simd_le(tmaxx) & y2.simd_ge(tminy) & y2.simd_le(tmaxy);
                let valid2 = valid2 & x2.simd_ge(vx0.simd_min(vx1)) & x2.simd_le(vx0.simd_max(vx1));
                let valid2 = valid2 & y2.simd_ge(vy0.simd_min(vy1)) & y2.simd_le(vy0.simd_max(vy1));
                let nan = f32x4::splat(f32::NAN);
                let x0 = valid0.select(x0, nan);
                let y0 = valid0.select(y0, nan);
                let x1 = valid1.select(x1, nan);
                let y1 = valid1.select(y1, nan);
                let x2 = valid2.select(x2, nan);
                let y2 = valid2.select(y2, nan);
                let x3 = intc.select(ptx, nan);
                let y3 = intc.select(pty, nan);
                let x = f32x4::from_array([vert0.proj[0], vert1.proj[0], vert2.proj[0], f32::NAN]);
                let y = f32x4::from_array([vert0.proj[1], vert1.proj[1], vert2.proj[1], f32::NAN]);
                let tminx = f32x4::splat(tmin[0]);
                let tminy = f32x4::splat(tmin[1]);
                let tmaxx = f32x4::splat(tmax[0]);
                let tmaxy = f32x4::splat(tmax[1]);
                let inside = x.simd_ge(tminx) & x.simd_le(tmaxx) & y.simd_ge(tminy) & y.simd_le(tmaxy);
                let x = inside.select(x, nan);
                let y = inside.select(y, nan);
                let xmin = x.reduce_min();
                let ymin = y.reduce_min();
                let xmax = x.reduce_max();
                let ymax = y.reduce_max();
                let tcol = (x0.simd_min(x1)
                              .simd_min(x2)
                              .simd_min(x3)
                              .reduce_min()
                              .min(xmin)
                              .max(tmin[0])
                              .min(tmax[0]) as usize
                            - col)
                           & !0x1;
                let trow = (y0.simd_min(y1)
                              .simd_min(y2)
                              .simd_min(y3)
                              .reduce_min()
                              .min(ymin)
                              .max(tmin[1])
                              .min(tmax[1]) as usize
                            - row)
                           & !0x1;
                let tcolmax = (x0.simd_max(x1)
                                 .simd_max(x2)
                                 .simd_max(x3)
                                 .reduce_max()
                                 .max(xmax)
                                 .max(tmin[0])
                                 .min(tmax[0]) as usize
                               - col
                               + 2)
                              & !0x1;
                let trowmax = (y0.simd_max(y1)
                                 .simd_max(y2)
                                 .simd_max(y3)
                                 .reduce_max()
                                 .max(ymax)
                                 .max(tmin[1])
                                 .min(tmax[1]) as usize
                               - row
                               + 2)
                              & !0x1;
                (tcol, trow, tcolmax, trowmax)
            };
        // Compute the starting barycentric coordinates and adjust the increments.
        let ftcol = tcol as f32;
        let ftrow = trow as f32;
        let bary0 = bary0[0] + hinc0 * ftcol + vinc0 * ftrow;
        let bary1 = bary1[0] + hinc1 * ftcol + vinc1 * ftrow;
        let bary2 = bary2[0] + hinc2 * ftcol + vinc2 * ftrow;
        let bary0 = f32x4::from_array([bary0, bary0 + hinc0, bary0 + vinc0, bary0 + hinc0 + vinc0]);
        let bary1 = f32x4::from_array([bary1, bary1 + hinc1, bary1 + vinc1, bary1 + hinc1 + vinc1]);
        let bary2 = f32x4::from_array([bary2, bary2 + hinc2, bary2 + vinc2, bary2 + hinc2 + vinc2]);
        let hinc0 = f32x4::splat(hinc0 + hinc0);
        let vinc0 = f32x4::splat(vinc0 + vinc0);
        let hinc1 = f32x4::splat(hinc1 + hinc1);
        let vinc1 = f32x4::splat(vinc1 + vinc1);
        let hinc2 = f32x4::splat(hinc2 + hinc2);
        let vinc2 = f32x4::splat(vinc2 + vinc2);
        // Declare some useful values that will hopefully will be kept in registers by
        // the optimizer.
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
        let project = vert0.proj[3] != vert1.proj[3] || vert0.proj[3] != vert2.proj[3];
        // Loop over all the fragments in the tile in groups of 2x2, and shade those
        // that belong to the triangle.
        let mut vbary0 = bary0;
        let mut vbary1 = bary1;
        let mut vbary2 = bary2;
        for trow in (trow .. trowmax).step_by(2) {
            let mut hbary0 = vbary0;
            let mut hbary1 = vbary1;
            let mut hbary2 = vbary2;
            for tcol in (tcol .. tcolmax).step_by(2) {
                // Validate only the fragments inside the triangle.
                let mut valid = hbary0.simd_gtz() & hbary1.simd_gtz() & hbary2.simd_gtz();
                // Include half of the edges.
                if (hbary0.simd_eqz() | hbary1.simd_eqz() | hbary2.simd_eqz()).any() {
                    valid |= hbary0.simd_eqz() & (hinc0.simd_ltz() | hinc0.simd_eqz() & vinc0.simd_ltz());
                    valid |= hbary1.simd_eqz() & (hinc1.simd_ltz() | hinc1.simd_eqz() & vinc1.simd_ltz());
                    valid |= hbary2.simd_eqz() & (hinc2.simd_ltz() | hinc2.simd_eqz() & vinc2.simd_ltz());
                }
                if !valid.any() {
                    // All fragments were invalidated.
                    hbary0 += hinc0;
                    hbary1 += hinc1;
                    hbary2 += hinc2;
                    continue;
                }
                let (bary0, bary1, bary2) = if project {
                    // Compute the perspective-correct barycentric coordinates.
                    let w0 = bary0.mul_lane::<3>(vert0.proj);
                    let w1 = bary1.mul_lane::<3>(vert1.proj);
                    let w2 = bary2.mul_lane::<3>(vert2.proj);
                    let itotal = (w0 + w1 + w2).fast_recip();
                    let bary0 = w0 * itotal;
                    let bary1 = w1 * itotal;
                    let bary2 = w2 * itotal;
                    (bary0, bary1, bary2)
                } else {
                    // Affine projection.
                    (hbary0, hbary1, hbary2)
                };
                // Offset for these 4 fragments in the tile buffers.
                let offset = (trow >> 1) * (twidth << 1) + (tcol << 1);
                // Compute the depth and exclude all fragments outside the range between the
                // values in the depth buffer and the near clipping plane.
                let db = unsafe { self.db.0.as_mut_ptr().add(offset).cast::<u16x4>() };
                let odepth = unsafe { db.read() };
                let z = bary0.mul_lane::<2>(vert0.proj);
                let z = z.fused_mul_add_lane::<2>(bary1, vert1.proj);
                let z = z.fused_mul_add_lane::<2>(bary2, vert2.proj);
                valid &= z.simd_le(one);
                let zb = z.to_bits().saturating_sub(dxb);
                let zx = (zb & dxm) >> ds;
                let zm = (zb & dmm) >> ds;
                let depth = (zx | zm).cast::<u16>();
                let mut valid = valid.cast::<i16>();
                valid &= depth.simd_gt(odepth);
                if !valid.any() {
                    // All the remaining fragments were invalidated by the depth test.
                    hbary0 += hinc0;
                    hbary1 += hinc1;
                    hbary2 += hinc2;
                    continue;
                }
                // Store the new depth values.
                let depth = valid.select(depth, odepth);
                unsafe { db.write(depth) };
                // Apply shading.
                let cb = unsafe { self.cb.0.as_mut_ptr().add(offset).cast::<u16x4>() };
                let ocolor = unsafe { cb.read() };
                let red = bary0.mul_lane::<0>(vert0.proj);
                let red = red.fused_mul_add_lane::<0>(bary1, vert1.proj);
                let red = red.fused_mul_add_lane::<0>(bary2, vert2.proj);
                let red = red.simd_max(zero).simd_min(one);
                let green = bary0.mul_lane::<1>(vert0.proj);
                let green = green.fused_mul_add_lane::<1>(bary1, vert1.proj);
                let green = green.fused_mul_add_lane::<1>(bary2, vert2.proj);
                let green = green.simd_max(zero).simd_min(one);
                let blue = bary0.mul_lane::<2>(vert0.proj);
                let blue = blue.fused_mul_add_lane::<2>(bary1, vert1.proj);
                let blue = blue.fused_mul_add_lane::<2>(bary2, vert2.proj);
                let blue = blue.simd_max(zero).simd_min(one);
                // Compute the RGB565 color values.
                let red = (red * rbmul).cast::<u32>() << rshift;
                let green = (green * gmul).cast::<u32>() << gshift;
                let blue = (blue * rbmul).cast::<u32>();
                let color = (red | green | blue).cast::<u16>();
                let color = valid.select(color, ocolor);
                unsafe { cb.write(color) };
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
        let buf = if self.fb.frame() & 0x1 == 1 {
            self.fb.fb0
        } else {
            self.fb.fb1
        };
        let buf = unsafe { buf.add(self.row * self.fb.width + self.col) };
        let eindices = usizex8::from_array([0, 1, 4, 5, 8, 9, 12, 13]);
        let oindices = usizex8::from_array([2, 3, 6, 7, 10, 11, 14, 15]);
        let black = u16x8::splat(0);
        for trow in 0 .. self.fb.theight {
            let indices = if trow & 0x1 == 0 { eindices } else { oindices };
            let buf = unsafe { buf.add(trow * self.fb.width) };
            for tcol in (0 .. self.fb.twidth).step_by(8) {
                let offset = usizex8::splat((self.fb.twidth << 1) * (trow >> 1) + (tcol << 1));
                let indices = indices + offset;
                let color = u16x8::gather_or(&self.cb.0[..], indices, black);
                unsafe { buf.add(tcol).cast::<u16x8>().write(color) };
            }
        }
        self.fb.tfinished.fetch_add(1, Ordering::Relaxed);
    }
}
