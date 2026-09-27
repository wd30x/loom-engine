//! Tests of switching clips ([`Scene::play_clip`]), changing their speed
//! ([`Scene::set_clip_rate`]) and of the blended offsets
//! ([`Animation::offsets`]), `docs/architecture.md` section 19, "播放和过渡"
//! and "播放速度和片段起点".

use std::collections::BTreeMap;
use std::f32::consts::FRAC_PI_4;

use glam::{Quat, Vec3};

use super::*;

/// A scene with an animated node `g/k` (clips `idle`, `walk` and `wave`,
/// playing `idle`) and a mesh node `c`.
fn scene() -> Scene {
    Scene::parse(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "g", children: [(name: "k", model: "k.glb", animation: (clips: [
                ("idle", "i.ron"), ("walk", "w.ron"), ("wave", "w.ron"),
            ], play: "idle"))]),
            (name: "c", mesh: Cube),
        ])"#,
    )
    .unwrap()
}

fn animation(scene: &Scene) -> Animation {
    let k = scene.find("g/k").unwrap();
    scene.node(k).model.clone().unwrap().animation.unwrap()
}

/// `clip` counted from `start` at `offset` ctick, `rate` percent fast.
fn time(clip: &str, start: u64, offset: u64, rate: u32) -> ClipTime {
    ClipTime {
        clip: clip.to_owned(),
        start,
        offset,
        rate,
    }
}

#[test]
fn play_clip_blends_from_what_played_until_then() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, Play::new("walk", 120).blend(6)).unwrap();
    let a = animation(&scene);
    assert_eq!(a.playing, "walk");
    assert_eq!((a.start, a.offset, a.rate), (120, 0, 100));
    assert_eq!((a.blend_start, a.blend), (120, 6));
    assert_eq!(a.from, Some(time("idle", 0, 0, 100)));
    assert_eq!(a.clip_path("wave"), Some("w.ron"));
    assert_eq!(a.clip_path("run"), None);
}

#[test]
fn play_clip_sets_the_clip_time_and_rate_it_starts_with() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene
        .play_clip(k, Play::new("walk", 40).offset(1000).rate(150).blend(3))
        .unwrap();
    let a = animation(&scene);
    assert_eq!((a.start, a.offset, a.rate), (40, 1000, 150));
    assert_eq!((a.blend_start, a.blend), (40, 3));
    assert_eq!(
        Play::new("walk", 40),
        Play {
            clip: "walk".to_owned(),
            start: 40,
            blend: 0,
            offset: 0,
            rate: 100,
        }
    );
}

#[test]
fn play_clip_during_a_blend_drops_the_older_source() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene
        .play_clip(k, Play::new("walk", 10).blend(5).offset(200).rate(50))
        .unwrap();
    scene.play_clip(k, Play::new("wave", 12).blend(3)).unwrap();
    let a = animation(&scene);
    assert_eq!(a.from, Some(time("walk", 10, 200, 50)));
    assert_eq!((a.playing.as_str(), a.start, a.blend), ("wave", 12, 3));
}

#[test]
fn play_clip_of_the_playing_clip_restarts_it_blended_from_its_old_start() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, Play::new("idle", 30).blend(4)).unwrap();
    let a = animation(&scene);
    assert_eq!(a.from, Some(time("idle", 0, 0, 100)));
    assert_eq!((a.playing.as_str(), a.start, a.blend), ("idle", 30, 4));
}

#[test]
fn play_clip_refuses_undeclared_clips_and_nodes_without_animation_changing_nothing() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, Play::new("walk", 10).blend(5)).unwrap();
    let before = animation(&scene);
    assert_eq!(
        scene
            .play_clip(k, Play::new("run", 20))
            .unwrap_err()
            .to_string(),
        r#"node "g/k": the scene declares no animation clip "run""#
    );
    assert_eq!(animation(&scene), before);
    for path in ["c", "g"] {
        let id = scene.find(path).unwrap();
        assert_eq!(
            scene
                .play_clip(id, Play::new("walk", 0))
                .unwrap_err()
                .to_string(),
            format!("node {path:?} has no animation")
        );
    }
}

#[test]
fn set_clip_rate_keeps_the_clip_time_unwrapped_and_the_blend() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene
        .play_clip(k, Play::new("walk", 100).blend(6).offset(500).rate(150))
        .unwrap();
    // At 104: 500 + 4 × 150 = 1100 ctick.
    scene.set_clip_rate(k, 104, 50).unwrap();
    let a = animation(&scene);
    assert_eq!((a.start, a.offset, a.rate), (104, 1100, 50));
    assert_eq!((a.blend_start, a.blend), (100, 6));
    assert_eq!(a.from, Some(time("idle", 0, 0, 100)));
    // Far past a clip's length: no wrap.
    scene.set_clip_rate(k, 100_104, 100).unwrap();
    assert_eq!(animation(&scene).offset, 1100 + 100_000 * 50);
    for path in ["c", "g"] {
        let id = scene.find(path).unwrap();
        assert_eq!(
            scene.set_clip_rate(id, 0, 100).unwrap_err().to_string(),
            format!("node {path:?} has no animation")
        );
    }
}

/// Clips by name: `zero` and `ninety` turn bone `b` 0° and 90° about X;
/// `move` loops `b` from 0 to 1 m along X every 10 ticks; `slide` moves it
/// from 0 to 10 m along X in 10 ticks and stops; `still` lists no bone.
fn clips() -> BTreeMap<&'static str, Clip> {
    let clip = |src: &str| Clip::parse(src).unwrap();
    BTreeMap::from([
        (
            "zero",
            clip(
                r#"(length: 10, looping: true, bones: {"b": (rotation: [(0, (0.0, 0.0, 0.0))])})"#,
            ),
        ),
        (
            "ninety",
            clip(
                r#"(length: 10, looping: true, bones: {"b": (rotation: [(0, (90.0, 0.0, 0.0))])})"#,
            ),
        ),
        (
            "move",
            clip(
                r#"(length: 10, looping: true, bones: {"b": (translation: [(0, (0.0, 0.0, 0.0)), (10, (1.0, 0.0, 0.0))])})"#,
            ),
        ),
        (
            "slide",
            clip(
                r#"(length: 10, looping: false, bones: {"b": (translation: [(0, (0.0, 0.0, 0.0)), (4, (4.0, 0.0, 0.0)), (10, (10.0, 0.0, 0.0))])})"#,
            ),
        ),
        ("still", clip("(length: 10, looping: true, bones: {})")),
    ])
}

/// Playing `playing` from its beginning since `start` at the written
/// speed, blended from `from` over `blend` starting at `start`.
fn playing(playing: &str, start: u64, from: Option<(&str, u64)>, blend: u64) -> Animation {
    Animation {
        clips: Vec::new(),
        playing: playing.to_owned(),
        start,
        offset: 0,
        rate: 100,
        from: from.map(|(c, s)| time(c, s, 0, 100)),
        blend_start: start,
        blend,
    }
}

fn assert_turn_x(got: (Quat, Vec3), degrees: f32) {
    let want = Quat::from_rotation_x(degrees.to_radians());
    assert!(got.0.dot(want).abs() > 1.0 - 1e-6, "{} != {want}", got.0);
    assert_eq!(got.1, Vec3::ZERO);
}

#[test]
fn blend_turns_linearly_from_the_old_clip_to_the_new_one() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    let a = playing("ninety", 100, Some(("zero", 0)), 10);
    assert_turn_x(a.offsets(clip, "b", 105), 45.0);
    assert!(
        a.offsets(clip, "b", 105)
            .0
            .abs_diff_eq(Quat::from_rotation_x(FRAC_PI_4), 1e-6)
    );
    for tau in [110, 111, 150] {
        assert_turn_x(a.offsets(clip, "b", tau), 90.0);
    }
    // Before the new clip starts: all the old one.
    for tau in [90, 100] {
        assert_eq!(a.offsets(clip, "b", tau), (Quat::IDENTITY, Vec3::ZERO));
    }
    // No blend: the new clip at once, before its start too.
    let cut = playing("ninety", 100, Some(("zero", 0)), 0);
    assert_turn_x(cut.offsets(clip, "b", 100), 90.0);
    assert_turn_x(cut.offsets(clip, "b", 90), 90.0);
    // As loaded: no source.
    assert_turn_x(playing("ninety", 0, None, 0).offsets(clip, "b", 3), 90.0);
}

#[test]
fn blend_source_keeps_its_own_time_and_unlisted_bones_count_as_still() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    // `move` has run since tick 0 and loops every 10; `still` lists no
    // bone, so it pulls b back to its bind pose.
    let a = playing("still", 100, Some(("move", 0)), 10);
    // Tick 102: move is at 0.2 m, weighing 0.8.
    let at = |tau| a.offsets(clip, "b", tau).1.x;
    assert!((at(102) - 0.16).abs() < 1e-6, "{}", at(102));
    // Tick 105: move is at 0.5 m, weighing 0.5.
    assert!((at(105) - 0.25).abs() < 1e-6, "{}", at(105));
    assert_eq!(at(110), 0.0);
    // A bone neither clip lists stays in its bind pose.
    assert_eq!(a.offsets(clip, "other", 105), (Quat::IDENTITY, Vec3::ZERO));
}

/// `s` of the way from `a` to `b` along X, as a translation channel mixes.
fn along(a: f32, b: f32, s: f32) -> f32 {
    Vec3::new(a, 0.0, 0.0).lerp(Vec3::new(b, 0.0, 0.0), s).x
}

/// Playing `clip` counted from `start` at `offset`, `rate` percent fast,
/// without a source.
fn timed(clip: &str, start: u64, offset: u64, rate: u32) -> Animation {
    Animation {
        rate,
        offset,
        ..playing(clip, start, None, 0)
    }
}

#[test]
fn rate_and_offset_scale_and_shift_the_clip_time() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    let x = |a: &Animation, tau| a.offsets(clip, "b", tau).1.x;
    // Half speed from tick 10 of `slide`: 1000 + 3 × 50 = 1150 ctick,
    // past the end of the one-shot clip.
    let half = timed("slide", 20, 1000, 50);
    assert_eq!(half.time_at(23), 1150);
    assert_eq!(x(&half, 23), 10.0);
    let half = timed("slide", 20, 200, 50);
    // 200 + 4 × 50 = 400: exactly the keyframe at tick 4.
    assert_eq!(x(&half, 24), 4.0);
    // 200 + 5 × 50 = 450: (450 - 400) / 600 of the way from 4 to 10 m.
    assert_eq!(x(&half, 25), along(4.0, 10.0, 50.0 / 600.0));
    // One and a half speed from 150: 150 + 2 × 150 = 450 again.
    let fast = timed("slide", 20, 150, 150);
    assert_eq!(x(&fast, 22), x(&half, 25));
    // A one-shot clip holds its end, a looping one wraps.
    assert_eq!(x(&fast, 90), 10.0);
    let looping = timed("move", 0, 1000, 150);
    // 1000 + 7 × 150 = 2050 → 50 ctick into the loop.
    assert_eq!(x(&looping, 7), along(0.0, 1.0, 50.0 / 1000.0));
    // Before its start the clip stays at `offset`.
    assert_eq!(timed("move", 10, 250, 150).time_at(3), 250);
    // Rate 0 holds still.
    assert_eq!(timed("move", 10, 250, 0).time_at(90), 250);
}

#[test]
fn rate_100_from_offset_0_samples_the_same_bits_as_whole_ticks() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    let a = timed("slide", 7, 0, 100);
    for t in 0..=12 {
        assert_eq!(
            a.offsets(clip, "b", 7 + t),
            clips["slide"].sample("b", t.min(10) * CTICKS).unwrap()
        );
    }
    // 1/6 of 6 m, as E7.3 worked it out from whole ticks.
    assert_eq!(a.offsets(clip, "b", 12).1.x, along(4.0, 10.0, 1.0 / 6.0));
}

#[test]
fn set_clip_rate_changes_speed_without_a_jump() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    // `walk` stands for `slide` here: the scene only names clips.
    let by_name = |name: &str| clip(if name == "walk" { "slide" } else { "still" });
    scene.play_clip(k, Play::new("walk", 0).rate(50)).unwrap();
    let before = animation(&scene).offsets(by_name, "b", 6);
    scene.set_clip_rate(k, 6, 150).unwrap();
    let a = animation(&scene);
    assert_eq!(a.offsets(by_name, "b", 6), before);
    // From 300 ctick on at 150 a tick.
    assert_eq!(a.time_at(8), 600);
    assert_eq!(
        a.offsets(by_name, "b", 8).1.x,
        along(4.0, 10.0, 200.0 / 600.0)
    );
}

#[test]
fn set_clip_rate_during_a_blend_leaves_the_weight_and_the_source_alone() {
    let clips = clips();
    let clip = |name: &str| &clips[name];
    // From `move` (at its own half speed, since tick 0) to `still`, over
    // 10 ticks from 100.
    let mut a = playing("still", 100, None, 10);
    a.from = Some(time("move", 0, 0, 50));
    let x = |a: &Animation, tau| a.offsets(clip, "b", tau).1.x;
    // Tick 104: move at 5200 ctick → 200 into the loop, 0.2 m, weighing 0.6.
    assert!((x(&a, 104) - 0.12).abs() < 1e-6, "{}", x(&a, 104));
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, Play::new("walk", 0).blend(10)).unwrap();
    scene
        .play_clip(k, Play::new("wave", 100).blend(10))
        .unwrap();
    scene.set_clip_rate(k, 104, 30).unwrap();
    let changed = animation(&scene);
    assert_eq!((changed.blend_start, changed.blend), (100, 10));
    assert_eq!(changed.from, Some(time("walk", 0, 0, 100)));
    // The weight still counts from 100: `still` pulls as hard as before.
    a.set_rate(104, 30);
    assert!((x(&a, 104) - 0.12).abs() < 1e-6, "{}", x(&a, 104));
    assert!((x(&a, 105) - 0.25 * 0.5).abs() < 1e-6, "{}", x(&a, 105));
}
