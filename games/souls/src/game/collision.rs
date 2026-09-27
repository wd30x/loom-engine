//! Collision: box colliders seen from above, pushing a capsule out of
//! them and out of other capsules, and the sector an attack sweeps.

use glam::{Vec2, Vec3};
use loom_scene::{NodeId, Scene};

use super::{bearing, wrap};

/// A `collider: Box` as seen from above: an oriented rectangle on the XZ
/// plane plus a height range. Computed once at load time
/// (`docs/architecture.md` section 11, collision).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct BoxCollider {
    pub(super) center: Vec2,
    /// The box's local X and Z axes on the XZ plane.
    pub(super) a: Vec2,
    pub(super) b: Vec2,
    /// Half extents along `a` and `b`.
    pub(super) ha: f32,
    pub(super) hb: f32,
    pub(super) y_min: f32,
    pub(super) y_max: f32,
}

impl BoxCollider {
    pub(super) fn new(scene: &Scene, id: NodeId) -> Self {
        // Ancestors only translate (scene validation), so the local rotation
        // and scale are the world ones.
        let m = scene.world_matrix(id);
        let t = scene.transform(id);
        let a3 = t.rotation * Vec3::X;
        let b3 = t.rotation * Vec3::Z;
        Self {
            center: Vec2::new(m.w_axis.x, m.w_axis.z),
            a: Vec2::new(a3.x, a3.z),
            b: Vec2::new(b3.x, b3.z),
            ha: 0.5 * t.scale.x,
            hb: 0.5 * t.scale.z,
            y_min: m.w_axis.y - 0.5 * t.scale.y,
            y_max: m.w_axis.y + 0.5 * t.scale.y,
        }
    }
}

/// What one active tick of an attack sweeps: the part `[lo, hi]` (radians
/// relative to `facing`, positive to the left) of a sector of radius
/// `reach` around the feet `p`, over the heights `low..=high` above them.
pub(super) struct Sweep {
    pub(super) p: Vec3,
    pub(super) facing: f32,
    pub(super) reach: f32,
    pub(super) lo: f32,
    pub(super) hi: f32,
    pub(super) low: f32,
    pub(super) high: f32,
}

impl Sweep {
    /// `swept()` of `docs/architecture.md` section 11 (命中判定): whether
    /// the hurtbox standing at `at` with `radius` and `height` is touched.
    /// Its height range must overlap the band (both closed), and its
    /// circle be within `radius` of the sector part.
    pub(super) fn touches(&self, at: Vec3, radius: f32, height: f32) -> bool {
        let p = self.p;
        if at.y > p.y + self.high || p.y + self.low > at.y + height {
            return false;
        }
        let d = Vec2::new(at.x - p.x, at.z - p.z);
        let rel = wrap(bearing(p, at) - self.facing);
        let dist = if self.lo <= rel && rel <= self.hi {
            // Within the angles: only the radius matters.
            f32::max(d.length() - self.reach, 0.0)
        } else {
            f32::min(self.ray(d, self.lo), self.ray(d, self.hi))
        };
        dist <= radius
    }

    /// `ray(θ)`: the distance from `d` (relative to `p`) to the edge of
    /// length `reach` at angle `theta`.
    fn ray(&self, d: Vec2, theta: f32) -> f32 {
        let u = Vec2::new(
            -libm::sinf(self.facing + theta),
            -libm::cosf(self.facing + theta),
        );
        let t = d.dot(u).clamp(0.0, self.reach);
        (d - u * t).length()
    }
}

/// Pushes a capsule of `radius` and `height` with its feet at `p` out of
/// `boxes`: only the component across a box's face changes, so the capsule
/// slides along walls. Exactly the steps of `docs/architecture.md` section
/// 11 (collision): a fixed 4 rounds over the boxes in pre-order, no early
/// exit, only exactly rounded operations.
pub(super) fn resolve(
    boxes: &[BoxCollider],
    r: f32,
    h: f32,
    mut p: Vec3,
    circles: &[(Vec2, f32)],
) -> Vec3 {
    for _ in 0..4 {
        for bx in boxes {
            if bx.y_max <= p.y || bx.y_min >= p.y + h {
                continue; // no overlap in height
            }
            let d = Vec2::new(p.x, p.z) - bx.center;
            let la = d.dot(bx.a);
            let lb = d.dot(bx.b);
            let qa = la.clamp(-bx.ha, bx.ha);
            let qb = lb.clamp(-bx.hb, bx.hb);
            let (na, nb);
            if la == qa && lb == qb {
                // Center inside the box: out through the nearest side.
                if bx.ha - la.abs() <= bx.hb - lb.abs() {
                    na = (if la >= 0.0 { 1.0 } else { -1.0 }) * (bx.ha + r);
                    nb = lb;
                } else {
                    na = la;
                    nb = (if lb >= 0.0 { 1.0 } else { -1.0 }) * (bx.hb + r);
                }
            } else {
                let off = Vec2::new(la - qa, lb - qb);
                let d2 = off.length_squared();
                if d2 >= r * r {
                    continue; // not touching
                }
                let n = Vec2::new(qa, qb) + off * (r / d2.sqrt());
                na = n.x;
                nb = n.y;
            }
            let push = bx.a * (na - la) + bx.b * (nb - lb);
            p.x += push.x;
            p.z += push.y;
        }
        // Then out of each circle `(center, reach)` on the XZ plane: along
        // the line from its center, or +X from on top of it.
        for &(c, reach) in circles {
            let d = Vec2::new(p.x - c.x, p.z - c.y);
            let d2 = d.length_squared();
            if d2 >= reach * reach {
                continue; // not touching
            }
            if d2 == 0.0 {
                p.x = c.x + reach;
                p.z = c.y;
            } else {
                let n = d * (reach / d2.sqrt());
                p.x = c.x + n.x;
                p.z = c.y + n.y;
            }
        }
    }
    p
}
