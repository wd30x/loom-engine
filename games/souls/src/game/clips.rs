//! Which clip the character's model plays, picked each tick from the
//! state at its end (`docs/architecture.md` section 11, "模型和动画"). Only
//! for a character drawn by a model; a capsule mesh plays nothing.

use glam::{Vec2, Vec3};
use loom_scene::{Play, Scene};

use crate::GameError;
use crate::health::{Condition, Health};
use crate::moves::{Action, Attack, AttackPhase, Params, default_of};

use super::Game;
use super::state::PlayerState;

/// Meters per second the `run`, `run_left`, `run_right` and `run_back`
/// clips are written for.
pub(super) const RUN_CLIP_SPEED: f32 = 4.0;
/// Meters per second the `sprint` clip is written for.
pub(super) const SPRINT_CLIP_SPEED: f32 = 6.0;
/// Ticks the blend into a roll or backstep takes.
const ROLL_BLEND: u64 = 2;
/// Ticks the blend into an attack's startup takes.
const STARTUP_BLEND: u64 = 3;
/// Ticks the blend into a stagger takes.
const STAGGER_BLEND: u64 = 2;
/// Ticks the blend into death takes.
const DEATH_BLEND: u64 = 3;
/// Ticks the blend into a drink takes.
const DRINK_BLEND: u64 = 4;
/// Ticks every other blend takes.
const BLEND: u64 = 6;

/// The clips a character's animation must have.
pub(super) const CLIPS: [&str; 16] = [
    "idle",
    "run",
    "run_left",
    "run_right",
    "run_back",
    "sprint",
    "roll",
    "backstep",
    "attack1",
    "attack2",
    "attack3",
    "heavy",
    "stagger",
    "death",
    "drink",
    "sit",
];

/// The action lengths the clips are written for: the props' defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ClipLengths {
    roll: u32,
    backstep: u32,
    /// Startup, active and recovery ticks.
    attack: [u32; 3],
    heavy: [u32; 3],
    stagger: u32,
    /// Ticks up to and including the heal, and after it.
    drink: [u32; 2],
}

impl ClipLengths {
    fn new() -> Self {
        let ticks = |prop: &str| default_of(prop) as u32;
        let phases = |action: &str| {
            ["startup", "active", "recovery"].map(|p| ticks(&format!("{action}_{p}_ticks")))
        };
        let heal = ticks("flask_heal_tick");
        Self {
            roll: ticks("roll_ticks"),
            backstep: ticks("backstep_ticks"),
            attack: phases("attack"),
            heavy: phases("heavy"),
            stagger: ticks("stagger_ticks"),
            drink: [heal, ticks("flask_ticks") - heal],
        }
    }
}

/// What drives the character's clips: the lengths they are written for
/// and the run speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Clips {
    lengths: ClipLengths,
    /// Meters per second at full input.
    run_speed: f32,
}

impl Clips {
    /// Checks that the animation of the character `player` has every clip
    /// in [`CLIPS`].
    pub fn new(scene: &Scene, player: loom_scene::NodeId) -> Result<Self, GameError> {
        let node = scene.node(player);
        let animation = node
            .model
            .as_ref()
            .and_then(|m| m.animation.as_ref())
            .expect("the character's form was checked before");
        if let Some(clip) = CLIPS.iter().find(|c| animation.clip_path(c).is_none()) {
            return Err(GameError::MissingClip {
                character: scene.path(player),
                clip: (*clip).to_owned(),
            });
        }
        let character = node
            .character
            .expect("Scene::character only returns nodes with a character");
        Ok(Self {
            lengths: ClipLengths::new(),
            run_speed: character.run_speed,
        })
    }
}

/// The clip picked for a tick, `rate` percent fast; `restart` when it
/// starts over this tick at clip time `offset` (ctick), blended over
/// `blend` ticks.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Choice {
    pub clip: &'static str,
    pub rate: u32,
    pub restart: Option<(u64, u64)>,
}

/// `round(x)` as a rate, at least 1 so that a clip never stops.
fn rate(x: f32) -> u32 {
    // `as` saturates; a NaN gives 0.
    (libm::roundf(x) as u32).max(1)
}

/// `100 × written / actual` ticks, rounded.
fn stretch(written: u32, actual: u32) -> u32 {
    rate(100.0 * written as f32 / actual as f32)
}

/// The clip for the state at the end of a tick: `player`, its `health`,
/// and the tick's movement input `v` (at most 1 long) and direction `dir`
/// (section 11, "每个 tick 选片段").
pub(super) fn choose(
    clips: &Clips,
    params: &Params,
    player: &PlayerState,
    health: &Health,
    v: Vec2,
    dir: Vec3,
) -> Choice {
    let lengths = &clips.lengths;
    let steady = |clip, rate| Choice {
        clip,
        rate,
        restart: None,
    };
    // A death or a stagger sets its countdown full on the tick it happens
    // (the countdown runs at the start of a tick, the hits later), so a
    // full countdown at the end of the tick is that tick, a stagger over a
    // stagger included.
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
                rate: stretch(lengths.stagger, health.stagger_ticks),
                restart: (now.state_ticks == health.stagger_ticks).then_some((0, STAGGER_BLEND)),
            };
        }
        Condition::Ok => {}
    }
    match player.action {
        Action::Drink => {
            // Two parts like an attack's: lifting up to the heal, then
            // drinking and lowering.
            let [lift, lower] = lengths.drink;
            let heal = params.flask_heal_tick;
            let (rate, restart) = if player.action_tick <= heal {
                (
                    stretch(lift, heal),
                    (player.action_tick == 1).then_some((0, DRINK_BLEND)),
                )
            } else {
                (
                    stretch(lower, params.flask_ticks - heal),
                    (player.action_tick == heal + 1).then_some((u64::from(lift) * 100, 0)),
                )
            };
            Choice {
                clip: "drink",
                rate,
                restart,
            }
        }
        // Played from the tick it sits down: the clip changes then.
        Action::Sit => steady("sit", 100),
        Action::Roll | Action::Backstep => {
            let (clip, written, actual) = if player.action == Action::Roll {
                ("roll", lengths.roll, params.roll.ticks)
            } else {
                ("backstep", lengths.backstep, params.backstep.ticks)
            };
            Choice {
                clip,
                rate: stretch(written, actual),
                restart: (player.action_tick == 1).then_some((0, ROLL_BLEND)),
            }
        }
        Action::Attack | Action::Heavy => {
            let (clip, written) = if player.action == Action::Heavy {
                ("heavy", lengths.heavy)
            } else {
                (
                    ["attack1", "attack2", "attack3"][player.combo.clamp(1, 3) as usize - 1],
                    lengths.attack,
                )
            };
            let a = params.attack_of(player.action);
            let (segment, first) = segment(&a, player.action_tick);
            let actual = [a.startup, a.active, a.recovery][segment];
            let offset = u64::from(written[..segment].iter().sum::<u32>()) * 100;
            let blend = if segment == 0 { STARTUP_BLEND } else { 0 };
            Choice {
                clip,
                rate: stretch(written[segment], actual),
                restart: (player.action_tick == first).then_some((offset, blend)),
            }
        }
        Action::Sprint => steady(
            "sprint",
            rate(100.0 * params.sprint_speed / SPRINT_CLIP_SPEED),
        ),
        Action::Idle if v.length_squared() > 0.0 => {
            let f = player.facing;
            let forward = Vec3::new(-libm::sinf(f), 0.0, -libm::cosf(f));
            let right = Vec3::new(libm::cosf(f), 0.0, -libm::sinf(f));
            let (a, b) = (dir.dot(forward), dir.dot(right));
            let clip = if a >= b.abs() {
                "run"
            } else if -a > b.abs() {
                "run_back"
            } else if b > 0.0 {
                "run_right"
            } else {
                "run_left"
            };
            steady(
                clip,
                rate(100.0 * v.length() * clips.run_speed / RUN_CLIP_SPEED),
            )
        }
        Action::Idle => steady("idle", 100),
    }
}

/// Which part of attack `a` its tick `tick` is in (0 startup, 1 active, 2
/// recovery), and that part's first tick.
fn segment(a: &Attack, tick: u32) -> (usize, u32) {
    let phase = if tick <= a.startup {
        AttackPhase::Startup
    } else if tick <= a.startup + a.active {
        AttackPhase::Active
    } else {
        AttackPhase::Recovery
    };
    match phase {
        AttackPhase::Startup => (0, 1),
        AttackPhase::Active => (1, a.startup + 1),
        AttackPhase::Recovery => (2, a.startup + a.active + 1),
    }
}

impl Game {
    /// Step 6 for a character drawn by a model: plays the clip [`choose`]
    /// picks, calling the scene only when something changes (section 11,
    /// "什么时候调接口").
    pub(super) fn show_clip(&mut self) {
        let Some(clips) = &self.clips else {
            return;
        };
        let choice = choose(
            clips,
            &self.params,
            &self.state.player,
            &self.health,
            self.move_input.0,
            self.move_input.1,
        );
        let tick = self.clock.tick();
        let animation = self
            .scene
            .node(self.player)
            .model
            .as_ref()
            .and_then(|m| m.animation.as_ref())
            .expect("a character with clips has an animation");
        let (same_clip, same_rate) = (
            animation.playing == choice.clip,
            animation.rate == choice.rate,
        );
        let play = |offset, blend| {
            Play::new(choice.clip, tick)
                .offset(offset)
                .blend(blend)
                .rate(choice.rate)
        };
        let result = if let Some((offset, blend)) = choice.restart {
            self.scene.play_clip(self.player, play(offset, blend))
        } else if !same_clip {
            self.scene.play_clip(self.player, play(0, BLEND))
        } else if !same_rate {
            self.scene.set_clip_rate(self.player, tick, choice.rate)
        } else {
            Ok(())
        };
        result.expect("every clip in CLIPS was checked at load");
    }
}
