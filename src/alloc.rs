//! First fit free list memory allocator.

use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::cmp::{max, min};
use core::ops::Range;
use core::ptr::{null_mut, NonNull};
use core::slice::from_raw_parts as slice_from_raw_parts;

use crate::sync::Lock;
#[cfg(not(test))]
use crate::{CACHED_RANGE, DMA_RANGE};

/// Global allocator instance.
#[cfg(not(test))]
#[global_allocator]
pub static GLOBAL: Alloc<0x10> = Alloc::with_region(&CACHED);
/// Cached region.
#[cfg(not(test))]
pub static CACHED: Lock<Region> = unsafe { Region::new(CACHED_RANGE) };
/// DMA region.
#[cfg(not(test))]
pub static DMA: Lock<Region> = unsafe { Region::new(DMA_RANGE) };

/// Free list allocator front-end.
#[derive(Clone, Copy, Debug)]
pub struct Alloc<'a, const ALIGN: usize>
    where Self: ValidAlign
{
    /// Allocator region.
    region: &'a Lock<Region>,
}

/// Allocator region.
#[derive(Debug)]
pub struct Region
{
    /// Initial free range.
    range: Range<usize>,
    /// Head of the list of free fragments.
    head: Option<*mut Fragment>,
}

/// Valid alignment marker.
pub trait ValidAlign {}

/// Free memory fragment.
#[derive(Debug)]
struct Fragment
{
    /// Size of this fragment.
    size: usize,
    /// Next fragment.
    next: *mut Fragment,
}

impl<'a, const ALIGN: usize> Alloc<'a, ALIGN> where Self: ValidAlign
{
    /// Creates and initializes a new allocator front-end.
    ///
    /// * `region`: Memory region covered by this allocator.
    ///
    /// Returns the created allocator front-end.
    pub const fn with_region(region: &'a Lock<Region>) -> Self
    {
        Self { region }
    }
}

unsafe impl<'a, const ALIGN: usize> GlobalAlloc for Alloc<'a, ALIGN> where Self: ValidAlign
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8
    {
        self.region
            .lock()
            .allocate(layout)
            .map(|base| base.as_mut_ptr().cast::<u8>())
            .unwrap_or(null_mut())
    }

    unsafe fn dealloc(&self, base: *mut u8, layout: Layout)
    {
        self.region.lock().deallocate(NonNull::new_unchecked(base), layout);
    }

    unsafe fn realloc(&self, base: *mut u8, layout: Layout, new_size: usize) -> *mut u8
    {
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        if new_size >= layout.size() {
            return self.region
                       .lock()
                       .grow(NonNull::new_unchecked(base), layout, new_layout)
                       .map(|ptr| ptr.as_mut_ptr().cast::<u8>())
                       .unwrap_or(null_mut());
        }
        self.region
            .lock()
            .shrink(NonNull::new_unchecked(base), layout, new_layout)
            .map(|ptr| ptr.as_mut_ptr().cast::<u8>())
            .unwrap_or(null_mut())
    }
}

unsafe impl<'a, const ALIGN: usize> Allocator for Alloc<'a, ALIGN> where Self: ValidAlign
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>
    {
        let layout = Layout::from_size_align(layout.size(), max(ALIGN, layout.align())).unwrap();
        self.region.lock().allocate(layout)
    }

    unsafe fn deallocate(&self, base: NonNull<u8>, layout: Layout)
    {
        let layout = Layout::from_size_align(layout.size(), max(ALIGN, layout.align())).unwrap();
        self.region.lock().deallocate(base, layout)
    }

    unsafe fn grow(&self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
                   -> Result<NonNull<[u8]>, AllocError>
    {
        let old_layout = Layout::from_size_align(old_layout.size(), max(ALIGN, old_layout.align())).unwrap();
        let new_layout = Layout::from_size_align(new_layout.size(), max(ALIGN, new_layout.align())).unwrap();
        self.region.lock().grow(base, old_layout, new_layout)
    }

    unsafe fn shrink(&self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
                     -> Result<NonNull<[u8]>, AllocError>
    {
        let old_layout = Layout::from_size_align(old_layout.size(), max(ALIGN, old_layout.align())).unwrap();
        let new_layout = Layout::from_size_align(new_layout.size(), max(ALIGN, new_layout.align())).unwrap();
        self.region.lock().shrink(base, old_layout, new_layout)
    }
}

impl<'a> ValidAlign for Alloc<'a, 0x10> {}
impl<'a> ValidAlign for Alloc<'a, 0x40> {}
impl<'a> ValidAlign for Alloc<'a, 0x1000> {}
impl<'a> ValidAlign for Alloc<'a, 0x200000> {}

impl Region
{
    /// Creates and initializes a new allocator region.
    ///
    /// * `range`: The memory range covered by this region.
    ///
    /// Returns the created region.
    const unsafe fn new(range: Range<usize>) -> Lock<Self>
    {
        let this = Self { range, head: None };
        Lock::new(this)
    }

    /// Attempts to allocate memory with the specified layout.
    ///
    /// * `layout`: Layout of the memory to allocate.
    ///
    /// Either returns the allocated memory or an error to signal an out of
    /// memory condition.
    fn allocate(&mut self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>
    {
        let layout = Layout::from_size_align((layout.size() + 0xF) & !0xF, max(layout.align(), 16)).unwrap();
        unsafe {
            let init = || {
                let frag = self.range.start as *mut Fragment;
                *frag = Fragment { next: null_mut(),
                                   size: self.range.end - self.range.start };
                frag
            };
            let head = self.head.get_or_insert_with(init);
            // Find the first fragment that can fit the new allocation.
            let mut current = *head;
            let mut prev = null_mut();
            while !current.is_null() {
                let start = current as usize;
                let end = ((start + (layout.align() - 1)) & !(layout.align() - 1)) + layout.size(); // Size plus space required to align the allocation.
                if (*current).size >= end - start {
                    break;
                }
                prev = current;
                current = (*current).next;
            }
            if current.is_null() {
                return Err(AllocError);
            }
            // At this point we have a free fragment with enough room for the allocation.
            let start = current as usize;
            let end = start + (*current).size;
            let base = (start + (layout.align() - 1)) & !(layout.align() - 1); // Align the base.
            let top = base + layout.size();
            if top < end {
                let next = top as *mut Fragment;
                (*next).next = (*current).next;
                (*next).size = end - top;
                (*current).next = next;
            }
            (*current).size = base - start;
            if (*current).size == 0 {
                if !prev.is_null() {
                    (*prev).next = (*current).next;
                } else {
                    *head = (*current).next;
                }
            }
            let slice = slice_from_raw_parts(base as *mut u8, layout.size());
            let slice = NonNull::from(slice);
            Ok(slice)
        }
    }

    /// Deallocates the memory starting at the specified base address with the
    /// specified layout.
    ///
    /// * `base`: Base address of the memory to deallocate.
    /// * `layout`: Layout of the allocated memory.
    unsafe fn deallocate(&mut self, base: NonNull<u8>, layout: Layout)
    {
        let base = base.addr().get();
        let layout = Layout::from_size_align((layout.size() + 0xF) & !0xF, max(layout.align(), 16)).unwrap();
        let top = base + layout.size();
        let head = self.head
                       .as_mut()
                       .expect("Attempted to deallocate using an uninitialized allocator");
        // Find the next and previous blocks.
        let mut next = *head;
        let mut prev = null_mut();
        while !next.is_null() && (next as usize) < base {
            prev = next;
            next = (*next).next;
        }
        let current = base as *mut Fragment;
        // Check whether the current fragment can be merged with the next.
        if !next.is_null() && next as usize == top {
            (*current).next = (*next).next;
            (*current).size = layout.size() + (*next).size;
        } else {
            (*current).next = next;
            (*current).size = layout.size();
        }
        if !prev.is_null() {
            // Check whether the current fragment can be merged with the previous.
            if prev as usize + (*prev).size == base {
                (*prev).next = (*current).next;
                (*prev).size += (*current).size;
            } else {
                (*prev).next = current;
            }
        } else {
            *head = current;
        }
    }

    /// Attempts to grow the block of memory at the specified base address with
    /// the specified layout to a new layout.
    ///
    /// * `base`: Base address of the memory block to grow.
    /// * `old_layout`: Old layout to grow from.
    /// * `new_layout`: New layout to grow to.
    ///
    /// Either returns the new base or an error to signal an out of memory
    /// condition or an unsupported request.
    unsafe fn grow(&mut self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
                   -> Result<NonNull<[u8]>, AllocError>
    {
        let base = base.addr().get();
        let old_layout =
            Layout::from_size_align((old_layout.size() + 0xF) & !0xF, max(old_layout.align(), 16)).unwrap();
        let new_layout =
            Layout::from_size_align((new_layout.size() + 0xF) & !0xF, max(new_layout.align(), 16)).unwrap();
        if new_layout.size() < old_layout.size() {
            return Err(AllocError);
        }
        if new_layout.size() == old_layout.size() && new_layout.align() == old_layout.align() {
            let slice = slice_from_raw_parts(base as *mut u8, old_layout.size());
            let slice = NonNull::from(slice);
            return Ok(slice);
        }
        let head = self.head
                       .as_mut()
                       .expect("Attempted to reallocate using an uninitialized allocator");
        // Find the previous and next free fragments.
        let mut next = *head;
        let mut prev = null_mut();
        while !next.is_null() {
            if next as usize > base {
                break;
            }
            prev = next;
            next = (*next).next;
        }
        let top = base + old_layout.size();
        let new_top = base + new_layout.size();
        // Check whether the new alignment is compatible with the current base and
        // there's an adjacent free fragment..
        if base & (new_layout.align() - 1) == 0 && top == next as usize {
            // Check whether resizing the next free fragment is enough to fulfill the
            // request.
            if new_top - top < (*next).size {
                let current = next;
                next = new_top as _;
                *next = Fragment { size: (*current).size - (new_top - top),
                                   next: (*current).next };
                if !prev.is_null() {
                    (*prev).next = next
                } else {
                    *head = next
                }
                let slice = slice_from_raw_parts(base as *mut u8, new_layout.size());
                let slice = NonNull::from(slice);
                return Ok(slice);
            }
            // Check whether consuming the next free block entirely is enough to fulfil the
            // request.
            if new_top - top == (*next).size {
                if !prev.is_null() {
                    (*prev).next = (*next).next
                } else {
                    *head = (*next).next
                }
                let slice = slice_from_raw_parts(base as *mut u8, new_layout.size());
                let slice = NonNull::from(slice);
                return Ok(slice);
            }
        }
        // Check whether deallocating and reallocating the block with the new size and
        // alignment won't fail.
        if !prev.is_null() && prev as usize + (*prev).size == base {
            let start = (prev as usize + (new_layout.align() - 1)) & !(new_layout.align() - 1);
            let end = if next as usize == top { top + (*next).size } else { top };
            if end - start >= new_layout.size() {
                let saved = (base as *mut Fragment).read(); // Save this as it will be overwritten by the deallocator.
                self.deallocate(NonNull::new_unchecked(base as *mut u8), old_layout);
                let new_base = self.allocate(new_layout).unwrap().as_mut_ptr().cast::<u8>();
                (new_base as *mut u8).copy_from(base as _, old_layout.size());
                (new_base as *mut Fragment).write(saved);
                let slice = slice_from_raw_parts(new_base as *mut u8, new_layout.size());
                let slice = NonNull::from(slice);
                return Ok(slice);
            }
        }
        // At this point the only option is to allocate a new block, copy everything
        // over, and deallocate the current one.
        let new_base = self.allocate(new_layout)?.as_mut_ptr().cast::<u8>();
        (new_base as *mut u8).copy_from_nonoverlapping(base as _, old_layout.size());
        self.deallocate(NonNull::new_unchecked(base as *mut u8), old_layout);
        let slice = slice_from_raw_parts(new_base as *mut u8, new_layout.size());
        let slice = NonNull::from(slice);
        Ok(slice)
    }

    /// Attempts to shrink the block of memory at the specified base address
    /// from the specified old layout to a new layout.
    ///
    /// * `base`: Base address of the memory block to shrink.
    /// * `old_layout`: Layout to shrink from.
    /// * `new_layout`: Layout to shrink to.
    ///
    /// Either returns the new base or an error to signal an out of memory
    /// condition or an unsupported operation.
    unsafe fn shrink(&mut self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
                     -> Result<NonNull<[u8]>, AllocError>
    {
        let base = base.addr().get();
        let old_layout =
            Layout::from_size_align((old_layout.size() + 0xF) & !0xF, max(old_layout.align(), 16)).unwrap();
        let new_layout =
            Layout::from_size_align((new_layout.size() + 0xF) & !0xF, max(new_layout.align(), 16)).unwrap();
        if new_layout.size() >= old_layout.size() {
            return Err(AllocError);
        }
        let head = self.head
                       .as_mut()
                       .expect("Attempted to reallocate using an uninitialized allocator");
        // Find the previous and next free fragments.
        let mut next = *head;
        let mut prev = null_mut();
        while !next.is_null() {
            if next as usize > base {
                break;
            }
            prev = next;
            next = (*next).next;
        }
        let top = base + old_layout.size();
        let new_top = base + new_layout.size();
        // Check whether the new alignment is compatible with the current base.
        if base & (new_layout.align() - 1) == 0 {
            // Deallocate the extra space.
            let layout = Layout::from_size_align(top - new_top, min(old_layout.align(), new_layout.align())).unwrap();
            self.deallocate(NonNull::new_unchecked(new_top as *mut u8), layout);
            let slice = slice_from_raw_parts(base as *mut u8, new_layout.size());
            let slice = NonNull::from(slice);
            return Ok(slice);
        }
        // Check whether deallocating and reallocating the block with the new size and
        // alignment won't fail.
        if !prev.is_null() && prev as usize + (*prev).size == base {
            let start = (prev as usize + (new_layout.align() - 1)) & !(new_layout.align() - 1);
            let end = if next as usize == top { top + (*next).size } else { top };
            if end - start >= new_layout.size() {
                let saved = (base as *mut Fragment).read(); // Save this as it will be overwritten by the deallocator.
                self.deallocate(NonNull::new_unchecked(base as *mut u8), old_layout);
                let new_base = self.allocate(new_layout).unwrap().as_mut_ptr().cast::<u8>();
                (new_base as *mut u8).copy_from(base as _, new_layout.size());
                (new_base as *mut Fragment).write(saved);
                let slice = slice_from_raw_parts(new_base as *mut u8, new_layout.size());
                let slice = NonNull::from(slice);
                return Ok(slice);
            }
        }
        // At this point the only option is to allocate a new block, copy everything
        // over, and deallocate the current one.
        let new_base = self.allocate(new_layout)?.as_mut_ptr().cast::<u8>();
        (new_base as *mut u8).copy_from_nonoverlapping(base as _, new_layout.size());
        self.deallocate(NonNull::new_unchecked(base as *mut u8), old_layout);
        let slice = slice_from_raw_parts(new_base as *mut u8, new_layout.size());
        let slice = NonNull::from(slice);
        Ok(slice)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[repr(align(0x1000))]
    struct Buffer
    {
        buf: [u8; 0x1000],
    }

    #[derive(Debug)]
    enum BufferProvisionError
    {
        InvalidRange(Range<usize>),
        ShortRange(Range<usize>),
        Overflow(Range<usize>),
        ShortGap(usize),
    }

    #[derive(Debug)]
    enum BufferValidationError
    {
        InvalidRange(Range<usize>),
        ShortRange(Range<usize>),
        Overflow(Range<usize>),
        ShortGap(usize),
        MissingBlock(Range<usize>),
        FragmentMismatch(usize, usize),
        SizeMismatch(usize, usize),
        ExcessBlock(usize),
    }

    #[derive(Debug)]
    enum TestError
    {
        Input(BufferProvisionError),
        Output(BufferValidationError),
        Full,
        Corrupted,
    }

    impl Buffer
    {
        fn new() -> Self
        {
            Self { buf: [0xFF; 0x1000] }
        }

        fn provide(&mut self, region: &Lock<Region>, frags: &[Range<usize>]) -> Result<(), BufferProvisionError>
        {
            let mut offset = 0usize;
            let mut prev = null_mut::<Fragment>();
            let buf = self.buf.as_mut_ptr();
            region.lock().head = Some(null_mut());
            for frag in frags {
                let frag = frag.start .. frag.end;
                if frag.start >= frag.end {
                    return Err(BufferProvisionError::InvalidRange(frag));
                }
                if frag.start + 16 > frag.end {
                    return Err(BufferProvisionError::ShortRange(frag));
                }
                if frag.end > 0x1000 {
                    return Err(BufferProvisionError::Overflow(frag));
                }
                if offset != 0 && frag.start - offset < 16 {
                    return Err(BufferProvisionError::ShortGap(frag.start - offset));
                }
                unsafe {
                    let current = buf.add(frag.start).cast::<Fragment>();
                    *current = Fragment { size: frag.end - frag.start,
                                          next: null_mut() };
                    if !prev.is_null() {
                        (*prev).next = current;
                    } else {
                        region.lock().head = Some(current);
                    }
                    offset += frag.end - frag.start;
                    prev = current;
                }
            }
            Ok(())
        }

        fn validate(&self, region: &Lock<Region>, frags: &[Range<usize>]) -> Result<(), BufferValidationError>
        {
            let mut offset = 0usize;
            let mut current = *region.lock().head.as_ref().unwrap();
            let buf = self.buf.as_ptr();
            for frag in frags {
                let frag = frag.start .. frag.end;
                if frag.start >= frag.end {
                    return Err(BufferValidationError::InvalidRange(frag));
                }
                if frag.start + 16 > frag.end {
                    return Err(BufferValidationError::ShortRange(frag));
                }
                if frag.end > 0x1000 {
                    return Err(BufferValidationError::Overflow(frag));
                }
                if offset != 0 && frag.start - offset < 16 {
                    return Err(BufferValidationError::ShortGap(frag.start - offset));
                }
                unsafe {
                    if current.is_null() {
                        return Err(BufferValidationError::MissingBlock(frag));
                    }
                    if current as usize - buf as usize != frag.start {
                        return Err(BufferValidationError::FragmentMismatch(current as usize - buf as usize,
                                                                           frag.start));
                    }
                    if (*current).size != frag.end - frag.start {
                        return Err(BufferValidationError::SizeMismatch((*current).size, frag.end - frag.start));
                    }
                    current = (*current).next;
                    offset += frag.end - frag.start;
                }
            }
            if !current.is_null() {
                return Err(BufferValidationError::ExcessBlock(current as usize - buf as usize));
            }
            Ok(())
        }

        fn range(&self) -> Range<usize>
        {
            self.buf.as_ptr() as usize .. self.buf.as_ptr() as usize + self.buf.len()
        }
    }

    #[test]
    fn alloc()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_alloc(layout, &[0x0 .. 0x1000], &[0x800 .. 0x1000]).unwrap();
        assert_eq!(base, 0x0);
    }

    #[test]
    fn alloc_tight()
    {
        let layout = Layout::from_size_align(0x1000, 16).unwrap();
        let base = test_alloc(layout, &[0x0 .. 0x1000], &[]).unwrap();
        assert_eq!(base, 0x0);
    }

    #[test]
    fn alloc_align()
    {
        let layout = Layout::from_size_align(0x800, 0x400).unwrap();
        let base = test_alloc(layout, &[0x100 .. 0x1000], &[0x100 .. 0x400, 0xC00 .. 0x1000]).unwrap();
        assert_eq!(base, 0x400);
    }

    #[test]
    fn alloc_align_tight()
    {
        let layout = Layout::from_size_align(0x600, 0x200).unwrap();
        let base = test_alloc(layout, &[0x100 .. 0x700, 0x800 .. 0xE00], &[0x100 .. 0x700]).unwrap();
        assert_eq!(base, 0x800);
    }

    #[test]
    fn alloc_first()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_alloc(layout,
                              &[0x100 .. 0x400, 0x500 .. 0x1000],
                              &[0x100 .. 0x400, 0xD00 .. 0x1000]).unwrap();
        assert_eq!(base, 0x500);
    }

    #[test]
    fn alloc_first_tight()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_alloc(layout, &[0x100 .. 0x400, 0x500 .. 0xD00], &[0x100 .. 0x400]).unwrap();
        assert_eq!(base, 0x500);
    }

    #[test]
    fn alloc_unfit()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let err = test_alloc(layout, &[0x0 .. 0x700, 0x800 .. 0xF00], &[0x0 .. 0x700, 0x800 .. 0xF00]).unwrap_err();
        assert!(matches!(err, TestError::Full));
    }

    #[test]
    fn alloc_full()
    {
        let layout = Layout::from_size_align(0x1000, 16).unwrap();
        let err = test_alloc(layout, &[], &[]).unwrap_err();
        assert!(matches!(err, TestError::Full));
    }

    #[test]
    fn dealloc_tight()
    {
        let layout = Layout::from_size_align(0x1000, 16).unwrap();
        test_dealloc(0x0, layout, &[], &[0x0 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_front()
    {
        let layout = Layout::from_size_align(0x600, 16).unwrap();
        test_dealloc(0x0, layout, &[0xA00 .. 0x1000], &[0x0 .. 0x600, 0xA00 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_front_tight()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        test_dealloc(0x0, layout, &[0x800 .. 0x1000], &[0x0 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_back()
    {
        let layout = Layout::from_size_align(0x600, 16).unwrap();
        test_dealloc(0xA00, layout, &[0x0 .. 0x600], &[0x0 .. 0x600, 0xA00 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_back_tight()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        test_dealloc(0x800, layout, &[0x0 .. 0x800], &[0x0 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_middle()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        test_dealloc(0x400,
                     layout,
                     &[0x0 .. 0x200, 0xE00 .. 0x1000],
                     &[0x0 .. 0x200, 0x400 .. 0xC00, 0xE00 .. 0x1000]).unwrap();
    }

    #[test]
    fn dealloc_middle_tight()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        test_dealloc(0x400, layout, &[0x0 .. 0x400, 0xC00 .. 0x1000], &[0x0 .. 0x1000]).unwrap();
    }

    #[test]
    fn realloc_shwrink()
    {
        let layout = Layout::from_size_align(0x1000, 16).unwrap();
        let base = test_realloc(0x0, layout, 0x800, &[], &[0x800 .. 0x1000]).unwrap();
        assert_eq!(base, 0x0);
    }

    #[test]
    fn realloc_grow()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_realloc(0x0, layout, 0xA00, &[0x800 .. 0x1000], &[0xA00 .. 0x1000]).unwrap();
        assert_eq!(base, 0x0);
    }

    #[test]
    fn realloc_grow_tight()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_realloc(0x0, layout, 0x1000, &[0x800 .. 0x1000], &[]).unwrap();
        assert_eq!(base, 0x0);
    }

    #[test]
    fn realloc_move()
    {
        let layout = Layout::from_size_align(0x800, 16).unwrap();
        let base = test_realloc(0x800, layout, 0xC00, &[0x400 .. 0x800], &[]).unwrap();
        assert_eq!(base, 0x400);
    }

    #[test]
    fn realloc_copy()
    {
        let layout = Layout::from_size_align(0x400, 16).unwrap();
        let base = test_realloc(0x0, layout, 0x600, &[0xA00 .. 0x1000], &[0x0 .. 0x400]).unwrap();
        assert_eq!(base, 0xA00);
    }

    fn test_alloc(layout: Layout, input: &[Range<usize>], output: &[Range<usize>]) -> Result<usize, TestError>
    {
        let mut buf = Buffer::new();
        let region = unsafe { Region::new(buf.range()) };
        let alloc = Alloc::<0x10>::with_region(&region);
        buf.provide(&region, input).map_err(TestError::Input)?;
        let base = unsafe { alloc.alloc(layout) as usize };
        buf.validate(&region, output).map_err(TestError::Output)?;
        if base == 0 {
            return Err(TestError::Full);
        }
        let base = base - buf.range().start;
        Ok(base)
    }

    fn test_dealloc(base: usize, layout: Layout, input: &[Range<usize>], output: &[Range<usize>])
                    -> Result<(), TestError>
    {
        let mut buf = Buffer::new();
        let region = unsafe { Region::new(buf.range()) };
        let alloc = Alloc::<0x10>::with_region(&region);
        buf.provide(&region, input).map_err(TestError::Input)?;
        let base = base + buf.range().start;
        unsafe { alloc.dealloc(base as _, layout) };
        buf.validate(&region, output).map_err(TestError::Output)?;
        Ok(())
    }

    fn test_realloc(base: usize, layout: Layout, new_size: usize, input: &[Range<usize>], output: &[Range<usize>])
                    -> Result<usize, TestError>
    {
        let mut buf = Buffer::new();
        let region = unsafe { Region::new(buf.range()) };
        let alloc = Alloc::<0x10>::with_region(&region);
        buf.provide(&region, input).map_err(TestError::Input)?;
        let base = base + buf.range().start;
        let size = min(layout.size(), new_size);
        for offset in 0 .. size / 2 {
            unsafe { (base as *mut u16).add(offset).write(offset as _) }
        }
        let base = unsafe { alloc.realloc(base as _, layout, new_size) as usize };
        buf.validate(&region, output).map_err(TestError::Output)?;
        if base == 0 {
            return Err(TestError::Full);
        }
        for offset in 0 .. size / 2 {
            if unsafe { (base as *const u16).add(offset).read() } != offset as _ {
                return Err(TestError::Corrupted);
            }
        }
        let base = base - buf.range().start;
        Ok(base)
    }
}
