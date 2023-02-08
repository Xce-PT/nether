//! Video core driver and rendering.
//!
//! Documentation:
//!
//! * [Mailbox property interface](https://github.com/raspberrypi/firmware/wiki/Mailbox-property-interface)
//!
//! Since there's no documented support for double-buffering, the pan property
//! tag is being used to move the display to the top of the frame buffer every
//! even frame and to the bottom of the frame buffer every odd frame.

extern crate alloc;

mod geom;

use alloc::vec::Vec;
use core::future::Future;
use core::mem::{align_of, size_of_val};
use core::pin::Pin;
use core::simd::u32x4;
use core::sync::atomic::{fence, AtomicBool, AtomicU64, AtomicUsize, Ordering};
use core::task::{Context, Poll, Waker};

pub use self::geom::*;
use crate::math::{Color, Matrix, Projector, Triangulation, Vector};
use crate::pixvalve::PIXVALVE;
use crate::sched::SCHED;
use crate::sync::{Lazy, Lock, RwLock};
use crate::{from_dma, mbox, CPU_COUNT};

/// Screen width.
const SCREEN_WIDTH: usize = 800;
/// Screen height.
const SCREEN_HEIGHT: usize = 480;
/// Tile width.
const TILE_WIDTH: usize = 16;
/// Tile height.
const TILE_HEIGHT: usize = 16;
/// Allocate frame buffer property tag.
const ALLOC_FB_TAG: u32 = 0x40001;
/// Set physical display dimensions property tag.
const SET_DIM_TAG: u32 = 0x48003;
/// Set virtual display dimensions property tag.
const SET_VDIM_TAG: u32 = 0x48004;
/// Set color depth property tag.
const SET_DEPTH_TAG: u32 = 0x48005;
/// Get horizontal pitch property tag.
const GET_PITCH_TAG: u32 = 0x40008;
/// Set position inside the virtual buffer property tag.
const SET_POS_TAG: u32 = 0x48009;

/// Global video driver instance.
pub static VIDEO: Lazy<Video> = Lazy::new(Video::new);

/// Video driver.
#[derive(Debug)]
pub struct Video
{
    /// Frame buffer base.
    base: *mut u32x4,
    /// Horizontal pitch.
    pitch: usize,
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

impl Video
{
    /// Creates and initializes a new video driver instance.
    ///
    /// Returns the newly created instance.
    fn new() -> Self
    {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        struct Dim(u32, u32);
        #[repr(C)]
        #[derive(Clone, Copy, Debug)]
        struct AllocOut(u32, u32);
        let dim_in = Dim(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32);
        let vdim_in = Dim(SCREEN_WIDTH as u32, 2 * SCREEN_HEIGHT as u32);
        let depth_in = 32u32;
        let alloc_in = align_of::<u32x4>() as u32;
        let dim_out: Dim;
        let vdim_out: Dim;
        let depth_out: u32;
        let pitch_out: u32;
        let alloc_out: AllocOut;
        mbox! {
            SET_DIM_TAG: dim_in => dim_out,
            SET_VDIM_TAG: vdim_in => vdim_out,
            SET_DEPTH_TAG: depth_in => depth_out,
            GET_PITCH_TAG: _ => pitch_out,
            ALLOC_FB_TAG: alloc_in => alloc_out,
        };
        assert!(dim_out == dim_in,
                "Returned and requested display dimensions do not match");
        assert!(vdim_out == vdim_in,
                "Returned and requested virtual display dimensions do not match");
        assert!(depth_out == depth_in, "Returned and requested depths do not match");
        assert!(pitch_out > 0, "Invalid pitch");
        let base = alloc_out.0 as usize;
        let base = from_dma(base);
        let size = alloc_out.1 as usize;
        assert!(size >= SCREEN_WIDTH * SCREEN_HEIGHT * 4,
                "Frame buffer size does not meet the minimum requirement for its dimensions");
        PIXVALVE.register_vsync(Self::vsync);
        Self { base: base as _,
               pitch: pitch_out as usize,
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
        PIXVALVE.ack_vsync();
        let vsync = VerticalSync::new(self.frame.load(Ordering::Relaxed));
        vsync.await;
    }

    /// Draws tiles to the frame buffer.
    async fn draw(&self)
    {
        let base = if self.frame.load(Ordering::Relaxed) & 0x1 == 0 {
            unsafe {
                self.base
                    .add(SCREEN_HEIGHT * self.pitch / size_of_val(self.base.as_ref().unwrap()))
            }
        } else {
            self.base
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
            let row = (SCREEN_HEIGHT - TILE_HEIGHT) - tile * TILE_WIDTH / SCREEN_WIDTH * TILE_HEIGHT;
            let offset = row * self.pitch + col * 4;
            let base = unsafe { base.add(offset / size_of_val(base.as_ref().unwrap())) };
            for row in 0 .. TILE_HEIGHT {
                for col in (0 .. TILE_WIDTH).step_by(4) {
                    let pix0 = colors[row * TILE_WIDTH + col].to_u32();
                    let pix1 = colors[row * TILE_WIDTH + col + 1].to_u32();
                    let pix2 = colors[row * TILE_WIDTH + col + 2].to_u32();
                    let pix3 = colors[row * TILE_WIDTH + col + 3].to_u32();
                    let pixgrp = u32x4::from([pix0, pix1, pix2, pix3]);
                    let offset = (TILE_HEIGHT - row - 1) * self.pitch + col * 4;
                    let base = unsafe { base.add(offset / size_of_val(base.as_ref().unwrap())) };
                    unsafe { base.write(pixgrp) };
                }
            }
        }
    }

    /// Flips the frame buffers.
    fn vsync()
    {
        if VIDEO.tile.load(Ordering::Relaxed) != 0 {
            return;
        }
        let frame = VIDEO.frame.fetch_add(1, Ordering::Relaxed);
        if frame != 0 {
            #[repr(C)]
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            struct Pos(u32, u32);
            let y = (SCREEN_HEIGHT * (frame as usize & 0x1)) as _;
            let pos_in = Pos(0, y);
            let pos_out: Pos;
            mbox! {SET_POS_TAG: pos_in => pos_out};
            assert!(pos_out == pos_in, "Returned and requested positions do not match");
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
