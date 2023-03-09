//! Triangle sampling math.
//!
//! Implements types for perspective correct triangle attribute linear
//! interpolation.

use core::ops::{Add, Mul};

use super::*;

/// Fragment triangulator.
#[derive(Debug)]
pub struct Triangulation
{
    /// Interpolated W component.
    wp: Scalar,
    /// First vertex's weighted W component.
    w0: Scalar,
    /// Second vertex's weighted W component.
    w1: Scalar,
    /// Third vertex's weighted W component.
    w2: Scalar,
}

impl Triangulation
{
    /// Creates and initializes a new triangulation from a point in normalized
    /// coordinates and a triangle.
    ///
    /// * `point`: Point to sample.
    /// * `vert0`: First vertex.
    /// * `vert1`: Second vertex.
    /// * `vert2`: Third vertex.
    ///
    /// Returns the newly created triangulator if the point is part of the
    /// triangle and is within clip range.
    pub fn from_point_triangle(point: Vector, vert0: ProjectedVector, vert1: ProjectedVector, vert2: ProjectedVector)
                               -> Option<Self>
    {
        let point = f32x4::from([point.vec[0], point.vec[1], 0.0, 0.0]);
        // Move the fragment to the origin.
        let vert0 = vert0.vec - point;
        let vert1 = vert1.vec - point;
        let vert2 = vert2.vec - point;
        // Compute the linear barycentric coordinates.  Return early if outside the
        // triangle.
        let area0 = vert1[0] * vert2[1] - vert1[1] * vert2[0];
        if area0.is_sign_negative() || area0 == 0.0 && vert1[0] >= vert2[0] && vert1[1] >= vert2[1] {
            return None;
        }
        let area1 = vert2[0] * vert0[1] - vert2[1] * vert0[0];
        if area1.is_sign_negative() || area1 == 0.0 && vert2[0] >= vert0[0] && vert2[1] >= vert0[1] {
            return None;
        }
        let area2 = vert0[0] * vert1[1] - vert0[1] * vert1[0];
        if area2.is_sign_negative() || area2 == 0.0 && vert0[0] >= vert1[0] && vert0[1] >= vert1[1] {
            return None;
        }
        let total = area0 + area1 + area2;
        let bary0 = area0 / total;
        let bary1 = area1 / total;
        let bary2 = area2 / total;
        let w0 = vert0[3] * bary0;
        let w1 = vert1[3] * bary1;
        let w2 = vert2[3] * bary2;
        let wp = (w0 + w1 + w2).recip();
        let z = (vert0[2] * w0 + vert1[2] * w1 + vert2[2] * w2) * wp;
        if !(0.0 ..= 1.0).contains(&z) {
            // Outside clip range.
            return None;
        }
        let this = Self { wp: Scalar::from_val(wp),
                          w0: Scalar::from_val(w0),
                          w1: Scalar::from_val(w1),
                          w2: Scalar::from_val(w2) };
        Some(this)
    }

    /// Samples the specified vertex attributes using this triangulation.
    ///
    /// * `attrib0`: Attribute from the first vertex.
    /// * `attrib1`: Attribute from the second vertex.
    /// * `attrib2`: Attribute from the third vertex.
    ///
    /// Returns the perspective correct linearly interpolated result.
    pub fn sample<A: Add<A, Output = A> + Mul<Scalar, Output = A>>(&self, attrib0: A, attrib1: A, attrib2: A) -> A
    {
        (attrib0 * self.w0 + attrib1 * self.w1 + attrib2 * self.w2) * self.wp
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn triangulation_linear_sampling()
    {
        let vert0 = Vector::from_components(-1.0, -1.0, -2.0);
        let vert1 = Vector::from_components(1.0, -1.0, -2.0);
        let vert2 = Vector::from_components(0.0, 1.0, -2.0);
        let prvert0 = ProjectedVector { vec: f32x4::from([-0.5, -0.5, 1.0, 0.5]) };
        let prvert1 = ProjectedVector { vec: f32x4::from([0.5, -0.5, 1.0, 0.5]) };
        let prvert2 = ProjectedVector { vec: f32x4::from([0.0, 0.5, 1.0, 0.5]) };
        let point = Vector::from_components(0.0, -1.0 / 6.0, 0.0);
        let sampler = Triangulation::from_point_triangle(point, prvert0, prvert1, prvert2).unwrap();
        let res = sampler.sample(vert0, vert1, vert2);
        assert!(is_roughly(res.vec, f32x4::from([0.0, -1.0 / 3.0, -2.0, 0.0])));
    }

    #[test]
    fn triangulation_perspective_sampling()
    {
        let vert0 = Vector::from_components(-1.0, -1.0, -1.0);
        let vert1 = Vector::from_components(1.0, -1.0, -1.0);
        let vert2 = Vector::from_components(0.0, -1.0, -2.0);
        let prvert0 = ProjectedVector { vec: f32x4::from([-1.0, -1.0, 0.0, 1.0]) };
        let prvert1 = ProjectedVector { vec: f32x4::from([1.0, -1.0, 0.0, 1.0]) };
        let prvert2 = ProjectedVector { vec: f32x4::from([0.0, -0.5, 1.0, 0.5]) };
        let point = Vector::from_components(0.0, -0.75, 0.0);
        let sampler = Triangulation::from_point_triangle(point, prvert0, prvert1, prvert2).unwrap();
        let res = sampler.sample(vert0, vert1, vert2);
        assert!(is_roughly(res.vec, f32x4::from([0.0, -1.0, -4.0 / 3.0, 0.0])));
    }
}
