//! Vector math.
//!
//! Provides a generic 3D vector with some common vector operations.

use core::default::Default;
use core::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

use super::*;

/// 3D vector.
#[derive(Clone, Copy, Debug)]
pub struct Vector
{
    /// Internal representation.
    pub(super) vec: f32x4,
}

impl Vector
{
    /// Creates and initializes a new vector from its components.
    ///
    /// * `x`: X component.
    /// * `y`: Y component.
    /// * `z`: Z component.
    ///
    /// Returns the newly created vector.
    pub fn from_components(x: f32, y: f32, z: f32) -> Self
    {
        Self { vec: f32x4::from([x, y, z, 0.0]) }
    }

    /// Computes the cross product between this and another vector.
    ///
    /// * `other`: Other vector to compute the cross product with.
    ///
    /// Returns the resulting vector.
    #[cfg(not(test))]
    pub fn cross(self, other: Self) -> Self
    {
        Self { vec: cross(self.vec, other.vec) }
    }

    /// Computes the length of this vector.
    ///
    /// Returns the computed length.
    #[cfg(not(test))]
    pub fn length(self) -> Scalar
    {
        Scalar { val: f32x4::splat(len(self.vec)) }
    }

    /// Computes the squared distance between this and another vector.
    ///
    /// * `other`: Other vector to compute the squared distance to.
    ///
    /// Returns the computed squared distance.
    #[cfg(not(test))]
    pub fn sq_distance(self, other: Self) -> Scalar
    {
        Scalar { val: f32x4::splat(sq_dist(self.vec, other.vec)) }
    }

    /// Computes the linear interpolation between this and another vector.
    ///
    /// * `other`: Destination vector.
    /// * `bias`: The bias towards either vector.
    ///
    /// Returns the computed interpolation.
    #[cfg(not(test))]
    pub fn lerp(self, other: Self, bias: Scalar) -> Self
    {
        self + (other - self) * bias
    }
}

impl Add<Self> for Vector
{
    type Output = Self;

    fn add(self, other: Self) -> Self
    {
        Self { vec: self.vec + other.vec }
    }
}

impl AddAssign<Self> for Vector
{
    fn add_assign(&mut self, other: Self)
    {
        self.vec += other.vec;
    }
}

impl Sub<Self> for Vector
{
    type Output = Self;

    fn sub(self, other: Self) -> Self
    {
        Self { vec: self.vec - other.vec }
    }
}

impl SubAssign<Self> for Vector
{
    fn sub_assign(&mut self, other: Self)
    {
        self.vec -= other.vec;
    }
}

impl Mul<Self> for Vector
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        Self { vec: self.vec * other.vec }
    }
}

impl Mul<Scalar> for Vector
{
    type Output = Self;

    fn mul(self, other: Scalar) -> Self
    {
        Self { vec: self.vec * other.val }
    }
}

impl MulAssign<Self> for Vector
{
    fn mul_assign(&mut self, other: Self)
    {
        self.vec *= other.vec;
    }
}

impl MulAssign<Scalar> for Vector
{
    fn mul_assign(&mut self, other: Scalar)
    {
        self.vec *= other.val;
    }
}

impl Default for Vector
{
    fn default() -> Self
    {
        Self { vec: f32x4::splat(0.0) }
    }
}
