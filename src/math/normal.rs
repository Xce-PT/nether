//! Unit vector math.
//!
//! Implements a unit 3D vector representing a direction.

use core::ops::{Add, AddAssign, Mul};

use super::*;

/// Normal vector.
#[derive(Clone, Copy, Debug)]
pub struct Normal
{
    /// Internal representation.
    pub(super) vec: f32x4,
    /// Weight of this normal in an addition.
    pub(super) weight: f32x4,
}

#[cfg(not(test))]
impl Normal
{
    /// Creates and initializes a new normal from a regular vector.
    ///
    /// * `vec`: The vector whose direction is to be extracted.
    ///
    /// Returns the newly created normal.
    pub fn from_vec(vec: Vector) -> Self
    {
        Self { vec: normalize(vec.vec),
               weight: f32x4::splat(1.0) }
    }
}

impl Add<Self> for Normal
{
    type Output = Self;

    fn add(self, other: Self) -> Self
    {
        Self { vec: normalize(self.vec * self.weight + other.vec * other.weight),
               weight: self.weight + other.weight }
    }
}

impl AddAssign<Self> for Normal
{
    fn add_assign(&mut self, other: Self)
    {
        *self = *self + other;
    }
}

impl Mul<Scalar> for Normal
{
    type Output = Self;

    fn mul(self, other: Scalar) -> Self
    {
        Self { vec: self.vec,
               weight: self.weight * other.val }
    }
}
