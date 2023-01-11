//! Color vector math.
//!
//! Provides a 4D vector for colors with red, green, blue, and alpha clamped
//! components.

use core::default::Default;
use core::ops::{Add, AddAssign, Mul, MulAssign};
use core::simd::SimdFloat;

use super::*;

/// Color vector.
#[derive(Clone, Copy, Debug)]
pub struct Color
{
    /// Red, green, blue, and alpha components.
    rgba: f32x4,
}

impl Color
{
    /// Blue color.
    pub const BLUE: Self = Self { rgba: f32x4::from_array([0.0, 0.0, 1.0, 1.0]) };
    /// Green color.
    pub const GREEN: Self = Self { rgba: f32x4::from_array([0.0, 1.0, 0.0, 1.0]) };
    /// Red color.
    pub const RED: Self = Self { rgba: f32x4::from_array([1.0, 0.0, 0.0, 1.0]) };

    /// Transforms another color by blending this color using its alpha value as
    /// the transparency source..
    ///
    /// * `source`: Source color.
    ///
    /// Returns the resulting color.
    pub fn blend_with(&mut self, source: Self)
    {
        if source.rgba[3] == 1.0 {
            *self = source;
            return;
        }
        if source.rgba[3] == 0.0 {
            return;
        }
        let mut alpha = f32x4::splat(source.rgba[3]);
        alpha[3] = 1.0;
        *self = Self { rgba: source.rgba * alpha + self.rgba * (f32x4::splat(1.0) - alpha) };
    }

    pub fn to_u32(self) -> u32
    {
        let rgba = self.rgba.simd_clamp(f32x4::splat(0.0), f32x4::splat(1.0));
        let red = (rgba[0] * 255.0) as u32;
        let green = (rgba[1] * 255.0) as u32;
        let blue = (rgba[2] * 255.0) as u32;
        0xFF000000 | (red << 16) | (green << 8) | blue
    }
}

impl Add<Self> for Color
{
    type Output = Self;

    fn add(self, other: Self) -> Self
    {
        Self { rgba: self.rgba + other.rgba }
    }
}

impl AddAssign<Self> for Color
{
    fn add_assign(&mut self, other: Self)
    {
        self.rgba += other.rgba;
    }
}

impl Mul<Self> for Color
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        Self { rgba: self.rgba * other.rgba }
    }
}

impl Mul<Scalar> for Color
{
    type Output = Self;

    fn mul(self, other: Scalar) -> Self
    {
        let mut val = other.val;
        val[3] = 1.0;
        Self { rgba: self.rgba * val }
    }
}

impl MulAssign<Self> for Color
{
    fn mul_assign(&mut self, other: Self)
    {
        self.rgba *= other.rgba;
    }
}

impl MulAssign<Scalar> for Color
{
    fn mul_assign(&mut self, other: Scalar)
    {
        *self = *self * other;
    }
}

impl Default for Color
{
    fn default() -> Self
    {
        Self { rgba: f32x4::from([0.0, 0.0, 0.0, 1.0]) }
    }
}
