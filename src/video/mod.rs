//! Video core driver and rendering.
//!
//! This driver makes use of the set plane mailbox property to configure the
//! Hardware Video Scaler and provides a double frame buffer outside of the
//! video core's reserved memory.  Since even this interface does not support
//! setting up double buffering I'm directly driving the Hardware Video Scaler
//! on vertical synchronization events.
//!
//! My sources of information are the librerpi/rpi-open-firmware project's
//! documentation [1] and the Linux kernel [2][3][4][5][6].
//!
//! [1]: https://github.com/librerpi/rpi-open-firmware/blob/master/docs/hvs.md
//! [2]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/gpu/drm/vc4/vc4_firmware_kms.c
//! [3]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/gpu/drm/vc4/vc4_plane.c
//! [4]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/gpu/drm/vc4/vc4_regs.h
//! [5]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/drivers/gpu/drm/vc4/vc_image_types.h
//! [6]: https://github.com/raspberrypi/linux/blob/rpi-5.15.y/include/soc/bcm2835/raspberrypi-firmware.h

extern crate alloc;

mod geom;

use alloc::vec::Vec;
use core::alloc::{Allocator, Layout};
use core::future::Future;
use core::mem::align_of;
use core::pin::Pin;
use core::simd::u32x4;
use core::sync::atomic::{fence, AtomicBool, AtomicU64, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};

pub use self::geom::*;
use crate::alloc::{Alloc, UNCACHED_REGION};
use crate::cpu::COUNT as CPU_COUNT;
use crate::math::{Color, Matrix, Projector, Triangulation, Vector};
use crate::pixvalve::PIXVALVE;
use crate::sched::SCHED;
use crate::sync::{Lazy, Lock, RwLock};
use crate::{mbox, to_dma, PERRY_RANGE};

/// Screen width in pixels.
const SCREEN_WIDTH: usize = 800;
/// Screen height in pixels.
const SCREEN_HEIGHT: usize = 480;
/// Pixel depth in bytes.
const DEPTH: usize = 4;
/// Horizontal pitch in bytes.
const PITCH: usize = SCREEN_WIDTH * DEPTH;
/// Vertical pitch in rows.
const VPITCH: usize = 1;
/// Tile width in pixels (must be multiple of 16 / DEPTH).
const TILE_WIDTH: usize = 16;
/// Tile height in pixels.
const TILE_HEIGHT: usize = 16;
/// Set plane property tag.
const SET_PLANE_TAG: u32 = 0x48015;
/// Hardware video scaler base address.
const HVS_BASE: usize = PERRY_RANGE.start + 0x2400000;
/// Hardware video scaler display list register 0.
const HVS_DISPLIST0: *const u32 = (HVS_BASE + 0x20) as _;
/// Hardware video scaler display list buffer.
const HVS_DISPLIST_BUF: *mut u32 = (HVS_BASE + 0x4000) as _;
/// Main LCD display ID.
const LCD_DISP_ID: u8 = 0;
/// Plane image type XRGB with 8 bits per channel setting.
const IMG_XRGB8888_TYPE: u8 = 44;
/// Image transformation (bit0 = 180 degree rotation, bit 16 = X flip, bit 17 =
/// Y flip).
const IMG_TRANSFORM: u32 = 0x20000;

/// Global video driver instance.
pub static VIDEO: Lazy<Video> = Lazy::new(Video::new);

/// DMA allocator.
static UNCACHED: Alloc<0x10> = Alloc::with_region(&UNCACHED_REGION);

/// Video driver.
#[derive(Debug)]
pub struct Video
{
    /// Frame buffer 0 base.
    fb0: *mut u32x4,
    /// Frame buffer 1 base.
    fb1: *mut u32x4,
    /// Frame counter.
    frame: AtomicU64,
    /// Whether this frame has been commited.
    did_commit: AtomicBool,
    /// Current tile index.
    tile: AtomicUsize,
    /// VSync waiters.
    waiters: Lock<Vec<Waker>>,
    /// Command queue.
    cmds: RwLock<Vec<Command>>,
}

/// Visual vertex.
#[derive(Clone, Copy, Debug)]
pub struct Vertex
{
    /// Position.
    pos: Vector,
    /// Color.
    color: Color,
}

/// Vertical sync future.
#[derive(Debug)]
struct VerticalSync
{
    /// ID of the frame when this future was created.
    frame: u64,
}

/// Draw command.
#[derive(Debug)]
struct Command
{
    /// Triangle vertices.
    tris: Vec<Vertex>,
    /// View transformation.
    view: Matrix,
    /// Projection transformation.
    proj: Projector,
}

/// Set plane property.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct SetPlaneProperty
{
    // Display ID (0 for main LCD).
    display_id: u8,
    /// Plane ID.
    plane_id: u8,
    /// Image type.
    img_type: u8,
    /// Display layer.
    layer: i8,
    /// Physical width.
    width: u16,
    /// Physical height.
    height: u16,
    /// Physical horizontal pitch (in bytes).
    pitch: u16,
    /// Physical vertical pitch (in rows).
    vpitch: u16,
    /// Horizontal offset into the source image (16.16 fixed point).
    src_x: u32,
    /// Vertical offset into the source image (16.16 fixed point).
    src_y: u32,
    /// Width of the source image (16.16 fixed point).
    src_w: u32,
    /// Height of the source image (16.16 fixed point).
    src_h: u32,
    /// Horizontal offset into the destination image.
    dst_x: i16,
    /// Vertical offset into the destination image.
    dst_y: i16,
    /// Width of the destination image.
    dst_w: u16,
    /// Height of the destination image.
    dst_h: u16,
    /// Opacity.
    alpha: u8,
    /// Number of subplanes comprising this plane (always 1 as other subplanes
    /// are used for composite formats).
    num_planes: u8,
    /// Whether this is a composite video plane (always 0).
    is_vu: u8,
    /// Color encoding (only relevant for composite video planes).
    color_encoding: u8,
    /// DMA addresses of the planes counted in `num_planes`.
    planes: [u32; 4],
    /// Rotation and / or flipping constant.
    transform: u32,
}

impl Video
{
    /// Creates and initializes a new video driver instance.
    ///
    /// Returns the newly created instance.
    fn new() -> Self
    {
        let layout = Layout::from_size_align(PITCH * VPITCH * SCREEN_HEIGHT, align_of::<u32x4>()).unwrap();
        let fb0 = UNCACHED.allocate_zeroed(layout)
                          .expect("Failed to allocate memory for the frame buffer")
                          .as_mut_ptr()
                          .cast::<u32x4>();
        let fb1 = UNCACHED.allocate_zeroed(layout)
                          .expect("Failed to allocate memory for the frame buffer")
                          .as_mut_ptr()
                          .cast::<u32x4>();
        let plane_in = SetPlaneProperty { display_id: LCD_DISP_ID,
                                          plane_id: 0,
                                          img_type: IMG_XRGB8888_TYPE,
                                          layer: 0,
                                          width: SCREEN_WIDTH as _,
                                          height: SCREEN_HEIGHT as _,
                                          pitch: PITCH as _,
                                          vpitch: VPITCH as _,
                                          src_x: 0,
                                          src_y: 0,
                                          src_w: (SCREEN_WIDTH << 16) as _,
                                          src_h: (SCREEN_HEIGHT << 16) as _,
                                          dst_x: 0,
                                          dst_y: 0,
                                          dst_w: SCREEN_WIDTH as _,
                                          dst_h: SCREEN_HEIGHT as _,
                                          alpha: 0xFF,
                                          num_planes: 1,
                                          is_vu: 0,
                                          color_encoding: 0,
                                          planes: [to_dma(fb0 as _) as _, 0x0, 0x0, 0x0],
                                          transform: IMG_TRANSFORM };
        mbox! {SET_PLANE_TAG: plane_in => _};
        PIXVALVE.register_vsync(Self::vsync);
        Self { fb0,
               fb1,
               frame: AtomicU64::new(0),
               did_commit: AtomicBool::new(false),
               tile: AtomicUsize::new(0),
               waiters: Lock::new(Vec::new()),
               cmds: RwLock::new(Vec::new()) }
    }

    /// Adds a draw command to the queue.
    ///
    /// * `tris`: Triangles to draw.
    /// * `lights`: Lights potentially illuminating the triangles.
    /// * `mdl`: Model to world transformation.
    /// * `cam`: Camera to world transformation.
    /// * `proj`: Projection transformation.
    pub fn enqueue(&self, tris: &[Vertex], mdl: Matrix, cam: Matrix, proj: Projector)
    {
        let view = cam.recip();
        let mut ttris = Vec::with_capacity(tris.len());
        for vert in tris {
            let mut vert = *vert;
            vert.pos = mdl * vert.pos;
            ttris.push(vert);
        }
        let cmd = Command { tris: ttris,
                            view,
                            proj };
        self.cmds.wlock().push(cmd);
    }

    /// Commits all the commands added to the queue, drawing them to the
    /// off-screen buffer.
    ///
    /// Returns a future that, when awaited, blocks the task until the next
    /// vertical synchronization event after drawing everything.
    pub async fn commit(&'static self)
    {
        if self.did_commit.swap(true, Ordering::Relaxed) {
            let vsync = VerticalSync::new(self.frame.load(Ordering::Relaxed));
            vsync.await;
            return;
        }
        let tasks = <[(); CPU_COUNT]>::map([(); CPU_COUNT], |_| SCHED.spawn(self.draw()));
        for task in tasks {
            task.await;
        }
        self.cmds.wlock().clear();
        self.tile.store(0, Ordering::Relaxed);
        let vsync = VerticalSync::new(self.frame.load(Ordering::Relaxed));
        vsync.await;
    }

    /// Draws tiles to the frame buffer.
    async fn draw(&self)
    {
        // Draw to the off-screen buffer.
        let base = if self.frame.load(Ordering::Relaxed) & 0x1 == 0 {
            self.fb1
        } else {
            self.fb0
        };
        let cmds = self.cmds.rlock();
        let tw = TILE_WIDTH as f32;
        let th = TILE_HEIGHT as f32;
        loop {
            let tile = self.tile.fetch_add(1, Ordering::Relaxed);
            if tile >= SCREEN_WIDTH * SCREEN_HEIGHT / (TILE_WIDTH * TILE_HEIGHT) {
                fence(Ordering::Release);
                return;
            }
            let mut colors = [Color::default(); TILE_WIDTH * TILE_HEIGHT];
            for cmd in cmds.iter() {
                let mut tris = cmd.tris.iter().fuse();
                let proj = cmd.proj
                              .for_tile(SCREEN_WIDTH, SCREEN_HEIGHT, TILE_WIDTH, TILE_HEIGHT, tile)
                              .for_view(cmd.view);
                while let (Some(vert0), Some(vert1), Some(vert2)) = (tris.next(), tris.next(), tris.next()) {
                    let (vert0p, vert1p, vert2p) =
                        if let Some((v1, v2, v3)) = proj.project_tri(vert0.pos, vert1.pos, vert2.pos) {
                            (v1, v2, v3)
                        } else {
                            continue;
                        };
                    for row in 0 .. TILE_HEIGHT {
                        for col in 0 .. TILE_WIDTH {
                            let x = ((col * 2) as f32 - tw + 1.0) / tw;
                            let y = ((row * 2) as f32 - th + 1.0) / th;
                            let point = Vector::from_components(x, y, 0.0);
                            if let Some(triang) = Triangulation::from_point_triangle(point, vert0p, vert1p, vert2p) {
                                let color = triang.sample(vert0.color, vert1.color, vert2.color);
                                colors[row * TILE_WIDTH + col].blend_with(color);
                            }
                        }
                    }
                }
            }
            let col = tile * TILE_WIDTH % SCREEN_WIDTH;
            let row = tile * TILE_WIDTH / SCREEN_WIDTH * TILE_HEIGHT;
            let offset = row * PITCH * VPITCH + col * DEPTH;
            let base = unsafe { base.byte_add(offset) };
            for row in 0 .. TILE_HEIGHT {
                for col in (0 .. TILE_WIDTH).step_by(unsafe { (*base).lanes() }) {
                    let pix0 = colors[row * TILE_WIDTH + col].to_u32();
                    let pix1 = colors[row * TILE_WIDTH + col + 1].to_u32();
                    let pix2 = colors[row * TILE_WIDTH + col + 2].to_u32();
                    let pix3 = colors[row * TILE_WIDTH + col + 3].to_u32();
                    let pixgrp = u32x4::from([pix0, pix1, pix2, pix3]);
                    let offset = row * PITCH * VPITCH + col * DEPTH;
                    unsafe { base.byte_add(offset).write(pixgrp) };
                }
            }
        }
    }

    /// Flips the frame buffers and reinitializes the frame drawing cycle.
    fn vsync()
    {
        if VIDEO.tile.load(Ordering::Relaxed) != 0 {
            return;
        }
        // Frame buffer pointers must point at the beginning of the last row instead of
        // the first because we are telling the HVS to draw with the Y axis flipped.
        let fb0 = (to_dma(VIDEO.fb0 as _) + PITCH * VPITCH * (SCREEN_HEIGHT - 1)) as u32;
        let fb1 = (to_dma(VIDEO.fb1 as _) + PITCH * VPITCH * (SCREEN_HEIGHT - 1)) as u32;
        // Look for the index of the frame buffer pointers in the HVS display list
        // buffer.  This should only loop a lot when the firmware configuration changes,
        // after that it should find the index to update very quickly.
        let idx = 'outer: loop {
            let mut idx = unsafe { HVS_DISPLIST0.read_volatile() as usize };
            'inner: loop {
                let ctrl = unsafe { HVS_DISPLIST_BUF.add(idx).read_volatile() };
                // Look for a plane with unity scaling.
                if ctrl >> 15 & 0x1 != 0 {
                    // Check whether this plane contains one of our frame buffers.
                    let fb = unsafe { HVS_DISPLIST_BUF.add(idx + 5).read_volatile() };
                    if fb == fb0 || fb == fb1 {
                        // Found the index to update.
                        break 'outer idx + 5;
                    }
                }
                // Check whether this is an end control word.
                if ctrl >> 30 == 0x2 {
                    break 'inner;
                }
                // Skip to the next plane.
                idx += (ctrl >> 24 & 0x3F) as usize;
            }
        };
        let frame = VIDEO.frame.fetch_add(1, Ordering::Relaxed);
        if frame & 0x1 == 0 {
            unsafe { HVS_DISPLIST_BUF.add(idx).write_volatile(fb0) };
        } else {
            unsafe { HVS_DISPLIST_BUF.add(idx).write_volatile(fb1) };
        }
        VIDEO.did_commit.store(false, Ordering::SeqCst);
        let mut waiters = VIDEO.waiters.lock();
        waiters.iter().for_each(|waker| waker.wake_by_ref());
        waiters.clear();
    }
}

unsafe impl Send for Video {}

unsafe impl Sync for Video {}

impl VerticalSync
{
    /// Creates and initializes a new vertical sync future.
    ///
    /// * `frame`: Current frame.
    ///
    /// Returns the newly created future.
    fn new(frame: u64) -> Self
    {
        Self { frame }
    }
}

impl Future for VerticalSync
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<()>
    {
        let frame = VIDEO.frame.load(Ordering::Relaxed);
        if frame != self.frame {
            return Poll::Ready(());
        }
        VIDEO.waiters.lock().push(ctx.waker().clone());
        Poll::Pending
    }
}
