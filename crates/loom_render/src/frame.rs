//! What the caller hands the renderer each frame, and how it is packed for
//! the GPU. The renderer knows nothing about scenes: the engine converts its
//! scene into a [`Frame`].

use bytemuck::{Pod, Zeroable};
use glam::camera::rh;
use glam::{Mat3, Mat4, Vec3};

use crate::MeshKind;

/// Everything needed to draw one frame.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    /// World to view space: the inverse of the camera's world matrix.
    pub view: Mat4,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Ambient light, linear RGB.
    pub ambient: Vec3,
    pub light: Option<DirectionalLight>,
    /// Drawn in this order.
    pub draws: &'a [DrawItem],
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

/// Right-handed perspective projection with depth 0..1 (the non-deprecated
/// name of glam's `Mat4::perspective_rh`).
pub(crate) fn projection(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    rh::proj::directx::perspective(fov_y, aspect, near, far)
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
        let view_proj = projection(frame.fov_y, aspect, frame.near, frame.far) * frame.view;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn projection_matches_the_deprecated_glam_name() {
        let (fov, aspect, near, far) = (1.0, 16.0 / 9.0, 0.1, 100.0);
        assert_eq!(
            projection(fov, aspect, near, far),
            Mat4::perspective_rh(fov, aspect, near, far)
        );
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
