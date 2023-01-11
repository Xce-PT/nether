//! Linear algebra math.

mod angle;
#[cfg(not(test))]
mod color;
mod mat;
mod normal;
mod proj;
mod quat;
mod scalar;
mod triang;
mod vec;

#[cfg(not(test))]
use core::arch::asm;
#[cfg(not(test))]
use core::f32::consts::{FRAC_PI_2, PI};
#[cfg(test)]
use core::simd::SimdPartialOrd;
use core::simd::{f32x4, SimdFloat};

pub use self::angle::*;
#[cfg(not(test))]
pub use self::color::*;
pub use self::mat::*;
pub use self::normal::*;
pub use self::proj::*;
pub use self::quat::*;
pub use self::scalar::*;
pub use self::triang::*;
pub use self::vec::*;

#[cfg(not(test))]
pub trait SoftFloatOps: Copy + Sized
{
    /// Computes an approximation of the cosine of this value in radians.
    ///
    /// Returns the computed cosine.
    fn cos(self) -> Self;

    /// Computes an approximation of the sine of this value in radians.
    ///
    /// Returns the computed sine.
    fn sin(self) -> Self;

    /// Computes an approximation of the tangent of this value in radians.
    ///
    /// Returns the computed tangent.
    fn tan(self) -> Self;

    /// Computes an approximation of the sine and cosine of this value.
    ///
    /// Returns the computed sine and cosine.
    fn sin_cos(self) -> (Self, Self)
    {
        (self.sin(), self.cos())
    }

    /// Computes an approximation of the inverse cosine of this value.
    ///
    /// Returns the computed inverse cosine.
    fn acos(self) -> Self;

    /// Computes the square root of this value.
    ///
    /// Returns the computed square root.
    fn sqrt(self) -> Self;
}

#[cfg(not(test))]
impl SoftFloatOps for f32
{
    fn cos(self) -> Self
    {
        // Formula: cos(x) = Sigma((-1^n)/(x^(2n)/(2n!)))
        // The following is required because ARMv8 does not have a floating point modulo
        // instruction.
        let mut div = self / PI;
        unsafe {
            asm!("frintz {dst:s}, {src:s}", src = in (vreg) div, dst = out (vreg) div, options (pure, nomem, nostack))
        };
        let angle = self - div * PI;
        let mut res = 1.0;
        let mut mul = angle * angle;
        res -= 1.0 / 2.0 * mul;
        mul *= angle * angle;
        res += 1.0 / 24.0 * mul;
        mul *= angle * angle;
        res -= 1.0 / 720.0 * mul;
        res
    }

    fn sin(self) -> Self
    {
        (self - FRAC_PI_2).cos()
    }

    fn tan(self) -> Self
    {
        self.sin() / self.cos()
    }

    fn acos(self) -> Self
    {
        // Formula: acos(z) = Sigma((2n!)/((2^2n)*(n!)^2)*((z^(2n+1))/(2n+1)))-pi/2
        let mut res = self;
        let mut mul = self * self * self;
        res += 2.0 / 4.0 * (mul / 3.0);
        mul *= self * self;
        res += 24.0 / 64.0 * (mul / 5.0);
        mul *= self * self;
        res += 720.0 / 2304.0 * (mul / 7.0);
        mul *= self * self;
        res += 40320.0 / 147456.0 * (mul / 9.0);
        mul *= self * self;
        res += 3628800.0 / 14745600.0 * (mul / 11.0);
        res - FRAC_PI_2
    }

    fn sqrt(self) -> Self
    {
        let res: Self;
        unsafe {
            asm!("fsqrt {res:s}, {val:s}", res = out (vreg) res, val = in (vreg) self, options(pure, nomem, nostack))
        };
        res
    }
}

/// Computes the dot product between two vectors.
///
/// * `lhs`: Left hand side vector.
/// * `rhs`: Right hand side vector.
///
/// Returns the computed dot product.
#[cfg(not(test))]
#[inline]
fn dot(lhs: f32x4, rhs: f32x4) -> f32
{
    (lhs * rhs).reduce_sum()
}

/// Computes the cross product between two vectors.
///
/// * `lhs`: Left hand side vector.
/// * `rhs`: Right hand side vector.
///
/// Returns the resulting vector.
#[cfg(not(test))]
#[inline]
fn cross(lhs: f32x4, rhs: f32x4) -> f32x4
{
    let x = lhs[1] * rhs[2] - lhs[2] * rhs[1];
    let y = lhs[0] * rhs[2] - lhs[2] * rhs[0];
    let z = lhs[0] * rhs[1] - lhs[1] * rhs[0];
    f32x4::from([x, y, z, 0.0])
}

/// Computes a unit vector with the same direction as the provided vector.
///
/// * `vec`: Vector to resize.
///
/// Returns the resulting vector.
#[inline]
fn normalize(vec: f32x4) -> f32x4
{
    vec * f32x4::splat(len(vec).recip())
}

/// Computes the square of the distance between two vectors.
///
/// * `lhs`: Left hand side vector.
/// * `rhs`: Right hand side vector.
///
/// Returns the resulting squared distance.
#[cfg(not(test))]
#[inline]
fn sq_dist(lhs: f32x4, rhs: f32x4) -> f32
{
    sq_len(rhs - lhs)
}

/// Computes the length of the provided vector.
///
/// * `vec`: Vector whose length is to be computed.
///
/// Returns the resulting length.
#[inline]
fn len(vec: f32x4) -> f32
{
    sq_len(vec).sqrt()
}

/// Computes the square of the length of the provided vector.
///
/// * `vec`: The vector whose squared length is to be computed.
///
/// Returns the resulting squared length.
#[inline]
fn sq_len(vec: f32x4) -> f32
{
    (vec * vec).reduce_sum()
}

/// Computes the product of two matrices.
///
/// * `lhs`: Left hand side matrix.
/// * `rhs`: Right hand side matrix.
///
/// Returns the resulting matrix.
#[inline]
fn mat_mul(lhs: [f32x4; 4], rhs: [f32x4; 4]) -> [f32x4; 4]
{
    let res0 = mat_vec_mul(lhs, rhs[0]);
    let res1 = mat_vec_mul(lhs, rhs[1]);
    let res2 = mat_vec_mul(lhs, rhs[2]);
    let res3 = mat_vec_mul(lhs, rhs[3]);
    [res0, res1, res2, res3]
}

/// Computes the product of a matrix and a vector.
///
/// * `mat`: Matrix to multiply the vector by.
/// * `vec`: Vector to be multiplied.
///
/// Returns the resulting vector.
#[inline]
fn mat_vec_mul(mat: [f32x4; 4], vec: f32x4) -> f32x4
{
    let mut res = mat[0] * f32x4::splat(vec[0]);
    res += mat[1] * f32x4::splat(vec[1]);
    res += mat[2] * f32x4::splat(vec[2]);
    res + mat[3] * f32x4::splat(vec[3])
}

#[cfg(test)]
fn is_roughly(actual: f32x4, expected: f32x4) -> bool
{
    let tolerance = f32x4::simd_max(expected.abs(), f32x4::splat(1.0)) / f32x4::splat(4096.0);
    let test0 = actual.simd_gt(expected + tolerance);
    let test1 = actual.simd_lt(expected - tolerance);
    if (test0 | test1).any() {
        eprintln!("Expected roughly: {expected:?}, got: {actual:?}");
        return false;
    }
    true
}
