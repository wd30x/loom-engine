//! The character's actions: rolls, drinks, attacks and their hits, being
//! hit, and the input buffer between actions.

use glam::Vec3;

use crate::enemy::EnemyCondition;
use crate::events::{DropReason, EndReason, Event, HitAttack, Input, LockReason, Outcome};
use crate::health::Condition;
use crate::moves::{Action, Attack, AttackPhase, Buffered};

use super::collision::{Sweep, resolve};
use super::enemy_ai::{enter_phase2, slice_of};
use super::state::{LastHit, PlayerHit, PlayerState};
use super::{Game, HP_TRAIL_WAIT, Intent, bearing, hdist, outranked, positive_zero, turn, wrap};

impl Game {
    /// `player_hit` of step 5c: the swinger `by`, standing at `from`, hits
    /// the character for `damage` and `impact`. A stagger or death breaks
    /// off whatever it was doing (no combo carries over); a death lets go
    /// of the lock, then drops the souls.
    pub(super) fn player_hit(
        &mut self,
        player: &mut PlayerState,
        from: Vec3,
        by: String,
        damage: f32,
        impact: f32,
        attack: HitAttack,
    ) {
        player.hits += 1;
        player.last_hit = Some(PlayerHit {
            tick: self.clock.tick(),
            by: by.clone(),
        });
        // Away from the swinger; -Z when on one spot, like a hurtbox.
        let fall = if hdist(from, player.position) > 0.0 {
            bearing(from, player.position)
        } else {
            0.0
        };
        let before = self.health.now.hp;
        let outcome = self.health.take(damage, impact, fall);
        if self.health.now.hp < before {
            player.hp_trail_wait = HP_TRAIL_WAIT;
        }
        self.events.push(Event::Hit {
            by,
            target: self.player_path.clone(),
            attack,
            combo: 0,
            damage: Some(damage),
            hp: Some(self.health.now.hp),
            result: outcome,
        });
        if self.health.now.state != Condition::Ok {
            if player.action != Action::Idle {
                self.events.push(Event::ActionEnd {
                    action: player.action,
                    reason: EndReason::Interrupted,
                });
            }
            player.action = Action::Idle;
            player.action_tick = 0;
            player.attack_phase = None;
            player.combo = 0;
            player.buffered = None;
            player.invulnerable = false;
            self.finished = (Action::Idle, 0);
        }
        if outcome == Outcome::Death && self.lock.is_some() {
            self.lock = None;
            self.lock_event(None, LockReason::Died);
        }
        if outcome == Outcome::Death {
            self.drop_souls(player);
        }
    }

    /// Starts a roll the way the input points, or a backstep without a
    /// direction, and runs its first tick.
    pub(super) fn start_roll(
        &mut self,
        player: &mut PlayerState,
        from_buffer: bool,
        intent: &Intent,
    ) {
        if intent.v.length_squared() > 0.0 {
            // Roll the way the input points, turning there at once.
            player.action = Action::Roll;
            self.action_dir = intent.dir / intent.dir.length();
            player.facing =
                positive_zero(wrap(libm::atan2f(-self.action_dir.x, -self.action_dir.z)));
        } else {
            // Backstep: away from where the character faces, no turn.
            player.action = Action::Backstep;
            self.action_dir = Vec3::new(libm::sinf(player.facing), 0.0, libm::cosf(player.facing));
        }
        self.events.push(Event::ActionStart {
            action: player.action,
            combo: 0,
            buffered: from_buffer,
        });
        let m = self.params.movement(player.action);
        player.stamina = f32::max(player.stamina - m.cost, 0.0);
        player.stamina_wait = self.params.delay_ticks;
        player.action_tick = 1;
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + self.action_dir * m.step,
            &self.enemy_circles(),
        );
        if m.ticks == 1 {
            self.end_action(player, 0);
        }
    }

    /// Starts a drink, using up a flask, and runs its first tick.
    pub(super) fn start_drink(
        &mut self,
        player: &mut PlayerState,
        from_buffer: bool,
        intent: &Intent,
    ) {
        player.flask -= 1;
        player.action = Action::Drink;
        player.action_tick = 1;
        self.events.push(Event::ActionStart {
            action: Action::Drink,
            combo: 0,
            buffered: from_buffer,
        });
        self.drink_tick(player, intent);
    }

    /// Tick `action_tick` of the running drink: walking slowly and turning
    /// as step 5 does, healing on `flask_heal_tick`. The last tick ends
    /// the drink.
    pub(super) fn drink_tick(&mut self, player: &mut PlayerState, intent: &Intent) {
        player.position = resolve(
            &self.boxes,
            self.radius,
            self.height,
            player.position + intent.dir * self.params.flask_step,
            &self.enemy_circles(),
        );
        self.turn_player(player, intent);
        if player.action_tick == self.params.flask_heal_tick {
            let now = &mut self.health.now;
            let before = now.hp;
            now.hp = f32::min(
                now.hp + self.params.flask_heal * self.health.hp_max,
                self.health.hp_max,
            );
            self.events.push(Event::Heal {
                amount: now.hp - before,
                hp: now.hp,
            });
        }
        if player.action_tick == self.params.flask_ticks {
            self.end_action(player, 0);
        }
    }

    /// Step 5, turning at most `max_turn` per tick: towards the target
    /// while locked on and not sprinting, otherwise towards `dir`.
    pub(super) fn turn_player(&self, player: &mut PlayerState, intent: &Intent) {
        if let Some(t) = self.lock
            && player.action != Action::Sprint
        {
            let to = self.target_position(t);
            if hdist(player.position, to) > 0.0 {
                player.facing = turn(player.facing, bearing(player.position, to), self.max_turn);
            }
        } else if intent.v.length_squared() > 0.0 {
            let dir = intent.dir;
            player.facing = turn(player.facing, libm::atan2f(-dir.x, -dir.z), self.max_turn);
        }
    }

    /// Starts the light or heavy attack `action` and runs its first tick.
    /// A light attack continues the combo only when it was buffered right
    /// after a light attack that had not reached `combo_max`.
    pub(super) fn start_attack(
        &mut self,
        player: &mut PlayerState,
        action: Action,
        from_buffer: bool,
        intent: &Intent,
    ) {
        let (last, last_combo) = self.finished;
        player.combo = match action {
            Action::Attack
                if from_buffer && last == Action::Attack && last_combo < self.params.combo_max =>
            {
                last_combo + 1
            }
            Action::Attack => 1,
            _ => 0,
        };
        player.action = action;
        player.action_tick = 1;
        self.events.push(Event::ActionStart {
            action,
            combo: player.combo,
            buffered: from_buffer,
        });
        let a = self.params.attack_of(action);
        player.stamina = f32::max(player.stamina - a.cost, 0.0);
        player.stamina_wait = self.params.delay_ticks;
        // Each attack hits each hurtbox at most once.
        self.struck.clear();
        self.attack_tick(player, intent);
    }

    /// Tick `action_tick` of the running attack: turning during startup,
    /// sliding forward and hitting while active, and from then on buffering
    /// the next action. The last tick ends the attack.
    pub(super) fn attack_tick(&mut self, player: &mut PlayerState, intent: &Intent) {
        let a = self.params.attack_of(player.action);
        let presses = intent.presses();
        let phase = if player.action_tick <= a.startup {
            // Presses on tick 1 are `next_action`'s, which started the attack.
            if player.action_tick > 1 {
                self.drop_presses(&presses, DropReason::Startup);
            }
            // Towards the target while locked on, otherwise towards `dir`.
            let step = self.params.attack_turn_step;
            if let Some(t) = self.lock {
                let to = self.target_position(t);
                if hdist(player.position, to) > 0.0 {
                    player.facing = turn(player.facing, bearing(player.position, to), step);
                }
            } else if intent.v.length_squared() > 0.0 {
                let target = libm::atan2f(-intent.dir.x, -intent.dir.z);
                player.facing = turn(player.facing, target, step);
            }
            AttackPhase::Startup
        } else if player.action_tick <= a.startup + a.active {
            let forward = Vec3::new(-libm::sinf(player.facing), 0.0, -libm::cosf(player.facing));
            player.position = resolve(
                &self.boxes,
                self.radius,
                self.height,
                player.position + forward * a.step,
                &self.enemy_circles(),
            );
            self.hit(player, a);
            AttackPhase::Active
        } else {
            AttackPhase::Recovery
        };
        player.attack_phase = Some(phase);
        if phase != AttackPhase::Startup {
            // A later press replaces an earlier one; on one tick, heavy
            // beats light beats the roll key. Startup presses are dropped.
            if let Some((&new, outranked)) = presses.split_first() {
                if let Some(old) = player.buffered {
                    self.events.push(Event::BufferReplaced { old, new });
                }
                player.buffered = Some(new);
                self.drop_presses(outranked, DropReason::Outranked);
            }
        }
        if player.action_tick == a.startup + a.active + a.recovery {
            let combo = player.combo;
            self.end_action(player, combo);
            player.attack_phase = None;
            player.combo = 0;
        }
    }

    /// The hit check of active tick `action_tick - startup` of the attack
    /// `a`, after the slide: the tick sweeps its slice of the sector, right
    /// to left, and hits every hurtbox it touches that is not dead, not a
    /// roaring enemy and this attack has not hit yet. A hurtbox with health
    /// takes the damage, and killed gives its souls; one killed while locked
    /// on is let go of; an enemy
    /// left with at most `phase2_at` of its health enters phase 2.
    fn hit(&mut self, player: &mut PlayerState, a: Attack) {
        let (lo, hi) = slice_of(a.arc, a.slice, player.action_tick - a.startup);
        let sweep = Sweep {
            p: player.position,
            facing: player.facing,
            reach: a.reach,
            lo,
            hi,
            low: self.params.swing_low,
            high: self.params.swing_high,
        };
        let tick = self.clock.tick();
        for (i, h) in self.hurtboxes.iter_mut().enumerate() {
            if h.health.is_some_and(|health| health.is_dead()) {
                continue;
            }
            // A roaring enemy is invulnerable: not even struck.
            if self
                .enemies
                .iter()
                .any(|e| e.hurtbox == i && e.state == EnemyCondition::Roar)
            {
                continue;
            }
            // Hurtboxes are root nodes: the local position is the world one.
            let at = self.scene.transform(h.id).position;
            if !self.struck.contains(&i) && sweep.touches(at, h.radius, h.height) {
                h.hits += 1;
                h.last_hit = Some(LastHit {
                    tick,
                    attack: player.action,
                    combo: player.combo,
                });
                self.struck.push(i);
                // Away from the character; -Z when they stand on one spot,
                // where `bearing` would give -π.
                let fall = if hdist(sweep.p, at) > 0.0 {
                    bearing(sweep.p, at)
                } else {
                    0.0
                };
                let attack = match player.action {
                    Action::Heavy => HitAttack::Heavy,
                    _ => HitAttack::Attack,
                };
                let (damage, hp, result) = match &mut h.health {
                    Some(health) => {
                        let outcome = health.take(a.damage, a.impact, fall);
                        (Some(a.damage), Some(health.now.hp), outcome)
                    }
                    None => (None, None, Outcome::Hit),
                };
                self.events.push(Event::Hit {
                    by: self.player_path.clone(),
                    target: h.path.clone(),
                    attack,
                    combo: player.combo,
                    damage,
                    hp,
                    result,
                });
                if result == Outcome::Death && h.souls > 0 {
                    player.souls = player.souls.saturating_add(h.souls);
                    self.events.push(Event::Souls {
                        from: h.path.clone(),
                        amount: h.souls,
                        total: player.souls,
                    });
                }
                if let Some(health) = &mut h.health
                    && let Some(e) = self.enemies.iter_mut().find(|e| e.hurtbox == i)
                {
                    enter_phase2(e, health, &mut self.events);
                }
                if result == Outcome::Death
                    && self
                        .lock
                        .is_some_and(|t| self.targets[t].hurtbox == Some(i))
                {
                    self.lock = None;
                    self.events.push(Event::Lock {
                        target: None,
                        reason: LockReason::TargetDied,
                    });
                }
            }
        }
    }

    /// Ends the running roll, backstep, attack or drink on its last tick; `combo`
    /// is what a buffered light attack continues from.
    pub(super) fn end_action(&mut self, player: &mut PlayerState, combo: u32) {
        self.events.push(Event::ActionEnd {
            action: player.action,
            reason: EndReason::Finished,
        });
        self.finished = (player.action, combo);
        player.action = Action::Idle;
        player.action_tick = 0;
    }

    /// `input_dropped` for each of `inputs`.
    pub(super) fn drop_inputs(&mut self, inputs: &[Input], reason: DropReason) {
        for &input in inputs {
            self.events.push(Event::InputDropped {
                input,
                reason,
                buffered: false,
            });
        }
    }

    /// `input_dropped` for each of this tick's `presses`.
    pub(super) fn drop_presses(&mut self, presses: &[Buffered], reason: DropReason) {
        let inputs: Vec<Input> = presses.iter().map(|&p| p.into()).collect();
        self.drop_inputs(&inputs, reason);
    }

    /// `next_action` of step 4: the buffered action, else this tick's first
    /// press (heavy, then light, then a roll-key tap), with whether it was
    /// buffered. With no stamina left there is none but a drink, and with
    /// no flask left no drink; then the input, buffered or not, is
    /// dropped, along with the presses it outranked.
    pub(super) fn next_action(
        &mut self,
        player: &mut PlayerState,
        presses: &[Buffered],
    ) -> Option<(Buffered, bool)> {
        let next = if let Some(buffered) = player.buffered.take() {
            (buffered, true)
        } else {
            (*presses.first()?, false)
        };
        let reason = match next.0 {
            Buffered::Drink if player.flask == 0 => DropReason::Empty,
            Buffered::Drink => return Some(next),
            _ if player.stamina > 0.0 => return Some(next),
            _ => DropReason::NoStamina,
        };
        let (next, from_buffer) = next;
        self.events.push(Event::InputDropped {
            input: next.into(),
            reason,
            buffered: from_buffer,
        });
        self.drop_presses(outranked(presses, from_buffer), DropReason::Outranked);
        None
    }
}
