//! PWM audio driver.
//!
//! This code interfaces with five distinct peripherals: the PWM, a channel of
//! the DMA controller, two GPIOs, a GP clock, and the interrupt controller.
//! Since all of these peripherals, except for the interrupt controller for
//! which I've already implemented a driver, are properly documented in the
//! BCM2711 peripherals datasheet [1], I didn't have to read Linux code for
//! once. !
//! [1]: https://datasheets.raspberrypi.com/bcm2711/bcm2711-peripherals.pdf

extern crate alloc;

use alloc::alloc::GlobalAlloc;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::future::Future;
use core::hint::spin_loop;
use core::pin::Pin;
use core::simd::prelude::*;
use core::sync::atomic::{fence, Ordering};
use core::task::{Context, Poll, Waker};

use crate::alloc::{Alloc, UNCACHED_REGION};
use crate::irq::IRQ;
use crate::prim::FloatExtra;
use crate::simd::SimdFloatExtra;
use crate::sync::{Lazy, Lock};
use crate::{to_dma, PERRY_RANGE};

/// Base address of the DMA channel.
const DMA_BASE: usize = PERRY_RANGE.start + 0x2007000;
/// Control and status register of the DMA channel.
const DMA_CHAN_CS: *mut u32 = (DMA_BASE + 0x100) as _;
/// Control block address register of the DMA channel.
const DMA_CHAN_CB: *mut u32 = (DMA_BASE + 0x104) as _;
/// Debug register of the DMA channel.
const DMA_CHAN_DBG: *mut u32 = (DMA_BASE + 0x120) as _;
/// DMA channel IRQ.
const DMA_CHAN_IRQ: u32 = 113;
/// Not sure what this register is supposed to be, but it must have a bit set in
/// order to enable DMA DREQs for the PWM.
const PACTL_CS: *mut u32 = (PERRY_RANGE.start + 0x2204E00) as _;
/// GPIO base address.
const GPIO_BASE: usize = PERRY_RANGE.start + 0x2200000;
/// GPIO select function register.
const GPIO_FSEL: *mut u32 = (GPIO_BASE + 0x10) as _;
/// GPIO pul-up pull-down register.
const GPIO_PUPD: *mut u32 = (GPIO_BASE + 0xEC) as _;
/// General purpose clock base address.
const GPCLK_BASE: usize = PERRY_RANGE.start + 0x2101000;
/// General purpose clock control register.
const GPCLK_CTL: *mut u32 = (GPCLK_BASE + 0xA0) as _;
/// General purpose clock divisor register.
const GPCLK_DIV: *mut u32 = (GPCLK_BASE + 0xA4) as _;
/// PWM base address.
const PWM_BASE: usize = PERRY_RANGE.start + 0x220C800;
/// PWM control register.
const PWM_CTL: *mut u32 = PWM_BASE as _;
/// PWM status register.
const PWM_STAT: *mut u32 = (PWM_BASE + 0x4) as _;
/// PWM DMA configuration.
const PWM_DMAC: *mut u32 = (PWM_BASE + 0x8) as _;
/// PWM range register for channel 0.
const PWM_RNG0: *mut u32 = (PWM_BASE + 0x10) as _;
/// PWM FIFO register.
const PWM_FIFO: *mut u32 = (PWM_BASE + 0x18) as _;
/// PWM range register for channel 1.
const PWM_RNG1: *mut u32 = (PWM_BASE + 0x20) as _;
/// Number of channels to sample.
const SMPL_CHAN_COUNT: usize = 2;
/// Number of audio samples per DMA buffer.
const SMPL_BUF_LEN: usize = 1600 * SMPL_CHAN_COUNT;
/// Sample bit depth.
const SMPL_DEPTH: usize = 10;
/// Sample rate.
const SMPL_RATE: u32 = 48000;
/// Clock rate.
const CLOCK_RATE: u32 = 54000000;
/// Maximum number of tones to process.
const POLYPHONY: usize = 8;

/// Audio driver instance.
pub static AUDIO: Lazy<Lock<Audio>> = Lazy::new(Audio::new);

/// Uncached memory allocator.
static UNCACHED: Alloc<0x40> = Alloc::with_region(&UNCACHED_REGION);

/// Audio driver.
pub struct Audio
{
    /// Audio buffer 0.
    ab0: Box<[u32; SMPL_BUF_LEN], Alloc<'static, 0x40>>,
    /// Audio buffer 1.
    ab1: Box<[u32; SMPL_BUF_LEN], Alloc<'static, 0x40>>,
    /// Time counter.
    time: u64,
    /// Scheduled tones (period, pan).
    tones: [(u32, f32); POLYPHONY],
    /// Tasks waiting to be awakened.
    waiters: Vec<Waker>,
    /// Whether the play tone commands have been committed.
    did_commit: bool,
    /// First control block's DMA address.
    cb: usize,
}

/// Future that that becomes ready at the next buffer swap.
#[derive(Debug)]
pub struct WillSwap
{
    /// Time at which this future was created.
    time: u64,
}

/// Control block.
#[repr(align(0x40), C)]
#[derive(Clone, Copy, Debug)]
struct ControlBlock
{
    /// Transfer information.
    ti: u32,
    /// Source DMA address.
    src: u32,
    /// Destination DMA address.
    dst: u32,
    /// Data length.
    len: u32,
    /// 2D mode stride.
    stride: u32,
    /// DMA address of the next control block.
    next: u32,
    /// Unused 0.
    _unused0: u32,
    /// Unused 1.
    _unused1: u32,
}

impl Audio
{
    /// Creates and initializes a new audio driver instance.
    ///
    /// Returns the newly created instance.
    fn new() -> Lock<Self>
    {
        IRQ.register(DMA_CHAN_IRQ, Self::refill);
        // Set up the GPIO.
        fence(Ordering::Acquire);
        unsafe {
            let val = GPIO_FSEL.read_volatile();
            GPIO_FSEL.write_volatile(val & 0xFFFFFFC0 | 0x24);
            let val = GPIO_PUPD.read_volatile();
            GPIO_PUPD.write_volatile(val & 0xFFF0FFFF);
        }
        fence(Ordering::Release);
        // Set up a general purpose clock.
        fence(Ordering::Acquire);
        unsafe {
            let val = GPCLK_CTL.read_volatile();
            GPCLK_CTL.write_volatile(val & 0xFFFFEF | 0x5A000000);
            while GPCLK_CTL.read_volatile() & 0x80 != 0 {
                spin_loop();
            }
            GPCLK_CTL.write_volatile(0x5A000001);
            GPCLK_DIV.write_volatile(0x5A002000);
            GPCLK_CTL.write_volatile(0x5A000011);
        }
        fence(Ordering::Release);
        // Set up the PWM.
        unsafe {
            PWM_CTL.write_volatile(0x2161);
            PWM_RNG0.write_volatile(CLOCK_RATE / SMPL_RATE / 2);
            PWM_RNG1.write_volatile(CLOCK_RATE / SMPL_RATE / 2);
            PWM_STAT.write_volatile(0x13C);
            PWM_DMAC.write_volatile(0x80000606);
            fence(Ordering::Release);
        }
        // Set up the DMA controller.
        let mut ab0 = Box::new_in([1 << (SMPL_DEPTH - 1); SMPL_BUF_LEN], UNCACHED);
        let mut ab1 = Box::new_in([1 << (SMPL_DEPTH - 1); SMPL_BUF_LEN], UNCACHED);
        let cb = ControlBlock { ti: 0x4010349,
                                src: 0,
                                dst: to_dma(PWM_FIFO as _) as _,
                                len: (SMPL_BUF_LEN * 4) as _,
                                stride: 0,
                                next: 0,
                                _unused0: 0,
                                _unused1: 0 };
        unsafe {
            let layout = Layout::new::<ControlBlock>();
            let cb0 = UNCACHED.alloc(layout).cast::<ControlBlock>();
            let cb1 = UNCACHED.alloc(layout).cast::<ControlBlock>();
            assert!(!cb0.is_null() && !cb1.is_null(),
                    "Failed to allocate uncached memory for the audio DMA control blocks");
            *cb0 = ControlBlock { next: to_dma(cb1 as _) as _,
                                  src: to_dma(ab0.as_mut_ptr() as _) as _,
                                  ..cb };
            *cb1 = ControlBlock { next: to_dma(cb0 as _) as _,
                                  src: to_dma(ab1.as_mut_ptr() as _) as _,
                                  ..cb };
            fence(Ordering::AcqRel);
            let val = PACTL_CS.read_volatile();
            PACTL_CS.write_volatile(val | 0x800000);
            fence(Ordering::Release);
            DMA_CHAN_CS.write_volatile(0x80000000);
            DMA_CHAN_DBG.write_volatile(0x7);
            DMA_CHAN_CB.write_volatile(to_dma(cb0 as _) as _);
            DMA_CHAN_CS.write_volatile(0xF70007);
            fence(Ordering::Release);
            let this = Self { ab0,
                              ab1,
                              time: 0,
                              tones: Default::default(),
                              waiters: Vec::new(),
                              did_commit: false,
                              cb: to_dma(cb0 as _) };
            Lock::new(this)
        }
    }

    /// Adds a tone to the command queue, ignoring it if maximum polyphony has
    /// already been reached.
    ///
    /// * `freq`: Frequency of the tone.
    /// * `pan`: Stereo pan.
    ///
    /// Panics if the frequency is 0.
    #[track_caller]
    pub fn play_tone(&mut self, freq: u16, pan: f32)
    {
        assert!(freq > 0, "Invalid zero frequency");
        for tone in self.tones.iter_mut() {
            if tone.0 == 0 {
                *tone = (SMPL_RATE / freq as u32, pan);
                break;
            }
        }
    }

    /// Commits all scheduled tones to be played at the next buffer swap.
    ///
    /// Returns a future that, when awaited on, blocks the task until the next
    /// buffer swap.
    pub fn commit(&mut self) -> WillSwap
    {
        let future = WillSwap::new(self.time);
        let ct = self.tones.iter().filter(|tone| tone.0 > 0).count();
        if self.did_commit || ct == 0 {
            return future;
        }
        let buf = if self.inactive_buffer() == 0 {
            &mut self.ab0[..]
        } else {
            &mut self.ab1[..]
        };
        let ict = f32x4::splat(ct as f32).fast_recip();
        let hamp = f32x4::splat((1 << (SMPL_DEPTH - 1)) as f32);
        let one = f32x4::splat(1.0);
        for time in (self.time .. self.time + (SMPL_BUF_LEN / SMPL_CHAN_COUNT) as u64).step_by(4) {
            let samples = self.tones
                              .iter()
                              .map(|tone| Self::compute_sample(time, tone.0))
                              .array_chunks::<POLYPHONY>()
                              .next()
                              .unwrap();
            let left = Self::pan_mix(&self.tones, samples, -1.0);
            let right = Self::pan_mix(&self.tones, samples, 1.0);
            let left = ((left * ict).simd_min(one).simd_max(-one) + one) * hamp;
            let right = ((right * ict).simd_min(one).simd_max(-one) + one) * hamp;
            // The audio jack is wired such that the first PWM channel plays on the right
            // side, and the second PWM channel plays on the left side, so even indices are
            // for the right channel, and odd indices are for the right channel.
            let (first, second) = right.interleave(left);
            let time = (time - self.time) as usize * SMPL_CHAN_COUNT;
            first.cast::<u32>().copy_to_slice(&mut buf[time .. time + 4]);
            second.cast::<u32>().copy_to_slice(&mut buf[time + 4 .. time + 8]);
        }
        self.tones = Default::default();
        self.did_commit = true;
        future
    }

    /// Computes a vector of samples starting at the specified time with the
    /// specified period.
    ///
    /// * `time`: Base time.
    /// * `period`: Wave period.
    ///
    /// Returns the computed vector of samples.
    #[inline(always)]
    fn compute_sample(time: u64, period: u32) -> f32x4
    {
        if period == 0 {
            return f32x4::splat(0.0);
        }
        let offset = u32x4::splat((time % period as u64) as u32);
        let period = u32x4::splat(period);
        let pos = u32x4::from_array([0, 1, 2, 3]);
        let offset = (offset + pos) % period;
        let two = u32x4::splat(2);
        let half = f32x4::splat(0.5);
        (offset * two).simd_ge(period).select(half, -half)
    }

    /// Pans and mixes a given array of vectors of samples into a single vector
    /// of samples.
    ///
    /// * `samples`: Input samples.
    /// * `bias`: Pan bias.
    ///
    /// Returns a mixed vector of samples with panning applied.
    #[inline(always)]
    fn pan_mix(tones: &[(u32, f32)], samples: [f32x4; POLYPHONY], bias: f32) -> f32x4
    {
        let one = f32x4::splat(1.0);
        tones.iter()
             .enumerate()
             .map(|(idx, tone)| samples[idx].mul_scalar((tone.1 + bias).abs()))
             .map(|sample| sample.simd_min(one).simd_max(-one))
             .array_chunks::<POLYPHONY>()
             .next()
             .unwrap()
             .iter()
             .array_chunks::<2>()
             .map(|samples| Self::mix(*samples[0], *samples[1]))
             .array_chunks::<2>()
             .map(|samples| Self::mix(samples[0], samples[1]))
             .array_chunks::<2>()
             .map(|samples| Self::mix(samples[0], samples[1]))
             .next()
             .unwrap()
    }

    /// Mixes the respective lanes of two vectors of samples into a single
    /// vector of samples.
    ///
    /// * `s0`: First vector of samples.
    /// * `s1`: Second vector of samples.
    ///
    /// Returns the computed results.
    #[inline(always)]
    fn mix(s0: f32x4, s1: f32x4) -> f32x4
    {
        s0 + s1
    }

    /// Returns the index of the buffer not currently being read.
    fn inactive_buffer(&self) -> u8
    {
        fence(Ordering::Acquire);
        let cb = unsafe { DMA_CHAN_CB.read() } as usize;
        if cb == self.cb {
            return 1;
        }
        0
    }

    /// Refills the buffer not currently in use with silence.
    fn refill()
    {
        unsafe { DMA_CHAN_CS.write_volatile(0x7) };
        fence(Ordering::Release);
        unsafe { PWM_STAT.write_volatile(0x13C) };
        fence(Ordering::Release);
        let mut audio = AUDIO.lock();
        let buf = if audio.inactive_buffer() == 0 {
            &mut audio.ab0[..]
        } else {
            &mut audio.ab1[..]
        };
        buf.fill(1 << (SMPL_DEPTH - 1));
        audio.time += (SMPL_BUF_LEN / SMPL_CHAN_COUNT) as u64;
        audio.waiters.iter().for_each(|waiter| waiter.wake_by_ref());
        audio.waiters.clear();
        audio.did_commit = false;
    }
}

impl WillSwap
{
    /// Creates and initialize a new will swap future.
    ///
    /// * `time`: Time at which this future was created.
    ///
    /// Returns the newly created future.
    fn new(time: u64) -> Self
    {
        Self { time }
    }
}

impl Future for WillSwap
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<()>
    {
        let mut audio = AUDIO.lock();
        if audio.time != self.time {
            return Poll::Ready(());
        }
        audio.waiters.push(ctx.waker().clone());
        Poll::Pending
    }
}
