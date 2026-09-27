//! How the game's state shows on the scene's nodes: the character's and
//! the enemies' colors and poses, the arms, the flash of a hit.

use glam::{Quat, Vec3};

use crate::health::Condition;
use crate::moves::{Action, AttackPhase};

use super::Game;
use super::roles::Enemy;

/// How long a hurtbox flashes after a hit, in ticks, counting the tick of
/// the hit. A stand-in for real hit feedback, like the constants below.
pub(super) const FLASH_TICKS: u64 = 8;
/// The color a hurtbox's meshes flash in.
pub(super) const FLASH_COLOR: Vec3 = Vec3::ONE;
/// The color the character's model flashes in: its own colors are white
/// already, so reddish.
pub(super) const FLASH_TINT: Vec3 = Vec3::new(1.0, 0.45, 0.45);
/// A dead hurtbox's meshes are their scene colors times this.
pub(super) const DEAD_DIM: f32 = 0.4;
/// How far a staggered hurtbox tips over, away from its attacker, in degrees.
const STAGGER_TILT_DEGREES: f32 = 20.0;
/// How far a dead hurtbox tips over: flat on the ground.
const DEAD_TILT_DEGREES: f32 = 90.0;
/// The color of a swinger's arm during its startup: the warning that a
/// swing is coming. A stand-in, like the flash.
pub(super) const WARN_COLOR: Vec3 = Vec3::new(1.0, 0.55, 0.1);
/// How far below its `position` the character node is drawn while sitting.
const SIT_DROP: f32 = 0.5;

/// What an enemy's scene colors are multiplied by in phase 2: dark red.
const PHASE2_TINT: Vec3 = Vec3::new(1.0, 0.3, 0.3);

/// The rotation of a node with health whose standing rotation is `base`:
/// `base` itself while `Ok` (bit for bit), otherwise tipped over towards
/// `fall` by [`STAGGER_TILT_DEGREES`] or [`DEAD_TILT_DEGREES`].
pub(super) fn pose(state: Condition, fall: f32, base: Quat) -> Quat {
    let degrees = match state {
        Condition::Ok => return base,
        Condition::Stagger => STAGGER_TILT_DEGREES,
        Condition::Dead => DEAD_TILT_DEGREES,
    };
    // The top tips over towards `fall`, however the node faces.
    Quat::from_rotation_y(fall)
        * Quat::from_rotation_x(-degrees.to_radians())
        * Quat::from_rotation_y(-fall)
        * base
}

/// The color a mesh of scene color `color` shows: darkened when `dead`,
/// else [`FLASH_COLOR`] while `flashing`, else its own.
fn shown(color: Vec3, dead: bool, flashing: bool) -> Vec3 {
    if dead {
        color * DEAD_DIM
    } else if flashing {
        FLASH_COLOR
    } else {
        color
    }
}

impl Game {
    /// Step 6 for the character node: its position, its pose (turned to
    /// `facing`, tipped over away from the swinger when staggered or dead)
    /// and the colors of its meshes, like a hurtbox's.
    pub(super) fn show_player(&mut self) {
        let tick = self.clock.tick();
        let player = &self.state.player;
        let state = self.health.now.state;
        let transform = self.scene.transform_mut(self.player);
        transform.position = if player.action == Action::Sit {
            player.position - Vec3::new(0.0, SIT_DROP, 0.0)
        } else {
            player.position
        };
        transform.rotation = pose(
            state,
            self.health.fall,
            Quat::from_rotation_y(player.facing),
        );
        let flashing = player
            .last_hit
            .as_ref()
            .is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
        for &(id, color, model) in &self.player_meshes {
            let dead = state == Condition::Dead;
            let color = if model && flashing && !dead {
                FLASH_TINT
            } else {
                shown(color, dead, flashing)
            };
            self.scene
                .set_mesh_color(id, color)
                .expect("meshes and models were picked for having one, and every color is valid");
        }
        if let Some(f) = self.flask {
            let drinking = self.state.player.action == Action::Drink;
            self.scene.set_visible(f, drinking);
        }
    }

    /// After the hurtboxes, every swinger's arm shows its round: at the
    /// sector's right end during startup, at the end of this tick's slice
    /// while active, otherwise on the center line as in the scene file.
    /// It darkens and flashes with its swinger, and otherwise shows
    /// [`WARN_COLOR`] during startup.
    pub(super) fn show_arms(&mut self) {
        let tick = self.clock.tick();
        for s in &self.swingers {
            let Some(arm) = s.arm else {
                continue;
            };
            let phase = s.phase();
            let angle = match phase {
                Some(AttackPhase::Startup) => Some(-(s.arc * 0.5)),
                Some(AttackPhase::Active) => Some(s.slice_of(s.swing_tick - s.startup).1),
                Some(AttackPhase::Recovery) | None => None,
            };
            let t = self.scene.transform_mut(arm.id);
            match angle {
                // Exactly as in the scene file.
                None => {
                    t.position = arm.p0;
                    t.rotation = arm.r0;
                }
                Some(angle) => {
                    let q = Quat::from_rotation_y(angle);
                    t.position = q * arm.p0;
                    t.rotation = q * arm.r0;
                }
            }
            let hurtbox = s.hurtbox.map(|h| &self.hurtboxes[h]);
            let dead = hurtbox
                .and_then(|h| h.health)
                .is_some_and(|health| health.is_dead());
            let flashing = hurtbox
                .and_then(|h| h.last_hit)
                .is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            let color = if !dead && !flashing && phase == Some(AttackPhase::Startup) {
                WARN_COLOR
            } else {
                shown(arm.color, dead, flashing)
            };
            self.scene
                .set_mesh_color(arm.id, color)
                .expect("an arm has a mesh (checked at load), and every color is valid");
        }
        // An enemy's arm: the same, by its move; at the other end of the
        // sector for an even cut of a combo.
        for e in &self.enemies {
            let Some(arm) = e.arm else {
                continue;
            };
            let moment = e.moment();
            let phase = moment.map(|(_, m)| m.phase);
            let angle = moment.and_then(|(kind, m)| {
                let half = e.params().of(kind).arc * 0.5;
                let angle = match m.phase {
                    AttackPhase::Startup => Some(if m.reversed { half } else { -half }),
                    AttackPhase::Active => {
                        let k = m.active.expect("an active tick has its number");
                        let (lo, hi) = e.slice(kind, k, m.reversed);
                        Some(if m.reversed { lo } else { hi })
                    }
                    AttackPhase::Recovery => None,
                };
                angle.map(|a| a + Enemy::center(kind))
            });
            let t = self.scene.transform_mut(arm.id);
            match angle {
                None => {
                    t.position = arm.p0;
                    t.rotation = arm.r0;
                }
                Some(angle) => {
                    let q = Quat::from_rotation_y(angle);
                    t.position = q * arm.p0;
                    t.rotation = q * arm.r0;
                }
            }
            let h = &self.hurtboxes[e.hurtbox];
            let dead = h.health.is_some_and(|health| health.is_dead());
            let flashing = h.last_hit.is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            let color = if !dead && !flashing && phase == Some(AttackPhase::Startup) {
                WARN_COLOR
            } else {
                shown(arm.color, dead, flashing)
            };
            self.scene
                .set_mesh_color(arm.id, color)
                .expect("an arm has a mesh (checked at load), and every color is valid");
        }
    }

    /// After step 6, every hurtbox shows its state; everything is set every
    /// tick, whether it changed or not:
    /// - its meshes (but the hp bar): the scene colors times [`DEAD_DIM`]
    ///   when dead, else [`FLASH_COLOR`] within [`FLASH_TICKS`] ticks of a
    ///   hit (this one included), else for an enemy (but its arm, which
    ///   [`Game::show_arms`] colors) [`WARN_COLOR`] roaring before the
    ///   blast and the scene colors times [`PHASE2_TINT`] in phase 2, else
    ///   the scene colors;
    /// - with health, its pose: the scene rotation, or when staggered or dead
    ///   tipped over by [`STAGGER_TILT_DEGREES`] or [`DEAD_TILT_DEGREES`]
    ///   towards its `fall`, away from the character;
    /// - its hp bar: as long as the health left, the left end in place,
    ///   hidden when dead.
    pub(super) fn show_hurtboxes(&mut self) {
        let tick = self.clock.tick();
        for (i, h) in self.hurtboxes.iter().enumerate() {
            let condition = h.health.map(|health| health.now.state);
            let dead = condition == Some(Condition::Dead);
            let flashing = h.last_hit.is_some_and(|hit| tick - hit.tick < FLASH_TICKS);
            // An enemy's meshes but its arm: the warning color before the
            // roar's blast, else darkened red in phase 2.
            let enemy = self.enemies.iter().find(|e| e.hurtbox == i);
            let warning =
                enemy.is_some_and(|e| e.roar().is_some_and(|(_, p)| p == AttackPhase::Startup));
            let phase2 = enemy.is_some_and(|e| e.stage == 2);
            let arm = enemy.and_then(|e| e.arm).map(|arm| arm.id);
            for &(id, color) in &h.meshes {
                let color = if dead || flashing || Some(id) == arm {
                    shown(color, dead, flashing)
                } else if warning {
                    WARN_COLOR
                } else if phase2 {
                    color * PHASE2_TINT
                } else {
                    color
                };
                self.scene
                    .set_mesh_color(id, color)
                    .expect("meshes were picked for having a mesh, and every color is valid");
            }
            let Some(health) = h.health else {
                continue;
            };
            self.scene.transform_mut(h.id).rotation = pose(health.now.state, health.fall, h.base);
            if let Some(bar) = health.bar {
                let frac = health.now.hp / health.hp_max;
                let t = self.scene.transform_mut(bar.id);
                t.scale.x = bar.width * frac;
                t.position.x = bar.x0 - bar.width * (1.0 - frac) / 2.0;
                // A bar scaled to nothing cannot be drawn properly: hide it.
                self.scene.set_visible(bar.id, !dead);
            }
        }
    }
}
