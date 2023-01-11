//! Rotation angle math.
//!
//! Only used to produce quaternions.

/// Rotation angle.
#[derive(Clone, Copy, Debug)]
pub struct Angle
{
    /// Internal representation.
    pub(super) angle: f32,
}

impl Angle
{
    /// Creates and initializes a new angle.
    ///
    /// * `angle`: Angle in radians.
    ///
    /// Returns the newly created angle.
    pub fn from_radians(angle: f32) -> Self
    {
        Self { angle }
    }
}
