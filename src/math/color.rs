//! Color vector math.
//!
//! Provides a 4D vector for colors with red, green, blue, and alpha saturating
//! components.

use core::ops::{Add, AddAssign, Mul, MulAssign};

use super::*;

/// Packed color value.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Color
{
    /// Red, green, and blue components packed in an RGB565 format.
    rgb565: u16,
}

impl Color
{
    /// Blue color.
    pub const BLUE: Self = Self { rgb565: 0x1F };
    /// Green color.
    pub const GREEN: Self = Self { rgb565: 0x7E0 };
    /// Red color.
    pub const RED: Self = Self { rgb565: 0xF800 };

    /// Creates and initializes a new color from its components.
    ///
    /// * `red`: Red component.
    /// * `green`: Green component.
    /// * `blue`: Blue component.
    ///
    /// Returns the newly created color.
    pub fn from_components(red: u8, green: u8, blue: u8) -> Self
    {
        Self { rgb565: ((red as u16 & 0xF8) << 8) | ((green as u16 & 0xFC) << 3) | ((blue as u16 & 0xF8) >> 3) }
    }

    /// Returns the color's red component.
    pub fn red(&self) -> u8
    {
        let res = ((self.rgb565 & 0xF800) >> 8) as u8;
        if res > 0 {
            return res | 0x7;
        }
        0
    }

    /// Returns the color's green component.
    pub fn green(&self) -> u8
    {
        let res = ((self.rgb565 & 0x7E0) >> 3) as u8;
        if res > 0 {
            return res | 0x3;
        }
        0
    }

    /// Returns the color's blue component.
    pub fn blue(&self) -> u8
    {
        let res = ((self.rgb565 & 0x1F) << 3) as u8;
        if res > 0 {
            return res | 0x7;
        }
        0
    }

    /// Returns the packed 16-bit RGB565 value representing the color.
    #[cfg(not(test))]
    pub fn into_inner(self) -> u16
    {
        self.rgb565
    }
}

impl Add<Self> for Color
{
    type Output = Self;

    fn add(self, other: Self) -> Self
    {
        Self::from_components(self.red().saturating_add(other.red()),
                              self.green().saturating_add(other.green()),
                              self.blue().saturating_add(other.blue()))
    }
}

impl AddAssign<Self> for Color
{
    fn add_assign(&mut self, other: Self)
    {
        *self = *self + other;
    }
}

impl Mul<Self> for Color
{
    type Output = Self;

    fn mul(self, other: Self) -> Self
    {
        let sred = (self.rgb565 >> 11) + 1;
        let sgreen = ((self.rgb565 >> 5) & 0x3F) + 1;
        let sblue = (self.rgb565 & 0x1F) + 1;
        let ored = (other.rgb565 >> 11) + 1;
        let ogreen = ((other.rgb565 >> 5) & 0x3F) + 1;
        let oblue = (other.rgb565 & 0x1F) + 1;
        let red = ((sred * ored - 1) << 6) & 0xF800;
        let green = ((sgreen * ogreen - 1) >> 1) & 0x7E0;
        let blue = (sblue * oblue - 1) >> 5;
        let rgb565 = red | green | blue;
        Self { rgb565 }
    }
}

impl Mul<Scalar> for Color
{
    type Output = Self;

    fn mul(self, other: Scalar) -> Self
    {
        let val = (other.val[0].clamp(0.0, 1.0) * 255.0) as u8;
        let other = Self::from_components(val, val, val);
        self * other
    }
}

impl MulAssign<Self> for Color
{
    fn mul_assign(&mut self, other: Self)
    {
        *self = *self * other;
    }
}

impl MulAssign<Scalar> for Color
{
    fn mul_assign(&mut self, other: Scalar)
    {
        *self = *self * other;
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn components()
    {
        let color = Color::from_components(0xFF, 0x0, 0x0);
        assert_eq!(color, Color::RED);
        assert_eq!(color.red(), 0xFF);
        assert_eq!(color.green(), 0x0);
        assert_eq!(color.blue(), 0x0);
        let color = Color::from_components(0x0, 0xFF, 0x0);
        assert_eq!(color, Color::GREEN);
        assert_eq!(color.red(), 0x0);
        assert_eq!(color.green(), 0xFF);
        assert_eq!(color.blue(), 0x0);
        let color = Color::from_components(0x0, 0x0, 0xFF);
        assert_eq!(color, Color::BLUE);
        assert_eq!(color.red(), 0x0);
        assert_eq!(color.green(), 0x0);
        assert_eq!(color.blue(), 0xFF);
    }

    #[test]
    fn mul_color()
    {
        let one = Color::from_components(0xFF, 0xFF, 0xFF);
        let res = one * one;
        assert_eq!(res, one);
        let half = Color::from_components(0x7F, 0x7F, 0x7F);
        let res = one * half;
        assert_eq!(res, half);
        let res = half * one;
        assert_eq!(res, half);
        let quarter = Color::from_components(0x3F, 0x3F, 0x3F);
        let res = half * half;
        assert_eq!(res, quarter);
        let zero = Color::from_components(0x0, 0x0, 0x0);
        let res = one * zero;
        assert_eq!(res, zero);
        let res = zero * one;
        assert_eq!(res, zero);
        let res = zero * zero;
        assert_eq!(res, zero);
    }

    #[test]
    fn mul_scalar()
    {
        let one_c = Color::from_components(0xFF, 0xFF, 0xFF);
        let one_s = Scalar::from_val(1.0);
        let res = one_c * one_s;
        assert_eq!(res, one_c);
        let half_c = Color::from_components(0x7F, 0x7F, 0x7F);
        let half_s = Scalar::from_val(0.5);
        let res = one_c * half_s;
        assert_eq!(res, half_c);
        let res = half_c * one_s;
        assert_eq!(res, half_c);
        let quarter_c = Color::from_components(0x3F, 0x3F, 0x3F);
        let res = half_c * half_s;
        assert_eq!(res, quarter_c);
        let zero_c = Color::from_components(0x0, 0x0, 0x0);
        let zero_s = Scalar::from_val(0.0);
        let res = one_c * zero_s;
        assert_eq!(res, zero_c);
        let res = zero_c * one_s;
        assert_eq!(res, zero_c);
        let res = zero_c * zero_s;
        assert_eq!(res, zero_c);
    }
}
