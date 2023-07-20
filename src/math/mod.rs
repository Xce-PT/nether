//! Linear algebra and trigonometry.

mod angle;
mod mat;
mod proj;
mod quat;
mod trans;
mod vec;

#[cfg(not(test))]
use core::arch::asm;

pub use angle::*;
pub use mat::*;
pub use proj::*;
pub use quat::*;
pub use trans::*;
pub use vec::*;

/// Tolerance for tests and generated values that depend on infinite series.
const TOLERANCE: f32 = 1.0 / 256.0;

/// Extra float functionality for embedded targets.
#[cfg(not(test))]
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

#[cfg(not(test))]
impl FloatExtra for f32
{
    fn sqrt(self) -> Self
    {
        unsafe {
            let res: f32;
            asm!(
                "fmov {tmp:s}, {val:w}",
                "fsqrt {tmp:s}, {tmp:s}",
                "fmov {val:w}, {tmp:s}",
                val = inout (reg) self => res,
                tmp = out (vreg) _,
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

#[cfg(test)]
#[track_caller]
fn expect_roughly(actual: f32, expected: f32)
{
    let passed = (expected - TOLERANCE ..= expected + TOLERANCE).contains(&actual);
    assert!(passed, "Value {actual} isn't anywhere close to {expected}");
}

#[cfg(test)]
#[track_caller]
fn expect_roughly_vec(actual: Vector, expected: Vector)
{
    for idx in 0 .. 4 {
        let actual_val = actual[idx];
        let expected_val = expected[idx];
        let passed = (expected_val - TOLERANCE ..= expected_val + TOLERANCE).contains(&actual_val);
        assert!(passed,
                "Value {actual:?} isn't anywhere close to {expected:?} at index {idx}");
    }
}

#[cfg(test)]
#[track_caller]
fn expect_roughly_mat(actual: Matrix, expected: Matrix)
{
    for idx in 0 .. 16 {
        let actual_val = actual[idx];
        let expected_val = expected[idx];
        let passed = (expected_val - TOLERANCE ..= expected_val + TOLERANCE).contains(&actual_val);
        assert!(passed,
                "Value {actual:?} isn't anywhere close to {expected:?} at index {idx}");
    }
}
