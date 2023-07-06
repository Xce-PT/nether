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

mod fb;
mod geom;

use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use core::task::{Context, Poll, Waker};

pub use self::fb::{FrameBuffer, Vertex as ProjectedVertex};
pub use self::geom::*;
use crate::cpu::COUNT as CPU_COUNT;
use crate::math::{Angle, Projection, Transform, Vector};
use crate::pixvalve::PIXVALVE;
use crate::sched::SCHED;
use crate::sync::{Lazy, Lock, RwLock};
use crate::{mbox, PERRY_RANGE};

/// Screen width in pixels.
const SCREEN_WIDTH: usize = 800;
/// Screen height in pixels.
const SCREEN_HEIGHT: usize = 480;
/// Pixel depth in bytes.
const DEPTH: usize = 2;
/// Horizontal pitch in bytes.
const PITCH: usize = SCREEN_WIDTH * DEPTH;
/// Vertical pitch in rows.
const VPITCH: usize = 1;
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
/// Plane image type RGB565 setting.
const IMG_RGB565_TYPE: u8 = 1;
/// Image transformation (bit0 = 180 degree rotation, bit 16 = X flip, bit 17 =
/// Y flip).
const IMG_TRANSFORM: u32 = 0x20000;

/// Global video driver instance.
pub static VIDEO: Lazy<Video> = Lazy::new(Video::new);

/// Video driver.
pub struct Video
{
    /// Frame buffer.
    fb: FrameBuffer,
    /// Current frame buffer address.
    cfb: AtomicU32,
    /// Whether this frame has been commited.
    did_commit: AtomicBool,
    /// Current frame.
    frame: AtomicU64,
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
    /// RGBA Color.
    color: Vector,
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
    /// Projected vertices.
    proj: Vec<ProjectedVertex>,
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
        let fb = FrameBuffer::new(SCREEN_WIDTH, SCREEN_HEIGHT);
        let cfb = fb.vsync();
        let plane_in = SetPlaneProperty { display_id: LCD_DISP_ID,
                                          plane_id: 0,
                                          img_type: IMG_RGB565_TYPE,
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
                                          planes: [cfb, 0x0, 0x0, 0x0],
                                          transform: IMG_TRANSFORM };
        mbox! {SET_PLANE_TAG: plane_in => _};
        PIXVALVE.register_vsync(Self::vsync);
        Self { fb,
               cfb: AtomicU32::new(cfb + ((PITCH * VPITCH * (SCREEN_HEIGHT - 1)) as u32)),
               did_commit: AtomicBool::new(false),
               frame: AtomicU64::new(0),
               waiters: Lock::new(Vec::new()),
               cmds: RwLock::new(Vec::new()) }
    }

    /// Adds a draw command to the queue.
    ///
    /// * `verts`: Triangle vertices to draw.
    /// * `cam`: Camera to world transformation.
    /// * `proj`: Projection transformation.
    pub fn draw_triangles(&self, verts: &[Vertex], mdl: Transform, cam: Transform, fov: Angle)
    {
        let proj = Projection::new_perspective(SCREEN_WIDTH, SCREEN_HEIGHT, fov);
        let proj = proj.into_matrix();
        let view = cam.recip().into_matrix();
        let mdl = mdl.into_matrix();
        let mdlviewproj = mdl * view * proj;
        let map = |vert: &Vertex| {
            let mut proj = vert.pos * mdlviewproj;
            let recip = proj[3].recip();
            proj *= recip;
            proj[3] = recip;
            ProjectedVertex { proj: proj.into_intrinsic(),
                              color: vert.color.into_intrinsic() }
        };
        let proj = Vec::from_iter(verts.iter().map(map));
        let cmd = Command { proj };
        self.cmds.wlock().push(cmd);
    }

    /// Commits all the commands added to the queue, drawing them to the
    /// frame buffer.
    ///
    /// Returns a future that, when awaited, blocks the task until the next
    /// vertical synchronization event after drawing everything.
    pub async fn commit(&'static self)
    {
        let frame = self.frame.load(Ordering::Relaxed);
        if self.did_commit.swap(true, Ordering::Relaxed) {
            let vsync = VerticalSync::new(frame);
            vsync.await;
            return;
        }
        let tasks = <[(); CPU_COUNT]>::map([(); CPU_COUNT], |_| SCHED.spawn(self.draw()));
        for task in tasks {
            task.await;
        }
        self.cmds.wlock().clear();
        let vsync = VerticalSync::new(frame);
        vsync.await;
    }

    /// Draws tiles to the frame buffer.
    async fn draw(&self)
    {
        for mut tile in self.fb.tiles() {
            let cmds = self.cmds.rlock();
            for cmd in cmds.iter() {
                for proj in cmd.proj.iter().array_chunks::<3>() {
                    tile.draw_triangle(*proj[0], *proj[1], *proj[2]);
                }
            }
        }
    }

    /// Flips the frame buffers and reinitializes the frame drawing cycle.
    fn vsync()
    {
        if VIDEO.frame.load(Ordering::Relaxed) == VIDEO.fb.frame() {
            return;
        }
        let cfb = VIDEO.cfb.load(Ordering::Relaxed);
        let ofb = VIDEO.fb.vsync();
        // Frame buffer pointers must point at the beginning of the last row instead of
        // the first because we are telling the HVS to draw with the Y axis flipped.
        let ofb = ofb + ((PITCH * VPITCH * (SCREEN_HEIGHT - 1)) as u32);
        if ofb == cfb {
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
                        if fb == cfb || fb == ofb {
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
            VIDEO.cfb.store(ofb, Ordering::Relaxed);
            unsafe { HVS_DISPLIST_BUF.add(idx).write_volatile(ofb) };
        }
        VIDEO.did_commit.store(false, Ordering::SeqCst);
        VIDEO.frame.store(VIDEO.fb.frame(), Ordering::SeqCst);
        let mut waiters = VIDEO.waiters.lock();
        waiters.iter().for_each(|waker| waker.wake_by_ref());
        waiters.clear();
    }
}

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
