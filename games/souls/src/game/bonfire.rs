//! Bonfires, souls and the bloodstain: sitting down, standing up, the
//! world reset, dropping souls on death.

use crate::events::{DropReason, EndReason, Event, Input, LockReason};
use crate::health::Condition;
use crate::moves::Action;

use super::roles::Respawn;
use super::state::{BloodstainState, PlayerState};
use super::{Game, hdist};

impl Game {
    /// Step 4a, the interact key: stands up while sitting; otherwise sits
    /// down at the first bonfire (pre-order) in reach, unless dead,
    /// staggered, busy, out of reach of every bonfire or near a living
    /// enemy, which drops the press. Whether it sat down or stood up.
    pub(super) fn interact(&mut self, player: &mut PlayerState) -> bool {
        if player.action == Action::Sit {
            self.stand(player);
            return true;
        }
        let bonfire = self
            .bonfires
            .iter()
            .position(|b| hdist(self.scene.transform(b.id).position, player.position) <= b.range);
        let enemy_near = self.enemies.iter().any(|e| {
            !self.hurtboxes[e.hurtbox]
                .health
                .is_some_and(|health| health.is_dead())
                && hdist(self.scene.transform(e.id).position, player.position)
                    <= e.params().aggro_range
        });
        let reason = if self.health.is_dead() {
            DropReason::Dead
        } else if self.health.now.state == Condition::Stagger {
            DropReason::Staggered
        } else if player.action == Action::Drink {
            DropReason::Drinking
        } else if !matches!(player.action, Action::Idle | Action::Sprint) {
            DropReason::Busy
        } else if let Some(stain) = self.bloodstain
            && hdist(stain.position, player.position) <= self.params.bloodstain_range
        {
            // At once, whatever the character is doing.
            self.bloodstain = None;
            player.souls = player.souls.saturating_add(stain.souls);
            self.events.push(Event::SoulsRecovered {
                amount: stain.souls,
                total: player.souls,
            });
            return false;
        } else if let Some(b) = bonfire
            && !enemy_near
        {
            self.sit(player, b);
            return true;
        } else if bonfire.is_none() {
            DropReason::NoBonfire
        } else {
            DropReason::EnemyNear
        };
        self.drop_inputs(&[Input::Interact], reason);
        false
    }

    /// Sits down at bonfire `b`: the sprint stops, full health and stamina,
    /// a full flask, the respawn point where the character stands, the
    /// lock let go, the world reset, then the `rest` event.
    fn sit(&mut self, player: &mut PlayerState, b: usize) {
        if player.action == Action::Sprint {
            self.events.push(Event::ActionEnd {
                action: Action::Sprint,
                reason: EndReason::Finished,
            });
        }
        player.action = Action::Sit;
        player.action_tick = 0;
        self.health = self.initial.health;
        player.flask = self.params.flask_charges;
        player.stamina = self.params.stamina_max;
        player.stamina_wait = 0;
        self.respawn = Respawn {
            position: player.position,
            facing: player.facing,
            bonfire: Some(b),
        };
        if self.lock.is_some() {
            self.lock = None;
            self.lock_event(None, LockReason::Rested);
        }
        self.reset_world();
        self.events.push(Event::Rest {
            bonfire: self.bonfires[b].path.clone(),
        });
    }

    /// Stands up from a bonfire.
    pub(super) fn stand(&mut self, player: &mut PlayerState) {
        player.action = Action::Idle;
        player.action_tick = 0;
        self.events.push(Event::Stand);
    }

    /// The world reset of sitting down and of the respawn: every hurtbox,
    /// swinger and enemy back as at load, but the hurtboxes' `hits` and
    /// `last_hit`, and the enemies' nodes where they stood at load.
    pub(super) fn reset_world(&mut self) {
        for (h, init) in self.hurtboxes.iter_mut().zip(&self.initial.hurtboxes) {
            let (hits, last_hit) = (h.hits, h.last_hit);
            *h = init.clone();
            h.hits = hits;
            h.last_hit = last_hit;
        }
        self.swingers.clone_from(&self.initial.swingers);
        self.enemies.clone_from(&self.initial.enemies);
        for (e, &(position, rotation)) in self.enemies.iter().zip(&self.initial.enemy_nodes) {
            let t = self.scene.transform_mut(e.id);
            t.position = position;
            t.rotation = rotation;
        }
    }

    /// On the character's death: the old bloodstain goes, and the souls it
    /// carries drop into a new one where it stands.
    pub(super) fn drop_souls(&mut self, player: &mut PlayerState) {
        let lost = self.bloodstain.take().map_or(0, |stain| stain.souls);
        let amount = std::mem::take(&mut player.souls);
        if amount > 0 {
            self.bloodstain = Some(BloodstainState {
                position: player.position,
                souls: amount,
            });
        }
        if amount > 0 || lost > 0 {
            self.events.push(Event::SoulsDropped {
                position: player.position,
                amount,
                lost,
            });
        }
    }

    /// Step 6 for the bloodstain marker: above the bloodstain by its scene
    /// file position, hidden without one.
    pub(super) fn show_bloodstain(&mut self) {
        let Some((id, offset)) = self.bloodstain_marker else {
            return;
        };
        if let Some(stain) = self.bloodstain {
            self.scene.transform_mut(id).position = stain.position + offset;
        }
        self.scene.set_visible(id, self.bloodstain.is_some());
    }
}
