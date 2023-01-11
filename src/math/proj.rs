//! Projection transformation math.
//!
//! Provides a transformation to turn vectors from world coordinates into
//! projected vectors in normalized coordinates.

#[cfg(not(test))]
use core::simd::SimdPartialOrd;

use super::*;

/// Projection transformation.
#[derive(Clone, Copy, Debug)]
pub struct Projector
{
    /// Internal representation.
    mat: [f32x4; 4],
}

/// Projector transformed for a specific tile.
#[derive(Clone, Copy, Debug)]
pub struct TileProjector
{
    /// Internal representation.
    mat: [f32x4; 4],
}

/// View transformed by a tiled projector.
#[derive(Clone, Copy, Debug)]
pub struct ViewTileProjector
{
    /// Internal representation.
    mat: [f32x4; 4],
}

/// Vector transformed by the projection transformation.
#[derive(Clone, Copy, Debug)]
pub struct ProjectedVector
{
    /// Internal representation.
    pub(super) vec: f32x4,
}

impl Projector
{
    /// Creates and initializes a new perspective projector.
    ///
    /// * `width`: Screen width in tiles.
    /// * `height`: Screen height in tiles.
    /// * `tile`: Tile identifier.
    /// * `fov`: Field of view.
    /// * `near`: Near clipping plane.
    /// * `far`: Far clipping plane.
    ///
    /// Returns the created projector.
    pub fn perspective(fov: f32, near: f32, far: f32) -> Self
    {
        let scale = (fov * 0.5).tan().recip();
        let invrange = (near - far).recip();
        let z = far * invrange;
        let zw = -1.0;
        let wz = near * far * invrange;
        let mat0 = f32x4::from([scale, 0.0, 0.0, 0.0]);
        let mat1 = f32x4::from([0.0, scale, 0.0, 0.0]);
        let mat2 = f32x4::from([0.0, 0.0, z, zw]);
        let mat3 = f32x4::from([0.0, 0.0, wz, 0.0]);
        let mat = [mat0, mat1, mat2, mat3];
        Self { mat }
    }

    /// Creates a new tile projector by applying the required changes to this
    /// projector, normalizing coordinates inside a specific tile.
    ///
    /// * `width`: Screen width in tiles.
    /// * `height`: Screen height in tiles.
    /// * `tile`: Tile index.
    ///
    /// Returns the newly created tile projector.
    pub fn for_tile(self, width: usize, height: usize, twidth: usize, theight: usize, tile: usize) -> TileProjector
    {
        let cols = width / twidth;
        let col = tile % cols;
        let row = tile / cols;
        assert!(row < height,
                "Tile {tile} is out of bounds for a screen with {width}x{height} tiles");
        let width = width as f32;
        let height = height as f32;
        let (xscale, yscale) = if width >= height {
            (height / width, 1.0)
        } else {
            (1.0, width / height)
        };
        let width = width / twidth as f32;
        let height = height / theight as f32;
        let xscale = xscale * width;
        let yscale = yscale * height;
        let left = width * -0.5 + col as f32;
        let bottom = height * -0.5 + row as f32;
        let x = self.mat[0][0] * xscale;
        let y = self.mat[1][1] * yscale;
        let zx = left + left + 1.0;
        let zy = bottom + bottom + 1.0;
        let z = self.mat[2][2];
        let zw = self.mat[2][3];
        let wz = self.mat[3][2];
        let mat0 = f32x4::from([x, 0.0, 0.0, 0.0]);
        let mat1 = f32x4::from([0.0, y, 0.0, 0.0]);
        let mat2 = f32x4::from([zx, zy, z, zw]);
        let mat3 = f32x4::from([0.0, 0.0, wz, 0.0]);
        let mat = [mat0, mat1, mat2, mat3];
        TileProjector { mat }
    }
}

impl TileProjector
{
    /// Creates a new view tile projector by applying a view transformation to
    /// this tile projector.
    ///
    /// * `view`: View transformation (reciprocal of the camera transformation).
    ///
    /// Returns the newly created projector.
    pub fn for_view(self, view: Matrix) -> ViewTileProjector
    {
        let mat = mat_mul(self.mat, view.transform);
        ViewTileProjector { mat }
    }
}

impl ViewTileProjector
{
    /// Projects a triangle from world space to clip space.
    ///
    /// * `vert0`: First vertex.
    /// * `vert1`: Second vertex.
    /// * `vert2`: Third vertex.
    ///
    /// Returns the projected vertices if they are to be drawn in
    /// counter-clockwise order and the triangle that they form might fit
    /// partially or wholly inside clip space.
    #[cfg(not(test))]
    pub fn project_tri(self, vert0: Vector, vert1: Vector, vert2: Vector)
                       -> Option<(ProjectedVector, ProjectedVector, ProjectedVector)>
    {
        let vert0 = self.project(vert0);
        let vert1 = self.project(vert1);
        let vert2 = self.project(vert2);
        let vert1r = vert1.vec - vert0.vec;
        let vert2r = vert2.vec - vert0.vec;
        let det = vert1r[0] * vert2r[1] - vert1r[1] * vert2r[0];
        if det <= 0.0 {
            return None;
        }
        let min = vert0.vec.simd_min(vert1.vec);
        let min = min.simd_min(vert2.vec);
        let max = vert0.vec.simd_max(vert1.vec);
        let max = max.simd_max(vert2.vec);
        let clipmin = f32x4::from([-1.0, -1.0, 0.0, 0.0]);
        let clipmax = f32x4::from([1.0, 1.0, 1.0, f32::INFINITY]);
        if (max.simd_lt(clipmin) | min.simd_gt(clipmax)).any() {
            return None;
        }
        Some((vert0, vert1, vert2))
    }

    /// Projects a vector from world coordinates too normalized coordinates.
    ///
    /// * `vec`: Vector to project.
    ///
    /// Returns the resulting vector.
    fn project(self, vec: Vector) -> ProjectedVector
    {
        let mut vec = vec.vec;
        vec[3] = 1.0;
        vec = mat_vec_mul(self.mat, vec);
        let mut w = f32x4::splat(vec[3].recip());
        w[3] = 1.0;
        vec *= w;
        ProjectedVector { vec }
    }
}

#[cfg(test)]
mod tests
{
    use core::f32::consts::{FRAC_PI_3, FRAC_PI_6};

    use super::*;

    #[test]
    fn projector_perspective_clip_frustum()
    {
        let proj = Projector::perspective(FRAC_PI_3, 0.5, 2.0);
        let tvproj = proj.for_tile(1920, 1080, 16, 18, 0).for_view(Matrix::default());
        let point = Vector::from_components(1920.0 / 1080.0 * -0.5 * FRAC_PI_6.tan(), -0.5 * FRAC_PI_6.tan(), -0.5);
        let res = tvproj.project(point);
        assert!(is_roughly(res.vec, f32x4::from([-1.0, -1.0, 0.0, 0.5])));
        let tvproj = proj.for_tile(1920, 1080, 16, 18, 7199).for_view(Matrix::default());
        let point = Vector::from_components(1920.0 / 1080.0 * 2.0 * FRAC_PI_6.tan(), 2.0 * FRAC_PI_6.tan(), -2.0);
        let res = tvproj.project(point);
        assert!(is_roughly(res.vec, f32x4::from([1.0, 1.0, 1.0, 2.0])));
    }
}
