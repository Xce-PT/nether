//! Video core mailbox interface.
//!
//! Documentation:
//!
//! * [Accessing mailboxes](https://github.com/raspberrypi/firmware/wiki/Accessing-mailboxes)
//! * [Mailbox property interface](https://github.com/raspberrypi/firmware/wiki/Mailbox-property-interface)
//! * [Mailboxes](https://github.com/raspberrypi/firmware/wiki/Mailboxes)
//!
//! At the moment  everything seems to be done through property tags, which are fully enumerated in the [Linux kernel source](https://github.com/raspberrypi/linux/blob/rpi-5.15.y/include/soc/bcm2835/raspberrypi-firmware.h).

// TODO: Switch from a poll strategy to an interrupt strategy once a task
// executor is implemented.

use core::alloc::{AllocError, Allocator, Layout};
use core::fmt::{Debug, Display, Formatter, Result as FormatResult};
#[cfg(not(test))]
use core::hint::spin_loop;
use core::mem::{size_of, size_of_val, MaybeUninit};
use core::ptr::{addr_of, NonNull};
use core::slice::from_raw_parts as slice_from_raw_parts;
#[cfg(not(test))]
use core::sync::atomic::{fence, Ordering};

#[cfg(test)]
use self::tests::*;
#[cfg(not(test))]
use crate::alloc::DMA;
#[cfg(not(test))]
use crate::sync::LockAdvisor;
#[cfg(not(test))]
use crate::{DMA_RANGE, PERRY_RANGE, VC_RANGE};

/// Offset of the physical RAM from the perspective of the video core.
#[cfg(not(test))]
const VC_OFFSET: usize = 0xC0000000;
/// Base address of the video core mailbox registers.
#[cfg(not(test))]
const BASE: usize = 0x200B880 + PERRY_RANGE.start;
/// Pointer to the inbox data register.
#[cfg(not(test))]
const INBOX_DATA: *const u32 = BASE as _;
/// Pointer to the inbox status register.
#[cfg(not(test))]
const INBOX_STATUS: *const u32 = (BASE + 0x18) as _;
/// Pointer to the outbox data register.
#[cfg(not(test))]
const OUTBOX_DATA: *mut u32 = (BASE + 0x20) as _;
/// Pointer to the outbox status register.
#[cfg(not(test))]
const OUTBOX_STATUS: *const u32 = (BASE + 0x38) as _;
/// Mailbox full status value.
#[cfg(not(test))]
const FULL_STATUS: u32 = 0x80000000;
/// Mailbox empty status value.
#[cfg(not(test))]
const EMPTY_STATUS: u32 = 0x40000000;
/// Delivery request code.
const REQUEST_CODE: u32 = 0x0;
/// Success delivery code.
#[cfg(not(test))]
const SUCCESS_CODE: u32 = 0x80000000;
/// End tag.
const END_TAG: u32 = 0x0;

/// Global video core mailbox interface driver instance.
#[cfg(not(test))]
pub static MBOX: Mailbox = Mailbox::new();

/// Mailbox interface driver.
#[cfg(not(test))]
#[derive(Debug)]
pub struct Mailbox
{
    /// Lock advisor to prevent multiple simultaneous accesses to the mailbox.
    advisor: LockAdvisor,
}

/// Type-safe property tag message.
pub struct Message
{
    /// Allocated message buffer.
    buf: *mut u8,
}

/// Property tag message iterator.
#[derive(Debug)]
pub struct MessageIterator<'a>
{
    /// Message whose contents are to be iterated.
    msg: &'a Message,
    /// Next property tag.
    next: usize,
}

/// Property tag item returned by [`MessageIterator`].
#[derive(Debug)]
pub struct Tag<'a>
{
    /// Message being iterated.
    msg: &'a Message,
    /// Tag ID.
    id: u32,
    /// Tag offset into the message.
    offset: usize,
    /// Tag data length.
    len: usize,
}

/// Mailbox error reported by the video core.
#[cfg(not(test))]
#[derive(Debug)]
pub struct MailboxExchangeError
{
    /// Error code.
    code: u32,
}

/// Message creation error.
#[derive(Debug)]
pub struct MessageCreationError
{
    /// Source error that originated this error.
    source: AllocError,
}

/// Message overflow error.
#[derive(Debug)]
pub struct MessageOverflowError
{
    /// Length of the tag that would cause the overflow.
    len: usize,
}

/// Tag interpretation error.
#[derive(Debug)]
pub struct TagError
{
    /// Size in bytes of the requested object type.
    expected: usize,
    /// Size in bytes of the property tag's data.
    actual: usize,
}

#[cfg(not(test))]
impl Mailbox
{
    /// Creates and initializes a new mailbox.
    ///
    /// Returns the newly created mailbox.
    const fn new() -> Self
    {
        Self { advisor: LockAdvisor::new() }
    }

    /// Sends the message and waits for a reply.
    ///
    /// * `msg`: The message to be sent.
    ///
    /// Either returns the provided message or an error with the response code.
    pub fn exchange(&self, msg: Message) -> Result<Message, MailboxExchangeError>
    {
        unsafe { self.advisor.lock() };
        while unsafe { OUTBOX_STATUS.read_volatile() } & FULL_STATUS != 0 {
            spin_loop()
        }
        let data = unsafe { Self::map_to_vc(msg.buf) } | 0x8; // Channel.
        fence(Ordering::Release);
        unsafe { OUTBOX_DATA.write_volatile(data) };
        while unsafe { INBOX_STATUS.read_volatile() } & EMPTY_STATUS != 0 {
            spin_loop()
        }
        unsafe { INBOX_DATA.read_volatile() }; // Don't care about this value, but read it to empty the inbox.
        fence(Ordering::Acquire);
        unsafe { self.advisor.unlock() };
        let code = unsafe { msg.buf.cast::<u32>().add(1).read() };
        if code != SUCCESS_CODE {
            return Err(MailboxExchangeError { code });
        }
        Ok(msg)
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
    pub unsafe fn map_to_vc<T>(buf: *mut T) -> u32
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
    /// Panics if the address is not in the DMA region or in the region shared
    /// with the VC.
    ///
    /// Returns the mapped address.
    pub fn map_from_vc<T>(data: u32) -> *mut T
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
}

impl Message
{
    /// Maximum number of bytes in a message.
    pub const CAPACITY: usize = 0x100;

    /// Creates and initializes a new message.
    ///
    /// Returns the created message.
    pub fn new() -> Result<Self, MessageCreationError>
    {
        let layout = Layout::from_size_align(Self::CAPACITY, 16).unwrap();
        let buf: *mut u8 = DMA.allocate(layout)
                              .map_err(|err| MessageCreationError { source: err })?
                              .cast::<u8>()
                              .as_ptr();
        unsafe {
            buf.cast::<u32>().write(Self::CAPACITY as _); // Buffer size.
            buf.cast::<u32>().add(1).write(REQUEST_CODE);
            buf.cast::<u32>().add(2).write(END_TAG);
        }
        Ok(Self { buf })
    }

    /// Adds a property tag to the message.
    ///
    /// * `id`: The ID of the tag.
    /// * `data`: The body of the tag.  Must have enough room for the response.:
    ///
    /// Either returns nothing on success or an overflow error.
    pub fn add_tag<T: Copy + Sized>(&mut self, id: u32, data: T) -> Result<(), MessageOverflowError>
    {
        unsafe {
            let mut offset = 8; // Skip buffer size and request code.
                                // Look for the end tag.
            while self.buf.cast::<u32>().add(offset / 4).read() != END_TAG {
                let len = (self.buf.add(offset + 4).cast::<u32>().read() as usize + 0x3) & !0x3; // Length rounded up to the next multiple of 4 bytes.
                offset += len + 12; // Add the tag ID, request length, and
                                    // response length fields.
            }
            let len = size_of_val(&data);
            let rd_len = (len + 0x3) & !0x3; // Round to the next multiple of 4 bytes.
            if offset + rd_len + 12 >= Self::CAPACITY {
                return Err(MessageOverflowError { len: rd_len + offset + 12 });
            }
            let buf = self.buf.add(offset);
            buf.cast::<u32>().write(id);
            buf.cast::<u32>().add(1).write(rd_len as u32);
            buf.cast::<u32>().add(2).write(0u32); // Reserved for response length.
            buf.add(12).copy_from_nonoverlapping(addr_of!(data).cast(), len);
            buf.add(rd_len).cast::<u32>().add(3).write(END_TAG);
        }
        Ok(())
    }
}

impl Debug for Message
{
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult
    {
        let len = unsafe { self.buf.cast::<u32>().read() };
        let code = unsafe { self.buf.cast::<u32>().add(1).read() };
        writeln!(fmt, "Length: {len}, Code: {code}, Tags:")?;
        let mut offset = 8;
        while unsafe { self.buf.add(offset).cast::<u32>().read() } != END_TAG {
            let id = unsafe { self.buf.add(offset).cast::<u32>().read() };
            let len = unsafe { self.buf.add(offset + 4).cast::<u32>().read() };
            let resp = unsafe { self.buf.add(offset + 8).read() };
            let data = unsafe { slice_from_raw_parts(self.buf.add(12), len as _) };
            writeln!(fmt, "ID: 0x{id:x}, Length: {len}, Response: 0x{resp:x}, Data: {data:?}")?;
            offset += len as usize + 12; // Length plus the ID, length, and response fields.
            offset += 0x3 & !0x3; // Round to the next multiple of 4 bytes.
            if offset >= Self::CAPACITY {
                return writeln!(fmt, "Truncated");
            }
        }
        writeln!(fmt, "Tag: {END_TAG:x}")
    }
}

impl Drop for Message
{
    fn drop(&mut self)
    {
        let layout = Layout::from_size_align(Self::CAPACITY, 16).unwrap();
        unsafe { DMA.deallocate(NonNull::new(self.buf).unwrap(), layout) };
    }
}

impl<'a> IntoIterator for &'a Message
{
    type IntoIter = MessageIterator<'a>;
    type Item = Tag<'a>;

    fn into_iter(self) -> Self::IntoIter
    {
        Self::IntoIter::new(self)
    }
}

impl<'a> MessageIterator<'a>
{
    /// Creates a new iterator.
    ///
    /// * `msg`: The message to iterate over.
    ///
    /// Returns the newly created iterator.
    fn new(msg: &'a Message) -> Self
    {
        Self { msg,
               next: 8 /* Skip buffer length and request / response code. */ }
    }
}

impl<'a> Iterator for MessageIterator<'a>
{
    type Item = Tag<'a>;

    fn next(&mut self) -> Option<Self::Item>
    {
        if self.next + 12 > Message::CAPACITY {
            return None;
        } // Truncated message.
        let buf = unsafe { self.msg.buf.add(self.next) };
        let id = unsafe { buf.cast::<u32>().read() };
        if id == END_TAG {
            return None;
        }
        let len = unsafe { buf.cast::<u32>().add(1).read() };
        let tag = Tag::new(id, self.next + 12, len as _, self.msg);
        self.next += 12 + len as usize;
        Some(tag)
    }
}

impl<'a> Tag<'a>
{
    /// Creates a new property tag.
    ///
    /// * `id`: Tag ID.
    /// * `offset`: Offset of the tag's data into the message buffer.
    /// * `len`: Length of the tag data.
    /// * `msg`: Message from where this tag originates.
    ///
    /// Returns the newly created tag.
    fn new(id: u32, offset: usize, len: usize, msg: &'a Message) -> Self
    {
        Self { msg, id, offset, len }
    }

    /// Returns the tag ID.
    pub fn id(&self) -> u32
    {
        self.id
    }

    /// Returns the tag data interpreted as the specified type, or an error if
    /// the type size doesn't match the length of the data in the tag.
    pub unsafe fn interpret_as<T: Copy + Sized>(&self) -> Result<T, TagError>
    {
        if self.len != size_of::<T>() {
            return Err(TagError { expected: size_of::<T>(),
                                  actual: self.len });
        }
        unsafe {
            let mut data = MaybeUninit::<T>::uninit();
            let base = self.msg.buf.add(self.offset);
            data.as_mut_ptr().cast::<u8>().copy_from_nonoverlapping(base, self.len);
            Ok(data.assume_init())
        }
    }
}

#[cfg(not(test))]
impl Display for MailboxExchangeError
{
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult
    {
        write!(fmt, "Video core failed to parse message (code: {:x})", self.code)
    }
}

impl Display for MessageCreationError
{
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult
    {
        write!(fmt, "Failed to create message: {}", self.source)
    }
}

impl Display for MessageOverflowError
{
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult
    {
        write!(fmt,
               "Adding this tag would increase the message size by {} bytes and cause it to overflow",
               self.len)
    }
}

impl Display for TagError
{
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult
    {
        write!(fmt,
               "Chosen type size ({} bytes) and tag buffer size ({} bytes) do not match",
               self.expected, self.actual)
    }
}

#[cfg(test)]
mod tests
{
    extern crate std;

    use std::alloc::Global;

    use super::*;

    pub static DMA: Global = Global;

    #[test]
    fn message_new_empty_buffer()
    {
        let msg = Message::new().unwrap();
        let expected: [u32; 3] = [Message::CAPACITY as _, REQUEST_CODE, END_TAG];
        let actual = msg.buf.cast::<[u32; 3]>();
        assert_eq!(unsafe { *actual }, expected);
        assert!(msg.into_iter().next().is_none());
    }

    #[test]
    fn message_with_tags_correctly_formatted()
    {
        let mut msg = Message::new().unwrap();
        msg.add_tag(0x48003, *b"Hello").unwrap();
        msg.add_tag(0x48004, 25u32).unwrap();
        let buf = msg.buf.cast::<u32>();
        assert_eq!(unsafe { buf.read() }, Message::CAPACITY as _);
        assert_eq!(unsafe { buf.add(1).read() }, REQUEST_CODE);
        assert_eq!(unsafe { buf.add(2).read() }, 0x48003);
        assert_eq!(unsafe { buf.add(3).read() }, 8);
        assert_eq!(unsafe { buf.add(4).read() }, 0);
        assert_eq!(unsafe { buf.add(5).cast::<[u8; 5]>().read() }, *b"Hello");
        assert_eq!(unsafe { buf.add(7).read() }, 0x48004);
        assert_eq!(unsafe { buf.add(8).read() }, 4);
        assert_eq!(unsafe { buf.add(9).read() }, 0);
        assert_eq!(unsafe { buf.add(10).read() }, 25);
        assert_eq!(unsafe { buf.add(11).read() }, END_TAG);
        let mut iter = msg.into_iter();
        let tag = iter.next().unwrap();
        assert_eq!(tag.id(), 0x48003);
        assert_eq!(unsafe { &tag.interpret_as::<[u8; 8]>().unwrap()[0 .. 5] }, b"Hello");
        let tag = iter.next().unwrap();
        assert_eq!(tag.id(), 0x48004);
        assert_eq!(unsafe { tag.interpret_as::<u32>().unwrap() }, 25);
        assert!(iter.next().is_none());
    }
}
