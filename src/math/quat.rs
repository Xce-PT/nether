//! Rotation quaternion math.
//!
//! Provides a packed and non-redundant 3D transformation.

use core::default::Default;
use core::ops::{Mul, MulAssign};

use super::*;

/// Rotation quaternion.
#[derive(Clone, Copy, Debug)]
pub struct Quaternion
{
    /// Internal representation.
    pub(super) quat: f32x4,
}

impl Quaternion
{
    /// Creates and initializes a new quaternion from an axis and angle.
    ///
    /// * `axis`: Rotation axis.
    /// * `angle`: Rotation angle.
    ///
    /// Returns the newly created quaternion.
    pub fn from_axis_angle(axis: Vector, angle: Angle) -> Self
    {
        if axis.vec == f32x4::from([0.0, 0.0, 0.0, 0.0]) {
            return Self { quat: f32x4::from([0.0, 0.0, 0.0, 1.0]) };
        }
        let angle = angle.angle * 0.5;
        let sin = f32x4::splat(angle.sin());
        let cos = angle.cos();
        let mut quat = normalize(axis.vec) * sin;
        quat[3] = cos;
        Self { quat }
    }

    /// Creates and initializes a quaternion from two normals representing a
    /// change in orientation.
    ///
    /// * `new`: New orientation.
    /// * `old`: Old orientation.
    ///
    /// Returns the resulting quaternion.
    #[cfg(not(test))]
    pub fn from_normals(old: Normal, new: Normal) -> Self
    {
        let cos = dot(old.vec, new.vec);
        let axis = cross(old.vec, new.vec);
        if axis.abs().reduce_max() == 0.0 {
            // Not possible to determine the axis of rotation, so assume no rotation.
            return Self { quat: f32x4::from([0.0, 0.0, 0.0, 1.0]) };
        }
        let angle = cos.acos() * 0.5;
        let (sin, cos) = angle.sin_cos();
        let axis = axis * f32x4::splat(sin);
        Self { quat: f32x4::from([axis[0], axis[1], axis[2], cos]) }
    }
}

impl Mul<Self> for Quaternion
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        let (qx, qy, qz, qw) = (self.quat[0], self.quat[1], self.quat[2], self.quat[3]);
        let m0 = f32x4::from([qw, qz, -qy, -qx]);
        let m1 = f32x4::from([-qz, qw, qx, -qy]);
        let m2 = f32x4::from([qy, -qx, qw, -qz]);
        let m3 = f32x4::from([qx, qy, qz, qw]);
        let m = [m0, m1, m2, m3];
        let quat = mat_vec_mul(m, other.quat);
        Self { quat: normalize(quat) }
    }
}

impl MulAssign<Self> for Quaternion
{
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

impl Default for Quaternion
{
    fn default() -> Self
    {
        Self { quat: f32x4::from([0.0, 0.0, 0.0, 1.0]) }
    }
}

#[cfg(test)]
mod tests
{
    use core::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    use super::*;

    #[test]
    fn quaternion_multiply()
    {
        let axis = Vector::from_components(1.0, 0.0, 0.0);
        let angle = Angle::from_radians(FRAC_PI_2);
        let rot = Quaternion::from_axis_angle(axis, angle);
        assert!(is_roughly(rot.quat, f32x4::from([FRAC_PI_4.sin(), 0.0, 0.0, FRAC_PI_4.cos()])));
        let axis = Vector::from_components(0.0, 1.0, 0.0);
        let rot = Quaternion::from_axis_angle(axis, angle) * rot;
        assert!(is_roughly(rot.quat, f32x4::from([0.5, 0.5, -0.5, 0.5])));
        let axis = Vector::from_components(0.0, 0.0, 1.0);
        let rot = Quaternion::from_axis_angle(axis, angle) * rot;
        assert!(is_roughly(rot.quat, f32x4::from([0.0, FRAC_PI_4.sin(), 0.0, FRAC_PI_4.cos()])));
    }
}
