//! General purpose 4x4 matrix.

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use core::arch::aarch64::*;
use core::mem::transmute;
use core::ops::{Index, IndexMut, Mul, MulAssign};

use super::*;

/// 4x4 matrix.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Matrix
{
    /// Raw matrix elements.
    #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
    raw: float32x4x4_t,
    #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
    raw: [Vector; 4],
}

impl From<[Vector; 4]> for Matrix
{
    fn from(vecs: [Vector; 4]) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            Self { raw: float32x4x4_t(vecs[0].into_intrinsic(),
                                      vecs[1].into_intrinsic(),
                                      vecs[2].into_intrinsic(),
                                      vecs[3].into_intrinsic()) }
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            assert!(cfg!(test), "Build target not configured correctly");
            Self { raw: vecs }
        }
    }
}

impl Default for Matrix
{
    fn default() -> Self
    {
        let vec0 = Vector::from([1.0, 0.0, 0.0, 0.0]);
        let vec1 = Vector::from([0.0, 1.0, 0.0, 0.0]);
        let vec2 = Vector::from([0.0, 0.0, 1.0, 0.0]);
        let vec3 = Vector::from([0.0, 0.0, 0.0, 1.0]);
        Self::from([vec0, vec1, vec2, vec3])
    }
}

impl Mul for Matrix
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Self) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let vec0 = vmulq_laneq_f32(other.raw.0, self.raw.0, 0);
            let vec1 = vmulq_laneq_f32(other.raw.0, self.raw.1, 0);
            let vec2 = vmulq_laneq_f32(other.raw.0, self.raw.2, 0);
            let vec3 = vmulq_laneq_f32(other.raw.0, self.raw.3, 0);
            let vec0 = vmlaq_laneq_f32(vec0, other.raw.1, self.raw.0, 1);
            let vec1 = vmlaq_laneq_f32(vec1, other.raw.1, self.raw.1, 1);
            let vec2 = vmlaq_laneq_f32(vec2, other.raw.1, self.raw.2, 1);
            let vec3 = vmlaq_laneq_f32(vec3, other.raw.1, self.raw.3, 1);
            let vec0 = vmlaq_laneq_f32(vec0, other.raw.2, self.raw.0, 2);
            let vec1 = vmlaq_laneq_f32(vec1, other.raw.2, self.raw.1, 2);
            let vec2 = vmlaq_laneq_f32(vec2, other.raw.2, self.raw.2, 2);
            let vec3 = vmlaq_laneq_f32(vec3, other.raw.2, self.raw.3, 2);
            let vec0 = vmlaq_laneq_f32(vec0, other.raw.3, self.raw.0, 3);
            let vec1 = vmlaq_laneq_f32(vec1, other.raw.3, self.raw.1, 3);
            let vec2 = vmlaq_laneq_f32(vec2, other.raw.3, self.raw.2, 3);
            let vec3 = vmlaq_laneq_f32(vec3, other.raw.3, self.raw.3, 3);
            Self { raw: float32x4x4_t(vec0, vec1, vec2, vec3) }
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let mut raw = [Vector::from([0.0; 4]); 4];
            for x in 0 .. 4 {
                for y in 0 .. 4 {
                    let scal = self.raw[x][y];
                    raw[x] += other.raw[y] * Vector::from([scal; 4]);
                }
            }
            Self { raw }
        }
    }
}

impl Mul<Matrix> for Vector
{
    type Output = Self;

    #[inline]
    fn mul(self, other: Matrix) -> Self
    {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        unsafe {
            let this = transmute::<_, float32x4_t>(self.raw);
            let that = other.raw;
            let raw = vmulq_laneq_f32(that.0, this, 0);
            let raw = vmlaq_laneq_f32(raw, that.1, this, 1);
            let raw = vmlaq_laneq_f32(raw, that.2, this, 2);
            let raw = vmlaq_laneq_f32(raw, that.3, this, 3);
            Self { raw: transmute(raw) }
        }
        #[cfg(not(all(target_arch = "aarch64", target_feature = "neon")))]
        {
            let mut res = Self::from([0.0; 4]);
            for x in 0 .. 4 {
                let scal = self[x];
                res += other.raw[x] * Self::from([scal; 4]);
            }
            res
        }
    }
}

impl MulAssign for Matrix
{
    #[inline]
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

impl MulAssign<Matrix> for Vector
{
    #[inline]
    fn mul_assign(&mut self, other: Matrix)
    {
        *self = *self * other;
    }
}

impl Index<usize> for Matrix
{
    type Output = f32;

    #[inline]
    #[track_caller]
    fn index(&self, idx: usize) -> &f32
    {
        unsafe { transmute::<_, &[f32; 16]>(&self.raw).index(idx) }
    }
}

impl IndexMut<usize> for Matrix
{
    #[inline]
    #[track_caller]
    fn index_mut(&mut self, idx: usize) -> &mut f32
    {
        unsafe { transmute::<_, &mut [f32; 16]>(&mut self.raw).index_mut(idx) }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn mul()
    {
        let vec0 = Vector::from([0.5, 0.5, -0.5, 0.5]);
        let vec1 = Vector::from([-0.5, 0.5, 0.5, 0.5]);
        let vec2 = Vector::from([0.5, -0.5, 0.5, 0.5]);
        let vec3 = Vector::from([-0.5, -0.5, -0.5, 0.5]);
        let lhs = Matrix::from([vec0, vec1, vec2, vec3]);
        let vec0 = Vector::from([0.5, 0.5, -0.5, -0.5]);
        let vec1 = Vector::from([-0.5, 0.5, 0.5, -0.5]);
        let vec2 = Vector::from([0.5, -0.5, 0.5, -0.5]);
        let vec3 = Vector::from([0.5, 0.5, 0.5, 0.5]);
        let rhs = Matrix::from([vec0, vec1, vec2, vec3]);
        let actual = lhs * rhs;
        let vec0 = Vector::from([0.0, 1.0, 0.0, 0.0]);
        let vec1 = Vector::from([0.0, 0.0, 1.0, 0.0]);
        let vec2 = Vector::from([1.0, 0.0, 0.0, 0.0]);
        let vec3 = Vector::from([0.0, 0.0, 0.0, 1.0]);
        let expected = Matrix::from([vec0, vec1, vec2, vec3]);
        expect_roughly_mat(actual, expected);
        let vec0 = Vector::from([0.5, -0.5, 0.5, -0.5]);
        let vec1 = Vector::from([0.5, 0.5, -0.5, -0.5]);
        let vec2 = Vector::from([-0.5, 0.5, 0.5, -0.5]);
        let vec3 = Vector::from([0.5, 0.5, 0.5, 0.5]);
        let rhs = Matrix::from([vec0, vec1, vec2, vec3]);
        let actual = lhs * rhs;
        let expected = Matrix::default();
        expect_roughly_mat(actual, expected);
    }

    #[test]
    fn vec_mul()
    {
        let vec = Vector::from([2.0, 3.0, 4.0, 1.0]);
        let vec0 = Vector::from([0.0, 2.0, 0.0, 0.0]);
        let vec1 = Vector::from([0.0, 0.0, 2.0, 0.0]);
        let vec2 = Vector::from([2.0, 0.0, 0.0, 0.0]);
        let vec3 = Vector::from([3.0, 4.0, 5.0, 1.0]);
        let mat = Matrix::from([vec0, vec1, vec2, vec3]);
        let actual = vec * mat;
        let expected = Vector::from([11.0, 8.0, 11.0, 1.0]);
        expect_roughly_vec(actual, expected);
    }
}
