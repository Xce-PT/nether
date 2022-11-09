//! Video core mailbox interface.
//!
//! Documentation:
//!
//! * [Accessing mailboxes](https://github.com/raspberrypi/firmware/wiki/Accessing-mailboxes)
//! * [Mailbox property interface](https://github.com/raspberrypi/firmware/wiki/Mailbox-property-interface)
//! * [Mailboxes](https://github.com/raspberrypi/firmware/wiki/Mailboxes)
//!
//! At the moment  everything seems to be done through property tags, which are fully enumerated in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/include/soc/bcm2835/raspberrypi-firmware.h).

extern crate alloc;

use alloc::boxed::Box;
use core::hint::spin_loop;
use core::sync::atomic::{fence, Ordering};

use crate::alloc::{Engine as AllocatorEngine, DMA};
use crate::sync::{Lazy, Lock};
use crate::{DMA_RANGE, PERRY_RANGE, VC_RANGE};

/// Offset of the physical RAM from the perspective of the video core.
const VC_OFFSET: usize = 0xC0000000;
/// Base address of the video core mailbox registers.
const BASE: usize = 0x200B880 + PERRY_RANGE.start;
/// Pointer to the inbox data register.
const INBOX_DATA: *const u32 = BASE as _;
/// Pointer to the inbox status register.
const INBOX_STATUS: *const u32 = (BASE + 0x18) as _;
/// Pointer to the outbox data register.
const OUTBOX_DATA: *mut u32 = (BASE + 0x20) as _;
/// Pointer to the outbox status register.
const OUTBOX_STATUS: *const u32 = (BASE + 0x38) as _;
/// Mailbox full status value.
const FULL_STATUS: u32 = 0x80000000;
/// Mailbox empty status value.
const EMPTY_STATUS: u32 = 0x40000000;
/// Delivery request code.
const REQUEST_CODE: u32 = 0x0;
/// Success delivery code.
const SUCCESS_CODE: u32 = 0x80000000;
/// End tag.
const END_TAG: u32 = 0x0;
/// Frame buffer allocation.
const ALLOC_TAG: u32 = 0x40001;
/// Set the physical size of the display.
const SET_PHYS_SIZE_TAG: u32 = 0x48003;
/// Set the virtual size of the buffer.
const SET_VIRT_SIZE_TAG: u32 = 0x48004;
/// Set the pixel depth.
const SET_DEPTH_TAG: u32 = 0x48005;
/// Set the position of the physical display inside the buffer.
const SET_POS_TAG: u32 = 0x48009;
/// Set the touchscreen DMA buffer.
const SET_TOUCH_BUF_TAG: u32 = 0x4801F;

/// Global video core mailbox interface driver instance.
pub static MBOX: Lazy<Mailbox> = Lazy::new(Mailbox::new);

/// Mailbox interface driver.
#[derive(Debug)]
pub struct Mailbox
{
    /// DMA buffer.
    buf: Lock<Box<Buffer, AllocatorEngine<'static>>>,
}

/// Request to send to the video core.
#[derive(Debug)]
pub struct Request
{
    /// Property request.
    props: [RequestProperty; 16],
    /// Property count.
    len: usize,
}

/// Response iterator.
#[derive(Debug)]
pub struct ResponseIterator
{
    /// Response to iterate over.
    resp: Response,
    /// Current index.
    idx: usize,
}

/// Response from the video core.
#[derive(Debug)]
pub struct Response
{
    /// Property response.
    props: [ResponseProperty; 16],
    /// Property count.
    len: usize,
}

/// Mailbox property request..
#[derive(Clone, Copy, Debug)]
pub enum RequestProperty
{
    /// No property.
    None,
    /// Allocate frame buffer.
    Allocate
    {
        align: usize
    },
    /// Set the physical size of the display.
    SetPhysicalSize
    {
        width: usize, height: usize
    },
    /// Set the virtual size of the buffer.
    SetVirtualSize
    {
        width: usize, height: usize
    },
    /// Set the pixel bit depth.
    SetDepth
    {
        bits: usize
    },
    /// Set the physical display position inside the buffer.
    SetPosition
    {
        x: usize, y: usize
    },
    /// Set the touchscreen DMA buffer.
    SetTouchBuffer
    {
        buf: *mut u8
    },
}

/// Mailbox property response..
#[derive(Clone, Copy, Debug)]
pub enum ResponseProperty
{
    /// No property.
    None,
    /// Allocate frame buffer.
    Allocate
    {
        base: *mut u8, size: usize
    },
    /// Set the physical size of the display.
    SetPhysicalSize
    {
        width: usize, height: usize
    },
    /// Set the virtual size of the buffer.
    SetVirtualSize
    {
        width: usize, height: usize
    },
    /// Set the pixel bit depth.
    SetDepth
    {
        bits: usize
    },
    /// Set the physical display position inside the buffer.
    SetPosition
    {
        x: usize, y: usize
    },
    /// Set the touchscreen DMA buffer.
    SetTouchBuffer,
}

/// Aligned message buffer.
#[repr(C, align(0x10))]
#[derive(Clone, Copy, Debug)]
struct Buffer
{
    /// Content of the buffer.
    content: [u32; 80], // Enough room for 16 properties of average size.
}

impl Mailbox
{
    /// Creates and initializes a new mailbox.
    ///
    /// Returns the newly created mailbox.
    fn new() -> Self
    {
        Self { buf: Lock::new(Box::new_in(Buffer { content: [0; 80] }, DMA)) }
    }

    /// Delivers the request and waits for a response.
    ///
    /// * `req`: Request.
    ///
    /// Returns a response object from which properties can be extracted.
    pub fn exchange(&self, req: Request) -> Response
    {
        let mut buf = req.into_buffer();
        let mut dma_buf = self.buf.lock();
        **dma_buf = buf;
        while unsafe { OUTBOX_STATUS.read_volatile() } & FULL_STATUS != 0 {
            spin_loop()
        }
        let data = unsafe { map_to_vc(dma_buf.content.as_mut_ptr().cast()) } | 0x8; // Channel.
        fence(Ordering::Release);
        unsafe { OUTBOX_DATA.write_volatile(data) };
        while unsafe { INBOX_STATUS.read_volatile() } & EMPTY_STATUS != 0 {
            spin_loop()
        }
        unsafe { INBOX_DATA.read_volatile() }; // Don't care about this value, just reading it to empty the inbox.
        fence(Ordering::Acquire);
        buf = **dma_buf;
        Response::from_buffer(buf)
    }
}

impl Request
{
    /// Creates and initializes a new request.
    ///
    /// Returns the created request.
    pub fn new() -> Self
    {
        Self { props: [RequestProperty::None; 16],
               len: 0 }
    }

    /// Pushes a new property into the request.
    pub fn push(&mut self, prop: RequestProperty)
    {
        if let RequestProperty::None = prop {
            return;
        }
        assert!(self.len < self.props.len(), "Too many properties in request");
        self.props[self.len] = prop;
        self.len += 1;
    }

    /// Creates and initializes a new buffer by consuming this request.
    ///
    /// Returns the newly created buffer.
    fn into_buffer(self) -> Buffer
    {
        let mut buf = [0; 80];
        buf[0] = (buf.len() * 4) as _;
        buf[1] = REQUEST_CODE;
        let mut idx = 2;
        for prop in self.props {
            match prop {
                RequestProperty::None => continue,
                RequestProperty::Allocate { align } => {
                    assert!(idx + 5 < buf.len(), "Buffer overflow");
                    buf[idx] = ALLOC_TAG;
                    buf[idx + 1] = 8; // Size reserved for request and response.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = align as _;
                    buf[idx + 4] = 0; // Reserved for response.
                    idx += 5;
                }
                RequestProperty::SetPhysicalSize { width, height } => {
                    assert!(idx + 5 < buf.len(), "Buffer overflow");
                    buf[idx] = SET_PHYS_SIZE_TAG;
                    buf[idx + 1] = 8; // Request and response size.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = width as _;
                    buf[idx + 4] = height as _;
                    idx += 5;
                }
                RequestProperty::SetVirtualSize { width, height } => {
                    assert!(idx + 5 < buf.len(), "Buffer overflow");
                    buf[idx] = SET_VIRT_SIZE_TAG;
                    buf[idx + 1] = 8; // Request and response size.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = width as _;
                    buf[idx + 4] = height as _;
                    idx += 5;
                }
                RequestProperty::SetDepth { bits } => {
                    assert!(idx + 4 < buf.len(), "Buffer overflow");
                    buf[idx] = SET_DEPTH_TAG;
                    buf[idx + 1] = 4; // Request and response size.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = bits as _;
                    idx += 4;
                }
                RequestProperty::SetPosition { x, y } => {
                    assert!(idx + 5 < buf.len(), "Buffer overflow");
                    buf[idx] = SET_POS_TAG;
                    buf[idx + 1] = 8; // Request and response size.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = x as _;
                    buf[idx + 4] = y as _;
                    idx += 5;
                }
                RequestProperty::SetTouchBuffer { buf: touchbuf } => {
                    assert!(DMA_RANGE.contains(&(touchbuf as usize)),
                            "Provided touch buffer is not in a DMA region");
                    let touchbuf = unsafe { map_to_vc(touchbuf) };
                    assert!(idx + 4 < buf.len(), "Buffer overflow");
                    buf[idx] = SET_TOUCH_BUF_TAG;
                    buf[idx + 1] = 4; // Request and response size.
                    buf[idx + 2] = 0; // Reserved for response length.
                    buf[idx + 3] = touchbuf;
                    idx += 4;
                }
            }
        }
        buf[idx] = END_TAG as _;
        Buffer { content: buf }
    }
}

impl Response
{
    /// Creates and initializes a new Response from a buffer.
    ///
    /// * `buf`: The buffer from which to initialize the response.
    ///
    /// Returns the newly created Response.
    fn from_buffer(buf: Buffer) -> Self
    {
        let buf = &buf.content;
        let mut props = [ResponseProperty::None; 16];
        let code = buf[1];
        assert_eq!(code, SUCCESS_CODE, "Error code returned by the VC: 0x{code:X}");
        let mut len = 0;
        let mut idx = 2;
        while idx + 3 < buf.len() {
            let tag = buf[idx];
            idx += 3;
            let prop = match tag {
                ALLOC_TAG => {
                    assert!(idx + 2 < buf.len(), "Truncated response from VC");
                    let base = map_from_vc(buf[idx]);
                    let size = buf[idx + 1] as usize;
                    idx += 2;
                    ResponseProperty::Allocate { base, size }
                }
                SET_PHYS_SIZE_TAG => {
                    assert!(idx + 2 < buf.len(), "Truncated response from VC");
                    let width = buf[idx] as usize;
                    let height = buf[idx + 1] as usize;
                    idx += 2;
                    ResponseProperty::SetPhysicalSize { width, height }
                }
                SET_VIRT_SIZE_TAG => {
                    assert!(idx + 2 < buf.len(), "Truncated response from VC");
                    let width = buf[idx] as usize;
                    let height = buf[idx + 1] as usize;
                    idx += 2;
                    ResponseProperty::SetVirtualSize { width, height }
                }
                SET_DEPTH_TAG => {
                    assert!(idx + 1 < buf.len(), "Truncated response from VC");
                    let bits = buf[idx] as usize;
                    idx += 1;
                    ResponseProperty::SetDepth { bits }
                }
                SET_POS_TAG => {
                    assert!(idx + 2 < buf.len(), "Truncated response from VC");
                    let x = buf[idx] as usize;
                    let y = buf[idx + 1] as usize;
                    idx += 2;
                    ResponseProperty::SetPosition { x, y }
                }
                SET_TOUCH_BUF_TAG => {
                    assert!(idx + 1 < buf.len(), "Truncated response from VC");
                    ResponseProperty::SetTouchBuffer
                }
                END_TAG => break,
                _ => panic!("Unknown property tag returned by the VC: 0x{tag:X}"),
            };
            assert!(len < props.len(), "Too many properties in response from VC");
            props[len] = prop;
            len += 1;
        }
        Self { props, len }
    }
}

impl IntoIterator for Response
{
    type IntoIter = ResponseIterator;
    type Item = ResponseProperty;

    fn into_iter(self) -> Self::IntoIter
    {
        Self::IntoIter::from_response(self)
    }
}

impl ResponseIterator
{
    /// Creates and initializes a new response iterator by consuming a response.
    ///
    /// * `resp`: Response to consume and iterate over.
    ///
    /// Returns the newly initialized iterator.
    fn from_response(resp: Response) -> Self
    {
        Self { resp, idx: 0 }
    }
}

impl Iterator for ResponseIterator
{
    type Item = ResponseProperty;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.idx >= self.resp.len {
            return None;
        }
        let idx = self.idx;
        self.idx += 1;
        Some(self.resp.props[idx])
    }
}

/// Maps from a RAM address from the perspective of the ARM core to an
/// address from the perspective of the video core suitable to be sent as
/// data through the mailbox.
///
/// * `buf`: The buffer whose address is to be converted.
///
/// Returns the converted buffer address in a format suitable to be sent to
/// the video core.
///
/// Panics if `buf` is not in the DMA region.
unsafe fn map_to_vc(buf: *mut u8) -> u32
{
    let virt = buf as usize;
    // The DMA region is identity mapped.
    assert!(DMA_RANGE.contains(&virt),
            "Provided buffer at virtual address 0x{virt} is not in the DMA region");
    (virt | VC_OFFSET) as u32 // The DMA range is identity mapped.
}

/// Maps data received in the mailbox from a RAM address from the
/// perspective of the video core to an address from the perspective of the
/// ARM core.
///
/// Returns the mapped address.
///
/// Panics if the address is not in the DMA region or in the region shared
/// with the VC.
fn map_from_vc(data: u32) -> *mut u8
{
    let phys = data as usize & !VC_OFFSET;
    if DMA_RANGE.contains(&phys) {
        return phys as _; // The DMA range is identity mapped.
    }
    let vc_size = VC_RANGE.end - VC_RANGE.start;
    let vc_phys_range = 0x40000000 - vc_size .. 0x40000000;
    assert!(vc_phys_range.contains(&phys),
            "Physical address 0x{phys:X} is not mapped to the DMA or VC regions");
    (phys - vc_phys_range.start + VC_RANGE.start) as _
}
