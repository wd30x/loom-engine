//! Tests of the game as a whole, moved unchanged from `game.rs`.

use super::*;
use crate::demo_scene;
use loom_core::input::TickInput;
use std::f32::consts::{FRAC_PI_3, FRAC_PI_4};

type Axes = Vec<(&'static str, f32)>;

/// Runs `ticks` more ticks; `axes(tick)` gives the axis values of each
/// tick (1-based, counting from the game's current tick).
fn advance(game: &mut Game, ticks: u64, axes: impl Fn(u64) -> Axes) {
    let mut input = ActionState::new();
    let first = game.clock().tick() + 1;
    for tick in first..first + ticks {
        input.advance(TickInput {
            actions: Default::default(),
            axes: axes(tick)
                .into_iter()
                .map(|(a, v)| (a.to_owned(), v))
                .collect(),
        });
        game.tick(&input);
    }
}

/// Runs `ticks` ticks of the demo scene.
fn run(ticks: u64, axes: impl Fn(u64) -> Axes) -> Game {
    run_from(Vec3::ZERO, ticks, axes)
}

/// Like [`run`], with the player starting at `start`.
fn run_from(start: Vec3, ticks: u64, axes: impl Fn(u64) -> Axes) -> Game {
    let mut scene = demo_scene();
    let player = scene.character().unwrap();
    scene.transform_mut(player).position = start;
    let mut game = Game::new(scene).unwrap();
    advance(&mut game, ticks, axes);
    game
}

fn bits3(v: Vec3) -> [u32; 3] {
    v.to_array().map(f32::to_bits)
}

fn bits4(q: Quat) -> [u32; 4] {
    q.to_array().map(f32::to_bits)
}

fn assert_vec3(actual: Vec3, expected: [f32; 3]) {
    assert_eq!(
        bits3(actual),
        expected.map(f32::to_bits),
        "{actual} != {expected:?}"
    );
}

fn assert_quat(actual: Quat, expected: [f32; 4]) {
    assert_eq!(
        bits4(actual),
        expected.map(f32::to_bits),
        "{actual} != {expected:?}"
    );
}

fn assert_f32(actual: f32, expected: f32) {
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{actual} != {expected}"
    );
}

fn node(game: &Game, path: &str) -> loom_scene::Transform {
    *game.scene().transform(game.scene().find(path).unwrap())
}

/// Checks the player against the state and the `player` node.
fn assert_player(game: &Game, position: [f32; 3], facing: f32) {
    let state = game.state().player;
    assert_vec3(state.position, position);
    assert_f32(state.facing, facing);
    assert_vec3(node(game, "player").position, position);
}

fn assert_camera_node(game: &Game, position: [f32; 3], rotation: [f32; 4]) {
    let cam = node(game, "main_cam");
    assert_vec3(cam.position, position);
    assert_quat(cam.rotation, rotation);
}

const LEVEL_ROTATION: [f32; 4] = [-0.1305262, 0.0, 0.0, 0.9914449];

#[test]
fn standing_still_keeps_the_initial_pose() {
    let game = run(60, |_| vec![]);
    assert_player(&game, [0.0, 0.0, 0.0], 0.0);
    assert_f32(game.state().camera.yaw, 0.0);
    assert_f32(game.state().camera.pitch, -0.2617994);
    assert_camera_node(&game, [0.0, 2.5352764, 3.8637035], LEVEL_ROTATION);
    assert_quat(node(&game, "player").rotation, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn camera_is_placed_before_the_first_tick() {
    let game = Game::new(demo_scene()).unwrap();
    assert_camera_node(&game, [0.0, 2.5352764, 3.8637035], LEVEL_ROTATION);
}

#[test]
fn running_forward_for_one_second_covers_four_meters() {
    let game = run(90, |t| if t <= 60 { vec![(MOVE_Y, 1.0)] } else { vec![] });
    assert_player(&game, [0.0, 0.0, -3.9999988], 0.0);
    assert_camera_node(&game, [0.0, 2.5352764, -0.13629532], LEVEL_ROTATION);
}

#[test]
fn running_right_turns_the_player_to_face_right() {
    let game = run(30, |_| vec![(MOVE_X, 1.0)]);
    assert_player(&game, [2.0000007, 0.0, 0.0], -1.5707964);
    assert_quat(
        node(&game, "player").rotation,
        [0.0, -0.70710677, 0.0, 0.70710677],
    );
    assert_camera_node(&game, [2.0000007, 2.5352764, 3.8637035], LEVEL_ROTATION);
}

#[test]
fn turn_is_still_in_progress_after_seven_ticks() {
    let game = run(7, |_| vec![(MOVE_X, 1.0)]);
    assert_f32(game.state().player.position.x, 0.46666667);
    assert_f32(game.state().player.facing, -1.4660766);
    assert_quat(
        node(&game, "player").rotation,
        [0.0, -0.6691306, 0.0, 0.7431448],
    );
}

#[test]
fn turn_completes_on_the_eighth_tick() {
    let game = run(8, |_| vec![(MOVE_X, 1.0)]);
    assert_f32(game.state().player.position.x, 0.53333336);
    assert_f32(game.state().player.facing, -1.5707964);
}

#[test]
fn running_while_turning_the_camera_curves_the_path() {
    let game = run(60, |t| {
        let mut axes = vec![(MOVE_Y, 1.0)];
        if t <= 30 {
            axes.push((LOOK_X, 0.04));
        }
        axes
    });
    assert_player(&game, [2.9577415, 0.0, -2.2566528], -1.1999999);
    assert_f32(game.state().camera.yaw, -1.1999999);
    assert_camera_node(
        &game,
        [-0.6433809, 2.5352762, -0.8566092],
        [-0.10772792, -0.55981183, -0.07370063, 0.8182748],
    );
    assert_quat(
        node(&game, "player").rotation,
        [0.0, -0.5646424, 0.0, 0.8253356],
    );
}

#[test]
fn looking_up_to_pitch_max_pulls_the_camera_in_above_the_ground() {
    let game = run(60, |_| vec![(LOOK_Y, 0.04)]);
    assert_f32(game.state().camera.pitch, FRAC_PI_3); // 1.0471976, pitch_max 60°
    assert_f32(game.state().camera.distance, 1.5011107);
    // y is the collision radius above the ground.
    assert_camera_node(
        &game,
        [0.0, 0.20000005, 0.75055534],
        [0.5, 0.0, 0.0, 0.8660254],
    );
}

#[test]
fn running_backwards_turns_around_to_minus_pi() {
    let game = run(20, |_| vec![(MOVE_Y, -1.0)]);
    assert_player(&game, [0.0, 0.0, 1.3333336], -PI); // -3.1415927
    assert_vec3(node(&game, "main_cam").position, [0.0, 2.5352764, 5.197037]);
}

fn turn_left_then_run(t: u64) -> Vec<(&'static str, f32)> {
    if t <= 40 {
        vec![(LOOK_X, -0.1)]
    } else {
        vec![(MOVE_Y, 1.0)]
    }
}

#[test]
fn turning_left_past_pi_wraps_the_camera_yaw() {
    let game = run(40, turn_left_then_run);
    assert_f32(game.state().camera.yaw, -2.2831872);
    assert_camera_node(
        &game,
        [-2.924056, 2.5352764, -2.5254908],
        [-0.05431796, -0.9015187, -0.11868719, 0.41258582],
    );
}

#[test]
fn running_after_a_wrapped_yaw_follows_the_camera() {
    let game = run(70, turn_left_then_run);
    assert_player(&game, [1.5136027, 0.0, 1.3072904], -2.2831872);
    assert_vec3(
        node(&game, "main_cam").position,
        [-1.4104533, 2.5352764, -1.2182003],
    );
    assert_quat(
        node(&game, "player").rotation,
        [0.0, -0.9092978, 0.0, 0.416146],
    );
}

/// Steps 1-5 as they were in G1, before G2.1 added the roll key,
/// copied here as the reference: `(position, facing, yaw, pitch)`.
fn g1_reference_tick(game: &Game, s: &mut (Vec3, f32, f32, f32), input: &ActionState) {
    let (position, facing, yaw, pitch) = s;
    *yaw = wrap(*yaw - input.axis(LOOK_X));
    *pitch = (*pitch + input.axis(LOOK_Y)).clamp(game.pitch_min, game.pitch_max);
    let mut v = Vec2::new(input.axis(MOVE_X), input.axis(MOVE_Y));
    if v.length_squared() > 1.0 {
        v = v / v.length();
    }
    let (sn, cs) = (libm::sinf(*yaw), libm::cosf(*yaw));
    let forward = Vec3::new(-sn, 0.0, -cs);
    let right = Vec3::new(cs, 0.0, -sn);
    let dir = right * v.x + forward * v.y;
    *position = resolve(
        &game.boxes,
        game.radius,
        game.height,
        *position + dir * game.step,
        &[],
    );
    if v.length_squared() > 0.0 {
        let target = libm::atan2f(-dir.x, -dir.z);
        let diff = wrap(target - *facing);
        *facing = wrap(*facing + diff.clamp(-game.max_turn, game.max_turn));
    }
}

#[test]
fn without_the_roll_key_every_tick_matches_g1_bit_for_bit() {
    // A copy of the old code rather than stored values: it checks every
    // tick, not just the end, with pillars and the slanted wall in the
    // way, and needs no numbers from before the change.
    let mut game = Game::new(demo_scene()).unwrap();
    let start = game.state();
    let mut reference = (
        start.player.position,
        start.player.facing,
        start.camera.yaw,
        start.camera.pitch,
    );
    let mut input = ActionState::new();
    for t in 1..=120_u32 {
        let axes: Axes = match t {
            1..=40 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
            41..=80 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
            _ => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0), (LOOK_X, -0.03)],
        };
        input.advance(TickInput {
            actions: Default::default(),
            axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
        });
        game.tick(&input);
        g1_reference_tick(&game, &mut reference, &input);
        let s = game.state();
        let actual = (
            s.player.position,
            s.player.facing,
            s.camera.yaw,
            s.camera.pitch,
        );
        assert_eq!(bits3(actual.0), bits3(reference.0), "position at tick {t}");
        for (a, r) in [
            (actual.1, reference.1),
            (actual.2, reference.2),
            (actual.3, reference.3),
        ] {
            assert_eq!(a.to_bits(), r.to_bits(), "tick {t}");
        }
        assert_eq!(s.player.action, Action::Idle);
    }
    assert_ne!(reference.0, start.player.position);
}

/// The input of [`without_attack_keys_every_tick_matches_g2_1`] at
/// `tick`: whether the roll key is held, and the axes.
fn g2_1_reference_input(tick: u32) -> (bool, Axes) {
    let roll = matches!(
        tick,
        41 | 90..=150 | 170 | 200 | 204 | 208 | 212 | 216 | 262 | 300..=330
    );
    let axes = match tick {
        1..=60 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
        61..=160 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
        161..=199 => vec![(LOOK_X, -0.03)],
        200..=260 => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0)],
        _ => vec![(MOVE_Y, 1.0), (LOOK_X, 0.02)],
    };
    (roll, axes)
}

/// FNV-1a over the bits of every field the state dump had in G2.1.
fn g2_1_fingerprint(hash: &mut u64, s: &GameState) {
    let p = &s.player;
    let words = [
        p.position.x.to_bits(),
        p.position.y.to_bits(),
        p.position.z.to_bits(),
        p.facing.to_bits(),
        p.stamina.to_bits(),
        p.stamina_wait,
        p.action as u32,
        p.action_tick,
        u32::from(p.invulnerable),
        s.camera.yaw.to_bits(),
        s.camera.pitch.to_bits(),
        s.camera.distance.to_bits(),
    ];
    for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
}

#[test]
fn without_attack_keys_every_tick_matches_g2_1() {
    // The expected hash was computed by this same test on the G2.1 code
    // (commit 6b9c77a), before attacks existed: it covers every tick
    // of a run with rolls, backsteps, sprints, taps without stamina
    // and regeneration, with pillars and the slanted wall in the way.
    let src = demo_with(r#"("stamina_max", 100)"#, r#"("stamina_max", 40)"#);
    let mut game = Game::from_source(&src).unwrap();
    let mut input = ActionState::new();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut seen = Vec::new();
    let mut taps_without_stamina = 0;
    for t in 1..=400 {
        let (roll, axes) = g2_1_reference_input(t);
        let released = !roll && g2_1_reference_input(t - 1).0;
        input.advance(TickInput {
            actions: if roll {
                [ROLL.to_owned()].into()
            } else {
                Default::default()
            },
            axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
        });
        game.tick(&input);
        let s = game.state();
        g2_1_fingerprint(&mut hash, &s);
        let p = &s.player;
        assert_eq!((p.attack_phase, p.combo, p.buffered), (None, 0, None));
        if !seen.contains(&s.player.action) {
            seen.push(s.player.action);
        }
        if released && s.player.stamina == 0.0 && s.player.action == Action::Idle {
            taps_without_stamina += 1;
        }
    }
    assert!(taps_without_stamina > 0);
    assert_eq!(
        seen,
        [Action::Idle, Action::Roll, Action::Sprint, Action::Backstep]
    );
    assert_eq!(hash, 309_999_793_288_603_552);
}

/// The input of [`without_the_lock_key_every_tick_matches_g2_3`] at
/// `tick`: the actions held, and the axes.
fn g2_3_reference_input(tick: u32) -> (Vec<&'static str>, Axes) {
    let mut actions = Vec::new();
    if matches!(tick, 100..=150 | 175 | 210 | 230 | 262 | 300..=330) {
        actions.push(ROLL);
    }
    if matches!(tick, 5 | 20 | 50 | 340 | 345) {
        actions.push(ATTACK);
    }
    if matches!(tick, 200 | 360) {
        actions.push(HEAVY);
    }
    let axes = match tick {
        1..=60 => vec![(MOVE_Y, 1.0), (LOOK_X, 0.01)],
        61..=160 => vec![(MOVE_X, -0.6), (MOVE_Y, 0.8), (LOOK_Y, -0.02)],
        161..=199 => vec![(LOOK_X, -0.03)],
        200..=260 => vec![(MOVE_X, 1.0), (MOVE_Y, -1.0)],
        _ => vec![(MOVE_Y, 1.0), (LOOK_X, 0.02)],
    };
    (actions, axes)
}

/// FNV-1a over the bits of every field the state dump had in G2.3.
fn g2_3_fingerprint(hash: &mut u64, s: &GameState) {
    let p = &s.player;
    let words = [
        p.position.x.to_bits(),
        p.position.y.to_bits(),
        p.position.z.to_bits(),
        p.facing.to_bits(),
        p.stamina.to_bits(),
        p.stamina_wait,
        p.action as u32,
        p.action_tick,
        u32::from(p.invulnerable),
        p.attack_phase.map_or(0, |a| a as u32 + 1),
        p.combo,
        p.buffered.map_or(0, |b| b as u32 + 1),
        s.camera.yaw.to_bits(),
        s.camera.pitch.to_bits(),
        s.camera.distance.to_bits(),
    ];
    for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
}

#[test]
fn without_the_lock_key_every_tick_matches_g2_3() {
    // The expected hash was computed by this same test on the G2.3 code
    // (commit b450286), before lock-on existed, on the G2.3 demo scene:
    // it covers every tick of a run with a light combo, heavy attacks,
    // buffered and dropped presses, rolls, a backstep and sprints, with
    // pillars and the slanted wall in the way.
    let mut game = Game::from_source(G2_3_DEMO).unwrap();
    let mut input = ActionState::new();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut seen = Vec::new();
    let mut max_combo = 0;
    let mut buffered = 0;
    for t in 1..=400 {
        let (actions, axes) = g2_3_reference_input(t);
        input.advance(TickInput {
            actions: actions.into_iter().map(str::to_owned).collect(),
            axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
        });
        game.tick(&input);
        let s = game.state();
        g2_3_fingerprint(&mut hash, &s);
        if !seen.contains(&s.player.action) {
            seen.push(s.player.action);
        }
        max_combo = max_combo.max(s.player.combo);
        buffered += u32::from(s.player.buffered.is_some());
    }
    assert_eq!(
        seen,
        [
            Action::Idle,
            Action::Attack,
            Action::Sprint,
            Action::Backstep,
            Action::Heavy,
            Action::Roll
        ]
    );
    assert_eq!(max_combo, 3);
    assert!(buffered > 0);
    assert_eq!(hash, 12_318_382_315_914_199_701);
}

/// The input of [`hits_leave_player_camera_and_lock_as_in_g3_1`] at
/// `tick`: the actions held, and the axes. Locks on to dummy_b, walks
/// up to it, a buffered three-hit light combo and a buffered heavy
/// attack; then flicks over to dummy_c, walks up to it, and a light and
/// a heavy attack on it.
fn g3_1_reference_input(tick: u32) -> (Vec<&'static str>, Axes) {
    let mut actions = Vec::new();
    if tick == 1 {
        actions.push(LOCK);
    }
    if matches!(tick, 100 | 115 | 145 | 340) {
        actions.push(ATTACK);
    }
    if matches!(tick, 175 | 380) {
        actions.push(HEAVY);
    }
    let axes = match tick {
        2..=95 | 260..=330 => vec![(MOVE_Y, 1.0)],
        250 => vec![(LOOK_X, 0.12)],
        _ => vec![],
    };
    (actions, axes)
}

/// FNV-1a over the bits of every `player` and `camera` field and the
/// `lock` path.
fn g3_1_fingerprint(hash: &mut u64, s: &GameState) {
    let p = &s.player;
    let words = [
        p.position.x.to_bits(),
        p.position.y.to_bits(),
        p.position.z.to_bits(),
        p.facing.to_bits(),
        p.stamina.to_bits(),
        p.stamina_wait,
        p.action as u32,
        p.action_tick,
        u32::from(p.invulnerable),
        p.attack_phase.map_or(0, |a| a as u32 + 1),
        p.combo,
        p.buffered.map_or(0, |b| b as u32 + 1),
        s.camera.yaw.to_bits(),
        s.camera.pitch.to_bits(),
        s.camera.distance.to_bits(),
    ];
    let lock = match &s.lock {
        Some(path) => path.as_bytes(),
        None => &[0xfe],
    };
    let bytes = words.iter().flat_map(|w| w.to_le_bytes());
    for byte in bytes.chain(lock.iter().copied()).chain([0xff]) {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
}

#[test]
fn hits_leave_player_camera_and_lock_as_in_g3_1() {
    // The expected hash was computed by this same test on the G3.1 code
    // (commit 2c5cfc0), before hits existed, on the demo scene with its
    // dummies: it covers every tick of attacks that hit two of them.
    let mut game = Game::new(demo_scene()).unwrap();
    let mut input = ActionState::new();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut locks = Vec::new();
    let mut max_combo = 0;
    for t in 1..=450 {
        let (actions, axes) = g3_1_reference_input(t);
        input.advance(TickInput {
            actions: actions.into_iter().map(str::to_owned).collect(),
            axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
        });
        game.tick(&input);
        let s = game.state();
        g3_1_fingerprint(&mut hash, &s);
        if locks.last() != Some(&s.lock) {
            locks.push(s.lock.clone());
        }
        max_combo = max_combo.max(s.player.combo);
    }
    assert_eq!(
        locks,
        [Some("dummy_b".to_owned()), Some("dummy_c".to_owned())]
    );
    assert_eq!(max_combo, 3);
    assert_eq!(game.state().player.action, Action::Idle);
    // The attacks did hit: dummy_b and dummy_c, never dummy_a.
    let hits: Vec<_> = game
        .state()
        .hurtboxes
        .iter()
        .map(|h| (h.node.clone(), h.hits))
        .collect();
    assert_eq!(hits[0], ("dummy_a".to_owned(), 0));
    assert!(hits[1].0 == "dummy_b" && hits[1].1 > 0, "{hits:?}");
    assert!(hits[2].0 == "dummy_c" && hits[2].1 > 0, "{hits:?}");
    assert_eq!(hash, 4_107_292_735_894_931_732);
}

/// FNV-1a over [`g3_1_fingerprint`]'s fields, then every hurtbox's
/// `hits`, `last_hit` and `health`, then the transforms of the three
/// dummies and their hp bars.
fn g3_3_fingerprint(hash: &mut u64, game: &Game) {
    let s = game.state();
    g3_1_fingerprint(hash, &s);
    let mut words = Vec::new();
    for h in &s.hurtboxes {
        words.push(h.hits);
        match h.last_hit {
            Some(hit) => words.extend([
                1,
                hit.tick as u32,
                (hit.tick >> 32) as u32,
                hit.attack as u32,
                hit.combo,
            ]),
            None => words.push(0),
        }
        match h.health {
            Some(health) => words.extend([
                1,
                health.hp.to_bits(),
                health.state as u32,
                health.state_ticks,
                health.impact.to_bits(),
                health.impact_wait,
            ]),
            None => words.push(0),
        }
    }
    for dummy in ["dummy_a", "dummy_b", "dummy_c"] {
        for path in [dummy.to_owned(), format!("{dummy}/hp_bar")] {
            let t = node(game, &path);
            words.extend(bits3(t.position));
            words.extend(bits4(t.rotation));
            words.extend(bits3(t.scale));
        }
    }
    for byte in words.iter().flat_map(|w| w.to_le_bytes()) {
        *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
}

#[test]
fn without_swingers_every_tick_matches_g3_3() {
    // The expected hash was computed by this same test on the G3.3 code
    // (commit d4e8080), before swingers existed, on the G3.3 demo scene:
    // it covers every tick of attacks that hit, stagger and hurt two
    // dummies, with their poses and hp bars.
    let mut game = Game::from_source(G3_3_DEMO).unwrap();
    let mut input = ActionState::new();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut staggers = 0;
    for t in 1..=450 {
        let (actions, axes) = g3_1_reference_input(t);
        input.advance(TickInput {
            actions: actions.into_iter().map(str::to_owned).collect(),
            axes: axes.into_iter().map(|(a, v)| (a.to_owned(), v)).collect(),
        });
        game.tick(&input);
        g3_3_fingerprint(&mut hash, &game);
        staggers += game
            .state()
            .hurtboxes
            .iter()
            .filter(|h| h.health.is_some_and(|x| x.state == Condition::Stagger))
            .count();
    }
    assert!(staggers > 0);
    let hits: Vec<_> = game.state().hurtboxes.iter().map(|h| h.hits).collect();
    assert!(hits[0] == 0 && hits[1] > 0 && hits[2] > 0, "{hits:?}");
    assert_eq!(hash, 17_394_125_667_144_692_205);
}

/// The demo scene as it was in G3.3, before the stake: tests whose
/// results the stake would change run on it.
const G3_3_DEMO: &str = include_str!("../g3_3_demo.scene.ron");

/// The demo scene as it was in G2.3, before the lock-on dummies:
/// tests whose results the dummies would change run on it.
const G2_3_DEMO: &str = include_str!("../g2_3_demo.scene.ron");

/// The demo scene with `player`'s line replaced.
fn demo_with(old: &str, new: &str) -> String {
    let src = include_str!("../demo.scene.ron");
    assert!(src.contains(old), "{old}");
    src.replace(old, new)
}

#[test]
fn unrotated_character_faces_positive_zero() {
    let game = Game::new(demo_scene()).unwrap();
    assert_eq!(game.state().player.facing.to_bits(), 0);
}

#[test]
fn rotated_character_starts_facing_its_rotation() {
    let src = demo_with(
        "position: (0.0, 0.0, 0.0),",
        "position: (0.0, 0.0, 0.0), rotation: (0.0, 90.0, 0.0),",
    );
    let game = Game::from_source(&src).unwrap();
    // Not bit-exact: `-f.z` is a tiny non-zero number here.
    assert!((game.state().player.facing - PI / 2.0).abs() <= 1e-6);
}

#[test]
fn game_rejects_a_scene_without_a_character() {
    // Without a character the orbit camera cannot exist either: drop both.
    let src = demo_with(
        "character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),",
        "",
    );
    let start = src.find(",\n            orbit_camera: (").unwrap();
    let end = src.find("return_speed: 4.0)").unwrap() + "return_speed: 4.0)".len();
    let src = format!("{}{}", &src[..start], &src[end..]);
    let err = Game::from_source(&src).unwrap_err();
    assert!(matches!(err, GameError::NoCharacter), "{err}");
    assert!(err.to_string().contains("character component"), "{err}");
}

#[test]
fn game_rejects_an_active_camera_without_orbit_camera() {
    let src = demo_with("camera: \"main_cam\",", "camera: \"still_cam\",").replace(
        "(name: \"sun\",",
        "(name: \"still_cam\", camera: (fov_y: 60.0, near: 0.1, far: 50.0)),\n        (name: \"sun\",",
    );
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::NoOrbitCamera(ref p) if p == "still_cam"),
        "{err}"
    );
    assert!(err.to_string().contains("orbit_camera"), "{err}");
}

#[test]
fn game_rejects_a_character_that_is_not_a_root_node() {
    // Move the whole `player` subtree under a new root `squad`.
    let src = include_str!("../demo.scene.ron");
    let start = src.find("        (name: \"player\"").unwrap();
    let end = src.find("        (name: \"pillars\"").unwrap();
    let player = &src[start..end];
    let src = format!(
        "{}        (name: \"squad\", children: [\n{}        ]),\n{}",
        &src[..start],
        player,
        &src[end..]
    )
    .replace("target: \"player\"", "target: \"squad/player\"");
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::NotRoot(ref p) if p == "squad/player"),
        "{err}"
    );
    assert!(err.to_string().contains("root node"), "{err}");
}

#[test]
fn game_rejects_an_orbit_camera_that_is_not_a_root_node() {
    let src = demo_with("camera: \"main_cam\",", "camera: \"rig/main_cam\",").replace(
        "        (name: \"main_cam\",",
        "        (name: \"rig\", children: [(name: \"main_cam\",",
    );
    // Close the extra `children: [(` after the camera node.
    let src = src.replace("return_speed: 4.0)),", "return_speed: 4.0))]),");
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::NotRoot(ref p) if p == "rig/main_cam"),
        "{err}"
    );
}

// ---- collision (G1.3)

#[test]
fn running_into_a_pillar_stops_in_front_of_it() {
    // On the G2.3 scene: dummy_b now stands in front of pillar c.
    let mut game = Game::from_source(G2_3_DEMO).unwrap();
    advance(&mut game, 150, |_| vec![(MOVE_Y, 1.0)]);
    // Pillar c's face is at z = -8.5; plus the radius 0.35.
    assert_player(&game, [0.0, 0.0, -8.15], 0.0);
}

#[test]
fn running_past_a_pillar_corner_slides_around_it() {
    let game = run(80, |_| vec![(MOVE_X, 1.0), (MOVE_Y, 1.0)]);
    assert_player(&game, [3.8095913, 0.0, -3.3155816], -FRAC_PI_4); // -0.7853982
}

#[test]
fn running_sideways_into_a_pillar_stops_at_its_face() {
    let game = run_from(Vec3::new(1.0, 0.0, -4.0), 30, |_| vec![(MOVE_X, 1.0)]);
    assert_player(&game, [2.15, 0.0, -4.0], -1.5707964);
}

#[test]
fn running_diagonally_along_a_pillar_slides_along_its_face() {
    let axes = |_| vec![(MOVE_X, 1.0), (MOVE_Y, 1.0)];
    let mut game = run_from(Vec3::new(1.0, 0.0, -3.0), 20, axes);
    // Not touching yet.
    assert_vec3(game.state().player.position, [1.9428096, 0.0, -3.9428072]);
    advance(&mut game, 10, axes);
    // x held at the face (2.5 - 0.35) while z keeps going.
    assert_vec3(game.state().player.position, [2.15, 0.0, -4.4142127]);
}

#[test]
fn running_into_the_slanted_wall_slides_along_it() {
    let game = run(50, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
    assert_player(&game, [-2.5300741, 0.0, 2.035868], 2.3561945);
}

#[test]
fn sliding_along_the_slanted_wall_keeps_the_center_on_its_face() {
    let mut game = run(39, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
    let wall = game.boxes[3];
    assert_eq!(
        game.scene().colliders()[3],
        game.scene().find("wall").unwrap()
    );
    for tick in 40..=60 {
        advance(&mut game, 1, |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)]);
        let p = game.state().player.position;
        let lb = (Vec2::new(p.x, p.z) - wall.center).dot(wall.b);
        // Half the wall's thickness (0.25) plus the radius (0.35).
        assert!((lb - (-0.6)).abs() <= 1e-6, "tick {tick}: lb = {lb}");
    }
}

#[test]
fn character_starting_inside_a_pillar_is_pushed_out_on_the_first_tick() {
    let start = Vec3::new(3.2, 0.0, -4.1);
    let game = Game::new({
        let mut scene = demo_scene();
        let player = scene.character().unwrap();
        scene.transform_mut(player).position = start;
        scene
    })
    .unwrap();
    // Not at load time.
    assert_eq!(game.state().player.position, start);
    let game = run_from(start, 1, |_| vec![]);
    // Out through the shallowest side, +X.
    assert_vec3(game.state().player.position, [3.85, 0.0, -4.1]);
}

#[test]
fn box_whose_top_is_at_the_feet_does_not_block() {
    let src = r#"(
            version: 1,
            camera: "cam",
            ambient: (0.2, 0.2, 0.2),
            nodes: [
                (name: "ground", mesh: Plane, scale: (40.0, 1.0, 40.0)),
                (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
                    character: (run_speed: 4.0, turn_speed: 720.0)),
                (name: "step", position: (0.0, -0.5, -2.0), scale: (2.0, 1.0, 2.0), collider: Box),
                (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 200.0),
                    orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                        yaw: 0.0, pitch: -15.0, pitch_min: -70.0, pitch_max: 20.0,
                        collision_radius: 0.2, return_speed: 4.0)),
            ],
        )"#;
    let mut game = Game::from_source(src).unwrap();
    assert_eq!(game.boxes[0].y_max, 0.0);
    advance(&mut game, 60, |_| vec![(MOVE_Y, 1.0)]);
    assert_vec3(game.state().player.position, [0.0, 0.0, -3.9999988]);
}

#[test]
fn camera_yaw_of_270_degrees_starts_wrapped() {
    let src = demo_with("yaw: 0.0, pitch: -15.0", "yaw: 270.0, pitch: -15.0");
    let game = Game::from_source(&src).unwrap();
    assert_f32(game.state().camera.yaw, -1.5707965);
}

#[test]
fn wrap_maps_plus_pi_to_minus_pi() {
    assert_f32(wrap(PI), -PI);
    assert_f32(wrap(-PI), -PI);
}

#[test]
fn game_rejects_a_character_without_a_capsule_mesh() {
    // The G3.3 scene still draws its character as a capsule.
    let old_form = |new: &str| {
        let old = "mesh: Capsule(radius: 0.35, height: 1.8),";
        assert!(G3_3_DEMO.contains(old));
        G3_3_DEMO.replace(old, new)
    };
    let src = old_form("mesh: Cube,");
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::NotACapsule(ref p) if p == "player"),
        "{err}"
    );
    assert!(err.to_string().contains("Capsule mesh"), "{err}");
    let src = old_form("");
    assert!(matches!(
        Game::from_source(&src),
        Err(GameError::NotACapsule(_))
    ));
}

#[test]
fn game_rejects_a_scaled_character() {
    let src = demo_with(
        "(name: \"player\", position: (0.0, 0.0, 0.0),",
        "(name: \"player\", position: (0.0, 0.0, 0.0), scale: (1.0, 2.0, 1.0),",
    );
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::ScaledCharacter(ref p) if p == "player"),
        "{err}"
    );
    assert!(err.to_string().contains("scaled"), "{err}");
}

#[test]
fn game_rejects_a_run_speed_too_fast_for_the_radius() {
    let src = demo_with("run_speed: 4.0,", "run_speed: 30.0,");
    let err = Game::from_source(&src).unwrap_err();
    assert!(matches!(err, GameError::TooFast { .. }), "{err}");
    assert_eq!(
        err.to_string(),
        "run_speed 30 is too fast for the capsule radius 0.35: \
             run_speed / 60 must be less than the radius"
    );
    // 21 / 60 = 0.35 exactly is still too fast; 4 (the demo) is fine.
    let src = demo_with("run_speed: 4.0,", "run_speed: 21.0,");
    assert!(matches!(
        Game::from_source(&src),
        Err(GameError::TooFast { .. })
    ));
    assert!(Game::new(demo_scene()).is_ok());
}

#[test]
fn game_rejects_a_collider_that_moves_with_the_player() {
    // The flask is the character's child.
    let src = demo_with(
        "mesh: Cube, color: (0.9, 0.7, 0.2)),",
        "mesh: Cube, color: (0.9, 0.7, 0.2), collider: Box),",
    );
    let err = Game::from_source(&src).unwrap_err();
    assert!(
        matches!(err, GameError::MovingCollider(ref p) if p == "player/flask"),
        "{err}"
    );
    assert!(
        err.to_string().contains("colliders must be static"),
        "{err}"
    );
}

// ---- camera collision (G1.7)

/// Pillar c is between the camera and a player starting here.
const BEHIND_PILLAR_C: Vec3 = Vec3::new(0.0, 0.0, -11.0);

#[test]
fn camera_returns_to_its_distance_at_return_speed() {
    let axes = |t| match t {
        1..=30 => vec![(LOOK_Y, 0.04)],
        31..=45 => vec![(LOOK_Y, -0.08)],
        _ => vec![],
    };
    let mut game = run(45, axes);
    assert_f32(game.state().camera.distance, 2.6119118);
    advance(&mut game, 5, axes);
    assert_f32(game.state().camera.distance, 2.9452448);
    assert_vec3(
        node(&game, "main_cam").position,
        [0.0, 2.2622848, 2.8448882],
    );
    advance(&mut game, 20, axes);
    assert_f32(game.state().camera.distance, 4.0);
    assert_f32(game.state().camera.pitch, -0.26179916);
    assert_vec3(
        node(&game, "main_cam").position,
        [0.0, 2.5352755, 3.8637037],
    );
}

#[test]
fn pillar_between_camera_and_player_pulls_the_camera_in_before_the_first_tick() {
    let game = run_from(BEHIND_PILLAR_C, 0, |_| vec![]);
    assert_eq!(game.clock().tick(), 0);
    assert_f32(game.state().camera.distance, 1.3458589);
    // Pillar c's face is at z = -9.5; plus the collision radius 0.2.
    assert_vec3(node(&game, "main_cam").position, [0.0, 1.848334, -9.7]);
}

#[test]
fn camera_stays_in_while_blocked_and_returns_after() {
    let axes = |_| vec![(MOVE_X, 1.0)];
    let mut game = run_from(BEHIND_PILLAR_C, 1, axes);
    assert_f32(game.state().camera.distance, 1.3458589);
    advance(&mut game, 9, axes);
    assert_f32(game.state().camera.distance, 1.3458589);
    assert_f32(game.state().player.position.x, 0.6666667);
    advance(&mut game, 10, axes);
    assert_f32(game.state().camera.distance, 2.012526);
    advance(&mut game, 20, axes);
    assert_f32(game.state().camera.distance, 3.345858);
    assert_vec3(
        node(&game, "main_cam").position,
        [2.6666667, 2.3659718, -7.7681494],
    );
}

#[test]
// 1.414214 is a position 4 ulps away from SQRT_2 (0x3FB504F7 vs
// 0x3FB504F3), not √2.
#[allow(clippy::approx_constant)]
fn backing_into_the_slanted_wall_keeps_the_camera_in_front_of_it() {
    let axes = |_| vec![(MOVE_X, -1.0), (MOVE_Y, -1.0)];
    let mut game = run(30, axes);
    assert_vec3(game.state().player.position, [-1.414214, 0.0, 1.414214]);
    assert_f32(game.state().camera.distance, 0.15592998);
    assert_vec3(
        node(&game, "main_cam").position,
        [-1.414214, 1.5403577, 1.5648308],
    );
    advance(&mut game, 30, axes);
    assert_vec3(game.state().player.position, [-3.0877507, 0.0, 2.3578424]);
    assert_f32(game.state().camera.distance, 0.17931521);
    assert_vec3(
        node(&game, "main_cam").position,
        [-3.0877507, 1.5464102, 2.5310476],
    );
}

#[test]
fn funnel_needs_more_than_one_push_round() {
    let src = demo_with(
        "        (name: \"sun\",",
        "        (name: \"funnel_l\", position: (-0.7, 1.0, -3.0), rotation: (0.0, 20.0, 0.0), \
             scale: (0.6, 2.0, 4.0), mesh: Cube, collider: Box),\n\
             (name: \"funnel_r\", position: (0.7, 1.0, -3.0), rotation: (0.0, -20.0, 0.0), \
             scale: (0.6, 2.0, 4.0), mesh: Cube, collider: Box),\n        (name: \"sun\",",
    );
    let mut game = Game::from_source(&src).unwrap();
    advance(&mut game, 30, |_| vec![(MOVE_Y, 1.0)]);
    assert_player(&game, [0.03972316, 0.0, -0.7395032], 0.0);
}
