//! The orbit camera and the lock-on: where the camera goes, what blocks
//! it, which target to lock on to or switch to.

use glam::{Quat, Vec2, Vec3};

use crate::events::{Event, LockReason};

use super::{Game, bearing, hdist, wrap};

impl Game {
    /// Whether lock-on target `t` is a hurtbox lying dead.
    pub(super) fn target_dead(&self, t: usize) -> bool {
        self.targets[t]
            .hurtbox
            .and_then(|h| self.hurtboxes[h].health)
            .is_some_and(|health| health.is_dead())
    }

    /// Step 6 for the camera node: orbit the pivot above the player, pulled
    /// in at once in front of anything in the way, moving back out at
    /// `return_step` per tick once it is gone. When `loading`, the camera
    /// takes the unobstructed distance directly.
    pub(super) fn place_camera(&mut self, loading: bool) {
        let position = self.state.player.position;
        let camera = self.state.camera;
        let rot = Quat::from_rotation_y(camera.yaw) * Quat::from_rotation_x(camera.pitch);
        let pivot = position + Vec3::new(0.0, self.pivot_height, 0.0);
        let target = self.obstruct(pivot, rot * Vec3::Z);
        let distance = if loading || target < camera.distance {
            target
        } else {
            f32::min(camera.distance + self.return_step, target)
        };
        self.state.camera.distance = distance;
        let transform = self.scene.transform_mut(self.camera);
        transform.position = pivot + rot * Vec3::new(0.0, 0.0, distance);
        transform.rotation = rot;
    }

    /// How far a sphere of `collision_radius` gets from `pivot` along the
    /// unit vector `u` before touching the ground or a collider, at most
    /// `distance`. Exactly the steps of `docs/architecture.md` section 11
    /// (camera collision): each box grown by the radius, slabs in the order
    /// a, y, b, only exactly rounded operations.
    fn obstruct(&self, pivot: Vec3, u: Vec3) -> f32 {
        let cr = self.collision_radius;
        let mut t = self.distance;
        if u.y < 0.0 {
            // The ground; pivot_height > cr, so this is positive.
            t = f32::min(t, (cr - self.pivot_height) / u.y);
        }
        let uxz = Vec2::new(u.x, u.z);
        'boxes: for bx in &self.boxes {
            let o = Vec2::new(pivot.x, pivot.z) - bx.center;
            let mut t0 = 0.0;
            let mut t1 = self.distance;
            for (s, w, lo, hi) in [
                (o.dot(bx.a), uxz.dot(bx.a), -(bx.ha + cr), bx.ha + cr),
                (pivot.y, u.y, bx.y_min - cr, bx.y_max + cr),
                (o.dot(bx.b), uxz.dot(bx.b), -(bx.hb + cr), bx.hb + cr),
            ] {
                if w == 0.0 {
                    if s < lo || s > hi {
                        continue 'boxes; // parallel to this pair of faces, and outside
                    }
                } else {
                    let ta = (lo - s) / w;
                    let tb = (hi - s) / w;
                    t0 = f32::max(t0, f32::min(ta, tb));
                    t1 = f32::min(t1, f32::max(ta, tb));
                }
            }
            if t0 <= t1 {
                t = f32::min(t, t0); // t0 is 0 when the pivot is inside the grown box
            }
        }
        t
    }

    /// An event for the lock-on target changing to `t` (`None`: let go).
    pub(super) fn lock_event(&mut self, t: Option<usize>, reason: LockReason) {
        let target = t.map(|t| self.targets[t].path.clone());
        self.events.push(Event::Lock { target, reason });
    }

    /// The paths of the lock-on targets, in their order.
    #[cfg(test)]
    pub(crate) fn target_paths(&self) -> Vec<&str> {
        self.targets.iter().map(|t| t.path.as_str()).collect()
    }

    /// Where target `t` is now: read from the scene every tick.
    pub(super) fn target_position(&self, t: usize) -> Vec3 {
        // Targets are root nodes, so this is their world position.
        self.scene.transform(self.targets[t].id).position
    }

    /// `acquire()` of step 1a: the living target within `lock_range` and
    /// `lock_angle` closest to where the camera looks (`yaw`), the first
    /// in pre-order on a tie.
    pub(super) fn acquire(&self, position: Vec3, yaw: f32) -> Option<usize> {
        let lock = self.params.lock;
        let mut best: Option<(usize, f32)> = None;
        for t in (0..self.targets.len()).filter(|&t| !self.target_dead(t)) {
            let to = self.target_position(t);
            let d = hdist(position, to);
            if d == 0.0 || d > lock.range {
                continue;
            }
            let a = wrap(bearing(position, to) - yaw).abs();
            if a > lock.angle {
                continue;
            }
            if best.is_none_or(|(_, best_a)| a < best_a) {
                best = Some((t, a));
            }
        }
        best.map(|(t, _)| t)
    }

    /// `switch_target(side)` of step 1b: of the living targets within
    /// `lock_range` on the right (or left) of `current` as seen from
    /// `position`, the one closest in direction; the first in pre-order on
    /// a tie.
    pub(super) fn switch_target(
        &self,
        position: Vec3,
        current: usize,
        right: bool,
    ) -> Option<usize> {
        let cur = bearing(position, self.target_position(current));
        let mut best: Option<(usize, f32)> = None;
        for t in (0..self.targets.len()).filter(|&t| t != current && !self.target_dead(t)) {
            let to = self.target_position(t);
            let d = hdist(position, to);
            if d == 0.0 || d > self.params.lock.range {
                continue;
            }
            // Positive is to the left, negative to the right.
            let delta = wrap(bearing(position, to) - cur);
            let better = if right {
                delta < 0.0 && best.is_none_or(|(_, b)| delta > b)
            } else {
                delta > 0.0 && best.is_none_or(|(_, b)| delta < b)
            };
            if better {
                best = Some((t, delta));
            }
        }
        best.map(|(t, _)| t)
    }
}
