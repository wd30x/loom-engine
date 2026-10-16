//! Tests of the enemy clips (`docs/architecture.md` section 11, "模型和动
//! 画"): an enemy's model on its child named `body`, which clip plays
//! when, and how it looks hit and dead. The character, a capsule, stands
//! at the origin facing -Z with the camera behind it; the enemy stands 2 m
//! ahead facing it. Every move but the one a test picks has weight 0, and
//! the enemy never chooses to approach; the move props hold their
//! defaults, the lengths the clips are written for.

use glam::{Quat, Vec3};
use loom_core::input::{ActionState, TickInput};
use loom_scene::{Animation, NodeId};

use super::enemy_clips::ENEMY_CLIPS;
use super::show::{DEAD_DIM, FLASH_TICKS, FLASH_TINT, front_or_back, pose};
use super::*;
use crate::enemy::EnemyCondition;

/// The body's animation, every clip declared but `missing`, playing the
/// first one declared.
fn animation_without(missing: &str) -> String {
    let clips: Vec<String> = ENEMY_CLIPS
        .iter()
        .filter(|&&c| c != missing)
        .map(|c| format!(r#"("{c}", "assets/anims/boss/{c}.ron")"#))
        .collect();
    let play = ENEMY_CLIPS.iter().find(|&&c| c != missing).unwrap();
    format!(
        r#"animation: (clips: [{}], play: "{play}")"#,
        clips.join(", ")
    )
}

/// The body child of the enemy: the boss model with every clip.
fn body() -> String {
    format!(
        r#"(name: "body", model: "assets/models/boss.glb", {})"#,
        animation_without("")
    )
}

/// The enemy "e" at `(0, 0, z)` facing +Z, `children` under it: 600 hp,
/// 2.5 m/s, no move nor approach, with `overrides` changed or added.
fn enemy(z: f32, overrides: &[(&str, f64)], children: &str) -> String {
    let mut props: Vec<(String, f64)> = [
        ("lock_height", 2.0),
        ("hurt_radius", 0.7),
        ("hurt_height", 3.0),
        ("hp", 600.0),
        ("enemy_speed", 2.5),
        ("enemy_dead_lift", 0.3),
        ("approach_weight_per_meter", 0.0),
        ("sweep_weight", 0.0),
        ("cleave_weight", 0.0),
        ("lunge_weight", 0.0),
        ("combo_weight", 0.0),
    ]
    .iter()
    .map(|&(n, v)| (n.to_owned(), v))
    .collect();
    for &(name, v) in overrides {
        match props.iter_mut().find(|(n, _)| n == name) {
            Some(p) => p.1 = v,
            None => props.push((name.to_owned(), v)),
        }
    }
    let props: Vec<String> = props
        .iter()
        .map(|(n, v)| format!(r#"("{n}", {v:?})"#))
        .collect();
    format!(
        r#"(name: "e", position: (0.0, 0.0, {z:?}), rotation: (0.0, 180.0, 0.0),
            props: [{}], children: [{children}]),"#,
        props.join(", ")
    )
}

fn scene(nodes: &str) -> String {
    format!(
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
            character: (run_speed: 4.0, turn_speed: 720.0)),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
        {nodes}
    ],
)"#
    )
}

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    /// The enemy 2 m ahead with `overrides` and a modeled body.
    fn new(overrides: &[(&str, f64)]) -> Self {
        Self::at(-2.0, overrides)
    }

    fn at(z: f32, overrides: &[(&str, f64)]) -> Self {
        Self {
            game: Game::from_source(&scene(&enemy(z, overrides, &body()))).unwrap(),
            input: ActionState::new(),
        }
    }

    /// One tick with exactly `actions` held; what the body plays after it.
    fn hold(&mut self, actions: &[&str]) -> Animation {
        self.input.advance(TickInput {
            actions: actions.iter().map(|&a| a.to_owned()).collect(),
            axes: Default::default(),
        });
        self.game.tick(&self.input);
        self.animation()
    }

    fn idle(&mut self) -> Animation {
        self.hold(&[])
    }

    fn body(&self) -> NodeId {
        self.game.enemies[0].body.expect("a modeled body").id
    }

    fn animation(&self) -> Animation {
        let model = self.game.scene.node(self.body()).model.clone();
        model
            .and_then(|m| m.animation)
            .expect("the body has an animation")
    }

    fn tick(&self) -> u64 {
        self.game.clock().tick()
    }

    fn enemy(&self) -> &Enemy {
        &self.game.enemies[0]
    }

    /// Ticks idle until the enemy starts a move, then through it; for every
    /// tick a part starts on, `(move_tick, clip, offset, rate, blend)`, and
    /// what plays on the tick after the move.
    fn parts(&mut self) -> (Vec<Part>, Animation) {
        while self.enemy().state != EnemyCondition::Attack {
            self.idle();
            assert!(self.tick() < 100, "no move started");
        }
        let mut parts = Vec::new();
        let mut a = self.animation();
        while self.enemy().state == EnemyCondition::Attack {
            if a.start == self.tick() {
                assert_eq!(a.blend_start, a.start);
                let e = self.enemy();
                parts.push((e.move_tick, a.playing.clone(), a.offset, a.rate, a.blend));
            }
            a = self.idle();
        }
        (parts, a)
    }
}

/// A part a move's clip starts on: `(move_tick, clip, offset, rate, blend)`.
type Part = (u32, String, u64, u32, u64);

fn part(t: u32, clip: &str, offset: u64, rate: u32, blend: u64) -> Part {
    (t, clip.to_owned(), offset, rate, blend)
}

/// The error loading the enemy with `children`, and `own` fields on the
/// enemy node itself.
fn load_error(own: &str, children: &str) -> String {
    let e = enemy(-2.0, &[("sweep_weight", 1.0)], children).replacen(
        "rotation: (0.0, 180.0, 0.0),",
        &format!("rotation: (0.0, 180.0, 0.0), {own}"),
        1,
    );
    Game::from_source(&scene(&e)).unwrap_err().to_string()
}

// ------------------------------------------------------------------ forms

#[test]
fn an_enemy_with_a_model_of_its_own_is_rejected() {
    let own = format!(
        r#"model: "assets/models/boss.glb", {},"#,
        animation_without("")
    );
    assert_eq!(
        load_error(&own, ""),
        r#"the enemy "e" has a model: an enemy's model goes on its child named body"#
    );
}

#[test]
fn an_enemy_body_model_without_animation_is_rejected() {
    assert_eq!(
        load_error("", r#"(name: "body", model: "assets/models/boss.glb")"#),
        r#"the enemy "e": the model on its body needs an animation"#
    );
}

#[test]
fn an_enemy_body_animation_missing_one_of_the_ten_clips_is_rejected() {
    for missing in ENEMY_CLIPS {
        let children = format!(
            r#"(name: "body", model: "assets/models/boss.glb", {})"#,
            animation_without(missing)
        );
        assert_eq!(
            load_error("", &children),
            format!(r#"the enemy "e": its body's animation needs a clip "{missing}""#)
        );
    }
}

#[test]
fn an_enemy_with_a_capsule_body_or_no_body_plays_nothing() {
    let capsule = r#"(name: "body", mesh: Capsule(radius: 0.7, height: 3.0))"#;
    for children in [capsule, ""] {
        let src = scene(&enemy(-2.0, &[("sweep_weight", 1.0)], children));
        let mut game = Game::from_source(&src).unwrap();
        assert!(game.enemies[0].body.is_none());
        for _ in 0..40 {
            game.tick(&ActionState::new());
        }
        assert_eq!(game.enemies[0].state, EnemyCondition::Attack);
    }
}

// ------------------------------------------------------------------ moves

#[test]
fn each_single_move_plays_its_three_parts_from_their_offsets() {
    // (move, its parts' first ticks, its clip's part offsets)
    for (m, firsts, offsets) in [
        ("sweep", [1, 31, 39], [0, 3000, 3800]),
        ("cleave", [1, 46, 50], [0, 4500, 4900]),
        ("lunge", [1, 36, 46], [0, 3500, 4500]),
    ] {
        let (weight, min_range) = (format!("{m}_weight"), format!("{m}_min_range"));
        let mut run = Run::new(&[(&weight, 1.0), (&min_range, 0.0)]);
        let (parts, after) = run.parts();
        let expected: Vec<_> = (0..3)
            .map(|i| part(firsts[i], m, offsets[i], 100, [3, 0, 0][i]))
            .collect();
        assert_eq!(parts, expected, "{m}");
        assert_eq!((after.playing.as_str(), after.blend), ("idle", 6), "{m}");
    }
}

#[test]
fn a_longer_startup_plays_the_startup_slower_only() {
    let mut run = Run::new(&[("sweep_weight", 1.0), ("sweep_startup_ticks", 60.0)]);
    let (parts, _) = run.parts();
    assert_eq!(
        parts,
        [
            part(1, "sweep", 0, 50, 3),
            part(61, "sweep", 3000, 100, 0),
            part(69, "sweep", 3800, 100, 0)
        ]
    );
}

#[test]
fn a_combo_plays_combo_then_combo_even_and_combo_odd_by_turns() {
    // Startup 25, cuts of 5, gaps of 12: cut 1 on 26, the gap before cut
    // 2 on 31 and the cut on 43, before cut 3 on 48 and 60, the recovery
    // on 65, played by the last cut's clip.
    let mut run = Run::new(&[("combo_weight", 1.0)]);
    let (parts, _) = run.parts();
    assert_eq!(
        parts,
        [
            part(1, "combo", 0, 100, 3),
            part(26, "combo", 2500, 100, 0),
            part(31, "combo_even", 0, 100, 0),
            part(43, "combo_even", 1200, 100, 0),
            part(48, "combo_odd", 0, 100, 0),
            part(60, "combo_odd", 1200, 100, 0),
            part(65, "combo_odd", 1700, 100, 0),
        ]
    );
    // Two cuts end on an even one; without gaps cut follows cut.
    let mut run = Run::new(&[
        ("combo_weight", 1.0),
        ("combo_hits", 2.0),
        ("combo_gap_ticks", 0.0),
    ]);
    let (parts, _) = run.parts();
    assert_eq!(
        parts,
        [
            part(1, "combo", 0, 100, 3),
            part(26, "combo", 2500, 100, 0),
            part(31, "combo_even", 1200, 100, 0),
            part(36, "combo_even", 1700, 100, 0),
        ]
    );
}

#[test]
fn a_combo_in_phase_2_plays_its_shorter_startups_and_gaps_faster() {
    let mut run = Run::new(&[
        ("combo_weight", 1.0),
        ("phase2_at", 0.5),
        ("spin_weight", 0.0),
    ]);
    run.game.enemies[0].stage = 2;
    let (parts, _) = run.parts();
    // Phase 2's 0.75: startup round(18.75) = 19, gaps 9, four cuts.
    let (startup, gap) = (clips::stretch(25, 19), clips::stretch(12, 9));
    assert_eq!((startup, gap), (132, 133));
    assert_eq!(
        parts,
        [
            part(1, "combo", 0, startup, 3),
            part(20, "combo", 2500, 100, 0),
            part(25, "combo_even", 0, gap, 0),
            part(34, "combo_even", 1200, 100, 0),
            part(39, "combo_odd", 0, gap, 0),
            part(48, "combo_odd", 1200, 100, 0),
            part(53, "combo_even", 0, gap, 0),
            part(62, "combo_even", 1200, 100, 0),
            part(67, "combo_even", 1700, 100, 0),
        ]
    );
}

#[test]
fn the_spin_plays_idle_and_so_does_the_rest_after_it() {
    let mut run = Run::new(&[("phase2_at", 0.5), ("spin_weight", 1.0)]);
    run.game.enemies[0].stage = 2;
    let (parts, after) = run.parts();
    assert!(parts.is_empty(), "{parts:?}");
    assert_eq!(after.playing, "idle");
    assert_eq!(run.enemy().state, EnemyCondition::Rest);
}

// ------------------------------------------------------------------ walking

#[test]
fn walking_up_plays_walk_at_its_written_speed_and_faster_walks_faster() {
    for (speed, rate) in [(2.5, 100), (5.0, 200)] {
        let mut run = Run::at(-12.0, &[("enemy_speed", speed)]);
        let a = run.idle();
        assert_eq!(run.enemy().state, EnemyCondition::Chase);
        assert_eq!((a.playing.as_str(), a.rate, a.blend), ("walk", rate, 6));
        assert_eq!(a.start, 1);
        // Walking on, nothing more is called.
        assert_eq!(run.idle(), a);
    }
}

// ------------------------------------------------------------------ hit

/// The enemy 1.5 m ahead with `overrides`; the character attacks it once.
/// The animation on each tick from the press on, `ticks` of them.
fn hit_once(overrides: &[(&str, f64)], ticks: usize) -> (Run, Vec<Animation>) {
    let mut run = Run::at(-1.5, overrides);
    let mut seen = vec![run.hold(&[ATTACK])];
    while seen.len() < ticks {
        seen.push(run.idle());
    }
    (run, seen)
}

#[test]
fn a_stagger_plays_stagger_stretched_to_stagger_ticks_from_the_hit() {
    let (run, seen) = hit_once(&[("poise", 5.0), ("stagger_ticks", 40.0)], 30);
    let hit = run.game.hurtboxes[0].last_hit.unwrap().tick;
    let a = &seen[hit as usize - 1];
    assert_eq!(a.playing, "stagger");
    // Written for 30 ticks, played over 40.
    assert_eq!((a.start, a.offset, a.rate, a.blend), (hit, 0, 75, 2));
    assert_eq!(seen[hit as usize], *a);
}

#[test]
fn a_dead_modeled_enemy_lies_on_its_back_lifted_and_stands_up_where_it_was() {
    let (mut run, seen) = hit_once(&[("hp", 20.0), ("revive_ticks", 30.0)], 20);
    let hit = run.game.hurtboxes[0].last_hit.unwrap().tick;
    let a = &seen[hit as usize - 1];
    assert_eq!(a.playing, "death");
    assert_eq!((a.start, a.offset, a.rate, a.blend), (hit, 0, 100, 3));
    // Hit from the front, it falls backwards: on its back, its body 0.3 m
    // up in the world.
    let e = run.enemy();
    let health = run.game.hurtboxes[0].health.unwrap();
    let back = front_or_back(health.fall, e.facing);
    assert!(libm::cosf(back - e.facing) < 0.0);
    let expected = pose(Condition::Dead, back, Quat::from_rotation_y(e.facing));
    let rotation = run.game.scene.transform(e.id).rotation;
    assert!(
        rotation.abs_diff_eq(expected, 1e-6),
        "{rotation} {expected}"
    );
    let world = run.game.scene.world_matrix(run.body()).w_axis.truncate();
    let node = run.game.scene.transform(e.id).position;
    assert!(
        (world - node - Vec3::new(0.0, 0.3, 0.0)).length() < 1e-5,
        "{world}"
    );
    // Up again, it walks up to the character (it has no moves), its body
    // back on its feet.
    while run.game.hurtboxes[0].health.unwrap().is_dead() {
        run.idle();
    }
    let a = run.animation();
    assert_eq!(
        (a.playing.as_str(), a.start, a.blend),
        ("walk", run.tick(), 6)
    );
    assert_eq!(a.from.unwrap().clip, "death");
    assert_eq!(run.game.scene.transform(run.body()).position, Vec3::ZERO);
}

#[test]
fn a_modeled_enemy_flashes_red_when_hit_and_darkens_dead() {
    let color = |run: &Run| {
        run.game
            .scene
            .node(run.body())
            .model
            .as_ref()
            .unwrap()
            .color
    };
    let hit = |hp: f64| {
        let mut run = Run::at(-1.5, &[("hp", hp)]);
        run.hold(&[ATTACK]);
        while run.game.hurtboxes[0].last_hit.is_none() {
            run.idle();
        }
        run
    };
    let mut run = hit(600.0);
    assert_eq!(color(&run), FLASH_TINT);
    for _ in 1..FLASH_TICKS {
        run.idle();
    }
    assert_eq!(color(&run), FLASH_TINT);
    run.idle();
    assert_eq!(color(&run), Vec3::ONE);
    let mut dead = hit(20.0);
    assert_eq!(color(&dead), Vec3::ONE * DEAD_DIM);
    dead.idle();
    assert_eq!(color(&dead), Vec3::ONE * DEAD_DIM);
}

// ------------------------------------------------------------------ boss.ron

/// boss.ron with its body a capsule: the form before E7.6.
fn capsule_boss() -> String {
    let source = include_str!("../../../../assets/scenes/boss.ron");
    let start = source.find(r#"(name: "body", model"#).unwrap();
    let close = r#"play: "idle")),"#;
    let end = start + source[start..].find(close).unwrap() + close.len();
    format!(
        "{}{}{}",
        &source[..start],
        r#"(name: "body", mesh: Capsule(radius: 0.7, height: 3.0), color: (0.28, 0.24, 0.3)),"#,
        &source[end..]
    )
}

#[test]
fn the_boss_drawn_by_its_model_or_a_capsule_plays_the_same_game() {
    let source = include_str!("../../../../assets/scenes/boss.ron");
    // Less health, so that the fight reaches phase 2 and the boss dies.
    let weaker = |s: &str| s.replace(r#"("hp", 600),"#, r#"("hp", 60),"#);
    let mut modeled = Game::from_source(&weaker(source)).unwrap();
    let mut capsule = Game::from_source(&weaker(&capsule_boss())).unwrap();
    assert!(modeled.enemies[0].body.is_some() && capsule.enemies[0].body.is_none());
    let mut input = ActionState::new();
    let (mut roared, mut died) = (false, false);
    for tick in 0..1500u32 {
        // Walk up, then attack every 12 ticks and roll every 97.
        let actions = [(ATTACK, tick % 12 == 0), (ROLL, tick % 97 == 0)]
            .into_iter()
            .filter(|&(_, on)| on)
            .map(|(a, _)| a.to_owned())
            .collect();
        let axes = [(MOVE_Y.to_owned(), 1.0)]
            .into_iter()
            .filter(|_| tick < 90)
            .collect();
        input.advance(TickInput { actions, axes });
        modeled.tick(&input);
        capsule.tick(&input);
        let (m, c) = (modeled.state(), capsule.state());
        // Debug tells -0.0 from 0.0, as the state dump does.
        assert_eq!(format!("{m:?}"), format!("{c:?}"), "tick {tick}");
        roared |= m.enemies[0].state == EnemyCondition::Roar;
        died |= m.enemies[0].state == EnemyCondition::Dead;
    }
    assert!(
        roared && died,
        "the fight never got to phase 2 and the boss's death"
    );
}
