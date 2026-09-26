//! What the caller hands the renderer each frame, and how it is packed for
//! the GPU. The renderer knows nothing about scenes: the engine converts its
//! scene into a [`Frame`].

use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Mat4, Vec3};

use crate::{MeshKind, TextureId};

/// Everything needed to draw one frame.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    /// World to view space: the inverse of the camera's world matrix.
    pub view: Mat4,
    pub projection: Projection,
    pub near: f32,
    pub far: f32,
    /// Ambient light, linear RGB.
    pub ambient: Vec3,
    pub light: Option<DirectionalLight>,
    /// Drawn in this order.
    pub draws: &'a [DrawItem],
    /// Drawn in this order, after `draws`, in the same pass and against the
    /// same depth buffer. Unlit and alpha-tested, so the order does not
    /// change the result.
    pub sprites: &'a [SpriteDraw],
}

/// How view space maps to clip space. Both look along -Z with depth 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// Vertical field of view in radians.
    Perspective { fov_y: f32 },
    /// `height` is the view-space length the view covers vertically; the
    /// width is `height * aspect`, centered on the view axis.
    Orthographic { height: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalLight {
    /// World-space direction the light travels in, normalized.
    pub direction: Vec3,
    /// Linear RGB.
    pub color: Vec3,
    pub intensity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawItem {
    pub mesh: MeshKind,
    /// Object to world.
    pub model: Mat4,
    /// Linear RGB.
    pub color: Vec3,
}

/// A 1x1 quad on the XY plane of `model`, centered at its origin and facing
/// +Z, showing the pixels `rect` of `texture` unfiltered: uv (0, 0) is the
/// quad's top-left corner (-0.5, 0.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteDraw {
    pub texture: TextureId,
    /// Object to world.
    pub model: Mat4,
    /// x, y, width, height in texture pixels; must lie inside the texture.
    pub rect: [u32; 4],
}

/// Right-handed projection with depth 0..1. The perspective one is the
/// non-deprecated name of glam's `Mat4::perspective_rh`.
pub(crate) fn projection(projection: Projection, aspect: f32, near: f32, far: f32) -> Mat4 {
    match projection {
        Projection::Perspective { fov_y } => {
            rh::proj::directx::perspective(fov_y, aspect, near, far)
        }
        Projection::Orthographic { height } => {
            let (half_w, half_h) = (height * aspect / 2.0, height / 2.0);
            rh::proj::directx::orthographic(-half_w, half_w, -half_h, half_h, near, far)
        }
    }
}

/// Per-frame uniforms; matches `Globals` in `shaders/mesh.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuGlobals {
    view_proj: [[f32; 4]; 4],
    ambient: [f32; 4],
    /// xyz: direction the light travels in.
    light_dir: [f32; 4],
    /// rgb: color * intensity; zero without a light.
    light_color: [f32; 4],
}

impl GpuGlobals {
    pub fn new(frame: &Frame<'_>, aspect: f32) -> Self {
        let view_proj = projection(frame.projection, aspect, frame.near, frame.far) * frame.view;
        let (dir, color) = match frame.light {
            Some(l) => (l.direction, l.color * l.intensity),
            None => (Vec3::NEG_Y, Vec3::ZERO),
        };
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            ambient: frame.ambient.extend(0.0).to_array(),
            light_dir: dir.extend(0.0).to_array(),
            light_color: color.extend(0.0).to_array(),
        }
    }
}

/// One draw item; matches `Draw` in `shaders/mesh.wgsl` (128 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuDraw {
    model: [[f32; 4]; 4],
    /// Inverse transpose of the model's upper 3x3, as three padded columns,
    /// so normals stay perpendicular under non-uniform scale.
    normal: [[f32; 4]; 3],
    color: [f32; 4],
}

impl GpuDraw {
    pub fn new(item: &DrawItem) -> Self {
        let n = Mat3::from_mat4(item.model).inverse().transpose();
        Self {
            model: item.model.to_cols_array_2d(),
            normal: [n.x_axis, n.y_axis, n.z_axis].map(|c| c.extend(0.0).to_array()),
            color: item.color.extend(1.0).to_array(),
        }
    }
}

/// One sprite; matches `Sprite` in `shaders/sprite.wgsl` (80 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct GpuSprite {
    model: [[f32; 4]; 4],
    rect: [u32; 4],
}

impl GpuSprite {
    pub fn new(sprite: &SpriteDraw) -> Self {
        Self {
            model: sprite.model.to_cols_array_2d(),
            rect: sprite.rect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn projection_matches_the_deprecated_glam_name() {
        let (fov, aspect, near, far) = (1.0, 16.0 / 9.0, 0.1, 100.0);
        assert_eq!(
            projection(Projection::Perspective { fov_y: fov }, aspect, near, far),
            Mat4::perspective_rh(fov, aspect, near, far)
        );
    }

    #[test]
    fn orthographic_projection_maps_the_view_box_to_the_ndc_corners() {
        let (height, aspect, near, far) = (11.25, 16.0 / 9.0, 0.1, 100.0);
        let proj = projection(Projection::Orthographic { height }, aspect, near, far);
        let (half_w, half_h) = (height * aspect / 2.0, height / 2.0);
        for (depth, ndc_z) in [(near, 0.0), (far, 1.0)] {
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let p = proj.project_point3(Vec3::new(sx * half_w, sy * half_h, -depth));
                let expected = Vec3::new(sx, sy, ndc_z);
                assert!(
                    (p - expected).abs().max_element() < 1e-5,
                    "{p} != {expected}"
                );
            }
        }
    }

    #[test]
    fn normal_matrix_keeps_normals_perpendicular_under_non_uniform_scale() {
        let model = Mat4::from_scale(Vec3::new(4.0, 1.0, 1.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let draw = GpuDraw::new(&DrawItem {
            mesh: MeshKind::Cube,
            model,
            color: Vec3::ONE,
        });
        let n = Mat3::from_cols_array_2d(&draw.normal.map(|c| [c[0], c[1], c[2]]));
        // A surface through the origin spanned by tangent t, with normal nrm.
        let (t, nrm) = (Vec3::new(1.0, 1.0, 0.0), Vec3::new(1.0, -1.0, 0.0));
        let world_t = Mat3::from_mat4(model) * t;
        assert!((n * nrm).dot(world_t).abs() < 1e-5);
    }
}
