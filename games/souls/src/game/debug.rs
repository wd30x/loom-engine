//! The debug draw (`docs/architecture.md` section 16): hurtboxes,
//! swings, colliders and the lock-on, by category.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec3};
use loom_core::debug::DebugDraw;

use crate::health::Condition;
use crate::moves::{Action, AttackPhase};

use super::Game;
use super::enemy_ai::slice_of;
use super::roles::Enemy;

/// The debug draw's bonfire circles and respawn cross: warm orange.
pub(super) const DEBUG_BONFIRE: Vec3 = Vec3::new(1.0, 0.6, 0.2);
/// The size of the cross above the respawn point's bonfire.
const DEBUG_BONFIRE_CROSS: f32 = 0.5;
/// The debug draw's bloodstain circle: the marker's green.
pub(super) const DEBUG_BLOODSTAIN: Vec3 = Vec3::new(0.3, 0.9, 0.5);

/// The debug draw categories, in this order (`docs/architecture.md`
/// section 11, "调试绘制").
pub const DEBUG_CATEGORIES: [&str; 6] = [
    "hurtboxes",
    "swings",
    "colliders",
    "lock",
    "bonfires",
    "bloodstain",
];

/// Debug draw colors, linear RGB.
const DEBUG_DEAD: Vec3 = Vec3::new(0.4, 0.4, 0.4);
const DEBUG_STAGGER: Vec3 = Vec3::new(1.0, 0.9, 0.1);
const DEBUG_INVULNERABLE: Vec3 = Vec3::new(0.1, 0.9, 1.0);
const DEBUG_HURTBOX: Vec3 = Vec3::new(0.2, 1.0, 0.2);
pub(super) const DEBUG_STARTUP: Vec3 = Vec3::new(1.0, 0.55, 0.1);
pub(super) const DEBUG_ACTIVE: Vec3 = Vec3::new(0.5, 0.1, 0.1);
pub(super) const DEBUG_SLICE: Vec3 = Vec3::new(1.0, 0.15, 0.15);
const DEBUG_COLLIDER: Vec3 = Vec3::new(1.0, 0.2, 0.9);
const DEBUG_TARGET: Vec3 = Vec3::new(0.9, 0.9, 0.9);
const DEBUG_LOCKED: Vec3 = Vec3::new(1.0, 0.9, 0.1);
const DEBUG_LOCK_RANGE: Vec3 = Vec3::new(0.3, 0.5, 1.0);
const DEBUG_BREAK_RANGE: Vec3 = Vec3::new(0.2, 0.3, 0.6);
/// Sizes of the lock point crosses: every target's, and the locked one's.
const DEBUG_TARGET_CROSS: f32 = 0.3;
const DEBUG_LOCKED_CROSS: f32 = 0.5;

/// One attack's sector for the debug draw: `(position, facing, reach,
/// arc)` at the heights `band` above the position. Startup: the whole
/// sector in orange. Active: the whole sector in dark red, then `slice`
/// (this tick's, given while active) in bright red. Recovery draws
/// nothing.
fn debug_sweep(
    draw: &mut DebugDraw,
    (at, facing, reach, arc): (Vec3, f32, f32, f32),
    (low, high): (f32, f32),
    phase: AttackPhase,
    slice: Option<(f32, f32)>,
) {
    let half = arc * 0.5;
    let levels = [low, high].map(|y| at + Vec3::new(0.0, y, 0.0));
    match phase {
        AttackPhase::Startup => {
            for p in levels {
                draw.sector(p, facing, reach, -half, half, DEBUG_STARTUP);
            }
        }
        AttackPhase::Active => {
            for p in levels {
                draw.sector(p, facing, reach, -half, half, DEBUG_ACTIVE);
            }
            if let Some((lo, hi)) = slice {
                for p in levels {
                    draw.sector(p, facing, reach, lo, hi, DEBUG_SLICE);
                }
            }
        }
        AttackPhase::Recovery => {}
    }
}

impl Game {
    /// The debug draw categories, in their order.
    pub fn debug_categories() -> &'static [&'static str] {
        &DEBUG_CATEGORIES
    }

    /// Draws the shapes the checks use now, for the `categories` given
    /// (names from [`Game::debug_categories`]; others are ignored), in
    /// category order (`docs/architecture.md` section 11, "调试绘制").
    /// Reads the state only.
    pub fn debug_draw(&self, categories: &[&str], draw: &mut DebugDraw) {
        let on = |name: &str| categories.contains(&name);
        if on("hurtboxes") {
            self.debug_hurtboxes(draw);
        }
        if on("swings") {
            self.debug_swings(draw);
        }
        if on("colliders") {
            for b in &self.boxes {
                let center = Vec3::new(b.center.x, 0.5 * (b.y_min + b.y_max), b.center.y);
                let half = Vec3::new(b.ha, 0.5 * (b.y_max - b.y_min), b.hb);
                // Turned about Y only: local X goes to (cos θ, 0, -sin θ).
                let turn = Quat::from_rotation_y(libm::atan2f(-b.a.y, b.a.x));
                draw.wire_box(center, half, turn, DEBUG_COLLIDER);
            }
        }
        if on("lock") {
            self.debug_lock(draw);
        }
        if on("bonfires") {
            // Bonfires are root nodes: the local position is the world one.
            for (i, b) in self.bonfires.iter().enumerate() {
                let at = self.scene.transform(b.id).position;
                draw.circle(at, b.range, DEBUG_BONFIRE);
                if self.respawn.bonfire == Some(i) {
                    draw.cross(
                        at + Vec3::new(0.0, 1.0, 0.0),
                        DEBUG_BONFIRE_CROSS,
                        DEBUG_BONFIRE,
                    );
                }
            }
        }
        if on("bloodstain")
            && let Some(stain) = self.bloodstain
        {
            draw.circle(
                stain.position,
                self.params.bloodstain_range,
                DEBUG_BLOODSTAIN,
            );
        }
    }

    /// Every hurtbox's cylinder (pre-order), then the character's.
    fn debug_hurtboxes(&self, draw: &mut DebugDraw) {
        let color = |condition: Option<Condition>, invulnerable: bool| match condition {
            Some(Condition::Dead) => DEBUG_DEAD,
            Some(Condition::Stagger) => DEBUG_STAGGER,
            _ if invulnerable => DEBUG_INVULNERABLE,
            _ => DEBUG_HURTBOX,
        };
        for h in &self.hurtboxes {
            // Hurtboxes are root nodes: the local position is the world one.
            let at = self.scene.transform(h.id).position;
            let condition = h.health.map(|health| health.now.state);
            draw.cylinder(at, h.radius, h.height, color(condition, false));
        }
        let player = &self.state.player;
        let color = color(Some(self.health.now.state), player.invulnerable);
        draw.cylinder(player.position, self.radius, self.height, color);
    }

    /// The character's attack, then every swinger's (pre-order), in
    /// startup or active only: the sector at both heights of the band,
    /// and while active this tick's slice.
    fn debug_swings(&self, draw: &mut DebugDraw) {
        let player = &self.state.player;
        if matches!(player.action, Action::Attack | Action::Heavy)
            && let Some(phase) = player.attack_phase
        {
            let a = self.params.attack_of(player.action);
            let slice = (phase == AttackPhase::Active)
                .then(|| slice_of(a.arc, a.slice, player.action_tick - a.startup));
            let band = (self.params.swing_low, self.params.swing_high);
            let sweep = (player.position, player.facing, a.reach, a.arc);
            debug_sweep(draw, sweep, band, phase, slice);
        }
        for e in &self.enemies {
            // Enemies are root nodes: the local position is the world one.
            let at = self.scene.transform(e.id).position;
            let band = (e.params().low, e.params().high);
            // The roar's blast: the whole circle, bright on its tick.
            if let Some((p, phase)) = e.roar() {
                let slice = (phase == AttackPhase::Active).then_some((-PI, PI));
                debug_sweep(draw, (at, e.facing, p.reach, TAU), band, phase, slice);
                continue;
            }
            let Some((kind, m)) = e.moment() else {
                continue;
            };
            let slice = m.active.map(|k| e.slice(kind, k, m.reversed));
            let mv = e.params().of(kind);
            let facing = e.facing + Enemy::center(kind);
            debug_sweep(draw, (at, facing, mv.reach, mv.arc), band, m.phase, slice);
        }
        for s in &self.swingers {
            let Some(phase) = s.phase() else {
                continue;
            };
            let slice =
                (phase == AttackPhase::Active).then(|| s.slice_of(s.swing_tick - s.startup));
            // Swingers are root nodes: the local position is the world one.
            let at = self.scene.transform(s.id).position;
            debug_sweep(
                draw,
                (at, s.facing, s.reach, s.arc),
                (s.low, s.high),
                phase,
                slice,
            );
        }
    }

    /// A cross on every target's lock point (pre-order), the locked one's
    /// and a line to it from the camera pivot, then the lock ranges
    /// around the character's feet.
    fn debug_lock(&self, draw: &mut DebugDraw) {
        let point =
            |t: usize| self.target_position(t) + Vec3::new(0.0, self.targets[t].height, 0.0);
        for t in 0..self.targets.len() {
            let color = if self.target_dead(t) {
                DEBUG_DEAD
            } else {
                DEBUG_TARGET
            };
            draw.cross(point(t), DEBUG_TARGET_CROSS, color);
        }
        let feet = self.state.player.position;
        if let Some(t) = self.lock {
            draw.cross(point(t), DEBUG_LOCKED_CROSS, DEBUG_LOCKED);
            let pivot = feet + Vec3::new(0.0, self.pivot_height, 0.0);
            draw.line(pivot, point(t), DEBUG_LOCKED);
        }
        draw.circle(feet, self.params.lock.range, DEBUG_LOCK_RANGE);
        if self.lock.is_some() {
            draw.circle(feet, self.params.lock.break_range, DEBUG_BREAK_RANGE);
        }
    }
}
