//! Primitive type extras.

use core::arch::asm;

pub trait FloatExtra
{
    /// Computes the square root of this float.
    ///
    /// Returns the computed result.
    fn sqrt(self) -> Self;

    /// Computes a unit value with the same signal as this value.
    ///
    /// Returns the computed result.
    fn signum(self) -> Self;

    /// Computes the absolute of this value.
    ///
    /// Returns the computed result.
    fn abs(self) -> Self;
}

impl FloatExtra for f32
{
    fn sqrt(self) -> Self
    {
        unsafe {
            let res: f32;
            asm!(
                "fsqrt {val:s}, {val:s}",
                val = inout (vreg) self => res,
                options (pure, nomem, nostack)
            );
            res
        }
    }

    fn signum(self) -> Self
    {
        Self::from_bits(self.to_bits() & 0x80000000 | 0x3F800000)
    }

    fn abs(self) -> Self
    {
        Self::from_bits(self.to_bits() & 0x7FFFFFFF)
    }
}
