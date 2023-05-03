//! Projections in a 2D canvas.
//!
//! To improve the efficiency of the fragment shading code, these projections are laid out such that the origin is at the bottom left corner of the canvas, the vanishing point is at the center,  vertices are offset by -0.5 pixels, and each unit represents a pixel.  In addition, to take advantage of the higher precision of floates with smaller values, the Z coordinate is also flipped such that the near clipping plane produces a Z value of 1.0 after the perspective divide, and the far clipping plane, which is at infinity, produces a value of 0.0 after the perspective divide.

use super::*;

const NEAR: f32 = 1.0 / 16.0;

/// Projection matrix.
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    /// Raw matrix.
    mat: Matrix,
}

impl Projection {
    /// Creates and initializes a new perspective projection.
    ///
    /// * `width`: Screen width.
    /// * `height`: Screen height.
    /// * `fov`: Field of view.
    ///
    /// Returns the newly created projection.
    pub fn new_perspective(width: usize, height: usize, fov: Angle) -> Self {
        let halfwidth = (width / 2) as f32;
        let halfheight = (height / 2) as f32;
        let angle = Angle::from_cos(fov.w); // Half angle.
        let scale = angle.tan().recip() * if width >= height {halfheight} else {halfwidth};
        let xoff = -halfwidth;
        let yoff = -halfheight;
        let vec0 = Vector::from([scale, 0.0, 0.0, 0.0]);
        let vec1 = Vector::from([0.0, scale, 0.0, 0.0]);
        let vec2 = Vector::from([xoff, yoff, 0.0, -1.0]);
        let vec3 = Vector::from([0.0, 0.0, NEAR, 0.0]);
        let mat = Matrix::from([vec0, vec1, vec2, vec3]);
        Self {mat}
    }
    
    /// Returns the matrix for this projection.
    pub fn into_matrix(self) -> Matrix {
        self.mat
    }
}

#[cfg(test)]
mod tests {
    use core::f32::consts::PI;
    use super::*;
    
    #[test]
    fn project() {
        let width = 320;
        let height = 240;
        let fov = Angle::from(PI / 3.0);
        let proj = Projection::new_perspective(width, height, fov);
        let rhs = proj.into_matrix();
        let tanpisix = (PI / 6.0).tan();
        let lhs = Vector::from([0.0, 0.0, -1.0, 1.0]);
        let actual = lhs * rhs;
        let expected = Vector::from([160.0, 120.0, NEAR, 1.0]);
        expect_roughly_vec(actual, expected);
        let lhs = Vector::from([tanpisix, tanpisix, -1.0, 1.0]);
        let actual = lhs * rhs;
        let expected = Vector::from([280.0, 240.0, NEAR, 1.0]);
        expect_roughly_vec(actual, expected);
        let lhs = Vector::from([tanpisix, tanpisix, -2.0, 1.0]);
        let actual = lhs * rhs;
        let actual = actual / actual[3];
        let expected = Vector::from([220.0, 180.0, NEAR / 2.0, 1.0]);
        expect_roughly_vec(actual, expected);
    }
}
