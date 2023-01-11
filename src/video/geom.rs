//! Graphical geometry.
//!
//! Contains geometry generation functionality.

use core::f32::consts::FRAC_PI_3;

use super::*;
use crate::math::{Color, SoftFloatOps, Vector};

/// Linearly interpolated color triangle.
#[derive(Debug)]
pub struct Triangle
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
        let tan = FRAC_PI_3.tan();
        let vert0 = Vector::from_components(-1.0, -tan / 3.0, 0.0);
        let vert1 = Vector::from_components(1.0, -tan / 3.0, 0.0);
        let vert2 = Vector::from_components(0.0, tan / 3.0 * 2.0, 0.0);
        let vert0 = Vertex { pos: vert0,
                             color: Color::RED };
        let vert1 = Vertex { pos: vert1,
                             color: Color::BLUE };
        let vert2 = Vertex { pos: vert2,
                             color: Color::GREEN };
        let geom = [vert0, vert1, vert2, vert2, vert1, vert0];
        Self { geom }
    }

    /// Returns the geometry of the triangle.
    pub fn geom(&self) -> &[Vertex]
    {
        &self.geom
    }
}
