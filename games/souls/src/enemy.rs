//! Enemies: nodes with `enemy_speed` that walk up to the character, turn
//! to it and pick moves at random by distance (`docs/architecture.md`
//! section 11, "敌人和 Boss"). This module holds what does not need the
//! rest of the game: the moves, their timeline, the random numbers and
//! the state dump; `game/enemy_ai.rs` runs them.

use schemars::JsonSchema;
use serde::Serialize;

use crate::moves::AttackPhase;

/// One of the moves every enemy has; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MoveKind {
    /// A wide horizontal swing.
    Sweep,
    /// A narrow, far-reaching swing.
    Cleave,
    /// A charge forward, swinging on the way.
    Lunge,
    /// Several cuts in a row, every other one left to right.
    Combo,
    /// A full turn on the spot, a 360° sweep; only in phase 2.
    Spin,
}

impl MoveKind {
    /// In the order they are checked, chosen and written in props.
    pub(crate) const ALL: [MoveKind; 5] = [
        Self::Sweep,
        Self::Cleave,
        Self::Lunge,
        Self::Combo,
        Self::Spin,
    ];

    /// The prefix of its props, as in `sweep_damage`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sweep => "sweep",
            Self::Cleave => "cleave",
            Self::Lunge => "lunge",
            Self::Combo => "combo",
            Self::Spin => "spin",
        }
    }
}

/// What an enemy chose: one of its moves, or walking up to the character
/// for a while; serialized in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EnemyChoice {
    Sweep,
    Cleave,
    Lunge,
    Combo,
    Spin,
    /// Walking up to the character for at most `approach_ticks`.
    Approach,
}

impl From<MoveKind> for EnemyChoice {
    fn from(kind: MoveKind) -> Self {
        match kind {
            MoveKind::Sweep => Self::Sweep,
            MoveKind::Cleave => Self::Cleave,
            MoveKind::Lunge => Self::Lunge,
            MoveKind::Combo => Self::Combo,
            MoveKind::Spin => Self::Spin,
        }
    }
}

/// What an enemy is doing; serialized in snake case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnemyCondition {
    /// Standing, or turning on the spot.
    Idle,
    /// Walking up to the character.
    Chase,
    /// Doing a move.
    Attack,
    /// Resting after a move.
    Rest,
    /// Staggered: its hurtbox's health says so.
    Stagger,
    /// Lying dead: its hurtbox's health says so.
    Dead,
    /// Roaring on entering phase 2: invulnerable, with a blast around it.
    Roar,
}

/// An enemy in the state dump (`game.enemies`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct EnemyState {
    /// The node's path.
    pub node: String,
    /// 1, or 2 from its health falling to `phase2_at` until it dies.
    pub stage: u8,
    pub state: EnemyCondition,
    /// The move being done; `None` outside `attack`.
    #[serde(rename = "move")]
    pub r#move: Option<MoveKind>,
    /// Which tick of the move or roar, from 1; 0 outside `attack` and
    /// `roar`.
    pub move_tick: u32,
    /// The part of the move; the wait between two cuts of a combo is
    /// `startup`. In a roar: `startup` before the blast, `active` on its
    /// tick, then `recovery`. `None` outside `attack` and `roar`.
    pub phase: Option<AttackPhase>,
    /// Which cut of a combo, from 1; 0 for the other moves and outside
    /// `attack`.
    pub hit: u32,
    /// Heading in radians around +Y, in `[-π, π)`; 0 faces -Z.
    pub facing: f32,
    /// Ticks of rest left; 0 outside `rest`.
    pub rest: u32,
    /// Ticks left of the approach it chose; 0 otherwise, also while it
    /// walks up with nothing to choose.
    pub chase: u32,
    /// The state of its random numbers.
    pub rng: u64,
}

/// SplitMix64 (Steele, Lea and Flood; Vigna's reference `splitmix64.c`):
/// the state is the seed, and each number moves it on by a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// One move of an enemy, in per-tick units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MoveParams {
    pub damage: f32,
    pub impact: f32,
    pub startup: u32,
    /// Of each cut, for the combo.
    pub active: u32,
    pub recovery: u32,
    pub reach: f32,
    /// Radians: the whole angle of the sector.
    pub arc: f32,
    /// Radians swept per active tick: `arc / active`.
    pub slice: f32,
    /// Meters: the character's horizontal distance must be within these.
    pub min_range: f32,
    pub max_range: f32,
    pub weight: u32,
    /// Radians per tick turned to the character during startup.
    pub track: f32,
}

/// Everything an enemy's props set, in per-tick units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EnemyParams {
    /// Meters per tick walked.
    pub step: f32,
    /// Radians per tick turned walking, resting and before a move.
    pub turn: f32,
    /// Radians: the most the facing may be off the character's direction
    /// for a move to start.
    pub face_angle: f32,
    pub rest_ticks: u32,
    pub aggro_range: f32,
    pub seed: u64,
    /// Meters above the feet: the height band every move sweeps.
    pub low: f32,
    pub high: f32,
    /// In [`MoveKind::ALL`]'s order.
    pub moves: [MoveParams; 5],
    /// Whether the spin is a choice: only in phase 2.
    pub spin: bool,
    /// Meters per active tick of the lunge: `lunge_distance / active`.
    pub lunge_step: f32,
    pub combo_hits: u32,
    pub combo_gap: u32,
    /// Meters: approaching is a choice only farther than this.
    pub approach_min: f32,
    /// The approach's weight per meter beyond `approach_min`.
    pub approach_per_meter: f32,
    /// The most ticks one approach walks.
    pub approach_ticks: u32,
    /// The most the approach's weight can be.
    pub approach_max: u32,
}

impl EnemyParams {
    pub fn of(&self, kind: MoveKind) -> &MoveParams {
        &self.moves[kind as usize]
    }

    /// The approach's weight at the character's horizontal distance `d`:
    /// `floor(approach_per_meter * (d - approach_min))` farther than
    /// `approach_min`, else 0; at most `approach_max`.
    pub fn approach_weight(&self, d: f32) -> u32 {
        if d > self.approach_min {
            // Not negative here; `as` saturates a huge one.
            let w = libm::floorf(self.approach_per_meter * (d - self.approach_min)) as u32;
            w.min(self.approach_max)
        } else {
            0
        }
    }

    /// Whether `kind` can be chosen at all: the spin only in phase 2.
    pub fn has(&self, kind: MoveKind) -> bool {
        kind != MoveKind::Spin || self.spin
    }

    /// These parameters in phase 2, with the time scale `s`
    /// (`phase2_speed_scale`): every startup `max(1, round(t * s))`, the
    /// combo's gap and the rest `round(t * s)`, the walk, turn and track
    /// speeds divided by `s`, `combo_hits` cuts and the spin a choice;
    /// `round(x)` is `floor(x + 0.5)`, all in `f32`.
    pub fn phase2(&self, s: f32, combo_hits: u32) -> EnemyParams {
        let round = |t: u32| libm::floorf(t as f32 * s + 0.5) as u32;
        EnemyParams {
            step: self.step / s,
            turn: self.turn / s,
            rest_ticks: round(self.rest_ticks),
            moves: self.moves.map(|m| MoveParams {
                startup: round(m.startup).max(1),
                track: m.track / s,
                ..m
            }),
            spin: true,
            combo_hits,
            combo_gap: round(self.combo_gap),
            ..*self
        }
    }

    /// How many cuts `kind` has: `combo_hits` for the combo, else 1.
    fn cuts(&self, kind: MoveKind) -> u32 {
        match kind {
            MoveKind::Combo => self.combo_hits,
            MoveKind::Sweep | MoveKind::Cleave | MoveKind::Lunge | MoveKind::Spin => 1,
        }
    }

    /// Where tick `t` (1-based, at most the move's length) of the move
    /// `kind` is: the timeline of `docs/architecture.md` section 11
    /// ("一招的时间线").
    pub fn moment(&self, kind: MoveKind, t: u32) -> Moment {
        let m = self.of(kind);
        let cuts = u64::from(self.cuts(kind));
        let (s, a, g, r) = (
            u64::from(m.startup),
            u64::from(m.active),
            u64::from(self.combo_gap),
            u64::from(m.recovery),
        );
        let t = u64::from(t);
        // Only the combo counts its cuts.
        let hit = |n: u64| match kind {
            MoveKind::Combo => n as u32,
            MoveKind::Sweep | MoveKind::Cleave | MoveKind::Lunge | MoveKind::Spin => 0,
        };
        let strikes = s + cuts * a + (cuts - 1) * g;
        let length = strikes + r;
        let (phase, n, k) = if t <= s {
            (AttackPhase::Startup, 1, None)
        } else if t > strikes {
            (AttackPhase::Recovery, cuts, None)
        } else {
            // Cut n: its gap (from the second on), then its active ticks.
            let u = t - s - 1;
            let n = u / (a + g) + 1;
            let r = u % (a + g) + 1;
            if r <= a {
                (AttackPhase::Active, n, Some(r as u32))
            } else {
                (AttackPhase::Startup, n + 1, None)
            }
        };
        Moment {
            phase,
            hit: hit(n),
            active: k,
            reversed: kind == MoveKind::Combo && n % 2 == 0 && phase != AttackPhase::Recovery,
            last: t == length,
        }
    }
}

/// One tick of a move's timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Moment {
    pub phase: AttackPhase,
    /// The cut of a combo this tick belongs to (the one being prepared
    /// during startup), from 1; 0 for the other moves.
    pub hit: u32,
    /// While active: which tick of the cut, from 1.
    pub active: Option<u32>,
    /// The cut sweeps left to right: an even cut of a combo, in its
    /// startup or active ticks.
    pub reversed: bool,
    /// The move's last tick.
    pub last: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_gives_the_reference_numbers_for_seeds_0_and_1() {
        let three = |seed| {
            let mut rng = SplitMix64(seed);
            [rng.next(), rng.next(), rng.next()]
        };
        assert_eq!(
            three(0),
            [
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F
            ]
        );
        assert_eq!(
            three(1),
            [
                0x910A_2DEC_8902_5CC1,
                0xBEEB_8DA1_658E_EC67,
                0xF893_A2EE_FB32_555E
            ]
        );
        // The state is the seed moved on once per number.
        let mut rng = SplitMix64(u64::MAX);
        rng.next();
        assert_eq!(rng.0, 0x9E37_79B9_7F4A_7C14);
    }

    fn params(startup: u32, active: u32, recovery: u32, hits: u32, gap: u32) -> EnemyParams {
        let m = MoveParams {
            damage: 1.0,
            impact: 1.0,
            startup,
            active,
            recovery,
            reach: 1.0,
            arc: 1.0,
            slice: 1.0 / active as f32,
            min_range: 0.0,
            max_range: 1.0,
            weight: 1,
            track: 0.0,
        };
        EnemyParams {
            step: 0.1,
            turn: 0.1,
            face_angle: 0.5,
            rest_ticks: 3,
            aggro_range: 10.0,
            seed: 1,
            low: 0.0,
            high: 2.0,
            moves: [m; 5],
            spin: false,
            lunge_step: 0.1,
            combo_hits: hits,
            combo_gap: gap,
            approach_min: 0.0,
            approach_per_meter: 0.0,
            approach_ticks: 1,
            approach_max: 0,
        }
    }

    #[test]
    fn phase2_parameters_round_half_ticks_up_with_a_startup_of_at_least_one() {
        // round(x) = floor(x + 0.5): 22.5 goes up to 23, 0.5 to 1, 0.25
        // down to 0, which the startup keeps at 1 but the gap and rest not.
        let mut p = params(30, 2, 7, 3, 2);
        p.rest_ticks = 3;
        let q = p.phase2(0.75, 4);
        assert_eq!(q.moves.map(|m| m.startup), [23; 5]);
        assert_eq!((q.combo_gap, q.rest_ticks), (2, 2));
        let q = params(2, 2, 2, 3, 2).phase2(0.25, 4);
        assert_eq!((q.moves[0].startup, q.combo_gap, q.rest_ticks), (1, 1, 1));
        let q = params(1, 2, 2, 3, 1).phase2(0.25, 4);
        assert_eq!((q.moves[0].startup, q.combo_gap, q.rest_ticks), (1, 0, 1));
        let mut p = params(1, 2, 2, 3, 1);
        p.rest_ticks = 1;
        let q = p.phase2(0.25, 4);
        assert_eq!((q.moves[0].startup, q.combo_gap, q.rest_ticks), (1, 0, 0));
    }

    #[test]
    fn phase2_parameters_divide_the_speeds_and_add_the_cuts_and_the_spin() {
        let mut p = params(4, 2, 7, 3, 2);
        p.moves[1].track = 0.3;
        assert!(!p.has(MoveKind::Spin) && p.has(MoveKind::Combo));
        let q = p.phase2(0.5, 4);
        assert_eq!((q.step, q.turn, q.moves[1].track), (0.2, 0.2, 0.6));
        assert_eq!(q.combo_hits, 4);
        assert!(q.has(MoveKind::Spin));
        // The rest is kept: active and recovery ticks, damage, ranges.
        assert_eq!(
            (q.moves[1].active, q.moves[1].recovery, q.moves[1].damage),
            (2, 7, 1.0)
        );
        assert_eq!((q.moves[1].max_range, q.moves[1].weight), (1.0, 1));
        assert_eq!(
            (q.aggro_range, q.lunge_step, q.face_angle),
            (10.0, 0.1, 0.5)
        );
    }

    #[test]
    fn approach_weight_is_capped_by_its_max() {
        let mut p = params(1, 1, 1, 1, 0);
        p.approach_min = 2.0;
        p.approach_per_meter = 2.0;
        p.approach_max = 4;
        // floor(2 * (d - 2)): 3 at 3.5 m, 16 at 10 m, cut to 4.
        assert_eq!(p.approach_weight(3.5), 3);
        assert_eq!(p.approach_weight(4.0), 4);
        assert_eq!(p.approach_weight(10.0), 4);
        p.approach_max = 0;
        assert_eq!(p.approach_weight(10.0), 0);
    }

    #[test]
    fn spin_is_a_single_sweep_of_its_own_timeline() {
        let p = params(2, 3, 1, 3, 2);
        assert_eq!(
            timeline(&p, MoveKind::Spin, 6),
            ["S/0", "S/0", "A1/0", "A2/0", "A3/0", "R/0!"]
        );
    }

    /// `(phase, hit, active, reversed, last)` for every tick of a move.
    fn timeline(p: &EnemyParams, kind: MoveKind, ticks: u32) -> Vec<String> {
        (1..=ticks)
            .map(|t| {
                let m = p.moment(kind, t);
                let phase = match m.phase {
                    AttackPhase::Startup => "S",
                    AttackPhase::Active => "A",
                    AttackPhase::Recovery => "R",
                };
                let k = m.active.map_or(String::new(), |k| k.to_string());
                let rev = if m.reversed { "<" } else { "" };
                let last = if m.last { "!" } else { "" };
                format!("{phase}{k}/{}{rev}{last}", m.hit)
            })
            .collect()
    }

    #[test]
    fn single_moves_run_startup_active_recovery_with_no_cut_number() {
        let p = params(2, 3, 2, 3, 4);
        assert_eq!(
            timeline(&p, MoveKind::Sweep, 7),
            ["S/0", "S/0", "A1/0", "A2/0", "A3/0", "R/0", "R/0!"]
        );
        assert_eq!(timeline(&p, MoveKind::Lunge, 7)[6], "R/0!");
    }

    #[test]
    fn combo_cuts_are_split_by_gaps_and_every_even_one_is_reversed() {
        // Startup 2, 3 cuts of 2 active ticks, gaps of 1, recovery 2:
        // 2 + 3 * 2 + 2 * 1 + 2 = 12 ticks.
        let p = params(2, 2, 2, 3, 1);
        assert_eq!(
            timeline(&p, MoveKind::Combo, 12),
            [
                "S/1", "S/1", "A1/1", "A2/1", "S/2<", "A1/2<", "A2/2<", "S/3", "A1/3", "A2/3",
                "R/3", "R/3!"
            ]
        );
    }

    #[test]
    fn combo_without_gaps_runs_its_cuts_back_to_back() {
        let p = params(1, 1, 1, 2, 0);
        assert_eq!(
            timeline(&p, MoveKind::Combo, 4),
            ["S/1", "A1/1", "A1/2<", "R/2!"]
        );
        // One cut: like the other moves, but counted.
        let p = params(1, 1, 1, 1, 5);
        assert_eq!(timeline(&p, MoveKind::Combo, 3), ["S/1", "A1/1", "R/1!"]);
    }
}
