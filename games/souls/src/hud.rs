//! The HUD (`docs/architecture.md` section 11, "HUD"), a child of `game` so
//! it can read the game's own fields: the health and stamina bars at the
//! top left, the boss bars at the bottom, the lock-on dot. HUD units: the
//! screen is 720 high and `view.width` wide.

use glam::{Vec2, Vec3, Vec4};
use loom_core::hud::{Hud, HudView};

use super::{Game, hdist};

/// Under every bar: its full length.
pub(super) const BAR_BACK: Vec4 = Vec4::new(0.02, 0.02, 0.02, 0.75);
/// The health left, on the character's bar and the bosses'.
pub(super) const BAR_HP: Vec4 = Vec4::new(0.6, 0.05, 0.05, 1.0);
/// The character's health bar trail, between the back and the health.
pub(super) const BAR_TRAIL: Vec4 = Vec4::new(0.85, 0.7, 0.35, 1.0);
pub(super) const BAR_STAMINA: Vec4 = Vec4::new(0.1, 0.5, 0.15, 1.0);
pub(super) const LOCK_DOT: Vec4 = Vec4::new(1.0, 1.0, 1.0, 0.9);

/// HUD units of bar per hp and per stamina point.
const PER_POINT: f32 = 3.0;
pub(super) const HP_BAR_AT: Vec2 = Vec2::new(40.0, 40.0);
pub(super) const HP_BAR_HEIGHT: f32 = 12.0;
pub(super) const STAMINA_BAR_AT: Vec2 = Vec2::new(40.0, 58.0);
pub(super) const STAMINA_BAR_HEIGHT: f32 = 8.0;
pub(super) const BOSS_BAR_LENGTH: f32 = 700.0;
pub(super) const BOSS_BAR_HEIGHT: f32 = 10.0;
/// How far above the bottom the first boss bar's top is.
pub(super) const BOSS_BAR_FROM_BOTTOM: f32 = 60.0;
/// How much higher each further boss bar is.
pub(super) const BOSS_BAR_STEP: f32 = 30.0;
pub(super) const LOCK_DOT_RADIUS: f32 = 5.0;

/// Adds the layers of a bar at `at`, `height` high: each `(length, color)`
/// in order, from the left end, skipping those of length 0.
fn bar(hud: &mut Hud, at: Vec2, height: f32, layers: &[(f32, Vec4)]) {
    for &(length, color) in layers {
        if length > 0.0 {
            hud.rect(at, Vec2::new(length, height), color);
        }
    }
}

impl Game {
    /// The HUD of the current state for `view`, in this order: the health
    /// bar (back, trail, health), the stamina bar (back, stamina), a bar
    /// for each boss in pre-order, the lock-on dot. Changes nothing.
    pub fn hud(&self, view: &HudView, hud: &mut Hud) {
        let player = &self.state.player;
        let params = &self.params;

        let full = params.hp * PER_POINT;
        let hp_max = self.health.hp_max;
        bar(
            hud,
            HP_BAR_AT,
            HP_BAR_HEIGHT,
            &[
                (full, BAR_BACK),
                (full * player.hp_trail / hp_max, BAR_TRAIL),
                (full * player.health.hp / hp_max, BAR_HP),
            ],
        );
        let full = params.stamina_max * PER_POINT;
        bar(
            hud,
            STAMINA_BAR_AT,
            STAMINA_BAR_HEIGHT,
            &[
                (full, BAR_BACK),
                (full * player.stamina / params.stamina_max, BAR_STAMINA),
            ],
        );

        let x = (view.width - BOSS_BAR_LENGTH) / 2.0;
        let mut y = view.height - BOSS_BAR_FROM_BOTTOM;
        if !self.health.is_dead() {
            for e in self.enemies.iter().filter(|e| e.boss_bar) {
                let Some(health) = self.hurtboxes[e.hurtbox].health else {
                    continue;
                };
                // Enemies are root nodes: the local position is the world one.
                let at = self.scene.transform(e.id).position;
                if health.is_dead() || hdist(at, player.position) > e.params().aggro_range {
                    continue;
                }
                bar(
                    hud,
                    Vec2::new(x, y),
                    BOSS_BAR_HEIGHT,
                    &[
                        (BOSS_BAR_LENGTH, BAR_BACK),
                        (BOSS_BAR_LENGTH * health.now.hp / health.hp_max, BAR_HP),
                    ],
                );
                y -= BOSS_BAR_STEP;
            }
        }

        if let Some(t) = self.lock {
            let point = self.target_position(t) + Vec3::new(0.0, self.targets[t].height, 0.0);
            if let Some(center) = view.project(point) {
                hud.disc(center, LOCK_DOT_RADIUS, LOCK_DOT);
            }
        }
    }
}
