//! Tests of the HUD and the health bar's trail (`docs/architecture.md`
//! section 11, "HUD"), a child of `game` so they can read and set its own
//! fields. The character stands at the origin facing -Z with the camera
//! behind it: 100 hp and 100 stamina, so both bars are 300 long.

use glam::{Mat4, Vec2, Vec3, Vec4};
use loom_core::hud::{Hud, HudItem, HudView};
use loom_core::input::{ActionState, TickInput};

use super::hud::*;
use super::*;

/// The character with `player` props, then `nodes`.
fn scene(player: &str, nodes: &str) -> String {
    format!(
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.2),
    nodes: [
        (name: "player", mesh: Capsule(radius: 0.35, height: 1.8),
            character: (run_speed: 3.75, turn_speed: 720.0), props: [{player}]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0),
            orbit_camera: (target: "player", pivot_height: 1.5, distance: 4.0,
                yaw: 0.0, pitch: 0.0, pitch_min: -60.0, pitch_max: 60.0,
                collision_radius: 0.2, return_speed: 4.0)),
        {nodes}
    ],
)"#
    )
}

/// An enemy `name` at `(x, 0, z)` with 200 hp and an aggro range of 5,
/// with `boss_bar` 1 when `boss`.
fn enemy(name: &str, x: f32, z: f32, boss: bool) -> String {
    let bar = if boss { r#", ("boss_bar", 1)"# } else { "" };
    format!(
        r#"(name: "{name}", position: ({x:?}, 0.0, {z:?}), props: [("lock_height", 1.5),
        ("hurt_radius", 0.5), ("hurt_height", 2), ("hp", 200), ("enemy_speed", 0.6),
        ("enemy_aggro_range", 5.0), ("enemy_face_angle", 1),
        ("approach_weight_per_meter", 0), ("lunge_distance", 0.2){bar}]),"#
    )
}

/// A stake 2 m ahead swinging all around, hitting for 30 on tick `at` and
/// every 1000 ticks after.
fn stake(at: u32) -> String {
    format!(
        r#"(name: "stake", position: (0.0, 0.0, -2.0), props: [("swing_damage", 30),
        ("swing_startup_ticks", {}), ("swing_active_ticks", 1), ("swing_recovery_ticks", 998),
        ("swing_arc", 360), ("swing_reach", 10)]),"#,
        at - 1
    )
}

/// A 1280x720 view with no camera: only the lock-on dot projects.
fn view() -> HudView {
    HudView::new(1280, 720, Mat4::IDENTITY)
}

struct Run {
    game: Game,
    input: ActionState,
}

impl Run {
    fn new(player: &str, nodes: &str) -> Self {
        Self {
            game: Game::from_source(&scene(player, nodes)).unwrap(),
            input: ActionState::new(),
        }
    }

    fn hold(&mut self, actions: &[&str]) -> PlayerState {
        self.input.advance(TickInput {
            actions: actions.iter().map(|&a| a.to_owned()).collect(),
            axes: Default::default(),
        });
        self.game.tick(&self.input);
        self.game.state().player
    }

    fn idle(&mut self) -> PlayerState {
        self.hold(&[])
    }

    fn hud(&self, view: &HudView) -> Vec<HudItem> {
        let mut hud = Hud::new();
        self.game.hud(view, &mut hud);
        hud.items().to_vec()
    }

    /// Ticks until the character is hit; its state then.
    fn until_hit(&mut self) -> PlayerState {
        let hits = self.game.state().player.hits;
        for _ in 0..2000 {
            let p = self.idle();
            if p.hits > hits {
                return p;
            }
        }
        panic!("no hit");
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32, color: Vec4) -> HudItem {
    HudItem::Rect {
        min: Vec2::new(x, y),
        size: Vec2::new(w, h),
        color,
    }
}

/// The two bars at the top left for these lengths, zero-length layers left
/// out.
fn bars(trail: f32, hp: f32, stamina: f32) -> Vec<HudItem> {
    let mut items = vec![rect(40.0, 40.0, 300.0, 12.0, BAR_BACK)];
    for (length, color) in [(trail, BAR_TRAIL), (hp, BAR_HP)] {
        if length > 0.0 {
            items.push(rect(40.0, 40.0, length, 12.0, color));
        }
    }
    items.push(rect(40.0, 58.0, 300.0, 8.0, BAR_BACK));
    if stamina > 0.0 {
        items.push(rect(40.0, 58.0, stamina, 8.0, BAR_STAMINA));
    }
    items
}

// ---------------------------------------------------------------- the bars

#[test]
fn full_health_and_stamina_draw_the_two_bars_full() {
    let run = Run::new("", "");
    assert_eq!(run.hud(&view()), bars(300.0, 300.0, 300.0));
    assert_eq!(BAR_BACK, Vec4::new(0.02, 0.02, 0.02, 0.75));
    assert_eq!(BAR_TRAIL, Vec4::new(0.85, 0.7, 0.35, 1.0));
    assert_eq!(BAR_HP, Vec4::new(0.6, 0.05, 0.05, 1.0));
    assert_eq!(BAR_STAMINA, Vec4::new(0.1, 0.5, 0.15, 1.0));
}

#[test]
fn bar_layers_are_as_long_as_the_trail_the_health_and_the_stamina() {
    let mut run = Run::new("", "");
    let p = &mut run.game.state.player;
    p.health.hp = 70.0;
    p.hp_trail = 90.0;
    p.stamina = 25.0;
    assert_eq!(run.hud(&view()), bars(270.0, 210.0, 75.0));
}

#[test]
fn bars_follow_hp_and_stamina_max() {
    let run = Run::new(r#"("hp", 50), ("stamina_max", 40)"#, "");
    let items = run.hud(&view());
    assert_eq!(items[0], rect(40.0, 40.0, 150.0, 12.0, BAR_BACK));
    assert_eq!(items[3], rect(40.0, 58.0, 120.0, 8.0, BAR_BACK));
}

#[test]
fn empty_layers_are_left_out() {
    let mut run = Run::new("", "");
    let p = &mut run.game.state.player;
    p.health.hp = 0.0;
    p.hp_trail = 0.0;
    p.stamina = 0.0;
    assert_eq!(run.hud(&view()), bars(0.0, 0.0, 0.0));
    assert_eq!(run.hud(&view()).len(), 2);
}

#[test]
fn drawing_the_hud_changes_nothing() {
    let mut run = Run::new("", &format!("{}{}", enemy("b", 0.0, -3.0, true), stake(5)));
    run.hold(&[LOCK]);
    for _ in 0..10 {
        run.idle();
    }
    let before = run.game.state();
    let hud = run.hud(&view());
    assert!(!hud.is_empty());
    assert_eq!(run.game.state(), before);
}

// --------------------------------------------------------------- the trail

#[test]
fn trail_waits_30_ticks_after_a_hit_then_falls_to_the_health() {
    let mut run = Run::new("", &stake(10));
    let p = run.until_hit();
    // The hit's own tick counts the wait down once.
    assert_eq!(
        (p.health.hp, p.hp_trail, p.hp_trail_wait),
        (70.0, 100.0, 29)
    );
    for wait in (0..29).rev() {
        let p = run.idle();
        assert_eq!((p.hp_trail, p.hp_trail_wait), (100.0, wait));
    }
    // Then 100 / 60 a tick, down to 70: 18 ticks.
    let mut trail = 100.0;
    for _ in 0..17 {
        let p = run.idle();
        trail -= 100.0 / 60.0;
        assert!((p.hp_trail - trail).abs() < 1e-4, "{} {trail}", p.hp_trail);
        assert!(p.hp_trail > 70.0);
    }
    // The 18th tick lands on 70 up to f32 rounding (100 - 18 × 100/60
    // leaves 70.00005); the next one clamps to the health exactly.
    let p = run.idle();
    assert!((p.hp_trail - 70.0).abs() < 1e-4, "{}", p.hp_trail);
    let p = run.idle();
    assert_eq!((p.hp_trail, p.hp_trail_wait), (70.0, 0));
    assert_eq!(run.idle().hp_trail, 70.0);
}

#[test]
fn second_hit_while_waiting_starts_the_wait_over() {
    let mut run = Run::new("", &stake(10));
    run.until_hit();
    for _ in 0..10 {
        run.idle();
    }
    assert_eq!(run.game.state().player.hp_trail_wait, 19);
    // The next swing, far off: hit the character directly.
    let mut player = run.game.state.player.clone();
    run.game.player_hit(
        &mut player,
        Vec3::new(0.0, 0.0, -2.0),
        "stake".to_owned(),
        10.0,
        0.0,
        HitAttack::Swing,
    );
    run.game.state.player = player;
    assert_eq!(run.game.state().player.hp_trail_wait, 30);
    let p = run.idle();
    assert_eq!(
        (p.health.hp, p.hp_trail, p.hp_trail_wait),
        (60.0, 100.0, 29)
    );
}

#[test]
fn trail_levels_with_a_heal_on_its_tick() {
    let mut run = Run::new(r#"("flask_ticks", 1), ("flask_heal_tick", 1)"#, "");
    run.game.health.take(50.0, 0.0, 0.0);
    run.game.state.player.hp_trail = 50.0;
    let p = run.hold(&[DRINK]);
    assert_eq!((p.health.hp, p.hp_trail, p.hp_trail_wait), (90.0, 90.0, 0));
}

#[test]
fn trail_levels_with_full_health_on_sitting_down_while_it_waits() {
    let bonfire = r#"(name: "bonfire", position: (0.0, 0.0, -1.0),
        props: [("bonfire_range", 1.5)]),"#;
    let mut run = Run::new("", &format!("{bonfire}{}", stake(10)));
    let p = run.until_hit();
    assert_eq!(p.hp_trail_wait, 29);
    // Wait out the stagger, still inside the trail's wait.
    for _ in 0..29 {
        run.idle();
    }
    run.idle();
    let p = run.hold(&[INTERACT]);
    assert_eq!(p.action, Action::Sit);
    assert_eq!(
        (p.health.hp, p.hp_trail, p.hp_trail_wait),
        (100.0, 100.0, 0)
    );
}

#[test]
fn trail_levels_with_full_health_on_the_respawn() {
    let mut run = Run::new(r#"("respawn_ticks", 3)"#, "");
    let mut player = run.game.state.player.clone();
    run.game.player_hit(
        &mut player,
        Vec3::new(0.0, 0.0, -2.0),
        "s".to_owned(),
        1000.0,
        0.0,
        HitAttack::Swing,
    );
    run.game.state.player = player;
    let p = run.idle();
    assert_eq!((p.health.hp, p.hp_trail), (0.0, 100.0));
    let mut p = run.idle();
    while p.health.hp == 0.0 {
        assert!(p.hp_trail == 100.0);
        p = run.idle();
    }
    assert_eq!(
        (p.health.hp, p.hp_trail, p.hp_trail_wait),
        (100.0, 100.0, 0)
    );
}

// ------------------------------------------------------------- boss bars

fn boss_bar(y: f32, hp: f32) -> Vec<HudItem> {
    let mut items = vec![rect(290.0, y, 700.0, 10.0, BAR_BACK)];
    if hp > 0.0 {
        items.push(rect(290.0, y, 700.0 * hp / 200.0, 10.0, BAR_HP));
    }
    items
}

#[test]
fn boss_bar_shows_while_the_character_is_within_its_aggro_range() {
    // 5 m away, an aggro range of 5.
    let mut run = Run::new("", &enemy("b", 0.0, -5.0, true));
    assert_eq!(run.hud(&view())[5..], boss_bar(660.0, 200.0));
    run.game.hurtboxes[0]
        .health
        .as_mut()
        .unwrap()
        .take(50.0, 0.0, 0.0);
    assert_eq!(run.hud(&view())[5..], boss_bar(660.0, 150.0));
    run.game.state.player.position = Vec3::new(0.0, 0.0, 0.01);
    assert_eq!(run.hud(&view()).len(), 5);
}

#[test]
fn boss_bar_is_centered_on_a_wider_screen() {
    let run = Run::new("", &enemy("b", 0.0, -3.0, true));
    let wide = HudView::new(1920, 720, Mat4::IDENTITY);
    assert_eq!(wide.width, 1920.0);
    assert_eq!(run.hud(&wide)[5], rect(610.0, 660.0, 700.0, 10.0, BAR_BACK));
}

#[test]
fn boss_bar_hides_when_the_boss_or_the_character_is_dead() {
    let mut run = Run::new("", &enemy("b", 0.0, -3.0, true));
    run.game.hurtboxes[0]
        .health
        .as_mut()
        .unwrap()
        .take(1000.0, 0.0, 0.0);
    assert_eq!(run.hud(&view()).len(), 5);

    let mut run = Run::new("", &enemy("b", 0.0, -3.0, true));
    run.game.health.take(1000.0, 0.0, 0.0);
    run.game.state.player.health = run.game.health.now;
    run.game.state.player.hp_trail = 0.0;
    assert_eq!(run.hud(&view()), bars(0.0, 0.0, 300.0));
}

#[test]
fn enemy_without_boss_bar_has_no_bar() {
    let run = Run::new("", &enemy("b", 0.0, -3.0, false));
    assert_eq!(run.hud(&view()).len(), 5);
}

#[test]
fn two_boss_bars_stack_upwards_in_pre_order() {
    let nodes = format!(
        "{}{}{}",
        enemy("a", 2.0, -3.0, true),
        enemy("x", 0.0, -3.0, false),
        enemy("b", -2.0, -3.0, true)
    );
    let mut run = Run::new("", &nodes);
    run.game.hurtboxes[2]
        .health
        .as_mut()
        .unwrap()
        .take(100.0, 0.0, 0.0);
    let mut want = boss_bar(660.0, 200.0);
    want.extend(boss_bar(630.0, 100.0));
    assert_eq!(run.hud(&view())[5..], want);
}

// ------------------------------------------------------------ the lock dot

#[test]
fn lock_dot_is_drawn_at_the_projected_lock_point_while_locked_on() {
    let mut run = Run::new("", &enemy("b", 1.0, -3.0, false));
    let view = HudView::new(
        1280,
        720,
        glam::camera::rh::proj::directx::perspective(1.0, 16.0 / 9.0, 0.1, 100.0),
    );
    assert_eq!(run.hud(&view).len(), 5);
    run.hold(&[LOCK]);
    assert!(run.game.lock.is_some());
    let at = run.game.scene().transform(run.game.enemies[0].id).position;
    let center = view.project(at + Vec3::new(0.0, 1.5, 0.0)).unwrap();
    assert_eq!(
        run.hud(&view)[5..],
        [HudItem::Disc {
            center,
            radius: 5.0,
            color: Vec4::new(1.0, 1.0, 1.0, 0.9),
        }]
    );
}

#[test]
fn lock_dot_behind_the_camera_is_not_drawn() {
    let mut run = Run::new("", &enemy("b", 1.0, -3.0, false));
    run.hold(&[LOCK]);
    assert!(run.game.lock.is_some());
    // A view looking along +Z: the target is behind it.
    let behind = glam::camera::rh::proj::directx::perspective(1.0, 16.0 / 9.0, 0.1, 100.0)
        * Mat4::from_rotation_y(std::f32::consts::PI);
    assert_eq!(run.hud(&HudView::new(1280, 720, behind)).len(), 5);
}

// ---------------------------------------------------------------- validation

#[test]
fn boss_bar_prop_is_checked_with_exact_messages() {
    let error_of = |nodes: &str| match Game::from_source(&scene("", nodes)) {
        Ok(_) => panic!("the scene should be rejected: {nodes}"),
        Err(e) => e.to_string(),
    };
    for (value, got) in [("2", "2"), ("0.5", "0.5"), ("-1", "-1")] {
        let nodes = enemy("b", 0.0, -3.0, false).replace(
            r#"("lunge_distance", 0.2)"#,
            &format!(r#"("lunge_distance", 0.2), ("boss_bar", {value})"#),
        );
        assert_eq!(
            error_of(&nodes),
            format!(
                "the node \"b\": prop \"boss_bar\" must be a whole number from 0 to 1, got {got}"
            )
        );
    }
    assert_eq!(
        error_of(r#"(name: "t", props: [("boss_bar", 1)]),"#),
        "the node \"t\": prop \"boss_bar\" needs enemy_speed"
    );
    // After the enemy props before it, whatever the order written.
    assert_eq!(
        error_of(r#"(name: "t", props: [("boss_bar", 1), ("spin_weight", 1)]),"#),
        "the node \"t\": prop \"spin_weight\" needs enemy_speed"
    );
    assert!(Game::from_source(&scene("", &enemy("b", 0.0, -3.0, true))).is_ok());
}
