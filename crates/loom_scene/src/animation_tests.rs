//! Tests of switching clips ([`Scene::play_clip`]) and of the blended
//! offsets ([`Animation::offsets`]), `docs/architecture.md` section 19,
//! "播放和过渡".

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

#[test]
fn play_clip_blends_from_what_played_until_then() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, "walk", 120, 6).unwrap();
    let a = animation(&scene);
    assert_eq!(a.playing, "walk");
    assert_eq!((a.start, a.blend), (120, 6));
    assert_eq!(a.from, Some(("idle".to_owned(), 0)));
    assert_eq!(a.clip_path("wave"), Some("w.ron"));
    assert_eq!(a.clip_path("run"), None);
}

#[test]
fn play_clip_during_a_blend_drops_the_older_source() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, "walk", 10, 5).unwrap();
    scene.play_clip(k, "wave", 12, 3).unwrap();
    let a = animation(&scene);
    assert_eq!(a.from, Some(("walk".to_owned(), 10)));
    assert_eq!((a.playing.as_str(), a.start, a.blend), ("wave", 12, 3));
}

#[test]
fn play_clip_of_the_playing_clip_restarts_it_blended_from_its_old_start() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, "idle", 30, 4).unwrap();
    let a = animation(&scene);
    assert_eq!(a.from, Some(("idle".to_owned(), 0)));
    assert_eq!((a.playing.as_str(), a.start, a.blend), ("idle", 30, 4));
}

#[test]
fn play_clip_refuses_undeclared_clips_and_nodes_without_animation_changing_nothing() {
    let mut scene = scene();
    let k = scene.find("g/k").unwrap();
    scene.play_clip(k, "walk", 10, 5).unwrap();
    let before = animation(&scene);
    assert_eq!(
        scene.play_clip(k, "run", 20, 0).unwrap_err().to_string(),
        r#"node "g/k": the scene declares no animation clip "run""#
    );
    assert_eq!(animation(&scene), before);
    for path in ["c", "g"] {
        let id = scene.find(path).unwrap();
        assert_eq!(
            scene.play_clip(id, "walk", 0, 0).unwrap_err().to_string(),
            format!("node {path:?} has no animation")
        );
    }
}

/// Clips by name: `zero` and `ninety` turn bone `b` 0° and 90° about X;
/// `move` loops `b` from 0 to 1 m along X every 10 ticks; `still` lists no
/// bone.
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
        ("still", clip("(length: 10, looping: true, bones: {})")),
    ])
}

/// Playing `playing` since `start`, blended from `from` over `blend`.
fn playing(playing: &str, start: u64, from: Option<(&str, u64)>, blend: u64) -> Animation {
    Animation {
        clips: Vec::new(),
        playing: playing.to_owned(),
        start,
        from: from.map(|(c, s)| (c.to_owned(), s)),
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
