//! Tests of the character's clips (`docs/architecture.md` section 11,
//! "模型和动画"): the two forms of the character node, which clip plays
//! when, and the model's colors. The character stands at the origin
//! facing -Z with the camera behind it; the defaults hold unless a test
//! says otherwise.

use glam::{Vec2, Vec3};
use loom_core::input::{ActionState, TickInput};
use loom_scene::{Animation, ClipTime};

use super::clips::{CLIPS, choose};
use super::show::{DEAD_DIM, FLASH_TICKS, FLASH_TINT};
use super::state::PlayerState;
use super::*;

/// The animation of a character drawn by a model, every clip declared.
fn animation() -> String {
    let clips: Vec<String> = CLIPS
        .iter()
        .map(|c| format!(r#"("{c}", "assets/anims/humanoid/{c}.ron")"#))
        .collect();
    format!(
        r#"animation: (clips: [{}], play: "idle")"#,
        clips.join(", ")
    )
}

/// The character drawn by `form` with `player` props, then `nodes`.
fn scene_of(form: &str, player: &str, nodes: &str) -> String {
    format!(
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", {form} props: [{player}]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
        {nodes}
    ],
)"#
    )
}

/// The character drawn by the humanoid model.
fn modeled(player: &str, nodes: &str) -> String {
    scene_of(
        &format!(
            r#"model: "assets/models/humanoid.glb", {},
            character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),"#,
            animation()
        ),
        player,
        nodes,
    )
}

/// A lock target 5 m ahead.
const TARGET: &str = r#"(name: "t", position: (0.0, 0.0, -5.0), props: [("lock_height", 1.0)]),"#;

/// A stake 2 m behind swinging all around once, hitting the character
/// for `damage` on tick 5.
fn stake(damage: u32) -> String {
    format!(
        r#"(name: "stake", position: (0.0, 0.0, 2.0), props: [("swing_damage", {damage}),
        ("swing_startup_ticks", 4), ("swing_active_ticks", 1), ("swing_recovery_ticks", 1000),
        ("swing_arc", 360), ("swing_reach", 10)]),"#
    )
}

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    fn new(player: &str, nodes: &str) -> Self {
        Self {
            game: Game::from_source(&modeled(player, nodes)).unwrap(),
            input: ActionState::new(),
        }
    }

    /// One tick with exactly `actions` held and `axes` pushed; what the
    /// character's model plays after it.
    fn hold(&mut self, actions: &[&str], axes: &[(&str, f32)]) -> Animation {
        self.input.advance(TickInput {
            actions: actions.iter().map(|&a| a.to_owned()).collect(),
            axes: axes.iter().map(|&(a, v)| (a.to_owned(), v)).collect(),
        });
        self.game.tick(&self.input);
        self.animation()
    }

    fn idle(&mut self) -> Animation {
        self.hold(&[], &[])
    }

    fn animation(&self) -> Animation {
        let player = self.game.player;
        let model = self.game.scene.node(player).model.clone().unwrap();
        model.animation.unwrap()
    }

    fn tick(&self) -> u64 {
        self.game.clock().tick()
    }

    fn player(&self) -> &PlayerState {
        &self.game.state.player
    }

    /// Taps the roll key while pushing `axes`: the roll or backstep starts
    /// on the release; what plays then.
    fn tap_roll(&mut self, axes: &[(&str, f32)]) -> Animation {
        self.hold(&[ROLL], axes);
        let a = self.hold(&[], axes);
        assert_eq!(self.player().action_tick, 1);
        a
    }

    /// Presses `key` once (after a tick without it) and plays the attack
    /// out; the animation on each tick, the first after the press.
    fn attack(&mut self, key: &str) -> Vec<Animation> {
        let mut seen = vec![self.hold(&[key], &[])];
        while self.player().action != Action::Idle {
            seen.push(self.idle());
        }
        seen
    }
}

fn time(clip: &str, start: u64, offset: u64, rate: u32) -> ClipTime {
    ClipTime {
        clip: clip.to_owned(),
        start,
        offset,
        rate,
    }
}

// ------------------------------------------------------------------ forms

#[test]
fn modeled_character_collides_with_its_character_capsule() {
    let game = Game::from_source(&modeled("", "")).unwrap();
    assert_eq!((game.radius, game.height), (0.35, 1.8));
    assert!(game.clips.is_some());
    let old = Game::from_source(&scene_of(
        "mesh: Capsule(radius: 0.3, height: 1.7), \
         character: (run_speed: 4.0, turn_speed: 720.0),",
        "",
        "",
    ))
    .unwrap();
    assert_eq!((old.radius, old.height), (0.3, 1.7));
    assert!(old.clips.is_none());
}

/// The load error of the character drawn by `form`.
fn form_error(form: &str) -> String {
    Game::from_source(&scene_of(form, "", ""))
        .unwrap_err()
        .to_string()
}

#[test]
fn a_model_without_character_capsule_is_rejected() {
    assert_eq!(
        form_error(&format!(
            r#"model: "m.glb", {}, character: (run_speed: 4.0, turn_speed: 720.0),"#,
            animation()
        )),
        r#"the character "player" has a model, so it needs character.capsule: the collision shape"#
    );
}

#[test]
fn a_character_capsule_beside_a_mesh_is_rejected() {
    assert_eq!(
        form_error(
            "mesh: Capsule(radius: 0.35, height: 1.8), \
             character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),"
        ),
        r#"the character "player" has both a mesh and character.capsule: the collision shape is given twice"#
    );
}

#[test]
fn a_model_without_animation_is_rejected() {
    assert_eq!(
        form_error(
            r#"model: "m.glb", character: (run_speed: 4.0, turn_speed: 720.0,
               capsule: (radius: 0.35, height: 1.8)),"#
        ),
        r#"the character "player" has a model, so it needs an animation: the game plays its clips"#
    );
}

#[test]
fn a_character_capsule_without_a_model_is_rejected() {
    assert_eq!(
        form_error(
            "character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),"
        ),
        r#"the character "player" has character.capsule, so it needs a model: the capsule is not drawn"#
    );
}

#[test]
fn an_animation_missing_one_of_the_twelve_clips_is_rejected() {
    let form = format!(
        r#"model: "m.glb", {},
           character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),"#,
        animation().replace(
            r#"("run_back", "assets/anims/humanoid/run_back.ron"), "#,
            ""
        )
    );
    assert_eq!(
        form_error(&form),
        r#"the character "player": its animation needs a clip "run_back""#
    );
}

#[test]
fn a_modeled_character_too_fast_for_its_capsule_is_rejected() {
    let src = modeled("", "").replace("capsule: (radius: 0.35", "capsule: (radius: 0.05");
    assert!(matches!(
        Game::from_source(&src),
        Err(GameError::TooFast { radius, .. }) if radius == 0.05
    ));
}

// ------------------------------------------------------------ roll, backstep

#[test]
fn roll_plays_from_its_first_tick_blended_over_two_ticks() {
    let mut run = Run::new("", "");
    let a = run.tap_roll(&[(MOVE_Y, 1.0)]);
    assert_eq!(run.player().action, Action::Roll);
    let now = run.tick();
    assert_eq!(a.playing, "roll");
    assert_eq!((a.start, a.offset, a.rate), (now, 0, 100));
    assert_eq!((a.blend_start, a.blend), (now, 2));
    // The tick the key went down it ran.
    assert_eq!(a.from, Some(time("run", now - 1, 0, 100)));
    // The rest of the roll leaves it alone.
    let later = run.hold(&[], &[(MOVE_Y, 1.0)]);
    assert_eq!(later, a);
}

#[test]
fn slower_roll_plays_its_clip_slower() {
    let mut run = Run::new(r#"("roll_ticks", 64), ("roll_invuln_to", 20)"#, "");
    let a = run.tap_roll(&[(MOVE_Y, 1.0)]);
    assert_eq!((a.playing.as_str(), a.rate), ("roll", 50));
}

#[test]
fn backstep_plays_its_clip_and_idle_follows_with_a_six_tick_blend() {
    let mut run = Run::new("", "");
    let a = run.tap_roll(&[]);
    assert_eq!(run.player().action, Action::Backstep);
    assert_eq!((a.playing.as_str(), a.rate, a.blend), ("backstep", 100, 2));
    while run.player().action == Action::Backstep {
        run.idle();
    }
    let a = run.animation();
    assert_eq!(
        (a.playing.as_str(), a.start, a.blend),
        ("idle", run.tick(), 6)
    );
    assert_eq!(a.from.map(|f| f.clip), Some("backstep".to_owned()));
}

// ----------------------------------------------------------------- attacks

/// The animations on the first tick of each part of an attack, by the
/// character's `action_tick` on the ticks of `seen`.
fn part_starts(seen: &[Animation], firsts: [usize; 3]) -> Vec<&Animation> {
    firsts.iter().map(|&t| &seen[t - 1]).collect()
}

#[test]
fn light_attack_plays_its_three_parts_from_their_offsets() {
    let mut run = Run::new("", "");
    let first = run.tick() + 1;
    let seen = run.attack(ATTACK);
    assert_eq!(seen.len(), 30);
    let parts = part_starts(&seen, [1, 11, 15]);
    for (i, (a, (offset, blend))) in parts.iter().zip([(0, 3), (1000, 0), (1400, 0)]).enumerate() {
        assert_eq!(a.playing, "attack1", "part {i}");
        assert_eq!(
            (a.offset, a.rate, a.blend),
            (offset, 100, blend),
            "part {i}"
        );
        let start = first + [0, 10, 14][i];
        assert_eq!((a.start, a.blend_start), (start, start), "part {i}");
    }
    // Between the parts nothing is called: tick 2 is tick 1's state.
    assert_eq!(seen[1], seen[0]);
    // Tick k of the attack shows clip time (k - 1) × 100: 2800 on the
    // second-last; on the last the attack is over, and idle blends in over
    // 6 ticks.
    assert_eq!(seen[28].time_at(first + 28), 2800);
    let a = &seen[29];
    assert_eq!(
        (a.playing.as_str(), a.start, a.blend),
        ("idle", first + 29, 6)
    );
}

#[test]
fn slower_startup_plays_the_startup_at_half_speed_only() {
    let mut run = Run::new(r#"("attack_startup_ticks", 20)"#, "");
    let seen = run.attack(ATTACK);
    let parts = part_starts(&seen, [1, 21, 25]);
    let rates: Vec<u32> = parts.iter().map(|a| a.rate).collect();
    assert_eq!(rates, [50, 100, 100]);
    let offsets: Vec<u64> = parts.iter().map(|a| a.offset).collect();
    assert_eq!(offsets, [0, 1000, 1400]);
}

#[test]
fn heavy_attack_plays_heavy_from_its_three_offsets() {
    let mut run = Run::new("", "");
    let seen = run.attack(HEAVY);
    assert_eq!(seen.len(), 50);
    let parts = part_starts(&seen, [1, 23, 28]);
    let got: Vec<(&str, u64, u64)> = parts
        .iter()
        .map(|a| (a.playing.as_str(), a.offset, a.blend))
        .collect();
    assert_eq!(
        got,
        [("heavy", 0, 3), ("heavy", 2200, 0), ("heavy", 2700, 0)]
    );
}

#[test]
fn combo_plays_attack2_second_and_attack3_from_the_third_on() {
    let mut run = Run::new(r#"("combo_max", 4), ("attack_cost", 1)"#, "");
    // Each press after the first is buffered in the recovery; the clip
    // on the first tick of each attack.
    let mut clips = Vec::new();
    for tick in 0..200 {
        let keys: &[&str] = if tick % 20 == 0 { &[ATTACK] } else { &[] };
        let a = run.hold(keys, &[]);
        let p = run.player();
        if p.action == Action::Attack && p.action_tick == 1 {
            clips.push((p.combo, a.playing));
        }
    }
    let clips: Vec<_> = clips
        .iter()
        .take(4)
        .map(|(c, a)| (*c, a.as_str()))
        .collect();
    assert_eq!(
        clips,
        [
            (1, "attack1"),
            (2, "attack2"),
            (3, "attack3"),
            (4, "attack3")
        ]
    );
}

// --------------------------------------------------------------- movement

#[test]
fn running_at_full_input_plays_run_at_its_written_speed_calling_once() {
    let mut run = Run::new("", "");
    let a = run.hold(&[], &[(MOVE_Y, 1.0)]);
    let now = run.tick();
    assert_eq!((a.playing.as_str(), a.rate), ("run", 100));
    assert_eq!((a.start, a.blend_start, a.blend), (now, now, 6));
    for _ in 0..30 {
        assert_eq!(run.hold(&[], &[(MOVE_Y, 1.0)]), a);
    }
}

#[test]
fn half_input_runs_the_clip_at_half_speed_and_changes_speed_without_restarting() {
    let mut run = Run::new("", "");
    let full = run.hold(&[], &[(MOVE_Y, 1.0)]);
    for _ in 0..3 {
        run.hold(&[], &[(MOVE_Y, 1.0)]);
    }
    let half = run.hold(&[], &[(MOVE_Y, 0.5)]);
    let now = run.tick();
    assert_eq!((half.playing.as_str(), half.rate), ("run", 50));
    // set_clip_rate: on from the clip time it had, the blend untouched.
    assert_eq!((half.start, half.offset), (now, 4 * 100));
    assert_eq!(
        (half.blend_start, half.blend),
        (full.blend_start, full.blend)
    );
}

#[test]
fn locked_on_moves_left_right_and_back_play_their_own_clips() {
    let mut run = Run::new("", TARGET);
    run.hold(&[LOCK], &[]);
    assert!(run.game.lock.is_some());
    let mut clips = Vec::new();
    for axes in [[(MOVE_X, -1.0)], [(MOVE_X, 1.0)], [(MOVE_Y, -1.0)]] {
        for _ in 0..3 {
            run.hold(&[], &axes);
        }
        clips.push(run.animation().playing);
        run.idle();
    }
    assert_eq!(clips, ["run_left", "run_right", "run_back"]);
}

#[test]
fn a_move_as_much_forward_as_sideways_picks_run() {
    let game = Game::from_source(&modeled("", "")).unwrap();
    let clips = game.clips.unwrap();
    let player = game.state.player.clone();
    // Facing -Z: forward is -Z, right is +X.
    let pick =
        |dir: Vec3| choose(&clips, &game.params, &player, Condition::Ok, Vec2::ONE, dir).clip;
    assert_eq!(pick(Vec3::new(0.5, 0.0, -0.5)), "run");
    assert_eq!(pick(Vec3::new(-0.5, 0.0, -0.5)), "run");
    assert_eq!(pick(Vec3::new(0.5, 0.0, 0.5)), "run_right");
    assert_eq!(pick(Vec3::new(-0.5, 0.0, 0.5)), "run_left");
    assert_eq!(pick(Vec3::new(0.4, 0.0, 0.5)), "run_back");
    // Staggered or dead: idle, whatever the input.
    let staggered = choose(
        &clips,
        &game.params,
        &player,
        Condition::Stagger,
        Vec2::ONE,
        Vec3::NEG_Z,
    );
    assert_eq!((staggered.clip, staggered.rate), ("idle", 100));
}

#[test]
fn sprint_plays_sprint_at_its_written_speed() {
    let mut run = Run::new("", "");
    let mut a = run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    while run.player().action != Action::Sprint {
        a = run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    }
    assert_eq!((a.playing.as_str(), a.rate, a.blend), ("sprint", 100, 6));
    assert_eq!(a.from.map(|f| f.clip), Some("run".to_owned()));
}

#[test]
fn faster_sprint_speeds_the_sprint_clip_up_rounded() {
    let mut run = Run::new(r#"("sprint_speed", 7)"#, "");
    let mut a = run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    while run.player().action != Action::Sprint {
        a = run.hold(&[ROLL], &[(MOVE_Y, 1.0)]);
    }
    // 100 × 7 / 6 = 116.67.
    assert_eq!(a.rate, 117);
}

// ----------------------------------------------------------------- colors

#[test]
fn modeled_character_flashes_red_when_hit_and_darkens_dead() {
    let mut run = Run::new("", &stake(10));
    let color = |run: &Run| {
        run.game
            .scene
            .node(run.game.player)
            .model
            .as_ref()
            .unwrap()
            .color
    };
    let mut hit = None;
    for _ in 0..10 {
        run.idle();
        if run.player().last_hit.is_some() && hit.is_none() {
            hit = Some(run.tick());
        }
        if let Some(at) = hit {
            let want = if run.tick() - at < FLASH_TICKS {
                FLASH_TINT
            } else {
                Vec3::ONE
            };
            assert_eq!(color(&run), want, "tick {}", run.tick());
        }
    }
    assert!(hit.is_some());
    let mut run = Run::new("", &stake(10_000));
    for _ in 0..6 {
        run.idle();
    }
    assert!(run.game.health.is_dead());
    assert_eq!(color(&run), Vec3::ONE * DEAD_DIM);
    // Dead: idle plays, whatever the input.
    assert_eq!(run.hold(&[], &[(MOVE_Y, 1.0)]).playing, "idle");
}
