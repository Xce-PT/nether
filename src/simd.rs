//! Portable SIMD extras.

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use core::arch::aarch64::*;
#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use core::mem::transmute;
use core::ops::{Mul, MulAssign};
use core::simd::prelude::*;
#[cfg(all(test, not(all(target_arch = "aarch64", target_feature = "neon"))))]
use std::simd::StdFloat;

#[cfg(not(test))]
use crate::prim::FloatExtra;

/// SIMD matrix type.
#[allow(non_camel_case_types)]
pub type f32x4x4 = Matrix;

/// Row-major 4x4 floating point SIMD matrix.
#[repr(align(0x40), C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix(f32x4, f32x4, f32x4, f32x4);

/// Extra functionality for the floating point SIMD implementations used in this
/// project.
pub trait SimdFloatExtra: SimdFloat
{
    /// Computes a fast reciprocal estimate of all lanes in this vector.
    ///
    /// Returns the computed result.
    fn fast_recip(self) -> Self;

    /// Computes a fast reciprocal square root estimate of all lanes in this
    /// vector.
    ///
    /// Returns the computed result.
    fn fast_sqrt_recip(self) -> Self;

    /// Computes a vector with the same direction as this vector and length 1.0.
    ///
    /// Returns the computed result.
    fn normalize(self) -> Option<Self>;

    /// Computes the length of this vector.
    ///
    /// Returns the computed result.
    fn len(self) -> f32;

    /// Computes the square of the length of this vector.
    ///
    /// Returns the computed result.
    fn sq_len(self) -> f32;

    /// Computes the multiple of a vector and a scalar.
    ///
    /// * `other`: Scalar to multiply by.
    ///
    /// Returns the computed result.
    fn mul_scalar(self, other: f32) -> Self;

    /// Computes the multiple of a vector and a lane of another vector.
    ///
    /// * `lane`: Index of the lane on the other vector.
    /// * `other`: Vector containing the lane to multiply by.
    ///
    /// Returns the computed result.
    fn mul_lane<const LANE: i32>(self, other: Self) -> Self;

    /// Multiplies this vector by a matrix.
    ///
    /// Returns the resulting vector.
    fn mul_mat(self, other: f32x4x4) -> Self;

    /// Computes the cross and dot products between this and another vector,
    /// ignoring the last lane.
    ///
    /// * `other`: Other vector to computed the cross and dot products with.
    ///
    /// Returns the cross product in the first three lanes and the dot product
    /// in the last lane of a vector.
    fn cross_dot(self, other: Self) -> Self;

    /// Computes a vector resulting from multiplying two vectors and adding the
    /// result to this vector.
    ///
    /// * `left`: Left side of the multiplication.
    /// * `right`: Right side of the multiplication.
    ///
    /// Returns the computed result.
    fn fused_mul_add(self, left: Self, right: Self) -> Self;

    /// Computes a vector resulting from multiplying a vector by a lane of
    /// another vector and adding the result to this vector.
    ///
    /// * `left`: Left side of the multiplication.
    /// * `right`: Right side of the multiplication.
    ///
    /// Returns the computed result.
    fn fused_mul_add_lane<const LANE: i32>(self, left: Self, right: Self) -> Self;

    /// Replaces a value in a lane of this vector.
    ///
    /// Returns a vector with the value replaced.
    fn replace_lane<const LANE: i32>(self, scalar: f32) -> Self;
}

pub trait SimdPartialEqExtra: SimdPartialEq
{
    /// Checks all lanes of self for equality to zero.
    ///
    /// Returns a vector with the results.
    fn simd_eqz(self) -> mask32x4;
}

pub trait SimdPartialOrdExtra: SimdPartialOrd
{
    /// Checks whether all lanes of self are greater than zero.
    ///
    /// Returns a vector with the results.
    fn simd_gtz(self) -> mask32x4;

    /// Checks whether all lanes of self are less than zero.
    ///
    /// Returns a vector with the results.
    fn simd_ltz(self) -> mask32x4;

    /// Checks whether all lanes of self are greater than or equal to zero.
    ///
    /// Returns a vector with the results.
    fn simd_gez(self) -> mask32x4;
}

impl Matrix
{
    /// Creates and initializes a new identity matrix.
    ///
    /// Returns the newly created matrix.
    pub const fn new() -> Self
    {
        Self(f32x4::from_array([1.0, 0.0, 0.0, 0.0]),
             f32x4::from_array([0.0, 1.0, 0.0, 0.0]),
             f32x4::from_array([0.0, 0.0, 1.0, 0.0]),
             f32x4::from_array([0.0, 0.0, 0.0, 1.0]))
    }

    /// Creates and initializes a new matrix from vectors representing its rows.
    ///
    /// * `rows`: Vectors for each row.
    ///
    /// Returns the newly created matrix.
    #[inline(always)]
    pub const fn from_row_array(rows: [f32x4; 4]) -> Self
    {
        Self(rows[0], rows[1], rows[2], rows[3])
    }

    /// Returns a copy of the element at the specified index.
    #[cfg(test)]
    pub fn get(self, idx: usize) -> f32
    {
        match idx {
            0 ..= 3 => self.0[idx],
            4 ..= 7 => self.1[idx & 0x3],
            8 ..= 11 => self.2[idx & 0x3],
            12 ..= 15 => self.3[idx & 0x3],
            _ => panic!("Index {idx} is out of bounds"),
        }
    }
}

impl Default for Matrix
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl From<[f32x4; 4]> for Matrix
{
    fn from(rows: [f32x4; 4]) -> Self
    {
        Self::from_row_array(rows)
    }
}

impl Mul for Matrix
{
    type Output = Self;

    #[inline(always)]
    fn mul(self, other: Self) -> Self
    {
        let r0 = self.0.mul_mat(other);
        let r1 = self.1.mul_mat(other);
        let r2 = self.2.mul_mat(other);
        let r3 = self.3.mul_mat(other);
        Self::from_row_array([r0, r1, r2, r3])
    }
}

impl MulAssign for Matrix
{
    #[inline(always)]
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

impl SimdFloatExtra for f32x4
{
    #[inline(always)]
    fn fast_recip(self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vrecpeq_f32(this);
            let step = vrecpsq_f32(res, this);
            let res = vmulq_f32(step, res);
            let step = vrecpsq_f32(res, this);
            let res = vmulq_f32(step, res);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self.recip()
        }
    }

    #[inline(always)]
    fn fast_sqrt_recip(self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vrsqrteq_f32(this);
            let res = vrsqrtsq_f32(res, this);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self.sqrt().recip()
        }
    }

    #[inline(always)]
    fn normalize(self) -> Option<Self>
    {
        let len = self.len();
        if len == 0.0 {
            return None;
        }
        Some(self.mul_scalar(len.recip()))
    }

    #[inline(always)]
    fn len(self) -> f32
    {
        self.sq_len().sqrt()
    }

    #[inline(always)]
    fn sq_len(self) -> f32
    {
        (self * self).reduce_sum()
    }

    #[inline(always)]
    fn mul_scalar(self, other: f32) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vmulq_n_f32(this, other);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self * f32x4::splat(other)
        }
    }

    #[inline(always)]
    fn mul_lane<const LANE: i32>(self, other: Self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let that = transmute::<Self, float32x4_t>(other);
            let res = vmulq_laneq_f32(this, that, LANE);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self * f32x4::splat(other[LANE as usize])
        }
    }

    #[inline(always)]
    fn mul_mat(self, other: f32x4x4) -> Self
    {
        let res = other.0.mul_lane::<0>(self);
        let res = res.fused_mul_add_lane::<1>(other.1, self);
        let res = res.fused_mul_add_lane::<2>(other.2, self);
        res.fused_mul_add_lane::<3>(other.3, self)
    }

    #[inline(always)]
    fn cross_dot(self, other: Self) -> Self
    {
        let (x, y, z) = (other[0], other[1], other[2]);
        let m0 = f32x4::from_array([0.0, -z, y, x]);
        let m1 = f32x4::from_array([z, 0.0, -x, y]);
        let m2 = f32x4::from_array([-y, x, 0.0, z]);
        let m3 = f32x4::splat(0.0);
        let that = f32x4x4::from_row_array([m0, m1, m2, m3]);
        self.mul_mat(that)
    }

    #[inline(always)]
    fn fused_mul_add(self, left: Self, right: Self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let left = transmute::<Self, float32x4_t>(left);
            let right = transmute::<Self, float32x4_t>(right);
            let res = vmlaq_f32(this, left, right);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self + left * right
        }
    }

    #[inline(always)]
    fn fused_mul_add_lane<const LANE: i32>(self, left: Self, right: Self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let left = transmute::<Self, float32x4_t>(left);
            let right = transmute::<Self, float32x4_t>(right);
            let res = vmlaq_laneq_f32(this, left, right, LANE);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            self + left * f32x4::splat(right[LANE as usize])
        }
    }

    #[inline(always)]
    fn replace_lane<const LANE: i32>(self, scalar: f32) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vsetq_lane_f32(scalar, this, LANE);
            transmute::<float32x4_t, Self>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let mut this = self;
            this[LANE as usize] = scalar;
            this
        }
    }
}

impl SimdPartialEqExtra for f32x4
{
    #[inline(always)]
    fn simd_eqz(self) -> mask32x4
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vceqzq_f32(this);
            transmute::<uint32x4_t, mask32x4>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let zero = f32x4::splat(0.0);
            self.simd_eq(zero)
        }
    }
}

impl SimdPartialOrdExtra for f32x4
{
    #[inline(always)]
    fn simd_gtz(self) -> mask32x4
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vcgtzq_f32(this);
            transmute::<uint32x4_t, mask32x4>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let zero = f32x4::splat(0.0);
            self.simd_gt(zero)
        }
    }

    #[inline(always)]
    fn simd_ltz(self) -> mask32x4
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vcltzq_f32(this);
            transmute::<uint32x4_t, mask32x4>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let zero = f32x4::splat(0.0);
            self.simd_lt(zero)
        }
    }

    #[inline(always)]
    fn simd_gez(self) -> mask32x4
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<Self, float32x4_t>(self);
            let res = vcgezq_f32(this);
            transmute::<uint32x4_t, mask32x4>(res)
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let zero = f32x4::splat(0.0);
            self.simd_ge(zero)
        }
    }
}

#[cfg(test)]
mod tests
{
    use core::simd::u32x4;

    use super::*;

    const PRECISION_MASK: u32x4 = u32x4::from_array([0xFFFF0000; 4]);

    #[test]
    fn f32x4_fast_recip()
    {
        let actual = f32x4::splat(2.0).fast_recip().to_bits() & PRECISION_MASK;
        let expected = f32x4::splat(0.5).to_bits() & PRECISION_MASK;
        assert_eq!(actual,
                   expected,
                   "Actual: {}, Expected: {}",
                   f32::from_bits(actual[0]),
                   f32::from_bits(expected[0]));
    }

    #[test]
    fn f32x4_fast_sqrt_recip()
    {
        let actual = f32x4::splat(4.0).fast_sqrt_recip().to_bits() & PRECISION_MASK;
        let expected = f32x4::splat(0.5).to_bits() & PRECISION_MASK;
        assert_eq!(actual,
                   expected,
                   "Actual: {}, Expected: {}",
                   f32::from_bits(actual[0]),
                   f32::from_bits(expected[0]));
    }

    #[test]
    fn f32x4_normalize()
    {
        let actual = f32x4::splat(8.0).normalize().unwrap();
        let expected = f32x4::splat(0.5);
        assert_eq!(actual, expected);
        let passed = f32x4::splat(0.0).normalize().is_none();
        assert!(passed);
    }

    #[test]
    fn f32x4_len()
    {
        let actual = f32x4::splat(8.0).len();
        let expected = 16.0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_sq_len()
    {
        let actual = f32x4::splat(8.0).sq_len();
        let expected = 256.0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_mul_scalar()
    {
        let actual = f32x4::splat(2.0).mul_scalar(3.0);
        let expected = f32x4::splat(6.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_mul_lane()
    {
        let left = f32x4::splat(2.0);
        let right = f32x4::from_array([1.0, 2.0, 3.0, 4.0]);
        let actual = left.mul_lane::<2>(right);
        let expected = f32x4::splat(6.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_mul_mat()
    {
        let left = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let r0 = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        let r1 = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let r2 = f32x4::from_array([0.0, 1.0, 0.0, 0.0]);
        let r3 = f32x4::from_array([0.0, 0.0, 1.0, 0.0]);
        let right = f32x4x4::from_row_array([r0, r1, r2, r3]);
        let actual = left.mul_mat(right);
        let expected = r0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_cross_dot()
    {
        let left = f32x4::from_array([2.0, 0.0, 0.0, 1.0]);
        let right = f32x4::from_array([0.0, 2.0, 0.0, 1.0]);
        let actual = left.cross_dot(right);
        let expected = f32x4::from_array([0.0, 0.0, 4.0, 0.0]);
        assert_eq!(actual, expected);
        let left = f32x4::from_array([0.0, 0.0, 2.0, 1.0]);
        let right = f32x4::from_array([2.0, 0.0, 0.0, 1.0]);
        let actual = left.cross_dot(right);
        let expected = f32x4::from_array([0.0, 4.0, 0.0, 0.0]);
        assert_eq!(actual, expected);
        let left = f32x4::from_array([0.0, 2.0, 0.0, 1.0]);
        let right = f32x4::from_array([0.0, 0.0, 2.0, 1.0]);
        let actual = left.cross_dot(right);
        let expected = f32x4::from_array([4.0, 0.0, 0.0, 0.0]);
        assert_eq!(actual, expected);
        let left = f32x4::from_array([2.0, 4.0, 8.0, 1.0]);
        let right = left;
        let actual = left.cross_dot(right);
        let expected = f32x4::from_array([0.0, 0.0, 0.0, 84.0]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_fused_mul_add()
    {
        let base = f32x4::splat(4.0);
        let left = f32x4::splat(2.0);
        let right = f32x4::splat(3.0);
        let actual = base.fused_mul_add(left, right);
        let expected = f32x4::splat(10.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_fused_mul_add_lane()
    {
        let base = f32x4::splat(4.0);
        let left = f32x4::splat(2.0);
        let right = f32x4::from_array([1.0, 2.0, 3.0, 4.0]);
        let actual = base.fused_mul_add_lane::<2>(left, right);
        let expected = f32x4::splat(10.0);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_replace_lane()
    {
        let actual = f32x4::splat(1.0).replace_lane::<2>(2.0);
        let expected = f32x4::from_array([1.0, 1.0, 2.0, 1.0]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4x4_mul()
    {
        let left = f32x4x4::new();
        let r0 = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        let r1 = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let r2 = f32x4::from_array([0.0, 1.0, 0.0, 0.0]);
        let r3 = f32x4::from_array([0.0, 0.0, 1.0, 0.0]);
        let right = f32x4x4::from_row_array([r0, r1, r2, r3]);
        let actual = left * right;
        let actual = [actual.0, actual.1, actual.2, actual.3];
        let expected = [right.0, right.1, right.2, right.3];
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_simd_eqz()
    {
        let actual = f32x4::from_array([1.0, 0.0, 1.0, 0.0]).simd_eqz();
        let expected = mask32x4::from_array([false, true, false, true]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_simd_gez()
    {
        let actual = f32x4::from_array([1.0, 0.0, -1.0, 0.0]).simd_gez();
        let expected = mask32x4::from_array([true, true, false, true]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_simd_gtz()
    {
        let actual = f32x4::from_array([1.0, 0.0, 1.0, 0.0]).simd_gtz();
        let expected = mask32x4::from_array([true, false, true, false]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn f32x4_simd_ltz()
    {
        let actual = f32x4::from_array([-1.0, 0.0, -1.0, 0.0]).simd_ltz();
        let expected = mask32x4::from_array([true, false, true, false]);
        assert_eq!(actual, expected);
    }
}
