use glam::camera::rh;
use glam::{Mat4, Vec3};

/// Perspective camera. Right-handed, +Y up, depth range 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    /// The fixed E0 view of the cube.
    fn default() -> Self {
        Self {
            eye: Vec3::new(2.5, 2.0, 3.5),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 60f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }
}

impl Camera {
    /// Projection * view for a viewport with the given width / height.
    ///
    /// `rh::proj::directx::perspective` is glam 0.33's non-deprecated name
    /// for `Mat4::perspective_rh` (right-handed, depth 0..1, Y up).
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        rh::proj::directx::perspective(self.fov_y, aspect, self.near, self.far)
            * rh::view::look_at_mat4(self.eye, self.target, self.up)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn matches_architecture_conventions() {
        let c = Camera::default();
        let aspect = 16.0 / 9.0;
        let expected = Mat4::perspective_rh(c.fov_y, aspect, c.near, c.far)
            * Mat4::look_at_rh(c.eye, c.target, c.up);
        assert_eq!(c.view_projection(aspect), expected);
    }

    #[test]
    fn origin_projects_to_screen_center() {
        let clip = Camera::default().view_projection(16.0 / 9.0) * glam::Vec4::W;
        let ndc = clip.truncate() / clip.w;
        assert!(ndc.x.abs() < 1e-6 && ndc.y.abs() < 1e-6);
        assert!((0.0..1.0).contains(&ndc.z));
    }
}
