//! Graphical geometry.
//!
//! Contains geometry generation functionality.

use super::*;

/// Rainbow cube.
#[derive(Debug)]
pub struct Cube
{
    /// Geometry.
    geom: [Triangle; 12],
}

impl Cube
{
    /// Creates and initializes a new rainbow cube.
    ///
    /// Returns the newly created cube.
    pub fn new() -> Self
    {
        // Vertices.
        let vbdl = f32x4::from_array([-1.0, -1.0, -1.0, 1.0]);
        let vbdr = f32x4::from_array([1.0, -1.0, -1.0, 1.0]);
        let vbul = f32x4::from_array([-1.0, 1.0, -1.0, 1.0]);
        let vbur = f32x4::from_array([1.0, 1.0, -1.0, 1.0]);
        let vfdl = f32x4::from_array([-1.0, -1.0, 1.0, 1.0]);
        let vfdr = f32x4::from_array([1.0, -1.0, 1.0, 1.0]);
        let vful = f32x4::from_array([-1.0, 1.0, 1.0, 1.0]);
        let vfur = f32x4::from_array([1.0, 1.0, 1.0, 1.0]);
        // Normals.
        let nl = f32x4::from_array([-1.0, 0.0, 0.0, 0.0]);
        let nr = f32x4::from_array([1.0, 0.0, 0.0, 0.0]);
        let nd = f32x4::from_array([0.0, -1.0, 0.0, 0.0]);
        let nu = f32x4::from_array([0.0, 1.0, 0.0, 0.0]);
        let nb = f32x4::from_array([0.0, 0.0, -1.0, 0.0]);
        let nf = f32x4::from_array([0.0, 0.0, 1.0, 0.0]);
        // Colors.
        let cbdl = f32x4::from_array([0.0, 0.0, 0.0, 1.0]);
        let cbdr = f32x4::from_array([1.0, 0.0, 0.0, 1.0]);
        let cbul = f32x4::from_array([0.0, 0.0, 1.0, 1.0]);
        let cbur = f32x4::from_array([1.0, 0.0, 1.0, 1.0]);
        let cfdl = f32x4::from_array([0.0, 1.0, 0.0, 1.0]);
        let cfdr = f32x4::from_array([1.0, 1.0, 0.0, 1.0]);
        let cful = f32x4::from_array([0.0, 1.0, 1.0, 1.0]);
        let cfur = f32x4::from_array([1.0, 1.0, 1.0, 1.0]);
        // Cube faces.
        let fb0 = Vertex { pos: vbdl,
                           normal: nb,
                           color: cbdl };
        let fb1 = Vertex { pos: vbul,
                           normal: nb,
                           color: cbul };
        let fb2 = Vertex { pos: vbdr,
                           normal: nb,
                           color: cbdr };
        let fb3 = Vertex { pos: vbur,
                           normal: nb,
                           color: cbur };
        let ff0 = Vertex { pos: vfdr,
                           normal: nf,
                           color: cfdr };
        let ff1 = Vertex { pos: vfur,
                           normal: nf,
                           color: cfur };
        let ff2 = Vertex { pos: vfdl,
                           normal: nf,
                           color: cfdl };
        let ff3 = Vertex { pos: vful,
                           normal: nf,
                           color: cful };
        let fl0 = Vertex { pos: vfdl,
                           normal: nl,
                           color: cfdl };
        let fl1 = Vertex { pos: vful,
                           normal: nl,
                           color: cful };
        let fl2 = Vertex { pos: vbdl,
                           normal: nl,
                           color: cbdl };
        let fl3 = Vertex { pos: vbul,
                           normal: nl,
                           color: cbul };
        let fr0 = Vertex { pos: vbdr,
                           normal: nr,
                           color: cbdr };
        let fr1 = Vertex { pos: vbur,
                           normal: nr,
                           color: cbur };
        let fr2 = Vertex { pos: vfdr,
                           normal: nr,
                           color: cfdr };
        let fr3 = Vertex { pos: vfur,
                           normal: nr,
                           color: cfur };
        let fd0 = Vertex { pos: vbdr,
                           normal: nd,
                           color: cbdr };
        let fd1 = Vertex { pos: vfdr,
                           normal: nd,
                           color: cfdr };
        let fd2 = Vertex { pos: vbdl,
                           normal: nd,
                           color: cbdl };
        let fd3 = Vertex { pos: vfdl,
                           normal: nd,
                           color: cfdl };
        let fu0 = Vertex { pos: vfur,
                           normal: nu,
                           color: cfur };
        let fu1 = Vertex { pos: vbur,
                           normal: nu,
                           color: cbur };
        let fu2 = Vertex { pos: vful,
                           normal: nu,
                           color: cful };
        let fu3 = Vertex { pos: vbul,
                           normal: nu,
                           color: cbul };
        // Cube triangles.
        let t0 = Triangle(fb0, fb1, fb2);
        let t1 = Triangle(fb2, fb1, fb3);
        let t2 = Triangle(ff0, ff1, ff2);
        let t3 = Triangle(ff2, ff1, ff3);
        let t4 = Triangle(fl0, fl1, fl2);
        let t5 = Triangle(fl2, fl1, fl3);
        let t6 = Triangle(fr0, fr1, fr2);
        let t7 = Triangle(fr2, fr1, fr3);
        let t8 = Triangle(fu0, fu1, fu2);
        let t9 = Triangle(fu2, fu1, fu3);
        let t10 = Triangle(fd0, fd1, fd2);
        let t11 = Triangle(fd2, fd1, fd3);
        let geom = [t0, t1, t2, t3, t4, t5, t6, t7, t8, t9, t10, t11];
        Self { geom }
    }

    /// Returns the geometry of the triangle.
    pub fn geom(&self) -> &[Triangle]
    {
        &self.geom
    }
}
