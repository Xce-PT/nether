//! Rotations in 3D space.

use core::ops::{Mul, MulAssign};

use super::*;

/// Rotation quaternion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Quaternion
{
    /// Supporting vector.
    pub(super) vec: Vector,
}

impl Quaternion
{
    /// Creates and initializes a new quaternion from a given axis and angle.
    ///
    /// * `axis`: Rotation axis.
    /// * `angle`: Rotation angle.
    ///
    /// Returns the newly created quaternion.
    pub fn from_axis_angle(axis: Vector, angle: Angle) -> Self
    {
        let w = angle.w.abs();
        let mut vec = axis * angle.w.signum();
        vec[3] = 0.0;
        let prop = (1.0 - w * w).sqrt();
        let len = vec.length();
        if len == 0.0 {
            return Self::default();
        }
        vec = vec / len * prop;
        vec[3] = w;
        Self { vec }
    }

    /// Computes the reciprocal of this quaternion.
    ///
    /// Returns a newly created quaternion with the results.
    pub fn recip(self) -> Self
    {
        Self { vec: Vector::from([-self.vec[0], -self.vec[1], -self.vec[2], self.vec[3]]) }
    }

    /// Computes a rotation matrix with the same properties as this quaternion.
    ///
    /// Returns the newly created matrix.
    pub fn into_matrix(self) -> Matrix
    {
        let (qx, qy, qz, qw) = (self.vec[0], self.vec[1], self.vec[2], self.vec[3]);
        let vec0 = Vector::from([qw, qz, -qy, qx]);
        let vec1 = Vector::from([-qz, qw, qx, qy]);
        let vec2 = Vector::from([qy, -qx, qw, qz]);
        let vec3 = Vector::from([-qx, -qy, -qz, qw]);
        let lhs = Matrix::from([vec0, vec1, vec2, vec3]);
        let vec0 = Vector::from([qw, qz, -qy, -qx]);
        let vec1 = Vector::from([-qz, qw, qx, -qy]);
        let vec2 = Vector::from([qy, -qx, qw, -qz]);
        let vec3 = Vector::from([qx, qy, qz, qw]);
        let rhs = Matrix::from([vec0, vec1, vec2, vec3]);
        lhs * rhs
    }
}

impl Default for Quaternion
{
    fn default() -> Self
    {
        Self { vec: Vector::from([0.0, 0.0, 0.0, 1.0]) }
    }
}

impl Mul for Quaternion
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self
    {
        let (qx, qy, qz, qw) = (other.vec[0], other.vec[1], other.vec[2], other.vec[3]);
        let vec0 = Vector::from([qw, qz, -qy, -qx]);
        let vec1 = Vector::from([-qz, qw, qx, -qy]);
        let vec2 = Vector::from([qy, -qx, qw, -qz]);
        let vec3 = Vector::from([qx, qy, qz, qw]);
        let mat = Matrix::from([vec0, vec1, vec2, vec3]);
        let vec = self.vec * mat;
        let len = vec.length();
        if len == 0.0 {
            return Self::default();
        }
        Self { vec: vec / len }
    }
}

impl Mul<Quaternion> for Vector
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Quaternion) -> Self
    {
        let mat = other.into_matrix();
        let mut vec = self;
        let w = vec[3];
        vec[3] = 0.0;
        vec = vec * mat;
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

impl MulAssign<Quaternion> for Vector
{
    #[inline]
    fn mul_assign(&mut self, other: Quaternion)
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
        let axis = Vector::from([0.0, 0.0, 0.0, 0.0]);
        let angle = Angle::from(0.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = Vector::from([0.0, 0.0, 0.0, 1.0]);
        expect_roughly_vec(actual.vec, expected);
        let axis = Vector::from([1.0, 1.0, 1.0, 1.0]);
        let angle = Angle::from(PI * 2.0 / 3.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = Vector::from([0.5; 4]);
        expect_roughly_vec(actual.vec, expected);
        let angle = Angle::from(-PI * 2.0 / 3.0);
        let actual = Quaternion::from_axis_angle(axis, angle);
        let expected = Vector::from([-0.5, -0.5, -0.5, 0.5]);
        expect_roughly_vec(actual.vec, expected);
    }

    #[test]
    fn into_matrix()
    {
        let quat = Quaternion { vec: Vector::from([0.5, 0.5, 0.5, 0.5]) };
        let actual = quat.into_matrix();
        let vec0 = Vector::from([0.0, 1.0, 0.0, 0.0]);
        let vec1 = Vector::from([0.0, 0.0, 1.0, 0.0]);
        let vec2 = Vector::from([1.0, 0.0, 0.0, 0.0]);
        let vec3 = Vector::from([0.0, 0.0, 0.0, 1.0]);
        let expected = Matrix::from([vec0, vec1, vec2, vec3]);
        expect_roughly_mat(actual, expected);
    }

    #[test]
    fn mul()
    {
        let lhs = Quaternion { vec: Vector::from([0.5f32.sqrt(), 0.0, 0.0, 0.5f32.sqrt()]) };
        let rhs = Quaternion { vec: Vector::from([0.0, 0.5f32.sqrt(), 0.0, 0.5f32.sqrt()]) };
        let actual = lhs * rhs;
        let expected = Vector::from([0.5, 0.5, -0.5, 0.5]);
        expect_roughly_vec(actual.vec, expected);
        let lhs = Quaternion { vec: expected };
        let rhs = Quaternion { vec: Vector::from([0.0, 0.0, 0.5f32.sqrt(), 0.5f32.sqrt()]) };
        let actual = lhs * rhs;
        let expected = Vector::from([0.0, 0.5f32.sqrt(), 0.0, 0.5f32.sqrt()]);
        expect_roughly_vec(actual.vec, expected);
    }

    #[test]
    fn vec_mul()
    {
        let lhs = Vector::from([2.0, 3.0, 4.0, 1.0]);
        let rhs = Quaternion { vec: Vector::from([0.5, 0.5, 0.5, 0.5]) };
        let actual = lhs * rhs;
        let expected = Vector::from([4.0, 2.0, 3.0, 1.0]);
        expect_roughly_vec(actual, expected);
    }
}
