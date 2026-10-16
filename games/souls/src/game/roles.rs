//! What each scene node is to the game, read once at load time from its
//! props: lock targets, hurtboxes, swingers and their arms, enemies,
//! bonfires, the respawn point.

use std::f32::consts::PI;

use glam::{Quat, Vec3};
use loom_scene::{NodeId, Scene};

use crate::enemy::{EnemyCondition, EnemyParams, MoveKind, MoveParams, SplitMix64};
use crate::health::{Health, HpBar};
use crate::moves::AttackPhase;

use super::enemy_ai::slice_of;
use super::enemy_clips::ENEMY_CLIPS;
use super::error::GameError;
use super::props::{
    BOSS_BAR_PROP, DEAD_LIFT_PROP, ENEMY_OTHER_PROPS, HP_PROPS, NODE_PROPS, PHASE2_OTHER_PROPS,
    SOULS_PROP, SWING_PROPS,
};
use super::state::LastHit;
use super::{positive_zero, wrap};

/// The name of the child of a hurtbox with `hp` that shows its health.
const HP_BAR: &str = "hp_bar";
/// The name of the child of a swinger that shows its swing.
const ARM: &str = "arm";
/// The name of the child of an enemy whose model shows what it does.
const BODY: &str = "body";

/// The name of the root node that marks the bloodstain.
const BLOODSTAIN: &str = "bloodstain";

/// A node with a `lock_height`: something to lock on to.
#[derive(Debug, Clone)]
pub(super) struct Target {
    pub(super) id: NodeId,
    /// The path, for the state dump.
    pub(super) path: String,
    /// The lock point is this far above the node's position.
    pub(super) height: f32,
    /// The index in `Game::hurtboxes` of the same node, if it is one: a
    /// dead one cannot be locked on to.
    pub(super) hurtbox: Option<usize>,
}

/// A node with `hurt_radius` and `hurt_height`: something attacks hit. A
/// vertical cylinder standing on the node's position.
#[derive(Debug, Clone)]
pub(super) struct Hurtbox {
    pub(super) id: NodeId,
    /// The path, for the state dump.
    pub(super) path: String,
    pub(super) radius: f32,
    pub(super) height: f32,
    /// Every node with a mesh or a model in the subtree (the node itself
    /// included) but the hp bar, in pre-order, with its color in the scene
    /// file and whether it is a model: what flashes and darkens.
    pub(super) meshes: Vec<(NodeId, Vec3, bool)>,
    /// The node's rotation in the scene file: its pose standing.
    pub(super) base: Quat,
    pub(super) hits: u32,
    pub(super) last_hit: Option<LastHit>,
    /// With an `hp` prop: health, poise and death.
    pub(super) health: Option<Health>,
    /// What killing it gives the character.
    pub(super) souls: u32,
}

/// A node with `swing_damage`: it swings at the character on a fixed
/// rhythm (startup, active, recovery, over and over), sweeping a sector
/// right to left like the character's attacks, and never turns.
#[derive(Debug, Clone)]
pub(super) struct Swinger {
    pub(super) id: NodeId,
    /// The path, for the state dump and `last_hit.by`.
    pub(super) path: String,
    /// Computed once from the node's rotation, like the character's.
    pub(super) facing: f32,
    pub(super) damage: f32,
    pub(super) impact: f32,
    pub(super) startup: u32,
    active: u32,
    /// `startup + active + recovery`, saturating.
    pub(super) period: u32,
    pub(super) reach: f32,
    /// Radians: the whole angle of the sector.
    pub(super) arc: f32,
    /// Radians swept per active tick: `arc / active`.
    slice: f32,
    /// Meters above the node: the height band swept.
    pub(super) low: f32,
    pub(super) high: f32,
    /// The index in `Game::hurtboxes` of the same node, if it is one: a
    /// staggered or dead one stops swinging.
    pub(super) hurtbox: Option<usize>,
    pub(super) arm: Option<Arm>,
    /// Which tick of the round this is, from 1; 0 while staggered or dead.
    pub(super) swing_tick: u32,
    /// This round has hit the character already.
    pub(super) struck: bool,
}

/// The `arm` child of a swinger, as in the scene file: its pose on the
/// center line, and its color.
#[derive(Debug, Clone, Copy)]
pub(super) struct Arm {
    pub(super) id: NodeId,
    pub(super) p0: Vec3,
    pub(super) r0: Quat,
    pub(super) color: Vec3,
}

/// A node with `enemy_speed`: it walks up to the character, turns to it
/// and does moves chosen at random by distance (`docs/architecture.md`
/// section 11, "敌人和 Boss"). Always a hurtbox with health and a lock-on
/// target too.
#[derive(Debug, Clone)]
pub(super) struct Enemy {
    pub(super) id: NodeId,
    /// The path, for the state dump and events.
    pub(super) path: String,
    /// Its parameters in phase 1.
    base: EnemyParams,
    /// With `phase2_at`: its phase 2.
    pub(super) phase2: Option<Phase2>,
    /// 1, or 2 from entering phase 2 until it dies.
    pub(super) stage: u8,
    /// Its index in `Game::hurtboxes`.
    pub(super) hurtbox: usize,
    /// Its hurtbox's cylinder, also what it collides with.
    pub(super) radius: f32,
    pub(super) height: f32,
    pub(super) arm: Option<Arm>,
    /// Its child named `body` when that has a model: what plays its clips.
    pub(super) body: Option<Body>,
    pub(super) state: EnemyCondition,
    /// The move being done, while `Attack`.
    pub(super) current: Option<MoveKind>,
    /// Which tick of the move or roar, from 1; 0 without one.
    pub(super) move_tick: u32,
    pub(super) facing: f32,
    pub(super) rest: u32,
    /// Ticks left of a chosen approach.
    pub(super) chase: u32,
    pub(super) rng: SplitMix64,
    /// The cut of the move (1 for the moves that have one) that has hit
    /// the character already; 0 for none.
    pub(super) struck: u32,
    /// `boss_bar` 1: the HUD shows its health.
    pub(super) boss_bar: bool,
}

/// An enemy's child named `body` with a model, which plays a clip for what
/// the enemy does (`docs/architecture.md` section 11, "Boss 的模型和动画").
#[derive(Debug, Clone, Copy)]
pub(super) struct Body {
    pub(super) id: NodeId,
    /// Its position in the scene file: where it stands, but lying dead.
    pub(super) p0: Vec3,
    /// `enemy_dead_lift`: how far it is lifted while lying dead.
    pub(super) lift: f32,
}

/// A node with `bonfire_range`: the character sits down at it.
#[derive(Debug, Clone)]
pub(super) struct Bonfire {
    pub(super) id: NodeId,
    pub(super) path: String,
    pub(super) range: f32,
}

/// Where the character comes back after dying: its spawn point at load,
/// then where it last sat down at a bonfire.
#[derive(Debug, Clone, Copy)]
pub(super) struct Respawn {
    pub(super) position: Vec3,
    pub(super) facing: f32,
    /// The bonfire sat at, as an index into `Game::bonfires`.
    pub(super) bonfire: Option<usize>,
}

/// What a world reset gives back: the load-time state of the character's
/// health, the hurtboxes, swingers and enemies, and the enemies' node
/// positions and rotations.
#[derive(Debug, Clone)]
pub(super) struct Initial {
    pub(super) health: Health,
    pub(super) hurtboxes: Vec<Hurtbox>,
    pub(super) swingers: Vec<Swinger>,
    pub(super) enemies: Vec<Enemy>,
    pub(super) enemy_nodes: Vec<(Vec3, Quat)>,
}

/// An enemy's phase 2, from its props, worked out at load.
#[derive(Debug, Clone, Copy)]
pub(super) struct Phase2 {
    /// The fraction of full health that enters it.
    pub(super) at: f32,
    params: EnemyParams,
    pub(super) roar_ticks: u32,
    pub(super) blast_tick: u32,
    pub(super) damage: f32,
    pub(super) impact: f32,
    pub(super) reach: f32,
}

impl Enemy {
    /// Its parameters in its stage.
    pub(super) fn params(&self) -> &EnemyParams {
        match &self.phase2 {
            Some(p) if self.stage == 2 => &p.params,
            _ => &self.base,
        }
    }

    /// Where the move is on this tick; `None` outside a move.
    pub(super) fn moment(&self) -> Option<(MoveKind, crate::enemy::Moment)> {
        let kind = self.current?;
        Some((kind, self.params().moment(kind, self.move_tick)))
    }

    /// While roaring: its phase 2, and the part of the roar it is in
    /// (startup before the blast, active on its tick, then recovery).
    pub(super) fn roar(&self) -> Option<(Phase2, AttackPhase)> {
        if self.state != EnemyCondition::Roar {
            return None;
        }
        let p = self.phase2.expect("only an enemy with a phase 2 roars");
        let phase = match self.move_tick.cmp(&p.blast_tick) {
            std::cmp::Ordering::Less => AttackPhase::Startup,
            std::cmp::Ordering::Equal => AttackPhase::Active,
            std::cmp::Ordering::Greater => AttackPhase::Recovery,
        };
        Some((p, phase))
    }

    /// Where the move `kind`'s sector is centered, off the facing: behind
    /// for the spin, so that its first slice (at `-half`, `-π`) is in
    /// front; ahead for the others.
    pub(super) fn center(kind: MoveKind) -> f32 {
        match kind {
            MoveKind::Spin => PI,
            MoveKind::Sweep | MoveKind::Cleave | MoveKind::Lunge | MoveKind::Combo => 0.0,
        }
    }

    /// `(lo, hi)` of the move `kind`'s active tick `k`, left to right when
    /// `reversed`.
    pub(super) fn slice(&self, kind: MoveKind, k: u32, reversed: bool) -> (f32, f32) {
        let m = self.params().of(kind);
        let (lo, hi) = slice_of(m.arc, m.slice, k);
        if reversed { (-hi, -lo) } else { (lo, hi) }
    }
}

impl Swinger {
    /// The part of the round `swing_tick` is in; `None` at 0.
    pub(super) fn phase(&self) -> Option<AttackPhase> {
        match self.swing_tick {
            0 => None,
            t if t <= self.startup => Some(AttackPhase::Startup),
            t if t - self.startup <= self.active => Some(AttackPhase::Active),
            _ => Some(AttackPhase::Recovery),
        }
    }

    /// `(lo, hi)` of active tick `k` (1-based): the part of the sector it
    /// sweeps, relative to `facing`, positive to the left.
    pub(super) fn slice_of(&self, k: u32) -> (f32, f32) {
        slice_of(self.arc, self.slice, k)
    }
}

/// What the nodes other than the character are for, each list in pre-order.
pub(super) struct Roles {
    pub(super) targets: Vec<Target>,
    pub(super) hurtboxes: Vec<Hurtbox>,
    pub(super) swingers: Vec<Swinger>,
    pub(super) enemies: Vec<Enemy>,
    pub(super) bonfires: Vec<Bonfire>,
    /// The root node named `bloodstain`, with its scene file position.
    pub(super) bloodstain_marker: Option<(NodeId, Vec3)>,
}

/// Which node props only go with another one, as [`roles`] checks them
/// (`loom schema` writes them out).
pub(crate) const NODE_REQUIRES: [&str; 6] = [
    "hurt_radius and hurt_height: both or neither",
    "hp: needs hurt_radius and hurt_height",
    "poise, stagger_ticks, poise_reset_ticks, revive_ticks, souls: need hp",
    "swing_impact, swing_startup_ticks, swing_active_ticks, swing_recovery_ticks, swing_reach, \
     swing_arc, swing_low, swing_high: need swing_damage",
    "every enemy_*, sweep_*, cleave_*, lunge_*, combo_*, approach_*, phase2_*, roar_* and \
     spin_* prop but enemy_speed, and boss_bar: need enemy_speed",
    "phase2_speed_scale, phase2_combo_hits, roar_ticks, roar_blast_tick, roar_damage, \
     roar_impact, roar_reach: need phase2_at",
];

/// The other rules [`roles`] checks on a node's props.
pub(crate) const NODE_CONSTRAINTS: [&str; 14] = [
    "a swinger: swing_low < swing_high",
    "lock-on targets, hurtboxes and swingers are root nodes, and not the camera",
    "a hurtbox with hp: its child named hp_bar, if any, needs a mesh",
    "a swinger or an enemy: its child named arm, if any, needs a mesh",
    "an enemy (a node with enemy_speed): <move>_min_range <= <move>_max_range for sweep, cleave, \
     lunge, combo and spin",
    "an enemy: roar_blast_tick <= roar_ticks",
    "an enemy: enemy_swing_low < enemy_swing_high",
    "an enemy needs hurt_radius, hurt_height, hp and lock_height, and is not a swinger",
    "an enemy: enemy_speed / 60, lunge_distance / lunge_active_ticks and, with phase2_at, \
     enemy_speed / 60 / phase2_speed_scale are less than hurt_radius",
    "an enemy has no collider in its subtree",
    "an enemy has no model itself; a model on its child named body has an animation with every \
     clip an enemy plays (idle, walk, sweep, cleave, lunge, combo, combo_even, combo_odd, \
     spin, roar, stagger, death)",
    "a bonfire (a node with bonfire_range) is a root node, and not the camera, and not also a \
     lock target, hurtbox, swinger or enemy",
    "the bloodstain marker (the root node named bloodstain) has a mesh, and no props or children",
    "an enemy with boss_bar 1: its display_name has only the characters 32 to 126",
];

/// Every lock-on target, hurtbox, swinger and enemy of `scene`, in
/// pre-order, checked node by node (every node but the character), in this
/// order: no props but [`NODE_PROPS`]; each value by its rule; the two hurt
/// props together or not at all; `hp` only on a hurtbox; the four props
/// after it only with `hp`; the eight swing props only with `swing_damage`;
/// a swinger's `swing_low < swing_high`; only on root nodes other than the
/// camera; an `hp_bar` with a mesh; an `arm` with a mesh; the enemy props
/// only with `enemy_speed`; the phase 2 props only with `phase2_at`; then,
/// for an enemy, each move's ranges, the roar's blast within it, its swing
/// band, the props it needs, not a swinger, its steps (with phase 2's
/// walk last) shorter than `hurt_radius`, no collider in its subtree (a
/// lock-on target, it is a root node already), no model on itself and a
/// model on its `body` with an animation of every clip in [`ENEMY_CLIPS`];
/// last, for a bonfire, a
/// root node other than the camera, then none of the other roles; last,
/// the bloodstain marker's mesh, props and children. `souls` goes with
/// `hp` after the four props before it.
pub(super) fn roles(scene: &Scene, player: NodeId, camera: NodeId) -> Result<Roles, GameError> {
    let mut targets = Vec::new();
    let mut hurtboxes = Vec::new();
    let mut swingers = Vec::new();
    let mut enemies = Vec::new();
    let mut bonfires = Vec::new();
    let mut bloodstain_marker = None;
    for id in scene.ids().filter(|&id| id != player) {
        let node = scene.node(id);
        let path = || scene.path(id);
        if let Some((prop, _)) = node
            .props()
            .find(|&(p, _)| !NODE_PROPS.iter().any(|spec| spec.name == p))
        {
            return Err(GameError::UnknownNodeProp {
                node: path(),
                prop: prop.to_owned(),
            });
        }
        for spec in NODE_PROPS {
            if let Some(value) = node.prop(spec.name)
                && !spec.rule.allows(value)
            {
                return Err(GameError::NodeBadProp {
                    node: path(),
                    prop: spec.name,
                    rule: spec.rule,
                    value,
                });
            }
        }
        // Present, or the default; `None` only for a missing prop without one.
        let get = |name: &str| {
            let spec = NODE_PROPS
                .iter()
                .find(|spec| spec.name == name)
                .expect("only names from NODE_PROPS are asked for");
            node.prop(name).or(spec.default)
        };
        // Only for props with a default.
        let value = |name: &str| get(name).expect("only props with a default are asked for");
        // Checked above: whole numbers of at least 1; `as` saturates.
        let ticks = |name: &str| value(name) as u32;
        let lock_height = node.prop("lock_height");
        let hurt = match (node.prop("hurt_radius"), node.prop("hurt_height")) {
            (Some(radius), Some(height)) => Some((radius as f32, height as f32)),
            (None, None) => None,
            _ => return Err(GameError::HurtPropsUnpaired(path())),
        };
        let hp = node.prop("hp");
        if hp.is_some() && hurt.is_none() {
            return Err(GameError::HpWithoutHurtbox(path()));
        }
        if hp.is_none()
            && let Some(spec) = NODE_PROPS[HP_PROPS]
                .iter()
                .chain([&NODE_PROPS[SOULS_PROP]])
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsHp {
                node: path(),
                prop: spec.name,
            });
        }
        let swing_damage = node.prop("swing_damage");
        if swing_damage.is_none()
            && let Some(spec) = NODE_PROPS[SWING_PROPS]
                .iter()
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsSwingDamage {
                node: path(),
                prop: spec.name,
            });
        }
        // Compared as used: after `as f32`, like the character's band.
        if swing_damage.is_some() && value("swing_low") as f32 >= value("swing_high") as f32 {
            return Err(GameError::NodeSwingBand(path()));
        }
        if (lock_height.is_some() || hurt.is_some() || swing_damage.is_some())
            && (node.parent.is_some() || id == camera)
        {
            return Err(if lock_height.is_some() {
                GameError::LockTargetNotRoot(path())
            } else if hurt.is_some() {
                GameError::HurtboxNotRoot(path())
            } else {
                GameError::SwingerNotRoot(path())
            });
        }
        // Only a hurtbox with hp has an hp bar, and only a swinger an arm;
        // anywhere else the names mean nothing.
        let child = |name: &str| {
            node.children
                .iter()
                .copied()
                .find(|&c| scene.node(c).name == name)
        };
        let bar = hp.and_then(|_| child(HP_BAR));
        if let Some(bar) = bar
            && scene.node(bar).mesh.is_none()
        {
            return Err(GameError::HpBarWithoutMesh(path()));
        }
        let enemy_speed = node.prop("enemy_speed");
        let arm = swing_damage.or(enemy_speed).and_then(|_| child(ARM));
        if let Some(arm) = arm
            && scene.node(arm).mesh.is_none()
        {
            return Err(GameError::ArmWithoutMesh(path()));
        }
        if enemy_speed.is_none()
            && let Some(spec) = NODE_PROPS[ENEMY_OTHER_PROPS]
                .iter()
                .chain([&NODE_PROPS[BOSS_BAR_PROP], &NODE_PROPS[DEAD_LIFT_PROP]])
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsEnemySpeed {
                node: path(),
                prop: spec.name,
            });
        }
        let phase2_at = node.prop("phase2_at");
        if phase2_at.is_none()
            && let Some(spec) = NODE_PROPS[PHASE2_OTHER_PROPS]
                .iter()
                .find(|spec| node.prop(spec.name).is_some())
        {
            return Err(GameError::NeedsPhase2 {
                node: path(),
                prop: spec.name,
            });
        }
        // Checked on, in order, only for an enemy; its parameters.
        let enemy = match enemy_speed {
            None => None,
            Some(speed) => {
                let range = |kind: MoveKind, end: &str| {
                    value(&format!("{}_{end}_range", kind.name())) as f32
                };
                for kind in MoveKind::ALL {
                    if range(kind, "min") > range(kind, "max") {
                        return Err(GameError::EnemyRanges {
                            node: path(),
                            m: kind.name(),
                        });
                    }
                }
                if ticks("roar_blast_tick") > ticks("roar_ticks") {
                    return Err(GameError::EnemyRoarBlast(path()));
                }
                let (low, high) = (
                    value("enemy_swing_low") as f32,
                    value("enemy_swing_high") as f32,
                );
                if low >= high {
                    return Err(GameError::EnemySwingBand(path()));
                }
                let (Some((radius, height)), Some(_), Some(_)) = (hurt, hp, lock_height) else {
                    return Err(GameError::EnemyIncomplete(path()));
                };
                if swing_damage.is_some() {
                    return Err(GameError::EnemySwinger(path()));
                }
                let moves = MoveKind::ALL.map(|kind| {
                    let v = |suffix: &str| value(&format!("{}_{suffix}", kind.name()));
                    let active = v("active_ticks") as u32;
                    let arc = (v("arc") as f32).to_radians();
                    MoveParams {
                        damage: v("damage") as f32,
                        impact: v("impact") as f32,
                        startup: v("startup_ticks") as u32,
                        active,
                        recovery: v("recovery_ticks") as u32,
                        reach: v("reach") as f32,
                        arc,
                        slice: arc / active as f32,
                        min_range: v("min_range") as f32,
                        max_range: v("max_range") as f32,
                        weight: v("weight") as u32,
                        track: (v("track_speed") as f32).to_radians() / 60.0,
                    }
                });
                let lunge_active = moves[MoveKind::Lunge as usize].active;
                let params = EnemyParams {
                    step: speed as f32 / 60.0,
                    turn: (value("enemy_turn_speed") as f32).to_radians() / 60.0,
                    face_angle: (value("enemy_face_angle") as f32).to_radians(),
                    rest_ticks: ticks("enemy_rest_ticks"),
                    aggro_range: value("enemy_aggro_range") as f32,
                    // Checked above: a whole number of at least 0; `as`
                    // saturates.
                    seed: value("enemy_seed") as u64,
                    low,
                    high,
                    lunge_step: value("lunge_distance") as f32 / lunge_active as f32,
                    combo_hits: ticks("combo_hits"),
                    combo_gap: ticks("combo_gap_ticks"),
                    approach_min: value("approach_min_range") as f32,
                    approach_per_meter: value("approach_weight_per_meter") as f32,
                    approach_ticks: ticks("approach_ticks"),
                    approach_max: ticks("approach_weight_max"),
                    moves,
                    spin: false,
                };
                let phase2 = phase2_at.map(|at| {
                    let s = value("phase2_speed_scale") as f32;
                    Phase2 {
                        at: at as f32,
                        params: params.phase2(s, ticks("phase2_combo_hits")),
                        roar_ticks: ticks("roar_ticks"),
                        blast_tick: ticks("roar_blast_tick"),
                        damage: value("roar_damage") as f32,
                        impact: value("roar_impact") as f32,
                        reach: value("roar_reach") as f32,
                    }
                });
                let steps = [
                    Some(("enemy", params.step)),
                    Some(("lunge", params.lunge_step)),
                    phase2.map(|p| ("enemy in phase 2", p.params.step)),
                ];
                for (what, step) in steps.into_iter().flatten() {
                    if step >= radius {
                        return Err(GameError::EnemyStepTooLong {
                            enemy: path(),
                            what,
                            step,
                            radius,
                        });
                    }
                }
                if scene
                    .colliders()
                    .iter()
                    .any(|&c| is_in_subtree(scene, c, id))
                {
                    return Err(GameError::EnemyCollider(path()));
                }
                if node.model.is_some() {
                    return Err(GameError::EnemyModelNotOnBody(path()));
                }
                let body = child(BODY).filter(|&b| scene.node(b).model.is_some());
                if let Some(b) = body {
                    let Some(animation) = scene
                        .node(b)
                        .model
                        .as_ref()
                        .and_then(|m| m.animation.as_ref())
                    else {
                        return Err(GameError::EnemyBodyWithoutAnimation(path()));
                    };
                    if let Some(clip) = ENEMY_CLIPS
                        .iter()
                        .find(|c| animation.clip_path(c).is_none())
                    {
                        return Err(GameError::EnemyMissingClip {
                            enemy: path(),
                            clip: (*clip).to_owned(),
                        });
                    }
                }
                let body = body.map(|b| Body {
                    id: b,
                    p0: scene.transform(b).position,
                    lift: value("enemy_dead_lift") as f32,
                });
                Some((params, phase2, radius, height, body))
            }
        };
        if let Some(range) = node.prop("bonfire_range") {
            if node.parent.is_some() || id == camera {
                return Err(GameError::BonfireNotRoot(path()));
            }
            if lock_height.is_some()
                || hurt.is_some()
                || swing_damage.is_some()
                || enemy_speed.is_some()
            {
                return Err(GameError::BonfireRole(path()));
            }
            bonfires.push(Bonfire {
                id,
                path: path(),
                range: range as f32,
            });
        }
        if node.name == BLOODSTAIN && node.parent.is_none() {
            if node.mesh.is_none() || node.props().next().is_some() || !node.children.is_empty() {
                return Err(GameError::BloodstainMarker(path()));
            }
            bloodstain_marker = Some((id, scene.transform(id).position));
        }
        let mut hurtbox = hurt.map(|(radius, height)| {
            let health = hp.map(|hp| {
                Health::new(
                    hp as f32,
                    value("poise") as f32,
                    ["stagger_ticks", "poise_reset_ticks", "revive_ticks"].map(ticks),
                    bar.map(|bar| {
                        let t = scene.transform(bar);
                        HpBar {
                            id: bar,
                            x0: t.position.x,
                            width: t.scale.x,
                        }
                    }),
                )
            });
            Hurtbox {
                id,
                path: path(),
                radius,
                height,
                meshes: scene
                    .ids()
                    .filter(|&n| is_in_subtree(scene, n, id) && Some(n) != bar)
                    .filter_map(|n| {
                        let node = scene.node(n);
                        node.mesh
                            .map(|m| (n, m.color, false))
                            .or(node.model.as_ref().map(|m| (n, m.color, true)))
                    })
                    .collect(),
                base: scene.transform(id).rotation,
                hits: 0,
                last_hit: None,
                health,
                // Checked above: a whole number of at least 0.
                souls: value("souls") as u32,
            }
        });
        let hurtbox_index = hurtbox.as_ref().map(|_| hurtboxes.len());
        if let Some(height) = lock_height {
            targets.push(Target {
                id,
                path: path(),
                height: height as f32,
                hurtbox: hurtbox_index,
            });
        }
        if let Some(damage) = swing_damage {
            let [startup, active, recovery] = [
                "swing_startup_ticks",
                "swing_active_ticks",
                "swing_recovery_ticks",
            ]
            .map(ticks);
            let arc = (value("swing_arc") as f32).to_radians();
            // As the character's facing at load.
            let f = scene.transform(id).rotation * Vec3::NEG_Z;
            swingers.push(Swinger {
                id,
                path: path(),
                facing: positive_zero(wrap(libm::atan2f(-f.x, -f.z))),
                damage: damage as f32,
                impact: value("swing_impact") as f32,
                startup,
                active,
                period: startup.saturating_add(active).saturating_add(recovery),
                reach: value("swing_reach") as f32,
                arc,
                slice: arc / active as f32,
                low: value("swing_low") as f32,
                high: value("swing_high") as f32,
                hurtbox: hurtbox_index,
                arm: arm.map(|arm| {
                    let t = scene.transform(arm);
                    Arm {
                        id: arm,
                        p0: t.position,
                        r0: t.rotation,
                        color: scene
                            .node(arm)
                            .mesh
                            .expect("checked above: the arm has a mesh")
                            .color,
                    }
                }),
                swing_tick: 0,
                struck: false,
            });
        }
        if let Some((params, phase2, radius, height, body)) = enemy {
            // As the character's facing at load.
            let f = scene.transform(id).rotation * Vec3::NEG_Z;
            let facing = positive_zero(wrap(libm::atan2f(-f.x, -f.z)));
            let hurtbox = hurtbox
                .as_mut()
                .expect("checked above: an enemy is a hurtbox");
            hurtbox.base = Quat::from_rotation_y(facing);
            enemies.push(Enemy {
                id,
                path: path(),
                base: params,
                phase2,
                stage: 1,
                hurtbox: hurtboxes.len(),
                radius,
                height,
                arm: arm.map(|arm| {
                    let t = scene.transform(arm);
                    Arm {
                        id: arm,
                        p0: t.position,
                        r0: t.rotation,
                        color: scene
                            .node(arm)
                            .mesh
                            .expect("checked above: the arm has a mesh")
                            .color,
                    }
                }),
                body,
                state: EnemyCondition::Idle,
                current: None,
                move_tick: 0,
                facing,
                rest: 0,
                chase: 0,
                rng: SplitMix64(params.seed),
                struck: 0,
                boss_bar: value("boss_bar") == 1.0,
            });
        }
        // Last: a boss's name is drawn with the HUD font.
        if enemy_speed.is_some()
            && value("boss_bar") == 1.0
            && let Some(name) = &node.display_name
            && let Some(c) = name.chars().find(|&c| !(' '..='~').contains(&c))
        {
            return Err(GameError::BossNameChar { node: path(), c });
        }
        hurtboxes.extend(hurtbox);
    }
    Ok(Roles {
        targets,
        hurtboxes,
        swingers,
        enemies,
        bonfires,
        bloodstain_marker,
    })
}

/// Whether `node` is `root` or one of its descendants.
pub(super) fn is_in_subtree(scene: &Scene, node: NodeId, root: NodeId) -> bool {
    let mut at = Some(node);
    while let Some(n) = at {
        if n == root {
            return true;
        }
        at = scene.node(n).parent;
    }
    false
}
