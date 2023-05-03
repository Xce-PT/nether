//! Graphical geometry.
//!
//! Contains geometry generation functionality.

use core::f32::consts::FRAC_PI_3;

use super::*;
use crate::math::{Angle, Vector};

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
        let tan = Angle::from(FRAC_PI_3).tan();
        let vert0 = Vector::from([-1.0, -tan / 3.0, 0.0, 1.0]);
        let vert1 = Vector::from([1.0, -tan / 3.0, 0.0, 1.0]);
        let vert2 = Vector::from([0.0, tan / 3.0 * 2.0, 0.0, 1.0]);
        let vert0 = Vertex { pos: vert0,
                             color: Vector::from([1.0, 0.0, 0.0, 1.0]) };
        let vert1 = Vertex { pos: vert1,
                             color: Vector::from([0.0, 0.0, 1.0, 1.0]) };
        let vert2 = Vertex { pos: vert2,
                             color: Vector::from([0.0, 1.0, 0.0, 1.0]) };
        let geom = [vert0, vert1, vert2, vert2, vert1, vert0];
        Self { geom }
    }

    /// Returns the geometry of the triangle.
    pub fn geom(&self) -> &[Vertex]
    {
        &self.geom
    }
}
