//! Tests of the character's clips (`docs/architecture.md` section 11,
//! "模型和动画"): the two forms of the character node, which clip plays
//! when, and the model's colors. The character stands at the origin
//! facing -Z with the camera behind it; the defaults hold unless a test
//! says otherwise.

use glam::{Vec2, Vec3};
use loom_core::input::{ActionState, TickInput};
use loom_scene::{Animation, ClipTime};

use super::clips::{CLIPS, choose};
use super::show::{DEAD_DIM, DEAD_LIFT, FLASH_TICKS, FLASH_TINT, front_or_back};
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
    stake_at((0.0, 2.0), damage, 4, 1000)
}

/// A stake at `(x, 0, z)` swinging all around, hitting the character for
/// `damage` (and the default impact, which staggers it) on tick
/// `startup + 1` of every round of `startup + 1 + recovery` ticks.
fn stake_at((x, z): (f32, f32), damage: u32, startup: u32, recovery: u32) -> String {
    format!(
        r#"(name: "stake", position: ({x:?}, 0.0, {z:?}), props: [("swing_damage", {damage}),
        ("swing_startup_ticks", {startup}), ("swing_active_ticks", 1),
        ("swing_recovery_ticks", {recovery}), ("swing_arc", 360), ("swing_reach", 10)]),"#
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

    /// One tick with exactly `actions` held and `axes` pushed.
    fn step(&mut self, actions: &[&str], axes: &[(&str, f32)]) {
        self.input.advance(TickInput {
            actions: actions.iter().map(|&a| a.to_owned()).collect(),
            axes: axes.iter().map(|&(a, v)| (a.to_owned(), v)).collect(),
        });
        self.game.tick(&self.input);
    }

    /// [`Run::step`]; what the character's model plays after it.
    fn hold(&mut self, actions: &[&str], axes: &[(&str, f32)]) -> Animation {
        self.step(actions, axes);
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
fn an_animation_missing_one_of_the_sixteen_clips_is_rejected() {
    assert_eq!(CLIPS.len(), 16);
    for clip in ["run_back", "drink"] {
        let form = format!(
            r#"model: "m.glb", {},
               character: (run_speed: 4.0, turn_speed: 720.0, capsule: (radius: 0.35, height: 1.8)),"#,
            animation().replace(
                &format!(r#"("{clip}", "assets/anims/humanoid/{clip}.ron"), "#),
                ""
            )
        );
        assert_eq!(
            form_error(&form),
            format!(r#"the character "player": its animation needs a clip "{clip}""#)
        );
    }
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
    let pick = |dir: Vec3| choose(&clips, &game.params, &player, &game.health, Vec2::ONE, dir).clip;
    assert_eq!(pick(Vec3::new(0.5, 0.0, -0.5)), "run");
    assert_eq!(pick(Vec3::new(-0.5, 0.0, -0.5)), "run");
    assert_eq!(pick(Vec3::new(0.5, 0.0, 0.5)), "run_right");
    assert_eq!(pick(Vec3::new(-0.5, 0.0, 0.5)), "run_left");
    assert_eq!(pick(Vec3::new(0.4, 0.0, 0.5)), "run_back");
    // Staggered or dead: their own clips, whatever the input.
    for (state, clip) in [(Condition::Stagger, "stagger"), (Condition::Dead, "death")] {
        let mut health = game.health;
        health.now.state = state;
        health.now.state_ticks = 10;
        let c = choose(
            &clips,
            &game.params,
            &player,
            &health,
            Vec2::ONE,
            Vec3::NEG_Z,
        );
        assert_eq!((c.clip, c.rate, c.restart), (clip, 100, None));
    }
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
    // Dead: death plays, whatever the input.
    assert_eq!(run.hold(&[], &[(MOVE_Y, 1.0)]).playing, "death");
}

// ------------------------------------------------ stagger, death, drink, sit

/// A bonfire 1 m ahead, in reach.
const BONFIRE: &str =
    r#"(name: "bonfire", position: (0.0, 0.0, -1.0), props: [("bonfire_range", 1.5)]),"#;

#[test]
fn death_plays_from_the_tick_of_the_death_and_idle_follows_the_respawn() {
    let mut run = Run::new("", &stake(10_000));
    let mut a = run.idle();
    while run.tick() < 5 {
        assert_eq!(a.playing, "idle");
        a = run.idle();
    }
    // Killed on tick 5.
    assert!(run.game.health.is_dead());
    assert_eq!(a.playing, "death");
    assert_eq!((a.start, a.offset, a.rate), (5, 0, 100));
    assert_eq!((a.blend_start, a.blend), (5, 3));
    assert_eq!(a.from, Some(time("idle", 0, 0, 100)));
    // Lying dead, pushed or not, nothing more is called.
    for _ in 0..10 {
        assert_eq!(run.hold(&[], &[(MOVE_Y, 1.0)]), a);
    }
    let mut b = run.idle();
    while run.game.health.is_dead() {
        assert_eq!(b, a);
        b = run.idle();
    }
    // The respawn, 120 ticks (the default respawn_ticks) later.
    assert_eq!(run.tick(), 125);
    assert_eq!((b.playing.as_str(), b.start, b.blend), ("idle", 125, 6));
    assert_eq!(b.from, Some(time("death", 5, 0, 100)));
}

#[test]
fn every_stagger_restarts_the_stagger_clip_stretched_to_stagger_ticks() {
    // Hits, each staggering, on ticks 5 and 19: the second while still
    // staggered by the first.
    let nodes = stake_at((0.0, 2.0), 1, 4, 9);
    for (props, rate) in [("", 100), (r#"("stagger_ticks", 60)"#, 50)] {
        let mut run = Run::new(props, &nodes);
        let mut starts = Vec::new();
        for _ in 0..25 {
            let a = run.idle();
            if run.game.health.now.state == Condition::Stagger && a.start == run.tick() {
                starts.push((a.start, a.offset, a.rate, a.blend, a.from.unwrap().clip));
            }
        }
        assert_eq!(
            starts,
            [
                (5, 0, rate, 2, "idle".to_owned()),
                (19, 0, rate, 2, "stagger".to_owned())
            ],
            "{props}"
        );
    }
}

/// Presses the drink key and drinks to the end; the animation on each
/// tick, the first the press.
fn drink(run: &mut Run) -> Vec<Animation> {
    let mut seen = vec![run.hold(&[DRINK], &[])];
    assert_eq!(
        (run.player().action, run.player().action_tick),
        (Action::Drink, 1)
    );
    while run.player().action == Action::Drink {
        seen.push(run.idle());
    }
    seen
}

#[test]
fn drink_plays_its_two_parts_from_their_offsets_stretched_to_the_heal_tick() {
    let mut run = Run::new(r#"("flask_heal_tick", 20), ("flask_ticks", 60)"#, "");
    let first = run.tick() + 1;
    let seen = drink(&mut run);
    assert_eq!(seen.len(), 60);
    // Tick 1 lifts at 100 × 30 / 20; tick 21, after the heal, drinks and
    // lowers at 100 × 30 / 40 from clip time 30.
    let (lift, lower) = (&seen[0], &seen[20]);
    assert_eq!(
        (lift.playing.as_str(), lift.start, lift.offset, lift.rate),
        ("drink", first, 0, 150)
    );
    assert_eq!((lift.blend_start, lift.blend), (first, 4));
    assert_eq!(
        (
            lower.playing.as_str(),
            lower.start,
            lower.offset,
            lower.rate
        ),
        ("drink", first + 20, 3000, 75)
    );
    assert_eq!((lower.blend_start, lower.blend), (first + 20, 0));
    // Inside a part nothing is called; the heal tick shows 19 × 150.
    assert!(seen[1..20].iter().all(|a| a == lift));
    assert!(seen[21..59].iter().all(|a| a == lower));
    assert_eq!(seen[19].time_at(first + 19), 2850);
    // On the last tick the drink is over and idle blends in.
    assert_eq!((seen[59].playing.as_str(), seen[59].blend), ("idle", 6));
}

#[test]
fn drink_healing_on_its_last_tick_plays_a_single_part() {
    let mut run = Run::new(r#"("flask_heal_tick", 60), ("flask_ticks", 60)"#, "");
    let seen = drink(&mut run);
    assert_eq!(seen.len(), 60);
    let a = &seen[0];
    assert_eq!(
        (a.playing.as_str(), a.offset, a.rate, a.blend),
        ("drink", 0, 50, 4)
    );
    assert!(seen[1..59].iter().all(|b| b == a));
    assert_eq!(seen[59].playing, "idle");
}

#[test]
fn sit_plays_from_the_tick_the_character_sits_down() {
    let mut run = Run::new("", BONFIRE);
    run.idle();
    let a = run.hold(&[INTERACT], &[]);
    let now = run.tick();
    assert_eq!(run.player().action, Action::Sit);
    assert_eq!(
        (a.playing.as_str(), a.start, a.offset, a.rate),
        ("sit", now, 0, 100)
    );
    assert_eq!((a.blend_start, a.blend), (now, 6));
    for _ in 0..60 {
        assert_eq!(run.idle(), a);
    }
    // A push stands it up, and it runs from there.
    let b = run.hold(&[], &[(MOVE_Y, 1.0)]);
    assert_eq!(run.player().action, Action::Idle);
    assert_eq!(
        (b.playing.as_str(), b.start, b.blend),
        ("run", run.tick(), 6)
    );
}

#[test]
fn modeled_character_sits_without_sinking() {
    let mut run = Run::new("", BONFIRE);
    run.hold(&[INTERACT], &[]);
    assert_eq!(run.player().action, Action::Sit);
    let node = run.game.scene.transform(run.game.player);
    assert_eq!(node.position, Vec3::ZERO);
}

#[test]
fn a_hit_while_drinking_plays_stagger_or_death_instead_of_drink() {
    // The stake hits on tick 11, the drink's tick 11.
    for (damage, clip, blend) in [(1, "stagger", 2), (10_000, "death", 3)] {
        let mut run = Run::new("", &stake_at((0.0, 2.0), damage, 10, 1000));
        let mut a = run.hold(&[DRINK], &[]);
        assert_eq!(a.playing, "drink");
        while run.tick() < 11 {
            a = run.idle();
        }
        assert_eq!(run.player().action, Action::Idle);
        assert_eq!((a.playing.as_str(), a.start, a.blend), (clip, 11, blend));
        assert_eq!(a.from.map(|f| f.clip), Some("drink".to_owned()));
    }
}

#[test]
fn dead_modeled_character_lies_on_its_face_or_back_lifted_whatever_side_it_was_hit_from() {
    // Facing -Z, killed on tick 5 by a stake behind, in front, on the
    // right a little ahead and on the left a little behind.
    for (at, face_down) in [
        ((0.0, 2.0), true),
        ((0.0, -2.0), false),
        ((2.0, -0.3), false),
        ((-2.0, 0.3), true),
    ] {
        let mut run = Run::new("", &stake_at(at, 10_000, 4, 1000));
        for _ in 0..5 {
            run.idle();
        }
        assert!(run.game.health.is_dead(), "{at:?}");
        let node = run.game.scene.transform(run.game.player);
        assert_eq!(node.position, Vec3::new(0.0, DEAD_LIFT, 0.0), "{at:?}");
        // Its shoulders stay level; its head points ahead, lying on its
        // face, or behind, lying on its back.
        let (right, up) = (node.rotation * Vec3::X, node.rotation * Vec3::Y);
        assert!(right.abs_diff_eq(Vec3::X, 1e-6), "{at:?}: {right}");
        let head = if face_down { Vec3::NEG_Z } else { Vec3::Z };
        assert!(up.abs_diff_eq(head, 1e-6), "{at:?}: {up}");
    }
    // A capsule still tips over away from the hit, from where it stands.
    let mut run = Run {
        game: Game::from_source(&scene_of(
            "mesh: Capsule(radius: 0.35, height: 1.8), \
             character: (run_speed: 4.0, turn_speed: 720.0),",
            "",
            &stake_at((2.0, -0.3), 10_000, 4, 1000),
        ))
        .unwrap(),
        input: ActionState::new(),
    };
    for _ in 0..5 {
        run.step(&[], &[]);
    }
    assert!(run.game.health.is_dead());
    let node = run.game.scene.transform(run.game.player);
    assert_eq!(node.position, Vec3::ZERO);
    let away = Vec3::new(-2.0, 0.0, 0.3).normalize();
    let up = node.rotation * Vec3::Y;
    assert!(up.abs_diff_eq(away, 1e-6), "{up}");
}

#[test]
fn front_or_back_picks_the_side_nearer_to_the_fall() {
    use std::f32::consts::PI;
    let facing = 1.0;
    // 80 degrees either side of the front, the back, then 103 degrees.
    for (fall, want) in [
        (1.0, 1.0),
        (1.0 + 1.4, 1.0),
        (1.0 - 1.4, 1.0),
        (1.0 + PI, 1.0 + PI),
        (1.0 + 1.8, 1.0 + PI),
        (1.0 - 1.8, 1.0 + PI),
    ] {
        assert_eq!(front_or_back(fall, facing), want, "{fall}");
    }
}
