//! First fit free list memory allocator.

use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::cmp::{max, min};
use core::ops::Range;
use core::ptr::{null_mut, NonNull};
use core::slice::from_raw_parts as slice_from_raw_parts;

use crate::sync::{Lock, LockGuard};
#[cfg(not(test))]
use crate::{DMA_RANGE, HEAP_RANGE};

/// Heap allocator instance.
#[cfg(not(test))]
#[global_allocator]
pub static HEAP: Engine = Engine::new(&HEAP_STATE);
/// DMA allocator instance.
#[cfg(not(test))]
pub static DMA: Engine = Engine::new(&DMA_STATE);

/// Heap allocator state.
#[cfg(not(test))]
static HEAP_STATE: State = unsafe { State::new(HEAP_RANGE) };
/// DMA allocator state.
#[cfg(not(test))]
static DMA_STATE: State = unsafe { State::new(DMA_RANGE) };

/// Free list allocator engine.
#[derive(Clone, Copy, Debug)]
pub struct Engine<'a>
{
    /// Shared state of all copies of this allocator.
    state: &'a State,
}

/// Shared allocator state.
#[derive(Debug)]
struct State
{
    /// Range covered by all alocator instances sharing this state.
    range: Range<usize>,
    /// Head of the list of free fragments.
    head: Lock<Fragment>,
}

/// Free memory fragment.
#[derive(Debug)]
struct Fragment
{
    /// Size of this fragment.
    size: usize,
    /// Next fragment.
    next: *mut Fragment,
}

impl<'a> Engine<'a>
{
    /// Creates and initializes a new allocator core.
    ///
    /// * `state`: Shared state of all instances of this allocator.
    ///
    /// Returns the created allocator.
    const fn new(state: &'a State) -> Self
    {
        Self { state }
    }
}

unsafe impl<'a> Allocator for Engine<'a>
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>
    {
        let layout = Layout::from_size_align((layout.size() + 0xF) & !0xF, max(layout.align(), 16)).unwrap();
        unsafe {
            let mut head = self.state.head();
            // Find the first fragment that can fit the new allocation.
            let mut current = head.next;
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
                    (*prev).next = (*current).next
                } else {
                    head.next = (*current).next
                }
            }
            let slice = slice_from_raw_parts(base as *mut u8, layout.size());
            let slice = NonNull::from(slice);
            Ok(slice)
        }
    }

    unsafe fn deallocate(&self, base: NonNull<u8>, layout: Layout)
    {
        let base = base.addr().get();
        let layout = Layout::from_size_align((layout.size() + 0xF) & !0xF, max(layout.align(), 16)).unwrap();
        let top = base + layout.size();
        let mut head = self.state.head.lock();
        // Find the next and previous blocks.
        let mut next = head.next;
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
            head.next = current;
        }
    }

    unsafe fn grow(&self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
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
        let mut head = self.state.head.lock();
        // Find the previous and next free fragments.
        let mut next = head.next;
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
                    head.next = next
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
                    head.next = (*next).next
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

    unsafe fn shrink(&self, base: NonNull<u8>, old_layout: Layout, new_layout: Layout)
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
        let head = self.state.head.lock();
        // Find the previous and next free fragments.
        let mut next = head.next;
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

unsafe impl<'a> GlobalAlloc for Engine<'a>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8
    {
        self.allocate(layout)
            .map(|base| base.as_mut_ptr().cast::<u8>())
            .unwrap_or(null_mut())
    }

    unsafe fn dealloc(&self, base: *mut u8, layout: Layout)
    {
        self.deallocate(NonNull::new_unchecked(base), layout);
    }

    unsafe fn realloc(&self, base: *mut u8, layout: Layout, new_size: usize) -> *mut u8
    {
        let new_layout = Layout::from_size_align(new_size, layout.align()).unwrap();
        if new_size >= layout.size() {
            return self.grow(NonNull::new_unchecked(base), layout, new_layout)
                       .map(|ptr| ptr.as_mut_ptr().cast::<u8>())
                       .unwrap_or(null_mut());
        }
        self.shrink(NonNull::new_unchecked(base), layout, new_layout)
            .map(|ptr| ptr.as_mut_ptr().cast::<u8>())
            .unwrap_or(null_mut())
    }
}

impl State
{
    /// Creates and initializes a new allocator shared state.
    ///
    /// * `range`: The memory range covered by all the core allocator instances
    ///   sharing this state.
    ///
    /// Returns the created state.
    const unsafe fn new(range: Range<usize>) -> Self
    {
        Self { range,
               head: Lock::new(Fragment { size: 0,
                                          next: null_mut() }) }
    }

    /// Initializes the allocator's state if necessary,
    ///
    /// Returns a locked head to the initialized state.
    fn head(&self) -> LockGuard<Fragment>
    {
        let mut head = self.head.lock();
        if head.size == 0 {
            let fragment = self.range.start as *mut Fragment;
            let size = self.range.end - self.range.start;
            unsafe {
                *fragment = Fragment { size: self.range.end - self.range.start,
                                       next: null_mut() }
            };
            *head = Fragment { size, next: fragment };
        }
        head
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

        fn provide(&mut self, state: &State, frags: &[Range<usize>]) -> Result<(), BufferProvisionError>
        {
            let mut offset = 0usize;
            let mut prev = null_mut::<Fragment>();
            let buf = self.buf.as_mut_ptr();
            *state.head.lock() = Fragment { size: 0x1000,
                                            next: null_mut() };
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
                        (*prev).next = current
                    } else {
                        state.head.lock().next = current
                    };
                    offset += frag.end - frag.start;
                    prev = current;
                }
            }
            Ok(())
        }

        fn validate(&self, state: &State, frags: &[Range<usize>]) -> Result<(), BufferValidationError>
        {
            let mut offset = 0usize;
            let mut current = state.head.lock().next;
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
        let state = unsafe { State::new(buf.range()) };
        let alloc = Engine::new(&state);
        buf.provide(&state, input).map_err(TestError::Input)?;
        let base = unsafe { alloc.alloc(layout) as usize };
        buf.validate(&state, output).map_err(TestError::Output)?;
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
        let state = unsafe { State::new(buf.range()) };
        let alloc = Engine::new(&state);
        buf.provide(&state, input).map_err(TestError::Input)?;
        let base = base + buf.range().start;
        unsafe { alloc.dealloc(base as _, layout) };
        buf.validate(&state, output).map_err(TestError::Output)?;
        Ok(())
    }

    fn test_realloc(base: usize, layout: Layout, new_size: usize, input: &[Range<usize>], output: &[Range<usize>])
                    -> Result<usize, TestError>
    {
        let mut buf = Buffer::new();
        let state = unsafe { State::new(buf.range()) };
        let alloc = Engine::new(&state);
        buf.provide(&state, input).map_err(TestError::Input)?;
        let base = base + buf.range().start;
        let size = min(layout.size(), new_size);
        for offset in 0 .. size / 2 {
            unsafe { (base as *mut u16).add(offset).write(offset as _) }
        }
        let base = unsafe { alloc.realloc(base as _, layout, new_size) as usize };
        buf.validate(&state, output).map_err(TestError::Output)?;
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
