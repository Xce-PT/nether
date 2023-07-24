//! Transformations in 3D space.

use core::ops::{Mul, MulAssign};

use super::*;

/// 3D transformation.
#[derive(Clone, Copy, Debug)]
pub struct Transform
{
    /// Position.
    pos: f32x4,
    /// Rotation.
    rot: Quaternion,
    /// Scale.
    scale: f32,
}

impl Transform
{
    /// Creates and initializes a new transformation.
    ///
    /// * `pos`: Position.
    /// * `rot`: Rotation.
    /// * `scale`: Scale.
    ///
    /// Returns the newly created transformation.
    pub fn from_components(pos: f32x4, rot: Quaternion, scale: f32) -> Self
    {
        Self { pos, rot, scale }
    }

    /// computes the reciprocal of this transformation.
    ///
    /// Returns a new transformation with the result.
    pub fn recip(self) -> Self
    {
        let rot = self.rot.recip();
        let scale = self.scale.recip();
        let pos = -(self.pos * rot).mul_scalar(scale);
        Self { pos, rot, scale }
    }

    /// Converts this transformation into a matrix with the same properties.
    ///
    /// Returns a newly created matrix with the results.
    pub fn into_matrix(self) -> f32x4x4
    {
        let rot = self.rot.into_matrix();
        let vec0 = f32x4::from_array([self.scale, 0.0, 0.0, 0.0]);
        let vec1 = f32x4::from_array([0.0, self.scale, 0.0, 0.0]);
        let vec2 = f32x4::from_array([0.0, 0.0, self.scale, 0.0]);
        let vec3 = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        let scale = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        let vec0 = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let vec1 = f32x4::from_array([0.0, 1.0, 0.0, 0.0]);
        let vec2 = f32x4::from_array([0.0, 0.0, 1.0, 0.0]);
        let vec3 = f32x4::from_array([self.pos[0], self.pos[1], self.pos[2], 1.0]);
        let pos = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        rot * scale * pos
    }
}

impl Default for Transform
{
    fn default() -> Self
    {
        Self { pos: f32x4::from_array([0.0, 0.0, 0.0, 1.0]),
               rot: Quaternion::default(),
               scale: 1.0 }
    }
}

impl Mul for Transform
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        let pos = (self.pos * other.rot).mul_scalar(other.scale) + other.pos;
        let rot = self.rot * other.rot;
        let scale = self.scale * other.scale;
        Self { pos, rot, scale }
    }
}

impl MulAssign<Self> for Transform
{
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
    fn into_matrix()
    {
        let pos = f32x4::from_array([2.0, 3.0, 4.0, 0.0]);
        let axis = f32x4::from_array([1.0; 4]);
        let angle = Angle::from(PI * 2.0 / 3.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let scale = 2.0;
        let actual = Transform::from_components(pos, rot, scale).into_matrix();
        let vec0 = f32x4::from_array([0.0, 2.0, 0.0, 0.0]);
        let vec1 = f32x4::from_array([0.0, 0.0, 2.0, 0.0]);
        let vec2 = f32x4::from_array([2.0, 0.0, 0.0, 0.0]);
        let vec3 = f32x4::from_array([2.0, 3.0, 4.0, 1.0]);
        let expected = f32x4x4::from_row_array([vec0, vec1, vec2, vec3]);
        expect_roughly_mat(actual, expected);
    }

    #[test]
    fn mul_recip()
    {
        let vec = f32x4::from_array([2.0, 3.0, 4.0, 0.0]);
        let pos = f32x4::from_array([3.0, 4.0, 5.0, 0.0]);
        let axis = f32x4::from_array([1.0; 4]);
        let angle = Angle::from(PI * 2.0 / 3.0);
        let rot = Quaternion::from_axis_angle(axis, angle);
        let scale = 2.0;
        let lhs = Transform::from_components(pos, rot, scale);
        let rhs = lhs.recip();
        let trans = lhs * rhs;
        let actual = vec.mul_scalar(trans.scale) * trans.rot + trans.pos;
        let expected = vec;
        expect_roughly_vec(actual, expected);
    }
}
