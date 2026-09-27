//! Debug drawing: world-space line segments a game adds on top of the frame
//! (`docs/architecture.md` section 16). Shapes are split into segments
//! here; the renderer only ever sees lines. Angles follow the games'
//! `bearing`: 0 looks along -Z, positive turns left (counterclockwise seen
//! from above).

use std::f32::consts::TAU;

use glam::{Quat, Vec3};

/// Segments of a full circle, and the most angle one arc segment covers is
/// `TAU / CIRCLE_SEGMENTS`.
pub const CIRCLE_SEGMENTS: u32 = 32;

/// One segment in world space, linear RGB, opaque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugLine {
    pub a: Vec3,
    pub b: Vec3,
    pub color: Vec3,
}

/// The segments of one frame, in the order they were added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DebugDraw {
    lines: Vec<DebugLine>,
}

/// The horizontal unit vector at angle `angle` (0 is -Z, positive left).
fn direction(angle: f32) -> Vec3 {
    Vec3::new(-libm::sinf(angle), 0.0, -libm::cosf(angle))
}

impl DebugDraw {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lines(&self) -> &[DebugLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn line(&mut self, a: Vec3, b: Vec3, color: Vec3) {
        self.lines.push(DebugLine { a, b, color });
    }

    /// A horizontal circle (on the XZ plane through `center`) of 32
    /// segments; the last one ends exactly where the first starts.
    pub fn circle(&mut self, center: Vec3, radius: f32, color: Vec3) {
        let point = |i: u32| {
            let i = i % CIRCLE_SEGMENTS;
            center + direction(i as f32 * TAU / CIRCLE_SEGMENTS as f32) * radius
        };
        for i in 0..CIRCLE_SEGMENTS {
            self.line(point(i), point(i + 1), color);
        }
    }

    /// A vertical cylinder standing on `base`: the bottom circle, the top
    /// circle and 4 vertical lines at 0°, 90°, 180° and 270°.
    pub fn cylinder(&mut self, base: Vec3, radius: f32, height: f32, color: Vec3) {
        let up = Vec3::new(0.0, height, 0.0);
        self.circle(base, radius, color);
        self.circle(base + up, radius, color);
        for k in 0..4 {
            let p = base + direction(k as f32 * TAU / 4.0) * radius;
            self.line(p, p + up, color);
        }
    }

    /// A horizontal sector around `center` of `radius`, from `facing + lo`
    /// to `facing + hi` (`lo <= hi`): the two radii (even when they meet,
    /// at 360°), then the outer arc in segments of at most 360°/32, at
    /// least one.
    pub fn sector(
        &mut self,
        center: Vec3,
        facing: f32,
        radius: f32,
        lo: f32,
        hi: f32,
        color: Vec3,
    ) {
        let n = arc_segments(hi - lo);
        let point = |j: u32| {
            let angle = if j == n {
                hi
            } else {
                lo + (hi - lo) * j as f32 / n as f32
            };
            center + direction(facing + angle) * radius
        };
        self.line(center, point(0), color);
        self.line(center, point(n), color);
        for j in 0..n {
            self.line(point(j), point(j + 1), color);
        }
    }

    /// The 12 edges of a box around `center` with half extents `half`
    /// along its local axes, turned by `rotation`: the 4 of the bottom
    /// face, the 4 of the top face, then the 4 vertical ones.
    pub fn wire_box(&mut self, center: Vec3, half: Vec3, rotation: Quat, color: Vec3) {
        let corner = |x: f32, y: f32, z: f32| center + rotation * (half * Vec3::new(x, y, z));
        let ring = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        for y in [-1.0, 1.0] {
            for i in 0..4 {
                let ((x0, z0), (x1, z1)) = (ring[i], ring[(i + 1) % 4]);
                self.line(corner(x0, y, z0), corner(x1, y, z1), color);
            }
        }
        for (x, z) in ring {
            self.line(corner(x, -1.0, z), corner(x, 1.0, z), color);
        }
    }

    /// Three segments through `point` along X, Y and Z, each `size` long.
    pub fn cross(&mut self, point: Vec3, size: f32, color: Vec3) {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let h = axis * (size * 0.5);
            self.line(point - h, point + h, color);
        }
    }
}

/// How many segments an arc of `angle` radians gets: at most 360°/32 each,
/// at least one. A small tolerance keeps 90° at exactly 8.
fn arc_segments(angle: f32) -> u32 {
    let n = libm::ceilf(angle / (TAU / CIRCLE_SEGMENTS as f32) - 1e-4);
    if n >= 1.0 { n as u32 } else { 1 }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use super::*;

    const RED: Vec3 = Vec3::new(1.0, 0.0, 0.0);

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    /// The angle of `v` in the games' `bearing` convention.
    fn bearing(v: Vec3) -> f32 {
        libm::atan2f(-v.x, -v.z)
    }

    #[test]
    fn circle_is_32_joined_segments_on_one_level_at_the_radius() {
        let center = Vec3::new(1.0, 2.0, -3.0);
        let mut d = DebugDraw::new();
        d.circle(center, 1.5, RED);
        let lines = d.lines();
        assert_eq!(lines.len(), 32);
        for (i, l) in lines.iter().enumerate() {
            assert_eq!(l.b, lines[(i + 1) % 32].a, "segment {i}");
            for p in [l.a, l.b] {
                assert!(near(p.distance(center), 1.5), "{p}");
                assert_eq!(p.y, 2.0);
            }
            assert_eq!(l.color, RED);
        }
    }

    #[test]
    fn cylinder_has_two_circles_and_four_uprights_a_quarter_turn_apart() {
        let base = Vec3::new(0.5, 0.0, 0.5);
        let mut d = DebugDraw::new();
        d.cylinder(base, 0.25, 1.8, RED);
        let lines = d.lines();
        assert_eq!(lines.len(), 32 + 32 + 4);
        assert!(lines[..32].iter().all(|l| l.a.y == 0.0));
        assert!(lines[32..64].iter().all(|l| l.a.y == 1.8));
        for (k, l) in lines[64..].iter().enumerate() {
            assert_eq!((l.a.y, l.b.y), (0.0, 1.8));
            assert_eq!((l.a.x, l.a.z), (l.b.x, l.b.z));
            assert!(near((l.a - base).length(), 0.25));
            let expected = [0.0, FRAC_PI_2, PI, -FRAC_PI_2][k];
            let got = bearing(l.a - base);
            // π and -π are the same direction.
            assert!(
                near(got, expected) || near(got.abs(), PI) && near(expected.abs(), PI),
                "upright {k}: {got}"
            );
        }
    }

    #[test]
    fn sector_radii_point_at_facing_plus_lo_and_plus_hi() {
        let center = Vec3::new(2.0, 0.5, 1.0);
        let (facing, lo, hi) = (0.3, -0.5, 0.25);
        let mut d = DebugDraw::new();
        d.sector(center, facing, 2.0, lo, hi, RED);
        let lines = d.lines();
        for (l, angle) in [(lines[0], facing + lo), (lines[1], facing + hi)] {
            assert_eq!(l.a, center);
            assert!(
                near(bearing(l.b - center), angle),
                "{}",
                bearing(l.b - center)
            );
            assert!(near(l.b.distance(center), 2.0));
            assert_eq!(l.b.y, 0.5);
        }
        // The arc runs from the first radius's end to the second's.
        let arc = &lines[2..];
        assert_eq!(arc[0].a, lines[0].b);
        assert_eq!(arc[arc.len() - 1].b, lines[1].b);
        for w in arc.windows(2) {
            assert_eq!(w[0].b, w[1].a);
        }
    }

    #[test]
    fn sector_arc_gets_a_segment_per_eleven_and_a_quarter_degrees_at_least_one() {
        for (degrees, segments) in [(90.0_f32, 8), (11.25, 1), (5.0, 1), (12.0, 2), (150.0, 14)] {
            let mut d = DebugDraw::new();
            d.sector(Vec3::ZERO, 0.0, 1.0, 0.0, degrees.to_radians(), RED);
            assert_eq!(d.lines().len(), 2 + segments, "{degrees}°");
        }
    }

    #[test]
    fn full_turn_sector_draws_both_radii_on_top_of_each_other() {
        let mut d = DebugDraw::new();
        d.sector(Vec3::ZERO, 0.0, 1.0, -PI, PI, RED);
        let lines = d.lines();
        assert_eq!(lines.len(), 2 + 32);
        assert!(lines[0].b.distance(lines[1].b) < 1e-5);
        assert!(lines[2].a.distance(lines[33].b) < 1e-5);
    }

    #[test]
    fn wire_box_has_twelve_edges_and_turns_with_its_rotation() {
        let mut d = DebugDraw::new();
        let center = Vec3::new(1.0, 1.0, 0.0);
        let half = Vec3::new(2.0, 1.0, 0.5);
        d.wire_box(center, half, Quat::from_rotation_y(FRAC_PI_2), RED);
        let lines = d.lines();
        assert_eq!(lines.len(), 12);
        // Turned 90° about Y, local +X points along world -Z and local +Z
        // along world +X: x spans ±0.5, z spans ±2.
        for l in lines {
            for p in [l.a, l.b] {
                let q = p - center;
                assert!(
                    near(q.x.abs(), 0.5) && near(q.z.abs(), 2.0) && near(q.y.abs(), 1.0),
                    "{q}"
                );
            }
            // Each edge runs along one axis.
            let e = (l.b - l.a).abs();
            assert_eq!(
                [e.x > 1e-5, e.y > 1e-5, e.z > 1e-5]
                    .iter()
                    .filter(|&&b| b)
                    .count(),
                1
            );
        }
        // The first corner is local (-2, -1, -0.5): world (-0.5, -1, 2).
        assert!(lines[0].a.distance(center + Vec3::new(-0.5, -1.0, 2.0)) < 1e-5);
    }

    #[test]
    fn cross_is_three_axis_segments_of_its_size_through_the_point() {
        let p = Vec3::new(3.0, 1.5, -2.0);
        let mut d = DebugDraw::new();
        d.cross(p, 0.3, RED);
        let lines = d.lines();
        assert_eq!(lines.len(), 3);
        for (l, axis) in lines.iter().zip([Vec3::X, Vec3::Y, Vec3::Z]) {
            assert!(near(l.a.distance(l.b), 0.3));
            assert!(((l.a + l.b) * 0.5).distance(p) < 1e-6);
            assert!(near((l.b - l.a).normalize().dot(axis), 1.0));
        }
    }
}
