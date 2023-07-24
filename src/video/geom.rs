//! Graphical geometry.
//!
//! Contains geometry generation functionality.

use core::f32::consts::FRAC_PI_3;

use super::*;
use crate::math::Angle;

/// Linearly interpolated color triangle.
#[derive(Debug)]
pub struct Triangle
{
    /// Geometry.
    geom: [Vertex; 6],
}

/// Gray background square.
pub struct Square
{
    /// Geometry.
    geom: [Vertex; 6],
}

impl Triangle
{
    /// Creates and initializes a new triangle.
    ///
    /// Returns the newly created triangle.
    pub fn new() -> Self
    {
        let tan = Angle::from(FRAC_PI_3).tan();
        let vert0 = f32x4::from_array([-1.0, -tan / 3.0, 0.0, 1.0]);
        let vert1 = f32x4::from_array([1.0, -tan / 3.0, 0.0, 1.0]);
        let vert2 = f32x4::from_array([0.0, tan / 3.0 * 2.0, 0.0, 1.0]);
        let vert0 = Vertex { pos: vert0,
                             color: f32x4::from_array([1.0, 0.0, 0.0, 1.0]) };
        let vert1 = Vertex { pos: vert1,
                             color: f32x4::from_array([0.0, 0.0, 1.0, 1.0]) };
        let vert2 = Vertex { pos: vert2,
                             color: f32x4::from_array([0.0, 1.0, 0.0, 1.0]) };
        let geom = [vert0, vert1, vert2, vert2, vert1, vert0];
        Self { geom }
    }

    /// Returns the geometry of the triangle.
    pub fn geom(&self) -> &[Vertex]
    {
        &self.geom
    }
}

impl Square
{
    /// Creates and initializes a new square.
    ///
    /// Returns the newly created square.
    pub fn new() -> Self
    {
        let vert0 = f32x4::from_array([-1.0, -1.0, 0.0, 1.0]);
        let vert1 = f32x4::from_array([1.0, -1.0, 0.0, 1.0]);
        let vert2 = f32x4::from_array([-1.0, 1.0, 0.0, 1.0]);
        let vert3 = f32x4::from_array([1.0, 1.0, 0.0, 1.0]);
        let color = f32x4::from_array([0.5, 0.5, 0.5, 1.0]);
        let vert0 = Vertex { pos: vert0, color };
        let vert1 = Vertex { pos: vert1, color };
        let vert2 = Vertex { pos: vert2, color };
        let vert3 = Vertex { pos: vert3, color };
        let geom = [vert0, vert1, vert2, vert2, vert1, vert3];
        Self { geom }
    }

    /// Returns the geometry of the square.
    pub fn geom(&self) -> &[Vertex]
    {
        &self.geom
    }
}
