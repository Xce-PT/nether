//! Scalar math.
//!
//! Provides a generic scalar factor.

use core::cmp::{Ordering, PartialOrd};
use core::default::Default;
use core::ops::{Mul, MulAssign, Neg};

use super::*;

/// Scalar factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scalar
{
    /// Internal representation.
    pub(super) val: f32x4,
}

impl Scalar
{
    /// Creates and initializes a new scalar from a value.
    ///
    /// * `val`: Value to represent.
    ///
    /// Returns the newly created scalar.
    pub fn from_val(val: f32) -> Self
    {
        Self { val: f32x4::splat(val) }
    }

    /// Returns self as a reinterpreted angle in radians.
    #[cfg(not(test))]
    pub fn to_angle(self) -> Angle
    {
        Angle::from_radians(self.val[0])
    }
}

impl PartialOrd<Self> for Scalar
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        self.val[0].partial_cmp(&other.val[0])
    }
}

impl Mul<Self> for Scalar
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        Self { val: self.val * other.val }
    }
}

impl Mul<Vector> for Scalar
{
    type Output = Vector;

    fn mul(self, other: Vector) -> Vector
    {
        Vector { vec: self.val * other.vec }
    }
}

impl MulAssign<Self> for Scalar
{
    fn mul_assign(&mut self, other: Self)
    {
        self.val *= other.val;
    }
}

impl Neg for Scalar
{
    type Output = Self;

    fn neg(self) -> Self
    {
        Self { val: -self.val }
    }
}

impl Default for Scalar
{
    fn default() -> Self
    {
        Self { val: f32x4::splat(1.0) }
    }
}
