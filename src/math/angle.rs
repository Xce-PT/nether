//! Angles and trigonometry.

use core::cmp::{PartialOrd, Ordering, Reverse};
use core::f32::consts::PI;
use core::fmt::{Display, Formatter, Result as FormatResult};
use super::*;

/// Angle.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Angle {
    /// Cosine of half the angle.
    pub(super) w: f32,
}

impl Angle {
    /// Creates and initializes a new angle with the provided cosine.
    ///
    /// * `cos`: Cosine of the angle.
    ///
    /// Returns the newly created angle.
    pub fn from_cos(cos: f32) -> Self {
        let cos = cos.clamp(-1.0, 1.0);
        if cos == -1.0 {return Self {w: 0.0}}
        let icos = 1.0 + cos;
        let sqx = icos * icos;
        let sqy = 1.0 - cos * cos;
        let w = icos / (sqx + sqy).sqrt();
        Self {w}
    }
    
    /// Computes the sine and cosine of this angle.
    ///
    /// Returns the computed values.
    pub fn sin_cos(self) -> (f32, f32) {
        let sqx = self.w * self.w;
        let sqy = 1.0 - sqx;
        let cos = sqx - sqy;
        let sin = (1.0 - cos * cos).sqrt() * self.w.signum();
        (sin, cos)
    }
    
    /// Computes the tangent of this angle.
    ///
    /// Returns the computed value.
    pub fn tan(self) -> f32 {
        let (sin, cos) = self.sin_cos();
        sin / cos
    }
}

impl From<f32> for Angle {
    fn from(radians: f32) -> Self {
        let angle = radians.abs() / 2.0 % PI;
        let mut res = 1.0f32;
        let mut n = 2.0f32;
        let mut fact = 1.0f32;
        let mut diff = 1.0f32;
        let mut mul = 1.0f32;
        while diff.abs() > TOLERANCE / 2.0 {
            mul = -mul * angle * angle;
            fact *= (n - 1.0) * n;
            n += 2.0;
            diff = mul / fact;
            res += diff;
        }
        res *= radians.signum();
        Self {w: res}
    }
}

impl PartialOrd<Self> for Angle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        <Reverse<f32> as PartialOrd<_>>::partial_cmp(&Reverse(self.w), &Reverse(other.w))
    }
}

impl Default for Angle {
    fn default() -> Self {
        Self {w: 1.0}
    }
}

impl Display for Angle {
    fn fmt(&self, fmt: &mut Formatter) -> FormatResult {
        let radians = f32::from(*self);
        let degrees = radians / PI * 180.0;
        write!(fmt, "{radians} radians ({degrees} degrees)")
    }
}

impl From<Angle> for f32 {
    fn from(angle: Angle) -> Self {
        let mut lim = 1.0;
        let mut cos = angle.w.abs();
        let sin = (1.0 - cos * cos).sqrt();
        while lim - cos > TOLERANCE / 2.0 {
            cos = 0.5 * (cos + lim);
            lim = (cos * lim).sqrt();
        }
        let arc = sin / lim * 2.0;
        if angle.w < 0.0 {
            return 2.0 * PI - arc;
        }
        arc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn from_radians() {
        let angle = Angle::from(0.0);
        expect_roughly(angle.w, 1.0);
        let angle = Angle::from(PI / 3.0);
        expect_roughly(angle.w, (PI / 6.0).cos());
        let angle = Angle::from(PI / 2.0);
        expect_roughly(angle.w, (PI / 4.0).cos());
        let angle = Angle::from(PI * 2.0 / 3.0);
        expect_roughly(angle.w, (PI / 3.0).cos());
        let angle = Angle::from(PI);
        expect_roughly(angle.w, 0.0);
        let angle = Angle::from(PI * 4.0 / 3.0);
        expect_roughly(angle.w, (PI * 2.0 / 3.0).cos());
        let angle = Angle::from(PI * 3.0 / 2.0);
        expect_roughly(angle.w, (PI * 3.0 / 4.0).cos());
        let angle = Angle::from(PI * 5.0 / 3.0);
        expect_roughly(angle.w, (PI * 5.0 / 6.0).cos());
        let angle = Angle::from(PI * 2.0);
        expect_roughly(angle.w, angle.w.signum());
        let angle = Angle::from(PI * 5.0 / 2.0);
        expect_roughly(angle.w, (PI / 4.0).cos());
        let angle = Angle::from(PI * 3.0);
        expect_roughly(angle.w, 0.0);
        let angle = Angle::from(PI * 7.0 / 2.0);
        expect_roughly(angle.w, (PI * 3.0 / 4.0).cos());
        let angle = Angle::from(-PI / 2.0);
        expect_roughly(angle.w, (PI * 3.0 / 4.0).cos());
        let angle = Angle::from(-PI);
        expect_roughly(angle.w, 0.0);
        let angle = Angle::from(-PI * 3.0 / 2.0);
        expect_roughly(angle.w, (PI / 4.0).cos());
    }
    
    #[test]
    fn from_cos() {
        let angle = Angle::from_cos(0.0f32.cos());
        expect_roughly(angle.w, 1.0);
        let angle = Angle::from_cos((PI / 3.0).cos());
        expect_roughly(angle.w, (PI / 6.0).cos());
        let angle = Angle::from_cos((PI / 2.0).cos());
        expect_roughly(angle.w, (PI / 4.0).cos());
        let angle = Angle::from_cos((PI * 2.0 / 3.0).cos());
        expect_roughly(angle.w, (PI / 3.0).cos());
        let angle = Angle::from_cos(PI.cos());
        expect_roughly(angle.w, 0.0);
        let angle = Angle::from_cos(2.0);
        expect_roughly(angle.w, 1.0);
    }
    
    #[test]
    fn sin_cos() {
        let angle = Angle {w: 0.0f32.cos()};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, 0.0);
        expect_roughly(cos, 1.0);
        let angle = Angle {w: (PI / 6.0).cos()};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, (PI / 3.0).sin());
        expect_roughly(cos, 0.5);
        let angle = Angle {w: (PI / 4.0).cos()};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, 1.0);
        expect_roughly(cos, 0.0);
        let angle = Angle {w: (PI / 3.0).cos()};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, (PI * 2.0 / 3.0).sin());
        expect_roughly(cos, -0.5);
        let angle = Angle {w: 0.0};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, 0.0);
        expect_roughly(cos, -1.0);
        let angle = Angle {w: (PI * 3.0 / 4.0).cos()};
        let (sin, cos) = angle.sin_cos();
        expect_roughly(sin, -1.0);
        expect_roughly(cos, 0.0);
    }
    
    #[test]
    fn tan() {
        let angle = Angle {w: (PI / 12.0).cos()};
        let tan = angle.tan();
        expect_roughly(tan, (PI / 6.0).tan());
    }
    
    #[test]
    fn into_radians() {
        let angle = Angle {w: (PI / 3.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI * 2.0 / 3.0);
        let angle = Angle {w: (PI * 2.0 / 3.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI * 4.0 / 3.0);
        let angle = Angle {w: (PI / 16.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI / 8.0);
        let angle = Angle {w: (PI * 5.0 / 16.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI * 5.0 / 8.0);
        let angle = Angle {w: (PI * 9.0 / 16.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI * 9.0 / 8.0);
        let angle = Angle {w: (PI * 13.0 / 16.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI * 13.0 / 8.0);
        let angle = Angle {w: (PI / 4.0).cos()};
        let radians = f32::from(angle);
        expect_roughly(radians, PI / 2.0);
        let angle = Angle {w: 0.0};
        let radians = f32::from(angle);
        expect_roughly(radians, PI);
    }
}
