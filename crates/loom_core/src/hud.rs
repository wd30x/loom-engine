//! The HUD: screen-space rectangles and discs a game adds over the frame
//! (`docs/architecture.md` section 18). Coordinates are HUD units: the
//! screen is always 720 units high and `720 × width / height` wide, the
//! origin at the top left, +X right, +Y down. The engine scales them to
//! pixels; the renderer draws the items in order, alpha-blended.

use glam::{Mat4, Vec2, Vec3, Vec4};

/// The screen height in HUD units, whatever the resolution.
pub const HUD_HEIGHT: f32 = 720.0;

/// One item on the screen. Colors are linear RGB plus alpha, every
/// component from 0 to 1 (not checked, like the debug draw's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HudItem {
    /// A filled axis-aligned rectangle: its top-left corner and its size.
    Rect { min: Vec2, size: Vec2, color: Vec4 },
    /// A filled disc with a hard edge.
    Disc {
        center: Vec2,
        radius: f32,
        color: Vec4,
    },
}

/// The items of one frame, in the order they were added: the order they
/// are drawn in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hud {
    items: Vec<HudItem>,
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[HudItem] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn rect(&mut self, min: Vec2, size: Vec2, color: Vec4) {
        self.items.push(HudItem::Rect { min, size, color });
    }

    pub fn disc(&mut self, center: Vec2, radius: f32, color: Vec4) {
        self.items.push(HudItem::Disc {
            center,
            radius,
            color,
        });
    }
}

/// What a game needs to lay out its HUD: the screen size in HUD units and
/// the camera's view-projection matrix of this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudView {
    /// `720 × width / height` of the render size.
    pub width: f32,
    /// Always [`HUD_HEIGHT`].
    pub height: f32,
    /// World to clip space (depth 0..1, as the renderer's).
    pub view_proj: Mat4,
}

impl HudView {
    /// The view of a `width` x `height` pixel frame; both must be non-zero.
    pub fn new(width: u32, height: u32, view_proj: Mat4) -> Self {
        Self {
            width: HUD_HEIGHT * width as f32 / height as f32,
            height: HUD_HEIGHT,
            view_proj,
        }
    }

    /// Where `world` shows on the screen, in HUD units; `None` behind the
    /// camera (clip `w <= 0`). Off-screen points in front of the camera are
    /// returned all the same.
    pub fn project(&self, world: Vec3) -> Option<Vec2> {
        let clip = self.view_proj * world.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = Vec2::new(clip.x, clip.y) / clip.w;
        Some(Vec2::new(
            (ndc.x + 1.0) * 0.5 * self.width,
            (1.0 - ndc.y) * 0.5 * self.height,
        ))
    }
}

#[cfg(test)]
mod tests {
    use glam::camera::rh;

    use super::*;

    /// A 1280x720 view from the origin looking along -Z, 90° vertically.
    fn view() -> HudView {
        let proj =
            rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 16.0 / 9.0, 0.1, 100.0);
        HudView::new(1280, 720, proj)
    }

    #[test]
    fn hud_view_is_720_units_high_and_as_wide_as_the_aspect() {
        let v = view();
        assert_eq!((v.width, v.height), (1280.0, 720.0));
        let v = HudView::new(640, 480, Mat4::IDENTITY);
        assert_eq!((v.width, v.height), (960.0, 720.0));
    }

    #[test]
    fn point_straight_ahead_projects_to_the_screen_center() {
        let p = view().project(Vec3::new(0.0, 0.0, -5.0)).unwrap();
        assert!(p.abs_diff_eq(Vec2::new(640.0, 360.0), 1e-3), "{p}");
    }

    #[test]
    fn point_behind_the_camera_does_not_project() {
        assert_eq!(view().project(Vec3::new(0.0, 0.0, 5.0)), None);
        assert_eq!(view().project(Vec3::new(1.0, 2.0, 0.0)), None);
    }

    #[test]
    fn points_on_the_top_and_left_edges_project_to_zero() {
        // 90° vertically: at depth 5 the top edge is 5 up; the left edge
        // 5 × 16/9 to the left.
        let v = view();
        let top = v.project(Vec3::new(0.0, 5.0, -5.0)).unwrap();
        assert!(top.abs_diff_eq(Vec2::new(640.0, 0.0), 1e-3), "{top}");
        let left = v.project(Vec3::new(-5.0 * 16.0 / 9.0, 0.0, -5.0)).unwrap();
        assert!(left.abs_diff_eq(Vec2::new(0.0, 360.0), 1e-3), "{left}");
    }

    #[test]
    fn point_off_screen_in_front_still_projects() {
        let p = view().project(Vec3::new(0.0, 10.0, -5.0)).unwrap();
        assert!(p.y < 0.0, "{p}");
    }

    #[test]
    fn hud_keeps_its_items_in_the_order_added() {
        let mut hud = Hud::new();
        assert!(hud.is_empty());
        hud.rect(Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0), Vec4::ONE);
        hud.disc(Vec2::new(5.0, 6.0), 7.0, Vec4::ZERO);
        assert_eq!(
            hud.items(),
            [
                HudItem::Rect {
                    min: Vec2::new(1.0, 2.0),
                    size: Vec2::new(3.0, 4.0),
                    color: Vec4::ONE,
                },
                HudItem::Disc {
                    center: Vec2::new(5.0, 6.0),
                    radius: 7.0,
                    color: Vec4::ZERO,
                },
            ]
        );
    }
}
