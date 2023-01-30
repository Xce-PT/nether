//! Replacement for the compiler-builtins crate.

#![compiler_builtins]
#![no_builtins]
#![no_std]
#![feature(compiler_builtins)]
#![feature(c_size_t)]

use core::arch::asm;
use core::ffi::{c_int, c_size_t, c_void};

#[no_mangle]
pub unsafe extern "C" fn memcpy(dst: *mut c_void, src: *const c_void, len: c_size_t) -> *mut c_void
{
    let ret = dst;
    let mut dst = dst as usize;
    let mut src = src as usize;
    let end = dst + len as usize;
    if len >= 16 && dst & 0xF == src & 0xF {
        while dst & 0xF != 0 {
            asm!("ldrb {tmp:w}, [{src}], #1", "strb {tmp:w}, [{dst}], #1", tmp = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
        }
        while dst != end & !0xF {
            asm!("ldp {tmp0}, {tmp1}, [{src}], #16", "stp {tmp0}, {tmp1}, [{dst}], #16", tmp0 = out (reg) _, tmp1 = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
        }
    }
    while dst != end {
        asm!("ldrb {tmp:w}, [{src}], #1", "strb {tmp:w}, [{dst}], #1", tmp = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
    }
    ret
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dst: *mut c_void, src: *const c_void, len: c_size_t) -> *mut c_void
{
    if dst as usize <= src as usize || src as usize + len as usize <= dst as usize {
        return memcpy(dst, src, len);
    }
    let ret = dst;
    let end = dst as usize;
    let mut dst = dst as usize + len as usize;
    let mut src = src as usize + len as usize;
    if len >= 16 && dst & 0xF == src & 0xF {
        while dst & 0xF != 0 {
            asm!("ldrb {tmp:w}, [{src}, #-1]!", "strb {tmp:w}, [{dst}, #-1]!", tmp = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
        }
        while dst != (end + 0xF) & !0xF {
            asm!("ldp {tmp0}, {tmp1}, [{src}, #-16]!", "stp {tmp0}, {tmp1}, [{dst}, #-16]!", tmp0 = out (reg) _, tmp1 = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
        }
    }
    while dst != end {
        asm!("ldrb {tmp:w}, [{src}, #-1]!", "strb {tmp:w}, [{dst}, #-1]!", tmp = out (reg) _, src = inout (reg) src, dst = inout (reg) dst, options (preserves_flags));
    }
    ret
}

#[no_mangle]
pub unsafe extern "C" fn memset(buf: *mut c_void, val: c_int, len: c_size_t) -> *mut c_void
{
    let ret = buf;
    let mut buf = buf as usize;
    let end = buf + len as usize;
    if len >= 16 {
        while buf & 0xF != 0 {
            asm!("strb {val:w}, [{buf}], #1", val = in (reg) val, buf = inout (reg) buf, options (preserves_flags));
        }
        let mut val = val as usize & 0xFF;
        val |= val << 8;
        val |= val << 16;
        val |= val << 32;
        while buf != end & !0xF {
            asm!("stp {val}, {val}, [{buf}], #16", val = in (reg) val, buf = inout (reg) buf, options (preserves_flags));
        }
    }
    while buf != end {
        asm!("strb {val:w}, [{buf}], #1", val = in (reg) val, buf = inout (reg) buf, options (preserves_flags));
    }
    ret
}
