//! Physical page allocation.
//!
//! [`Alloc`] is a physical page allocator based on a free list implementation
//! of the buddy allocator.  By default it assumes that all the memory is used,
//! so free regions must be initially tracked by calling [`Alloc::track`].

use core::cmp::{max, min};
use core::fmt::{Display, Formatter, Result as FormatResult};
use core::ops::Range;
use core::ptr::null_mut;
use core::write;

#[cfg(test)]
use self::tests::*;
use crate::sync::Lock;
#[cfg(not(test))]
use crate::{PAGE_GRANULE, RAM_BASE, TOTAL_RAM};

/// Global page allocator instance.
#[cfg(not(test))]
pub static ALLOC: Alloc = Alloc::new();

/// Page allocator.
#[derive(Debug)]
pub struct Alloc
{
    /// Linked list heads for blocks of specific sizes.
    block_lists: Lock<[*mut FreeBlock; usize::trailing_zeros(TOTAL_RAM / PAGE_GRANULE) as usize + 1]>,
}

/// Allocator error.
#[derive(Debug)]
pub struct AllocError
{
    /// Size of the allocation that would exceed the amount of free tracked
    /// physical memory.
    size: usize,
}

/// Free page block.
#[derive(Debug)]
struct FreeBlock
{
    /// Next block of the same size.
    next: *mut FreeBlock,
    /// Previous block of the same size.
    prev: *mut FreeBlock,
}

impl Alloc
{
    /// Creates and initializes a new page allocator.
    ///
    /// Returns the newly created allocator.
    const fn new() -> Self
    {
        Self { block_lists: Lock::new([null_mut(); usize::trailing_zeros(TOTAL_RAM / PAGE_GRANULE) as usize + 1]) }
    }

    /// Allocates a contiguous region of physical memory of at least the
    /// requested number of bytes.
    ///
    /// * `size`: The minimum size of the buffer to allocate in bytes.
    ///
    /// Returns the allocated buffer.
    ///
    /// The memory is not initialized.
    pub unsafe fn alloc(&self, size: usize) -> Result<*mut u8, AllocError>
    {
        let size = max(size.next_power_of_two(), PAGE_GRANULE);
        let start_idx = (size / PAGE_GRANULE) >> PAGE_GRANULE.trailing_zeros() as usize;
        let mut idx = start_idx;
        let mut block_lists = self.block_lists.lock();
        // Look for the smallest possible block that can store an allocation of the
        // requested size.
        for cur_idx in start_idx .. block_lists.len() {
            if !block_lists[cur_idx].is_null() {
                break;
            }
            idx = cur_idx + 1;
        }
        if idx == block_lists.len() {
            return Err(AllocError { size });
        }
        // Split larger blocks until we get a block rounded up to the granule size or
        // next power of two.
        while idx > start_idx {
            let buddy0 = block_lists[idx];
            let size = 1 << (idx + PAGE_GRANULE.trailing_zeros() as usize - 1);
            let next = (*buddy0).next;
            block_lists[idx] = next;
            if !next.is_null() {
                (*next).prev = null_mut()
            }
            let buddy1 = buddy0.byte_add(size);
            *buddy0 = FreeBlock { next: buddy1,
                                  prev: null_mut() };
            *buddy1 = FreeBlock { next: null_mut(),
                                  prev: buddy0 };
            idx -= 1;
            block_lists[idx] = buddy0;
        }
        let block = block_lists[idx];
        let next = (*block).next;
        block_lists[idx] = next;
        if !next.is_null() {
            (*next).prev = null_mut()
        }
        drop(block_lists);
        Ok(block.cast())
    }

    /// Deallocates a previously allocated memory region of the specified size
    /// starting at the specified base.
    ///
    /// * `base`: Location of the buffer to be deallocated.
    /// * `size`: Size of the buffer to be deallocated.
    ///
    /// The caller is responsible for ensuring that the specified base and size
    /// are the same values that were returned by and provided to
    /// [`Self::alloc`] respectively.
    pub unsafe fn dealloc(&self, block: *mut u8, size: usize)
    {
        let mut size = max(size.next_power_of_two(), PAGE_GRANULE);
        let mut block_lists = self.block_lists.lock();
        let mut block = block.cast::<FreeBlock>();
        let mut idx = usize::trailing_zeros(size / PAGE_GRANULE) as usize;
        // Coalesce buddies into bigger blocks if possible.
        loop {
            if size == TOTAL_RAM {
                break;
            }
            let mut left = null_mut();
            let mut right = block_lists[idx];
            while !right.is_null() && (right as usize) < (block as usize) {
                left = right;
                right = (*right).next;
            }
            // Check whether we're the left buddy and there's a free matching right buddy.
            if !right.is_null() && (block as usize / size) & 0x1 == 0x0 && block as usize + size == right as usize {
                // Remove the buddy from its list since we're merging with it.
                if !left.is_null() {
                    (*left).next = (*right).next
                } else {
                    block_lists[idx] = (*right).next
                }
                if !(*right).next.is_null() {
                    (*(*right).next).prev = left
                }
                size <<= 1;
                idx += 1;
                continue;
            }
            // Check whether we're the right buddy and there's a free matching left buddy.
            if !left.is_null() && (block as usize / size) & 0x1 == 0x1 && block as usize == left as usize + size {
                // Remove the buddy from its list since we're merging with it.
                if !(*left).prev.is_null() {
                    (*(*left).prev).next = right
                } else {
                    block_lists[idx] = right
                }
                if !right.is_null() {
                    (*right).prev = (*left).prev
                }
                size <<= 1;
                idx += 1;
                block = left;
                continue;
            }
            break;
        }
        let mut prev = null_mut();
        let mut next = block_lists[idx];
        while !next.is_null() && (next as usize) < (block as usize) {
            prev = next;
            next = (*next).next;
        }
        *block = FreeBlock { prev, next };
        if prev.is_null() {
            block_lists[idx] = block
        }
        drop(block_lists);
    }

    /// Tracks the specified regions as free memory.
    ///
    /// * `regions`: The regions to be marked free.
    ///
    /// The caller is responsible for calling this function only once before any
    /// allocation attempts are made and never after that.
    pub unsafe fn track(&self, regions: &[Range<usize>])
    {
        let mut block_heads = [null_mut(); usize::trailing_zeros(TOTAL_RAM / PAGE_GRANULE) as usize + 1];
        let mut block_tails = [null_mut(); usize::trailing_zeros(TOTAL_RAM / PAGE_GRANULE) as usize + 1];
        for region in regions {
            let mut cur = region.start;
            while cur < region.end {
                let size = min(usize::next_power_of_two(region.end - cur + 1) >> 1,
                               1 << cur.trailing_zeros() as usize);
                let idx = usize::trailing_zeros(size / PAGE_GRANULE) as usize;
                let block = (cur + RAM_BASE) as *mut FreeBlock;
                *block = FreeBlock { next: null_mut(),
                                     prev: block_tails[idx] };
                if !(*block).prev.is_null() {
                    (*block_tails[idx]).next = block
                } else {
                    block_heads[idx] = block
                }
                block_tails[idx] = block;
                cur += size;
            }
            *self.block_lists.lock() = block_heads;
        }
    }
}

impl Display for AllocError
{
    fn fmt(&self, formatter: &mut Formatter) -> FormatResult
    {
        write!(formatter, "Insufficient memory for a {} byte allocation", self.size)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    pub const TOTAL_RAM: usize = 0x1000;
    pub const RAM_BASE: usize = 0x0;
    pub const PAGE_GRANULE: usize = 0x100;

    #[derive(Debug)]
    #[repr(align(0x1000))]
    struct Sandbox([u8; TOTAL_RAM]);

    #[test]
    fn track_splits_into_buddies()
    {
        let mut sandbox = Sandbox([0; TOTAL_RAM]);
        let sandbox = sandbox.0.as_mut_ptr();
        let start = sandbox as usize + PAGE_GRANULE;
        let end = sandbox as usize + TOTAL_RAM - PAGE_GRANULE;
        let alloc = Alloc::new();
        unsafe { alloc.track(&[start .. end]) };
        let block_lists = alloc.block_lists.lock();
        assert_eq!(block_lists[0], unsafe { sandbox.add(PAGE_GRANULE).cast() });
        assert_eq!(block_lists[1], unsafe { sandbox.add(PAGE_GRANULE * 2).cast() });
        assert_eq!(block_lists[2], unsafe { sandbox.add(PAGE_GRANULE * 4).cast() });
        assert!(block_lists[3].is_null());
        assert!(block_lists[4].is_null());
        let block = unsafe { block_lists[0].read() };
        assert_eq!(block.next, unsafe { sandbox.add(PAGE_GRANULE * 0xE).cast() });
        assert!(block.prev.is_null());
        let block = unsafe { block.next.read() };
        assert!(block.next.is_null());
        assert_eq!(block.prev, unsafe { sandbox.add(PAGE_GRANULE).cast() });
        let block = unsafe { block_lists[1].cast::<FreeBlock>().read() };
        assert_eq!(block.next, unsafe { sandbox.add(PAGE_GRANULE * 0xC).cast() });
        assert!(block.prev.is_null());
        let block = unsafe { block.next.read() };
        assert!(block.next.is_null());
        assert_eq!(block.prev, unsafe { sandbox.add(PAGE_GRANULE * 0x2).cast() });
        let block = unsafe { block_lists[2].cast::<FreeBlock>().read() };
        assert_eq!(block.next, unsafe { sandbox.add(PAGE_GRANULE * 0x8).cast() });
        assert!(block.prev.is_null());
        let block = unsafe { block.next.read() };
        assert!(block.next.is_null());
        assert_eq!(block.prev, unsafe { sandbox.add(PAGE_GRANULE * 0x4).cast() });
        drop(block_lists);
    }

    #[test]
    fn track_multiple_regions_same_universe()
    {
        let mut sandbox = Sandbox([0; TOTAL_RAM]);
        let sandbox = sandbox.0.as_mut_ptr();
        let first = sandbox as usize .. sandbox as usize + PAGE_GRANULE * 0x4;
        let second = sandbox as usize + PAGE_GRANULE * 0x8 .. sandbox as usize + PAGE_GRANULE * 0xC;
        let alloc = Alloc::new();
        unsafe { alloc.track(&[first, second]) };
        let block_lists = alloc.block_lists.lock();
        assert_eq!(block_lists[2], sandbox.cast());
        let block = unsafe { block_lists[2].read() };
        assert_eq!(block.next, unsafe { sandbox.add(PAGE_GRANULE * 0x8).cast() });
        assert!(block.prev.is_null());
        let block = unsafe { block.next.read() };
        assert!(block.next.is_null());
        assert_eq!(block.prev, sandbox.cast());
        drop(block_lists);
    }

    #[test]
    fn alloc_splits_into_buddies()
    {
        let mut sandbox = Sandbox([0; TOTAL_RAM]);
        let sandbox = sandbox.0.as_mut_ptr();
        unsafe {
            *sandbox.cast::<FreeBlock>() = FreeBlock { next: null_mut(),
                                                       prev: null_mut() }
        };
        let alloc = Alloc::new();
        alloc.block_lists.lock()[4] = sandbox.cast();
        let buf = unsafe { alloc.alloc(PAGE_GRANULE).unwrap() };
        assert_eq!(buf, sandbox.cast());
        let block_lists = alloc.block_lists.lock();
        assert_eq!(block_lists[0], unsafe { sandbox.add(PAGE_GRANULE).cast() });
        assert_eq!(block_lists[1], unsafe { sandbox.add(PAGE_GRANULE * 0x2).cast() });
        assert_eq!(block_lists[2], unsafe { sandbox.add(PAGE_GRANULE * 0x4).cast() });
        assert_eq!(block_lists[3], unsafe { sandbox.add(PAGE_GRANULE * 0x8).cast() });
        assert!(block_lists[4].is_null());
        let block = unsafe { block_lists[0].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        let block = unsafe { block_lists[1].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        let block = unsafe { block_lists[2].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        let block = unsafe { block_lists[3].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        drop(block_lists);
    }

    #[test]
    #[should_panic]
    fn alloc_fails_without_memory()
    {
        let alloc = Alloc::new();
        unsafe { alloc.alloc(PAGE_GRANULE).unwrap() };
    }

    #[test]
    fn dealloc_coalesces_into_left_buddy()
    {
        let mut sandbox = Sandbox([0; TOTAL_RAM]);
        let sandbox = sandbox.0.as_mut_ptr();
        let alloc = Alloc::new();
        let mut block_lists = alloc.block_lists.lock();
        let block = unsafe { sandbox.add(PAGE_GRANULE).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[0] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0x2).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[1] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0x4).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[2] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0x8).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[3] = block;
        drop(block_lists);
        unsafe { alloc.dealloc(sandbox, PAGE_GRANULE) };
        let block_lists = alloc.block_lists.lock();
        assert!(block_lists[0].is_null());
        assert!(block_lists[1].is_null());
        assert!(block_lists[2].is_null());
        assert!(block_lists[3].is_null());
        assert_eq!(block_lists[4], sandbox.cast());
        let block = unsafe { block_lists[4].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        drop(block_lists);
    }

    #[test]
    fn dealloc_coalesces_into_right_buddy()
    {
        let mut sandbox = Sandbox([0; TOTAL_RAM]);
        let sandbox = sandbox.0.as_mut_ptr();
        let alloc = Alloc::new();
        let mut block_lists = alloc.block_lists.lock();
        let block = sandbox.cast::<FreeBlock>();
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[3] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0x8).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[2] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0xC).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[1] = block;
        let block = unsafe { sandbox.add(PAGE_GRANULE * 0xE).cast::<FreeBlock>() };
        unsafe {
            *block = FreeBlock { next: null_mut(),
                                 prev: null_mut() }
        };
        block_lists[0] = block;
        drop(block_lists);
        unsafe { alloc.dealloc(sandbox.add(PAGE_GRANULE * 0xF), PAGE_GRANULE) };
        let block_lists = alloc.block_lists.lock();
        assert!(block_lists[0].is_null());
        assert!(block_lists[1].is_null());
        assert!(block_lists[2].is_null());
        assert!(block_lists[3].is_null());
        assert_eq!(block_lists[4], sandbox.cast());
        let block = unsafe { block_lists[4].read() };
        assert!(block.next.is_null());
        assert!(block.prev.is_null());
        drop(block_lists);
    }
}
