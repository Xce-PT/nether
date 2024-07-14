//! Fragment shader.

use core::simd::prelude::*;

use crate::simd::SimdFloatExtra;

/// Fragment shader state.
#[derive(Debug)]
pub struct Shader<'a>
{
    /// Triangle to shade.
    tri: &'a Triangle,
    /// Shader context.
    ctx: Context,
    /// Combined red light.
    red: f32x4,
    /// Combined green light.
    green: f32x4,
    /// Combined blue light.
    blue: f32x4,
}

/// Triangle to draw, with vertices in counter-clockwise order.
#[derive(Debug)]
pub struct Triangle(pub Vertex, pub Vertex, pub Vertex);

/// Vertex attributes.
#[derive(Debug)]
pub struct Vertex
{
    /// Projected position.
    pub proj: f32x4,
    /// World position.
    pub pos: f32x4,
    /// Surface normal.
    pub normal: f32x4,
    /// Color.
    pub color: f32x4,
}

/// Light.
#[derive(Clone, Copy, Debug)]
pub struct Light
{
    /// Color.
    color: f32x4,
    /// World position.
    pos: f32x4,
    /// Radius.
    radius: f32x4,
    /// Attenuation.
    attn: f32x4,
}

/// Shader context.
#[derive(Debug)]
pub struct Context
{
    /// Weights of the first vertex.
    pub bary0: f32x4,
    /// Weights of the second vertex.
    pub bary1: f32x4,
    /// Weights of the third vertex.
    pub bary2: f32x4,
    /// Whether the normals are constant along the triangle's surface.
    pub is_plane: bool,
}

impl<'a> Shader<'a>
{
    /// Creates and initializes a new shader for the specified triangle.
    ///
    /// * `tri`: Triangle to shade.
    /// * `ctx`: Shader context.
    ///
    /// Returns the newly created shader.
    #[inline]
    pub const fn new(tri: &'a Triangle, ctx: Context) -> Self
    {
        let zero = f32x4::from_array([0.0; 4]);
        Self { tri,
               ctx,
               red: zero,
               green: zero,
               blue: zero }
    }

    /// Returns the depth of the fragments.
    #[inline]
    #[must_use]
    pub fn depth(&self) -> f32x4
    {
        self.lerp_attr::<2>(self.tri.0.proj, self.tri.1.proj, self.tri.2.proj)
    }

    /// Illuminates the triangle with a light.
    ///
    /// * `light`: Light to illuminate the triangle with.
    #[inline]
    pub fn illuminate(&mut self, light: &Light)
    {
        let posx = self.lerp_attr::<0>(self.tri.0.pos, self.tri.1.pos, self.tri.2.pos);
        let posy = self.lerp_attr::<1>(self.tri.0.pos, self.tri.1.pos, self.tri.2.pos);
        let posz = self.lerp_attr::<2>(self.tri.0.pos, self.tri.1.pos, self.tri.2.pos);
        let (normalx, normaly, normalz) = if self.ctx.is_plane {
            (f32x4::splat(self.tri.0.normal[0]), f32x4::splat(self.tri.0.normal[1]), f32x4::splat(self.tri.0.normal[2]))
        } else {
            let normalx = self.lerp_attr::<0>(self.tri.0.normal, self.tri.1.normal, self.tri.2.normal);
            let normaly = self.lerp_attr::<1>(self.tri.0.normal, self.tri.1.normal, self.tri.2.normal);
            let normalz = self.lerp_attr::<2>(self.tri.0.normal, self.tri.1.normal, self.tri.2.normal);
            let ilen = (normalx * normalx).fused_mul_add(normaly, normaly)
                                          .fused_mul_add(normalz, normalz)
                                          .fast_sqrt_recip();
            (normalx * ilen, normaly * ilen, normalz * ilen)
        };
        let diffx = f32x4::splat(light.pos[0]) - posx;
        let diffy = f32x4::splat(light.pos[1]) - posy;
        let diffz = f32x4::splat(light.pos[2]) - posz;
        let idist = (diffx * diffx).fused_mul_add(diffy, diffy)
                                   .fused_mul_add(diffz, diffz)
                                   .fast_sqrt_recip();
        let dirx = diffx * idist;
        let diry = diffy * idist;
        let dirz = diffz * idist;
        let dist = idist.fast_recip();
        let intensity = (normalx * dirx).fused_mul_add(normaly, diry)
                                        .fused_mul_add(normalz, dirz)
                                        .simd_max(f32x4::splat(0.4));
        let intensity = (light.radius - dist) * light.attn * intensity;
        let red = intensity.mul_lane::<0>(light.color);
        let green = intensity.mul_lane::<1>(light.color);
        let blue = intensity.mul_lane::<2>(light.color);
        self.red = self.red.simd_max(red);
        self.green = self.green.simd_max(green);
        self.blue = self.blue.simd_max(blue);
    }

    /// Consumes self and finishes shading.
    ///
    /// Returns the computed red, green, and blue values with all shading
    /// effects applied to all fragments.
    #[inline]
    #[must_use]
    pub fn finish(self) -> (f32x4, f32x4, f32x4)
    {
        let red = self.lerp_attr::<0>(self.tri.0.color, self.tri.1.color, self.tri.2.color);
        let green = self.lerp_attr::<1>(self.tri.0.color, self.tri.1.color, self.tri.2.color);
        let blue = self.lerp_attr::<2>(self.tri.0.color, self.tri.1.color, self.tri.2.color);
        let red = red * self.red;
        let green = green * self.green;
        let blue = blue * self.blue;
        (red, green, blue)
    }

    /// Computes the linear interpolation for the specified vertex attributes.
    ///
    /// * `attr0`: First attribute.
    /// * `attr1`: Second attribute.
    /// * `attr2`: Third attribute.
    ///
    /// Returns the computed results.
    #[inline(always)]
    #[must_use]
    fn lerp_attr<const LANE: i32>(&self, attr0: f32x4, attr1: f32x4, attr2: f32x4) -> f32x4
    {
        let res = self.ctx.bary0.mul_lane::<LANE>(attr0);
        let res = res.fused_mul_add_lane::<LANE>(self.ctx.bary1, attr1);
        res.fused_mul_add_lane::<LANE>(self.ctx.bary2, attr2)
    }
}

impl Light
{
    /// Creates and initializes a new omni light.
    ///
    /// * `pos`: World position.
    /// * `color`: Color of the light.
    /// * `radius`: Light radius.
    ///
    /// Returns the newly created light.
    pub fn new_omni(pos: f32x4, color: f32x4, radius: f32) -> Self
    {
        Self { pos: pos.replace_lane::<3>(0.0),
               color,
               radius: f32x4::splat(radius),
               attn: f32x4::splat(radius.recip()) }
    }
}
