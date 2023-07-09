//! General purpose 4 component vector.

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use core::arch::aarch64::*;
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use core::mem::transmute;
use core::ops::{Add, AddAssign, Div, DivAssign, Index, IndexMut, Mul, MulAssign, Neg, Sub, SubAssign};
use core::simd::{f32x4, SimdFloat};

use super::*;

/// 4D vector.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Vector
{
    /// Raw vector.
    pub(super) raw: f32x4,
}

impl Vector
{
    /// Computes the length of this vector.
    ///
    /// Returns the computed result.
    #[inline]
    pub fn length(self) -> f32
    {
        self.sq_length().sqrt()
    }

    /// Computes the square of the length of this vector.
    ///
    /// Returns the computed result.
    #[inline]
    pub fn sq_length(self) -> f32
    {
        (self.raw * self.raw).reduce_sum()
    }

    /// Computes the cross and dot products between this and another vector.
    ///
    /// * `other`: Right hand side vector.
    ///
    /// Returns the computed result with the corss product in the first three
    /// components and the dot product in the fourth component.
    ///
    /// The fourth dimension of both vectors is ignored, since the cross product
    /// is not possible in four dimensions.
    pub fn cross_dot(self, other: Self) -> Self
    {
        let this = Self::from([self.raw[0], self.raw[1], self.raw[2], 0.0]);
        let (x, y, z) = (other.raw[0], other.raw[1], other.raw[2]);
        let vec0 = Self::from([0.0, -z, y, x]);
        let vec1 = Self::from([z, 0.0, -x, y]);
        let vec2 = Self::from([-y, x, 0.0, z]);
        let vec3 = Self::from([x, y, z, 0.0]);
        let that = Matrix::from([vec0, vec1, vec2, vec3]);
        this * that
    }

    /// Computes the linear interpolation between two vectors.
    ///
    /// * `other`: Right hand side vector.
    /// * `bias`: Influence of each side in the interpolation.
    ///
    /// Returns the computed result.
    #[cfg(not(test))]
    pub fn lerp(self, other: Self, bias: f32) -> Self
    {
        self + (other - self) * bias
    }

    /// Returns the inner intrinsic SIMD data.
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    #[inline]
    pub fn into_intrinsic(self) -> float32x4_t
    {
        unsafe { transmute(self.raw) }
    }

    /// Returns the inner portable SIMD data.
    #[cfg(not(test))]
    #[inline]
    pub fn into_simd(self) -> f32x4
    {
        self.raw
    }
}

impl From<[f32; 4]> for Vector
{
    #[inline]
    fn from(scals: [f32; 4]) -> Self
    {
        Self { raw: f32x4::from(scals) }
    }
}

impl Default for Vector
{
    fn default() -> Self
    {
        Self::from([0.0, 0.0, 0.0, 0.0])
    }
}

impl Add for Vector
{
    type Output = Self;

    #[inline]
    fn add(self, other: Self) -> Self
    {
        Self { raw: self.raw + other.raw }
    }
}

impl AddAssign for Vector
{
    #[inline]
    fn add_assign(&mut self, other: Self)
    {
        *self = *self + other;
    }
}

impl Mul for Vector
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self
    {
        Self { raw: self.raw * other.raw }
    }
}

impl Mul<f32> for Vector
{
    type Output = Self;

    #[inline]
    fn mul(self, other: f32) -> Self
    {
        Self { raw: self.raw * f32x4::splat(other) }
    }
}

impl MulAssign for Vector
{
    #[inline]
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

impl MulAssign<f32> for Vector
{
    #[inline]
    fn mul_assign(&mut self, other: f32)
    {
        *self = *self * other;
    }
}

impl Sub for Vector
{
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self
    {
        Self { raw: self.raw - other.raw }
    }
}

impl SubAssign for Vector
{
    #[inline]
    fn sub_assign(&mut self, other: Self)
    {
        *self = *self - other;
    }
}

impl Div for Vector
{
    type Output = Self;

    #[inline]
    fn div(self, other: Self) -> Self
    {
        Self { raw: self.raw / other.raw }
    }
}

impl Div<f32> for Vector
{
    type Output = Self;

    #[inline]
    fn div(self, other: f32) -> Self
    {
        Self { raw: self.raw / f32x4::splat(other) }
    }
}

impl DivAssign for Vector
{
    #[inline]
    fn div_assign(&mut self, other: Self)
    {
        *self = *self / other;
    }
}

impl DivAssign<f32> for Vector
{
    #[inline]
    fn div_assign(&mut self, other: f32)
    {
        *self = *self / other;
    }
}

impl Neg for Vector
{
    type Output = Self;

    #[inline]
    fn neg(self) -> Self
    {
        Self { raw: -self.raw }
    }
}

impl Index<usize> for Vector
{
    type Output = f32;

    #[inline]
    #[track_caller]
    fn index(&self, idx: usize) -> &f32
    {
        self.raw.index(idx)
    }
}

impl IndexMut<usize> for Vector
{
    #[inline]
    #[track_caller]
    fn index_mut(&mut self, idx: usize) -> &mut f32
    {
        self.raw.index_mut(idx)
    }
}

#[cfg(test)]
mod tests
{
    use core::f32::consts::PI;

    use super::*;

    #[test]
    fn cross_dot()
    {
        let x = Vector::from([1.0, 0.0, 0.0, 1.0]);
        let y = Vector::from([0.0, 1.0, 0.0, 1.0]);
        let actual = x.cross_dot(y);
        let expected = Vector::from([0.0, 0.0, 1.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let z = Vector::from([0.0, 0.0, 1.0, 1.0]);
        let actual = y.cross_dot(z);
        let expected = Vector::from([1.0, 0.0, 0.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let actual = z.cross_dot(x);
        let expected = Vector::from([0.0, 1.0, 0.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let actual = y.cross_dot(x);
        let expected = Vector::from([0.0, 0.0, -1.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let actual = z.cross_dot(y);
        let expected = Vector::from([-1.0, 0.0, 0.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let actual = x.cross_dot(z);
        let expected = Vector::from([0.0, -1.0, 0.0, 0.0]);
        expect_roughly_vec(actual, expected);
        let actual = x.cross_dot(x);
        let expected = Vector::from([0.0, 0.0, 0.0, 1.0]);
        expect_roughly_vec(actual, expected);
        let (sin, cos) = (PI / 8.0).sin_cos();
        let (cross, dot) = (PI / 4.0).sin_cos();
        let lhs = Vector::from([cos, sin, 0.0, 1.0]);
        let rhs = Vector::from([sin, cos, 0.0, 1.0]);
        let actual = lhs.cross_dot(rhs);
        let expected = Vector::from([0.0, 0.0, cross, dot]);
        expect_roughly_vec(actual, expected);
        let lhs = Vector::from([0.0, cos, sin, 1.0]);
        let rhs = Vector::from([0.0, sin, cos, 1.0]);
        let actual = lhs.cross_dot(rhs);
        let expected = Vector::from([cross, 0.0, 0.0, dot]);
        expect_roughly_vec(actual, expected);
        let lhs = Vector::from([sin, 0.0, cos, 1.0]);
        let rhs = Vector::from([cos, 0.0, sin, 1.0]);
        let actual = lhs.cross_dot(rhs);
        let expected = Vector::from([0.0, cross, 0.0, dot]);
        expect_roughly_vec(actual, expected);
    }
}
