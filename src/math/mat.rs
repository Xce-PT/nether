//! Transformation matrix math.
//!
//! Flexible packed transformation that can include translations, rotations, and
//! scales in any order.

use core::default::Default;
use core::mem::swap;
use core::ops::{Mul, MulAssign};

use super::*;

/// Transformation matrix.
#[derive(Clone, Copy, Debug)]
pub struct Matrix
{
    /// Actual transformation matrix.
    pub(super) transform: [f32x4; 4],
    /// The rotation part of the transformation matrix.
    pub(super) rot: [f32x4; 4],
}

impl Matrix
{
    /// Creates and initializes a new matrix from the provided components.
    ///
    /// * `pos`: Translation vector.
    /// * `rot`: Rotation quaternion.
    /// * `scale`: Scale scalar.
    ///
    /// Returns the newly created matrix.
    pub fn from_components(pos: Vector, rot: Quaternion, scale: Scalar) -> Self
    {
        let (qx, qy, qz, qw) = (rot.quat[0], rot.quat[1], rot.quat[2], rot.quat[3]);
        let rot0 = f32x4::from([qw, qz, -qy, -qx]);
        let rot1 = f32x4::from([-qz, qw, qx, -qy]);
        let rot2 = f32x4::from([qy, -qx, qw, -qz]);
        let rot3 = f32x4::from([qx, qy, qz, qw]);
        let lhs = [rot0, rot1, rot2, rot3];
        let rot0 = f32x4::from([qw, qz, -qy, qx]);
        let rot1 = f32x4::from([-qz, qw, qx, qy]);
        let rot2 = f32x4::from([qy, -qx, qw, qz]);
        let rot3 = f32x4::from([-qx, -qy, -qz, qw]);
        let rhs = [rot0, rot1, rot2, rot3];
        let rot = mat_mul(lhs, rhs);
        let mut transform = rot;
        transform[0] *= scale.val;
        transform[1] *= scale.val;
        transform[2] *= scale.val;
        transform[3] += pos.vec;
        Self { transform, rot }
    }

    /// Computes the reciprocal or inverse of this matrix.
    ///
    /// Returns the computed reciprocal.
    pub fn recip(self) -> Self
    {
        let sq_recip = f32x4::splat(sq_len(self.transform[0]).recip());
        let mut transform0 = self.transform[0] * sq_recip;
        let mut transform1 = self.transform[1] * sq_recip;
        let mut transform2 = self.transform[2] * sq_recip;
        let mut transform3 = f32x4::from([0.0, 0.0, 0.0, -1.0]);
        swap(&mut transform0[1], &mut transform1[0]);
        swap(&mut transform0[2], &mut transform2[0]);
        swap(&mut transform1[2], &mut transform2[1]);
        transform3 = mat_vec_mul([transform0, transform1, transform2, transform3], -self.transform[3]);
        let mut rot0 = self.rot[0];
        let mut rot1 = self.rot[1];
        let mut rot2 = self.rot[2];
        swap(&mut rot0[1], &mut rot1[0]);
        swap(&mut rot0[2], &mut rot2[0]);
        swap(&mut rot1[2], &mut rot2[1]);
        let transform = [transform0, transform1, transform2, transform3];
        let rot = [rot0, rot1, rot2, self.rot[3]];
        Self { transform, rot }
    }
}

impl Mul<Self> for Matrix
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        Self { transform: mat_mul(self.transform, other.transform),
               rot: mat_mul(self.rot, other.rot) }
    }
}

impl Mul<Vector> for Matrix
{
    type Output = Vector;

    fn mul(self, other: Vector) -> Vector
    {
        let mut vec = other.vec;
        vec[3] = 1.0; // Needed for translation.
        vec = mat_vec_mul(self.transform, vec);
        vec[3] = 0.0;
        Vector { vec }
    }
}

impl Mul<Normal> for Matrix
{
    type Output = Normal;

    fn mul(self, other: Normal) -> Normal
    {
        Normal { vec: normalize(mat_vec_mul(self.rot, other.vec)),
                 weight: other.weight }
    }
}

impl MulAssign<Self> for Matrix
{
    fn mul_assign(&mut self, other: Self)
    {
        self.transform = mat_mul(self.transform, other.transform);
    }
}

impl Default for Matrix
{
    fn default() -> Self
    {
        let mat0 = f32x4::from([1.0, 0.0, 0.0, 0.0]);
        let mat1 = f32x4::from([0.0, 1.0, 0.0, 0.0]);
        let mat2 = f32x4::from([0.0, 0.0, 1.0, 0.0]);
        let mat3 = f32x4::from([0.0, 0.0, 0.0, 1.0]);
        let mat = [mat0, mat1, mat2, mat3];
        Self { transform: mat,
               rot: mat }
    }
}

#[cfg(test)]
mod tests
{
    use core::f32::consts::{FRAC_PI_2, FRAC_PI_3};

    use super::*;

    #[test]
    fn matrix_translate()
    {
        let pos = Vector::from_components(1.0, 2.0, 3.0);
        let rot = Quaternion::default();
        let scale = Scalar::default();
        let transform = Matrix::from_components(pos, rot, scale);
        let point = Vector::from_components(0.0, 0.0, 0.0);
        let point = transform * point;
        assert!(is_roughly(point.vec, f32x4::from([1.0, 2.0, 3.0, 0.0])));
        let pos = Vector::from_components(-1.0, -2.0, -3.0);
        let transform = Matrix::from_components(pos, rot, scale) * transform;
        let point = Vector::from_components(0.0, 0.0, 0.0);
        let point = transform * point;
        assert!(is_roughly(point.vec, f32x4::from([0.0, 0.0, 0.0, 0.0])));
    }

    #[test]
    fn matrix_rotate()
    {
        let pos = Vector::default();
        let axis = Vector::from_components(1.0, 0.0, 0.0);
        let angle = Angle::from_radians(FRAC_PI_2);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let scale = Scalar::default();
        let transform = Matrix::from_components(pos, rot, scale);
        let point0 = Vector::from_components(1.0, 0.0, 0.0);
        let point1 = Vector::from_components(0.0, 1.0, 0.0);
        let point2 = Vector::from_components(0.0, 0.0, 1.0);
        let point0t = transform * point0;
        let point1t = transform * point1;
        let point2t = transform * point2;
        assert!(is_roughly(point0t.vec, f32x4::from([1.0, 0.0, 0.0, 0.0])));
        assert!(is_roughly(point1t.vec, f32x4::from([0.0, 0.0, 1.0, 0.0])));
        assert!(is_roughly(point2t.vec, f32x4::from([0.0, -1.0, 0.0, 0.0])));
        let axis = Vector::from_components(0.0, 1.0, 0.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let transform = Matrix::from_components(pos, rot, scale) * transform;
        let point0t = transform * point0;
        let point1t = transform * point1;
        let point2t = transform * point2;
        assert!(is_roughly(point0t.vec, f32x4::from([0.0, 0.0, -1.0, 0.0])));
        assert!(is_roughly(point1t.vec, f32x4::from([1.0, 0.0, 0.0, 0.0])));
        assert!(is_roughly(point2t.vec, f32x4::from([0.0, -1.0, 0.0, 0.0])));
        let axis = Vector::from_components(0.0, 0.0, 1.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let transform = Matrix::from_components(pos, rot, scale) * transform;
        let point0t = transform * point0;
        let point1t = transform * point1;
        let point2t = transform * point2;
        assert!(is_roughly(point0t.vec, f32x4::from([0.0, 0.0, -1.0, 0.0])));
        assert!(is_roughly(point1t.vec, f32x4::from([0.0, 1.0, 0.0, 0.0])));
        assert!(is_roughly(point2t.vec, f32x4::from([1.0, 0.0, 0.0, 0.0])));
    }

    #[test]
    fn matrix_scale()
    {
        let pos = Vector::default();
        let rot = Quaternion::default();
        let scale = Scalar::from_val(2.0);
        let transform = Matrix::from_components(pos, rot, scale);
        let point = Vector::from_components(1.0, 2.0, 3.0);
        let res = transform * point;
        assert!(is_roughly(res.vec, f32x4::from([2.0, 4.0, 6.0, 0.0])));
        let scale = Scalar::from_val(0.5);
        let transform = Matrix::from_components(pos, rot, scale) * transform;
        let res = transform * point;
        assert!(is_roughly(res.vec, f32x4::from([1.0, 2.0, 3.0, 0.0])));
    }

    #[test]
    fn matrix_combined_components()
    {
        let pos = Vector::from_components(1.0, 2.0, 3.0);
        let axis = Vector::from_components(1.0, 1.0, -1.0);
        let angle = Angle::from_radians(FRAC_PI_3 * 2.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let scale = Scalar::from_val(2.0);
        let transform = Matrix::from_components(pos, rot, scale);
        let point = Vector::from_components(1.0, 0.0, 0.0);
        let point = transform * point;
        assert!(is_roughly(point.vec, f32x4::from([1.0, 2.0, 1.0, 0.0])));
        let point = Vector::from_components(0.0, 1.0, 0.0);
        let point = transform * point;
        assert!(is_roughly(point.vec, f32x4::from([3.0, 2.0, 3.0, 0.0])));
        let point = Vector::from_components(0.0, 0.0, 1.0);
        let point = transform * point;
        assert!(is_roughly(point.vec, f32x4::from([1.0, 0.0, 3.0, 0.0])));
    }

    #[test]
    fn matrix_inverse()
    {
        let pos = Vector::from_components(1.0, 2.0, 3.0);
        let axis = Vector::from_components(1.0, 1.0, -1.0);
        let angle = Angle::from_radians(FRAC_PI_3 * 2.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let scale = Scalar::from_val(2.0);
        let transform = Matrix::from_components(pos, rot, scale);
        let transform = transform * transform.recip();
        assert!(is_roughly(transform.transform[0], f32x4::from([1.0, 0.0, 0.0, 0.0])));
        assert!(is_roughly(transform.transform[1], f32x4::from([0.0, 1.0, 0.0, 0.0])));
        assert!(is_roughly(transform.transform[2], f32x4::from([0.0, 0.0, 1.0, 0.0])));
        assert!(is_roughly(transform.transform[3], f32x4::from([0.0, 0.0, 0.0, 1.0])));
    }
}
