//! Which clip an enemy's body model plays, picked each tick from the state
//! at its end (`docs/architecture.md` section 11, "Boss 的模型和动画").
//! Only for an enemy with a model on its child named `body`; a capsule
//! plays nothing.

use crate::enemy::{EnemyCondition, EnemyParams, MoveKind};
use crate::health::{Condition, Health};
use crate::moves::AttackPhase;

use super::Game;
use super::clips::{Choice, DEATH_BLEND, STAGGER_BLEND, STARTUP_BLEND, rate, show, stretch};
use super::props::NODE_PROPS;
use super::roles::Enemy;

/// Meters per second the `walk` clip is written for.
pub(super) const WALK_CLIP_SPEED: f32 = 2.5;

/// The clips an enemy's body animation must have.
pub(super) const ENEMY_CLIPS: [&str; 12] = [
    "idle",
    "walk",
    "sweep",
    "cleave",
    "lunge",
    "combo",
    "combo_even",
    "combo_odd",
    "spin",
    "roar",
    "stagger",
    "death",
];

/// The default of the node prop `name`, a whole number of ticks.
fn default_ticks(name: &str) -> u32 {
    NODE_PROPS
        .iter()
        .find(|spec| spec.name == name)
        .and_then(|spec| spec.default)
        .expect("only props with a default are asked for") as u32
}

/// The ticks of startup, active and recovery the move clip `clip` is
/// written for: the props' defaults. `combo_even` and `combo_odd` start
/// with the wait before their cut, `combo_gap_ticks`.
fn written(clip: &str) -> [u32; 3] {
    let m = match clip {
        "combo_even" | "combo_odd" => "combo",
        other => other,
    };
    let startup = if m == clip {
        default_ticks(&format!("{m}_startup_ticks"))
    } else {
        default_ticks("combo_gap_ticks")
    };
    [
        startup,
        default_ticks(&format!("{m}_active_ticks")),
        default_ticks(&format!("{m}_recovery_ticks")),
    ]
}

/// The clip tick `t` of the move `kind` plays, the part of it (0 startup,
/// 1 active, 2 recovery) and how long that part actually is: a combo's
/// first cut plays `combo`, its even cuts `combo_even` and its odd ones
/// after the first `combo_odd`, each from the wait before it to its
/// recovery, which only the last cut reaches.
fn piece(p: &EnemyParams, kind: MoveKind, t: u32) -> (&'static str, usize, u32) {
    let moment = p.moment(kind, t);
    let clip = match (kind, moment.hit) {
        (MoveKind::Combo, 1) => "combo",
        (MoveKind::Combo, n) if n % 2 == 0 => "combo_even",
        (MoveKind::Combo, _) => "combo_odd",
        _ => kind.name(),
    };
    let m = p.of(kind);
    let (part, actual) = match moment.phase {
        AttackPhase::Startup if kind == MoveKind::Combo && moment.hit >= 2 => (0, p.combo_gap),
        AttackPhase::Startup => (0, m.startup),
        AttackPhase::Active => (1, m.active),
        AttackPhase::Recovery => (2, m.recovery),
    };
    (clip, part, actual)
}

/// The clip for the state at the end of a tick: the enemy `e` and its
/// `health` (section 11, "每个 tick 选片段", the enemy's list).
pub(super) fn choose(e: &Enemy, health: &Health) -> Choice {
    let steady = |clip, rate| Choice {
        clip,
        rate,
        restart: None,
    };
    // As the character's: a full countdown at the end of a tick is that
    // tick's death or stagger.
    let now = &health.now;
    match now.state {
        Condition::Dead => {
            return Choice {
                clip: "death",
                rate: 100,
                restart: (now.state_ticks == health.revive_ticks).then_some((0, DEATH_BLEND)),
            };
        }
        Condition::Stagger => {
            return Choice {
                clip: "stagger",
                rate: stretch(default_ticks("stagger_ticks"), health.stagger_ticks),
                restart: (now.state_ticks == health.stagger_ticks).then_some((0, STAGGER_BLEND)),
            };
        }
        Condition::Ok => {}
    }
    let p = e.params();
    match (e.state, e.current) {
        (EnemyCondition::Attack, Some(kind)) => {
            let t = e.move_tick;
            let (clip, part, actual) = piece(p, kind, t);
            let written = written(clip);
            // Each part starts over on its first tick, where the clip or the
            // part changes.
            let first = t == 1 || {
                let (before, before_part, _) = piece(p, kind, t - 1);
                (before, before_part) != (clip, part)
            };
            let offset = u64::from(written[..part].iter().sum::<u32>()) * 100;
            let blend = if t == 1 { STARTUP_BLEND } else { 0 };
            Choice {
                clip,
                rate: stretch(written[part], actual),
                restart: first.then_some((offset, blend)),
            }
        }
        // Two parts, each stretched on its own: up to the blast, then
        // after it.
        (EnemyCondition::Roar, _) => {
            let roar = e.phase2.expect("only an enemy with a phase 2 roars");
            let t = e.move_tick;
            // The clip is written for the defaults.
            let (blast, length) = (
                default_ticks("roar_blast_tick"),
                default_ticks("roar_ticks"),
            );
            let (offset, written, actual) = if t <= roar.blast_tick {
                (0, blast, roar.blast_tick)
            } else {
                (blast, length - blast, roar.roar_ticks - roar.blast_tick)
            };
            let first = t == 1 || t == roar.blast_tick + 1;
            let blend = if t == 1 { STARTUP_BLEND } else { 0 };
            Choice {
                clip: "roar",
                rate: stretch(written, actual),
                restart: first.then_some((u64::from(offset) * 100, blend)),
            }
        }
        (EnemyCondition::Chase, _) => steady("walk", rate(100.0 * p.step * 60.0 / WALK_CLIP_SPEED)),
        // Standing, turning on the spot, resting.
        _ => steady("idle", 100),
    }
}

impl Game {
    /// Step 6 for every enemy with a body model: plays the clip [`choose`]
    /// picks, calling the scene only when something changes, like the
    /// character's.
    pub(super) fn show_enemy_clips(&mut self) {
        let tick = self.clock.tick();
        for e in &self.enemies {
            let Some(body) = e.body else {
                continue;
            };
            let health = self.hurtboxes[e.hurtbox]
                .health
                .as_ref()
                .expect("an enemy has hp: checked at load");
            show(&mut self.scene, body.id, &choose(e, health), tick)
                .expect("every clip in ENEMY_CLIPS was checked at load");
        }
    }
}
