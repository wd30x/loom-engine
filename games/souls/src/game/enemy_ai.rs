//! The enemies and the stakes: what each chooses to do, how it moves,
//! its swings, its second phase.

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use crate::enemy::{EnemyChoice, EnemyCondition, MoveKind, MoveParams};
use crate::events::{Event, HitAttack};
use crate::health::{Condition, Health};
use crate::moves::AttackPhase;

use super::collision::{Sweep, resolve};
use super::roles::Enemy;
use super::state::PlayerState;
use super::{Game, bearing, hdist, turn, wrap};

/// `(lo, hi)` of active tick `k` (1-based) of a sweep of `arc` in slices
/// of `slice`, right to left: relative to `facing`, positive to the left.
pub(super) fn slice_of(arc: f32, slice: f32, k: u32) -> (f32, f32) {
    let half = arc * 0.5;
    (-half + (k - 1) as f32 * slice, -half + k as f32 * slice)
}

/// An enemy with `phase2_at` still in phase 1, hit to at most that much of
/// its full health but not killed, enters phase 2: the hit's stagger is
/// dropped for a roar, its move broken off, and a `stage` event follows
/// the hit's.
pub(super) fn enter_phase2(e: &mut Enemy, health: &mut Health, events: &mut Vec<Event>) {
    let Some(p) = e.phase2 else {
        return;
    };
    let hp = health.now.hp;
    if e.stage != 1 || hp <= 0.0 || hp > p.at * health.hp_max {
        return;
    }
    e.stage = 2;
    health.now.state = Condition::Ok;
    health.now.state_ticks = 0;
    health.now.impact = 0.0;
    health.now.impact_wait = 0;
    e.state = EnemyCondition::Roar;
    e.current = None;
    e.move_tick = 0;
    e.rest = 0;
    e.chase = 0;
    e.struck = 0;
    events.push(Event::Stage {
        by: e.path.clone(),
        stage: 2,
    });
}

impl Game {
    /// Step 5c: every swinger (pre-order) moves on in its round, or stops
    /// while staggered or dead, to start a new round once it stands again.
    /// An active tick sweeps its slice of the sector and hits the character
    /// it touches, at most once a round, unless the character is dead or
    /// invulnerable.
    pub(super) fn swing(&mut self, player: &mut PlayerState) {
        for i in 0..self.swingers.len() {
            let s = &mut self.swingers[i];
            if s.hurtbox
                .and_then(|h| self.hurtboxes[h].health)
                .is_some_and(|health| health.now.state != Condition::Ok)
            {
                s.swing_tick = 0;
                continue;
            }
            s.swing_tick = s.swing_tick % s.period + 1;
            if s.swing_tick == 1 {
                s.struck = false;
            }
            if s.phase() != Some(AttackPhase::Active) || s.struck || self.health.is_dead() {
                continue;
            }
            let (lo, hi) = s.slice_of(s.swing_tick - s.startup);
            // Swingers are root nodes: the local position is the world one.
            let at = self.scene.transform(s.id).position;
            let sweep = Sweep {
                p: at,
                facing: s.facing,
                reach: s.reach,
                lo,
                hi,
                low: s.low,
                high: s.high,
            };
            if !sweep.touches(player.position, self.radius, self.height) {
                continue;
            }
            // Invulnerable: no hit, and the round may still hit later.
            if player.invulnerable {
                self.events.push(Event::Dodged {
                    by: s.path.clone(),
                    action: player.action,
                    action_tick: player.action_tick,
                });
                continue;
            }
            s.struck = true;
            let (by, damage, impact) = (s.path.clone(), s.damage, s.impact);
            self.player_hit(player, at, by, damage, impact, HitAttack::Swing);
        }
    }

    /// Step 5c for the enemies (pre-order), before the swingers: each one
    /// follows its hurtbox into a stagger or death, breaking off its move,
    /// else goes on with its move or rest, or chooses a move, or turns to
    /// the character and walks up to it.
    pub(super) fn run_enemies(&mut self, player: &mut PlayerState) {
        for i in 0..self.enemies.len() {
            let h = self.enemies[i].hurtbox;
            let condition = self.hurtboxes[h]
                .health
                .expect("an enemy has hp (checked at load)")
                .now
                .state;
            let player_dead = self.health.is_dead();
            let at = self.scene.transform(self.enemies[i].id).position;
            let d = hdist(at, player.position);
            let e = &mut self.enemies[i];
            if condition != Condition::Ok {
                e.state = match condition {
                    Condition::Dead => EnemyCondition::Dead,
                    Condition::Stagger | Condition::Ok => EnemyCondition::Stagger,
                };
                e.current = None;
                e.move_tick = 0;
                e.rest = 0;
                e.chase = 0;
                e.struck = 0;
                continue;
            }
            if e.state == EnemyCondition::Dead {
                // Revived: back in phase 1.
                e.stage = 1;
            }
            if matches!(e.state, EnemyCondition::Stagger | EnemyCondition::Dead) {
                e.state = EnemyCondition::Idle;
            }
            if (player_dead || d > e.params().aggro_range)
                && !matches!(e.state, EnemyCondition::Attack | EnemyCondition::Roar)
            {
                e.state = EnemyCondition::Idle;
                e.rest = 0;
                e.chase = 0;
                continue;
            }
            match e.state {
                EnemyCondition::Attack => {
                    e.move_tick += 1;
                    self.enemy_move_tick(i, player);
                }
                EnemyCondition::Rest => {
                    if d > 0.0 {
                        e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
                    }
                    e.rest = e.rest.saturating_sub(1);
                    if e.rest == 0 {
                        e.state = EnemyCondition::Idle;
                    }
                }
                EnemyCondition::Roar => self.enemy_roar(i, player),
                EnemyCondition::Chase if e.chase > 0 => self.enemy_approach(i, player),
                EnemyCondition::Idle | EnemyCondition::Chase => self.enemy_choose(i, player),
                EnemyCondition::Stagger | EnemyCondition::Dead => {
                    unreachable!("a stagger or death was left above")
                }
            }
        }
    }

    /// An idle enemy `i`, or one walking up with nothing to choose: when it
    /// faces the character closely enough, chooses at random among the
    /// moves in range and the approach, and starts the choice; otherwise
    /// turns to the character, walking up while there is nothing to choose.
    fn enemy_choose(&mut self, i: usize, player: &mut PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let d = hdist(at, player.position);
        let e = &mut self.enemies[i];
        let p = *e.params();
        let mut weighted: Vec<(EnemyChoice, u32)> = MoveKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let m = p.of(kind);
                (p.has(kind) && m.weight > 0 && m.min_range <= d && d <= m.max_range)
                    .then_some((kind.into(), m.weight))
            })
            .collect();
        let approach = p.approach_weight(d);
        if approach >= 1 {
            weighted.push((EnemyChoice::Approach, approach));
        }
        let off = wrap(bearing(at, player.position) - e.facing).abs();
        if !weighted.is_empty() && off <= p.face_angle {
            let total: u64 = weighted.iter().map(|&(_, w)| u64::from(w)).sum();
            let r = e.rng.next() % total;
            let mut sum = 0;
            let chosen = weighted
                .iter()
                .find(|&&(_, w)| {
                    sum += u64::from(w);
                    sum > r
                })
                .expect("r is less than the total of the weights")
                .0;
            self.events.push(Event::EnemyMove {
                by: e.path.clone(),
                r#move: chosen,
                distance: d,
                candidates: weighted.iter().map(|&(c, _)| c).collect(),
            });
            let kind = match chosen {
                EnemyChoice::Sweep => MoveKind::Sweep,
                EnemyChoice::Cleave => MoveKind::Cleave,
                EnemyChoice::Lunge => MoveKind::Lunge,
                EnemyChoice::Combo => MoveKind::Combo,
                EnemyChoice::Spin => MoveKind::Spin,
                EnemyChoice::Approach => {
                    e.state = EnemyCondition::Chase;
                    e.chase = p.approach_ticks;
                    self.enemy_approach(i, player);
                    return;
                }
            };
            e.state = EnemyCondition::Attack;
            e.current = Some(kind);
            e.move_tick = 1;
            e.struck = 0;
            self.enemy_move_tick(i, player);
            return;
        }
        if d > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), p.turn);
        }
        if weighted.is_empty() {
            e.state = EnemyCondition::Chase;
            self.enemy_walk(i, player.position);
        } else {
            e.state = EnemyCondition::Idle;
        }
    }

    /// A tick of enemy `i`'s chosen approach: turns to the character and
    /// walks up to it; idle again once the approach's ticks are used up or
    /// it has come within `approach_min_range`.
    fn enemy_approach(&mut self, i: usize, player: &PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let e = &mut self.enemies[i];
        if hdist(at, player.position) > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
        }
        let moved = self.enemy_walk(i, player.position);
        let e = &mut self.enemies[i];
        e.chase -= 1;
        if e.chase == 0 || hdist(moved, player.position) <= e.params().approach_min {
            e.chase = 0;
            e.state = EnemyCondition::Idle;
        }
    }

    /// Enemy `i` walks a step straight at the character at `to` (none on
    /// its spot), with collisions; where it ends up.
    fn enemy_walk(&mut self, i: usize, to: Vec3) -> Vec3 {
        let id = self.enemies[i].id;
        let at = self.scene.transform(id).position;
        let d = hdist(at, to);
        if d == 0.0 {
            return at;
        }
        let dir = Vec3::new(to.x - at.x, 0.0, to.z - at.z) / d;
        let moved = self.enemy_resolve(i, at + dir * self.enemies[i].params().step, to);
        self.scene.transform_mut(id).position = moved;
        moved
    }

    /// Tick `move_tick` of enemy `i`'s move: turning to the character
    /// during startup; while active, the lunge's step forward, then the
    /// hit check of this tick's slice; after the last tick, the rest.
    fn enemy_move_tick(&mut self, i: usize, player: &mut PlayerState) {
        let e = &self.enemies[i];
        let (kind, moment) = e.moment().expect("only called during a move");
        let m: MoveParams = *e.params().of(kind);
        let mut at = self.scene.transform(e.id).position;
        match moment.phase {
            AttackPhase::Startup => {
                if hdist(at, player.position) > 0.0 {
                    let e = &mut self.enemies[i];
                    e.facing = turn(e.facing, bearing(at, player.position), m.track);
                }
            }
            AttackPhase::Active => {
                if kind == MoveKind::Lunge {
                    let f = e.facing;
                    let forward = Vec3::new(-libm::sinf(f), 0.0, -libm::cosf(f));
                    at = self.enemy_resolve(
                        i,
                        at + forward * e.params().lunge_step,
                        player.position,
                    );
                    self.scene.transform_mut(e.id).position = at;
                }
                let e = &self.enemies[i];
                let k = moment.active.expect("an active tick has its number");
                let (lo, hi) = e.slice(kind, k, moment.reversed);
                let cut = moment.hit.max(1);
                let sweep = Sweep {
                    p: at,
                    facing: e.facing + Enemy::center(kind),
                    reach: m.reach,
                    lo,
                    hi,
                    low: e.params().low,
                    high: e.params().high,
                };
                if e.struck != cut
                    && !self.health.is_dead()
                    && sweep.touches(player.position, self.radius, self.height)
                {
                    let by = e.path.clone();
                    if player.invulnerable {
                        self.events.push(Event::Dodged {
                            by,
                            action: player.action,
                            action_tick: player.action_tick,
                        });
                    } else {
                        self.enemies[i].struck = cut;
                        self.player_hit(player, at, by, m.damage, m.impact, kind.into());
                    }
                }
            }
            AttackPhase::Recovery => {}
        }
        if moment.last {
            let e = &mut self.enemies[i];
            e.current = None;
            e.move_tick = 0;
            e.struck = 0;
            if e.params().rest_ticks > 0 {
                e.state = EnemyCondition::Rest;
                e.rest = e.params().rest_ticks;
            } else {
                e.state = EnemyCondition::Idle;
            }
        }
    }

    /// A tick of enemy `i`'s roar on entering phase 2: turning to the
    /// character, the blast all around on its tick, then idle after the
    /// last one.
    fn enemy_roar(&mut self, i: usize, player: &mut PlayerState) {
        let at = self.scene.transform(self.enemies[i].id).position;
        let e = &mut self.enemies[i];
        e.move_tick += 1;
        if hdist(at, player.position) > 0.0 {
            e.facing = turn(e.facing, bearing(at, player.position), e.params().turn);
        }
        let (p, phase) = e.roar().expect("roaring");
        if phase == AttackPhase::Active {
            let sweep = Sweep {
                p: at,
                facing: e.facing,
                reach: p.reach,
                lo: -PI,
                hi: PI,
                low: e.params().low,
                high: e.params().high,
            };
            if !self.health.is_dead() && sweep.touches(player.position, self.radius, self.height) {
                let by = e.path.clone();
                if player.invulnerable {
                    self.events.push(Event::Dodged {
                        by,
                        action: player.action,
                        action_tick: player.action_tick,
                    });
                } else {
                    self.player_hit(player, at, by, p.damage, p.impact, HitAttack::Roar);
                }
            }
        }
        let e = &mut self.enemies[i];
        if e.move_tick == p.roar_ticks {
            e.state = EnemyCondition::Idle;
            e.move_tick = 0;
        }
    }

    /// Enemy `i`'s feet moved to `p`, pushed out of the colliders (its
    /// hurtbox as the capsule) and of the character's circle.
    fn enemy_resolve(&self, i: usize, p: Vec3, player: Vec3) -> Vec3 {
        let e = &self.enemies[i];
        let circle = (Vec2::new(player.x, player.z), self.radius + e.radius);
        resolve(&self.boxes, e.radius, e.height, p, &[circle])
    }

    /// The circles the character is pushed out of: every living enemy's,
    /// grown by the character's radius.
    pub(super) fn enemy_circles(&self) -> Vec<(Vec2, f32)> {
        self.enemies
            .iter()
            .filter(|e| {
                !self.hurtboxes[e.hurtbox]
                    .health
                    .is_some_and(|health| health.is_dead())
            })
            .map(|e| {
                let at = self.scene.transform(e.id).position;
                (Vec2::new(at.x, at.z), self.radius + e.radius)
            })
            .collect()
    }
}
