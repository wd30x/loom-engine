//! The props a node may have, as the tables that check them
//! (`docs/architecture.md` section 11), and the indices `roles` reads
//! groups of them by.

use loom_core::props::{PropRule, PropSpec};

/// The eleven props of the move whose names start with `$m`, with its
/// defaults in the order damage, impact, startup, active, recovery,
/// reach, arc, min_range, max_range, weight, track_speed.
macro_rules! move_props {
    ($m:literal, $what:literal, [$damage:expr, $impact:expr, $startup:expr, $active:expr,
        $recovery:expr, $reach:expr, $arc:expr, $min:expr, $max:expr, $weight:expr,
        $track:expr]) => {
        [
            PropSpec {
                name: concat!($m, "damage"),
                default: Some($damage),
                rule: PropRule::Positive,
                unit: "hp",
                description: concat!($what, ": health each hit takes off the character."),
            },
            PropSpec {
                name: concat!($m, "impact"),
                default: Some($impact),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "impact",
                description: concat!($what, ": what each hit adds up against the character's poise."),
            },
            PropSpec {
                name: concat!($m, "startup_ticks"),
                default: Some($startup),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": the warning before the swing, turning to the character at track_speed."),
            },
            PropSpec {
                name: concat!($m, "active_ticks"),
                default: Some($active),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": the swing (for a combo, each cut's)."),
            },
            PropSpec {
                name: concat!($m, "recovery_ticks"),
                default: Some($recovery),
                rule: PropRule::Whole { min: 1, max: None },
                unit: "ticks",
                description: concat!($what, ": after the swing (for a combo, the last cut's)."),
            },
            PropSpec {
                name: concat!($m, "reach"),
                default: Some($reach),
                rule: PropRule::Positive,
                unit: "meters",
                description: concat!($what, ": radius of the swept sector."),
            },
            PropSpec {
                name: concat!($m, "arc"),
                default: Some($arc),
                rule: PropRule::Arc,
                unit: "degrees",
                description: concat!($what, ": angle swept, right to left, centered on the facing."),
            },
            PropSpec {
                name: concat!($m, "min_range"),
                default: Some($min),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "meters",
                description: concat!($what, ": chosen only with the character at least this far, horizontally."),
            },
            PropSpec {
                name: concat!($m, "max_range"),
                default: Some($max),
                rule: PropRule::Positive,
                unit: "meters",
                description: concat!($what, ": chosen only with the character at most this far; at least min_range."),
            },
            PropSpec {
                name: concat!($m, "weight"),
                default: Some($weight),
                rule: PropRule::Whole { min: 0, max: None },
                unit: "weight",
                description: concat!($what, ": how likely it is chosen, against the weights of the other moves in range; 0: never."),
            },
            PropSpec {
                name: concat!($m, "track_speed"),
                default: Some($track),
                rule: PropRule::AtLeast { min: 0.0 },
                unit: "degrees per second",
                description: concat!($what, ": how fast the enemy turns to the character during the startup."),
            },
        ]
    };
}

/// The props a node other than the character may have, in the order they
/// are checked, with the default of those that have one:
/// - `lock_height` makes it a lock-on target (locked on this many meters
///   above its position);
/// - `hurt_radius` and `hurt_height` together make it a hurtbox;
/// - `hp` makes a hurtbox take damage; the four after it go with `hp`;
/// - `swing_damage` makes it a swinger, hitting the character; the eight
///   after it go with `swing_damage`;
/// - `enemy_speed` makes it an enemy; all the others go with it: the rest
///   of [`ENEMY_PROPS`], eleven for each move, then [`ENEMY_TAIL`],
///   [`APPROACH_PROPS`] and [`PHASE2_PROPS`] (all but `phase2_at` going
///   with it too), then the spin's eleven;
/// - `bonfire_range` makes it a bonfire;
/// - `souls` goes with `hp`: what killing it gives;
/// - `boss_bar` goes with `enemy_speed`: the enemy's bar on the HUD;
/// - `enemy_dead_lift` goes with `enemy_speed`: its body model's lift while
///   lying dead.
pub(crate) const NODE_PROPS: [PropSpec; 99] = join(&[
    &BASE_PROPS,
    &ENEMY_PROPS,
    &move_props!(
        "sweep_",
        "Sweep, a wide swing",
        [25.0, 20.0, 30.0, 8.0, 30.0, 3.0, 160.0, 0.0, 3.0, 3.0, 90.0]
    ),
    &move_props!(
        "cleave_",
        "Cleave, a narrow far-reaching swing",
        [45.0, 40.0, 45.0, 4.0, 40.0, 4.0, 20.0, 0.0, 4.0, 2.0, 150.0]
    ),
    &move_props!(
        "lunge_",
        "Lunge, a charge forward",
        [
            30.0, 30.0, 35.0, 10.0, 35.0, 2.5, 90.0, 4.0, 9.0, 2.0, 180.0
        ]
    ),
    &move_props!(
        "combo_",
        "Combo, cuts in a row",
        [20.0, 15.0, 25.0, 5.0, 45.0, 2.8, 140.0, 0.0, 2.8, 2.0, 90.0]
    ),
    &ENEMY_TAIL,
    &APPROACH_PROPS,
    &PHASE2_PROPS,
    &move_props!(
        "spin_",
        "Spin, a full turn on the spot, only in phase 2",
        [40.0, 40.0, 50.0, 12.0, 40.0, 3.5, 360.0, 0.0, 3.5, 2.0, 0.0]
    ),
    &[PropSpec {
        name: "bonfire_range",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "Makes the node a bonfire: the character sits down at it within this \
                      horizontal distance.",
    }],
    &[PropSpec {
        name: "souls",
        default: Some(0.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "souls",
        description: "What the character gets each time it kills the node.",
    }],
    &[PropSpec {
        name: "boss_bar",
        default: Some(0.0),
        rule: PropRule::Whole {
            min: 0,
            max: Some(1),
        },
        unit: "flag",
        description: "1: the HUD shows the enemy's health at the bottom while the character is \
                      within its aggro range.",
    }],
    &[PropSpec {
        name: "enemy_dead_lift",
        default: Some(0.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "With a model on its child named body: how far the body is lifted while \
                      lying dead, about half its thickness, so that it does not sink into the \
                      ground.",
    }],
]);

/// `parts` one after the other, which must be `N` in all.
const fn join<const N: usize>(parts: &[&[PropSpec]]) -> [PropSpec; N] {
    let mut out = [BASE_PROPS[0]; N];
    let mut i = 0;
    let mut p = 0;
    while p < parts.len() {
        let mut j = 0;
        while j < parts[p].len() {
            out[i] = parts[p][j];
            i += 1;
            j += 1;
        }
        p += 1;
    }
    assert!(i == N, "the parts add up to N");
    out
}

/// The props a node could always have: targets, hurtboxes, health and
/// swingers.
const BASE_PROPS: [PropSpec; 17] = [
    PropSpec {
        name: "lock_height",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "Makes the node a lock-on target, locked on this far above its position.",
    },
    PropSpec {
        name: "hurt_radius",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "With hurt_height, makes the node a hurtbox: a vertical cylinder of this radius around its position.",
    },
    PropSpec {
        name: "hurt_height",
        default: None,
        rule: PropRule::Positive,
        unit: "meters",
        description: "With hurt_radius: the height of the cylinder, from the node's position up.",
    },
    PropSpec {
        name: "hp",
        default: None,
        rule: PropRule::Positive,
        unit: "hp",
        description: "Gives a hurtbox health: it takes damage, staggers and dies.",
    },
    PropSpec {
        name: "poise",
        default: Some(0.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "Impacts added up past this stagger it.",
    },
    PropSpec {
        name: "stagger_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long a stagger lasts.",
    },
    PropSpec {
        name: "poise_reset_ticks",
        default: Some(60.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks without a hit before impacts are forgotten.",
    },
    PropSpec {
        name: "revive_ticks",
        default: Some(180.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "Ticks lying dead before coming back with full health.",
    },
    PropSpec {
        name: "swing_damage",
        default: None,
        rule: PropRule::Positive,
        unit: "hp",
        description: "Makes the node a swinger, swinging at the character: health a hit takes off it.",
    },
    PropSpec {
        name: "swing_impact",
        default: Some(20.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What a hit adds up against the character's poise.",
    },
    PropSpec {
        name: "swing_startup_ticks",
        default: Some(40.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The warning before the swing.",
    },
    PropSpec {
        name: "swing_active_ticks",
        default: Some(6.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The swing.",
    },
    PropSpec {
        name: "swing_recovery_ticks",
        default: Some(74.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "After the swing, until the next round.",
    },
    PropSpec {
        name: "swing_reach",
        default: Some(2.2),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Radius of the swept sector.",
    },
    PropSpec {
        name: "swing_arc",
        default: Some(150.0),
        rule: PropRule::Arc,
        unit: "degrees",
        description: "Angle swept, right to left, centered on -Z.",
    },
    PropSpec {
        name: "swing_low",
        default: Some(0.3),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Above the character's feet: the bottom of the band the swing sweeps.",
    },
    PropSpec {
        name: "swing_high",
        default: Some(1.6),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Above the character's feet: the top of the band the swing sweeps.",
    },
];

/// An enemy's own props, `enemy_speed` first.
const ENEMY_PROPS: [PropSpec; 8] = [
    PropSpec {
        name: "enemy_speed",
        default: None,
        rule: PropRule::Positive,
        unit: "meters per second",
        description: "Makes the node an enemy: how fast it walks up to the character. Per tick, \
                      less than hurt_radius.",
    },
    PropSpec {
        name: "enemy_turn_speed",
        default: Some(180.0),
        rule: PropRule::Positive,
        unit: "degrees per second",
        description: "How fast it turns to the character walking, resting and before a move.",
    },
    PropSpec {
        name: "enemy_face_angle",
        default: Some(30.0),
        rule: PropRule::PositiveAtMost { max: 180.0 },
        unit: "degrees",
        description: "A move starts only with the character at most this far off its facing.",
    },
    PropSpec {
        name: "enemy_rest_ticks",
        default: Some(40.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "The rest after each move: the character's window to hit back.",
    },
    PropSpec {
        name: "enemy_aggro_range",
        default: Some(15.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "With the character farther than this, horizontally, it stands still.",
    },
    PropSpec {
        name: "enemy_seed",
        default: Some(1.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "seed",
        description: "The seed of its random numbers, which choose its moves.",
    },
    PropSpec {
        name: "enemy_swing_low",
        default: Some(0.3),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Above its feet: the bottom of the band every move sweeps.",
    },
    PropSpec {
        name: "enemy_swing_high",
        default: Some(2.4),
        rule: PropRule::Positive,
        unit: "meters",
        description: "Above its feet: the top of the band every move sweeps; above \
                      enemy_swing_low.",
    },
];

/// The enemy props after the moves'.
const ENEMY_TAIL: [PropSpec; 3] = [
    PropSpec {
        name: "lunge_distance",
        default: Some(5.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "How far the lunge charges, evenly over its active ticks; per tick, less \
                      than hurt_radius.",
    },
    PropSpec {
        name: "combo_hits",
        default: Some(3.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "cuts",
        description: "How many cuts the combo has.",
    },
    PropSpec {
        name: "combo_gap_ticks",
        default: Some(12.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "ticks",
        description: "The wait before each cut of the combo after the first, a startup again.",
    },
];

/// An enemy's approach, the choice of walking up to the character: the
/// last enemy props.
const APPROACH_PROPS: [PropSpec; 3] = [
    PropSpec {
        name: "approach_min_range",
        default: Some(2.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "meters",
        description: "Approaching is a choice only with the character farther than this, \
                      horizontally.",
    },
    PropSpec {
        name: "approach_weight_per_meter",
        default: Some(2.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "weight per meter",
        description: "The approach's weight is this times the meters beyond \
                      approach_min_range, rounded down; 0 leaves it out.",
    },
    PropSpec {
        name: "approach_ticks",
        default: Some(30.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "The most ticks one approach walks.",
    },
];

/// The approach's cap, then phase 2: `phase2_at` turns it on, and the
/// seven after it go with it.
const PHASE2_PROPS: [PropSpec; 9] = [
    PropSpec {
        name: "approach_weight_max",
        default: Some(4.0),
        rule: PropRule::Whole { min: 0, max: None },
        unit: "weight",
        description: "The most the approach's weight can be; 0 never approaches.",
    },
    PropSpec {
        name: "phase2_at",
        default: None,
        rule: PropRule::Between { min: 0.0, max: 1.0 },
        unit: "fraction of full health",
        description: "Makes a phase 2: entered, roaring, once a hit leaves at most this much of \
                      its full health.",
    },
    PropSpec {
        name: "phase2_speed_scale",
        default: Some(0.75),
        rule: PropRule::PositiveAtMost { max: 1.0 },
        unit: "time scale",
        description: "Phase 2: startups, the combo's gap and the rest times this, rounded; \
                      walking, turning and tracking divided by it.",
    },
    PropSpec {
        name: "phase2_combo_hits",
        default: Some(4.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "cuts",
        description: "Phase 2: how many cuts the combo has.",
    },
    PropSpec {
        name: "roar_ticks",
        default: Some(70.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "ticks",
        description: "How long the roar on entering phase 2 lasts, invulnerable.",
    },
    PropSpec {
        name: "roar_blast_tick",
        default: Some(20.0),
        rule: PropRule::Whole { min: 1, max: None },
        unit: "tick",
        description: "The tick of the roar with the blast all around; at most roar_ticks.",
    },
    PropSpec {
        name: "roar_damage",
        default: Some(30.0),
        rule: PropRule::Positive,
        unit: "hp",
        description: "Health the blast takes off the character.",
    },
    PropSpec {
        name: "roar_impact",
        default: Some(40.0),
        rule: PropRule::AtLeast { min: 0.0 },
        unit: "impact",
        description: "What the blast adds up against the character's poise.",
    },
    PropSpec {
        name: "roar_reach",
        default: Some(4.0),
        rule: PropRule::Positive,
        unit: "meters",
        description: "The radius of the blast.",
    },
];

/// Where the props that go with `hp`, with `swing_damage` and with
/// `enemy_speed` are in [`NODE_PROPS`].
pub(super) const HP_PROPS: std::ops::Range<usize> = 4..8;
pub(super) const SWING_PROPS: std::ops::Range<usize> = 9..17;
pub(super) const ENEMY_OTHER_PROPS: std::ops::Range<usize> = 18..95;
pub(super) const PHASE2_OTHER_PROPS: std::ops::Range<usize> = 77..84;
/// Where `souls`, which also goes with `hp`, is in [`NODE_PROPS`].
pub(super) const SOULS_PROP: usize = 96;
/// Where `boss_bar`, which also goes with `enemy_speed`, is in
/// [`NODE_PROPS`].
pub(super) const BOSS_BAR_PROP: usize = 97;
/// Where `enemy_dead_lift`, which also goes with `enemy_speed`, is in
/// [`NODE_PROPS`].
pub(super) const DEAD_LIFT_PROP: usize = 98;
