//! Rotations in 3D space.

use core::ops::{Mul, MulAssign};

use super::*;

/// Rotation quaternion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Quaternion
{
    /// Supporting vector.
    pub(super) vec: f32x4,
}

impl Quaternion
{
    /// Creates and initializes a new quaternion from a given axis and angle.
    ///
    /// * `axis`: Rotation axis.
    /// * `angle`: Rotation angle.
    ///
    /// Returns the newly created quaternion.
    pub fn from_axis_angle(axis: f32x4, angle: Angle) -> Self
    {
        let w = angle.w.abs();
        let mut vec = axis.mul_scalar(angle.w.signum());
        vec[3] = 0.0;
        let prop = (1.0 - w * w).sqrt();
        let Some(mut vec) = vec.normalize() else {
            return Self::default();
        };
        vec = vec.mul_scalar(prop);
        vec[3] = w;
        Self { vec }
    }

    /// Computes the reciprocal of this quaternion.
    ///
    /// Returns a newly created quaternion with the results.
    pub fn recip(self) -> Self
    {
        Self { vec: f32x4::from_array([-self.vec[0], -self.vec[1], -self.vec[2], self.vec[3]]) }
    }

    /// Computes a rotation matrix with the same properties as this quaternion.
    ///
    /// Returns the newly created matrix.
    pub fn into_matrix(self) -> f32x4x4
    {
        let (qx, qy, qz, qw) = (self.vec[0], self.vec[1], self.vec[2], self.vec[3]);
        let vec0 = f32x4::from_array([qw, qz, -qy, qx]);
        let vec1 = f32x4::from_array([-qz, qw, qx, qy]);
        let vec2 = f32x4::from_array([qy, -qx, qw, qz]);
        let vec3 = f32x4::from_array([-qx, -qy, -qz, qw]);
        let lhs = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        let vec0 = f32x4::from_array([qw, qz, -qy, -qx]);
        let vec1 = f32x4::from_array([-qz, qw, qx, -qy]);
        let vec2 = f32x4::from_array([qy, -qx, qw, -qz]);
        let vec3 = f32x4::from_array([qx, qy, qz, qw]);
        let rhs = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        lhs * rhs
    }
}

impl Default for Quaternion
{
    fn default() -> Self
    {
        Self { vec: f32x4::from_array([0.0, 0.0, 0.0, 1.0]) }
    }
}

impl Mul for Quaternion
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self
    {
        let (qx, qy, qz, qw) = (other.vec[0], other.vec[1], other.vec[2], other.vec[3]);
        let vec0 = f32x4::from_array([qw, qz, -qy, -qx]);
        let vec1 = f32x4::from_array([-qz, qw, qx, -qy]);
        let vec2 = f32x4::from_array([qy, -qx, qw, -qz]);
        let vec3 = f32x4::from_array([qx, qy, qz, qw]);
        let mat = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        let vec = self.vec.mul_mat(mat);
        let Some(vec) = vec.normalize() else {
            return Self::default();
        };
        Self { vec }
    }
}

impl Mul<Quaternion> for f32x4
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Quaternion) -> Self
    {
        let mat = other.into_matrix();
        let mut vec = self;
        let w = vec[3];
        vec[3] = 0.0;
        vec = vec.mul_mat(mat);
        vec[3] = w;
        vec
    }
}

impl MulAssign for Quaternion
{
    #[inline]
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

#[cfg(test)]
mod tests
{
    use core::f32::consts::PI;

    use super::*;

    #[test]
    fn from_axis_angle()
    {
        let axis = f32x4::from_array([0.0, 0.0, 0.0, 0.0]);
        let angle = Angle::from(0.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        expect_roughly_vec(actual.vec, expected);
        let axis = f32x4::from_array([1.0, 1.0, 1.0, 1.0]);
        let angle = Angle::from(PI * 2.0 / 3.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = f32x4::from_array([0.5; 4]);
        expect_roughly_vec(actual.vec, expected);
        let angle = Angle::from(-PI * 2.0 / 3.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = f32x4::from_array([-0.5, -0.5, -0.5, 0.5]);
        expect_roughly_vec(actual.vec, expected);
    }

    #[test]
    fn into_matrix()
    {
        let quat = Quaternion { vec: f32x4::from_array([0.5, 0.5, 0.5, 0.5]) };
        let actual = quat.into_matrix();
        let vec0 = f32x4::from_array([0.0, 1.0, 0.0, 0.0]);
        let vec1 = f32x4::from_array([0.0, 0.0, 1.0, 0.0]);
        let vec2 = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let vec3 = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        let expected = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        expect_roughly_mat(actual, expected);
    }

    #[test]
    fn mul()
    {
        let lhs = Quaternion { vec: f32x4::from_array([0.5f32.sqrt(), 0.0, 0.0, 0.5f32.sqrt()]) };
        let rhs = Quaternion { vec: f32x4::from_array([0.0, 0.5f32.sqrt(), 0.0, 0.5f32.sqrt()]) };
        let actual = lhs * rhs;
        let expected = f32x4::from_array([0.5, 0.5, -0.5, 0.5]);
        expect_roughly_vec(actual.vec, expected);
        let lhs = Quaternion { vec: expected };
        let rhs = Quaternion { vec: f32x4::from_array([0.0, 0.0, 0.5f32.sqrt(), 0.5f32.sqrt()]) };
        let actual = lhs * rhs;
        let expected = f32x4::from_array([0.0, 0.5f32.sqrt(), 0.0, 0.5f32.sqrt()]);
        expect_roughly_vec(actual.vec, expected);
    }

    #[test]
    fn vec_mul()
    {
        let lhs = f32x4::from_array([2.0, 3.0, 4.0, 1.0]);
        let rhs = Quaternion { vec: f32x4::from_array([0.5, 0.5, 0.5, 0.5]) };
        let actual = lhs * rhs;
        let expected = f32x4::from_array([4.0, 2.0, 3.0, 1.0]);
        expect_roughly_vec(actual, expected);
    }
}
