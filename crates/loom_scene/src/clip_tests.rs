use std::f32::consts::FRAC_PI_4;

use glam::{EulerRot, Quat, Vec3};

use super::*;
use crate::Scene;

/// A clip of `length` ticks with the one bone `b` of channels `channels`.
fn clip(length: u64, looping: bool, channels: &str) -> Clip {
    Clip::parse(&format!(
        "(length: {length}, looping: {looping}, bones: {{\"b\": ({channels})}})"
    ))
    .unwrap()
}

/// At whole tick `t`.
fn rotation_at(c: &Clip, t: u64) -> Quat {
    c.sample("b", t * CTICKS).unwrap().0
}

/// At whole tick `t`.
fn translation_at(c: &Clip, t: u64) -> Vec3 {
    c.sample("b", t * CTICKS).unwrap().1
}

fn assert_quat_near(a: Quat, b: Quat) {
    // q and -q are the same rotation.
    assert!(a.dot(b).abs() > 1.0 - 1e-6, "{a} != {b}");
}

#[test]
fn rotation_halfway_between_keyframes_is_half_the_angle() {
    let c = clip(
        60,
        false,
        "rotation: [(0, (0.0, 0.0, 0.0)), (60, (90.0, 0.0, 0.0))]",
    );
    assert_quat_near(rotation_at(&c, 30), Quat::from_rotation_x(FRAC_PI_4));
    assert_eq!(translation_at(&c, 30), Vec3::ZERO);
    assert_eq!(rotation_at(&c, 0), Quat::IDENTITY);
}

#[test]
fn rotation_keyframes_use_the_yxz_order_of_a_scene_rotation() {
    let c = clip(10, false, "rotation: [(0, (90.0, 90.0, 0.0))]");
    let scene = Scene::parse(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "n", rotation: (90.0, 90.0, 0.0)),
        ])"#,
    )
    .unwrap();
    let node = scene.node(scene.find("n").unwrap()).transform.rotation;
    assert_eq!(rotation_at(&c, 5), node);
    // Y first, then X in the turned frame.
    let yxz = Quat::from_rotation_y(90f32.to_radians()) * Quat::from_rotation_x(90f32.to_radians());
    assert_quat_near(rotation_at(&c, 5), yxz);
    assert_quat_near(
        rotation_at(&c, 5),
        Quat::from_euler(EulerRot::YXZ, 90f32.to_radians(), 90f32.to_radians(), 0.0),
    );
}

#[test]
fn looping_clip_wraps_and_one_shot_clip_stops_at_its_length() {
    let keys = "translation: [(0, (0.0, 0.0, 0.0)), (60, (6.0, 0.0, 0.0))]";
    let looping = clip(60, true, keys);
    assert_eq!(translation_at(&looping, 75), translation_at(&looping, 15));
    assert_eq!(translation_at(&looping, 15), Vec3::new(1.5, 0.0, 0.0));
    assert_eq!(translation_at(&looping, 120), Vec3::ZERO);
    let once = clip(60, false, keys);
    assert_eq!(translation_at(&once, 200), translation_at(&once, 60));
    assert_eq!(translation_at(&once, 200), Vec3::new(6.0, 0.0, 0.0));
}

#[test]
fn values_hold_before_the_first_and_after_the_last_keyframe() {
    let c = clip(
        60,
        false,
        "translation: [(10, (1.0, 2.0, 3.0)), (40, (4.0, 5.0, 6.0))]",
    );
    assert_eq!(translation_at(&c, 0), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(translation_at(&c, 50), Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(translation_at(&c, 40), Vec3::new(4.0, 5.0, 6.0));
}

#[test]
fn translation_is_interpolated_linearly() {
    let c = clip(
        40,
        false,
        "translation: [(0, (0.0, 0.0, 0.0)), (10, (1.0, -2.0, 0.5)), (40, (4.0, 1.0, 0.5))]",
    );
    assert_eq!(translation_at(&c, 5), Vec3::new(0.5, -1.0, 0.25));
    // A third of the way from tick 10 to 40.
    let t20 = translation_at(&c, 20);
    assert!(
        (t20 - Vec3::new(2.0, -1.0, 0.5)).abs().max_element() < 1e-6,
        "{t20}"
    );
    assert_eq!(translation_at(&c, 10), Vec3::new(1.0, -2.0, 0.5));
}

#[test]
fn rotation_takes_the_shortest_way_between_keyframes() {
    let c = clip(
        10,
        false,
        "rotation: [(0, (0.0, 0.0, 0.0)), (10, (0.0, 350.0, 0.0))]",
    );
    assert_quat_near(
        rotation_at(&c, 5),
        Quat::from_rotation_y(-5f32.to_radians()),
    );
}

#[test]
fn unlisted_bones_sample_to_none_and_listed_ones_have_neutral_missing_channels() {
    let c = clip(10, true, "translation: [(0, (0.0, 1.0, 0.0))]");
    assert_eq!(c.sample("other", 300), None);
    assert_eq!(
        c.sample("b", 300),
        Some((Quat::IDENTITY, Vec3::new(0.0, 1.0, 0.0)))
    );
    let empty = Clip::parse("(length: 1, looping: true, bones: {})").unwrap();
    assert_eq!(empty.bones().count(), 0);
    assert_eq!((empty.length(), empty.looping()), (1, true));
}

#[test]
fn bones_lists_the_bone_names_in_name_order() {
    let c = Clip::parse(
        r#"(length: 10, looping: false, bones: {
            "spine": (rotation: [(0, (0.0, 0.0, 0.0))]),
            "hips": (translation: [(0, (0.0, 0.0, 0.0))]),
        })"#,
    )
    .unwrap();
    assert_eq!(c.bones().collect::<Vec<_>>(), ["hips", "spine"]);
}

fn error(src: &str) -> String {
    Clip::parse(src).unwrap_err().to_string()
}

#[test]
fn clip_rules_are_errors_naming_the_bone_and_channel() {
    let cases = [
        (
            "(length: 0, looping: true, bones: {})",
            "length must be greater than 0".to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": ()})"#,
            r#"bone "hips": give rotation, translation or both"#.to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": (rotation: [])})"#,
            r#"bone "hips" rotation: needs at least one keyframe"#.to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": (translation: [(5, (0.0, 0.0, 0.0)), (5, (1.0, 0.0, 0.0))])})"#,
            r#"bone "hips" translation: keyframe tick 5 is not after the one before it"#.to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": (translation: [(5, (0.0, 0.0, 0.0)), (2, (1.0, 0.0, 0.0))])})"#,
            r#"bone "hips" translation: keyframe tick 2 is not after the one before it"#.to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": (rotation: [(11, (0.0, 0.0, 0.0))])})"#,
            r#"bone "hips" rotation: keyframe tick 11 is past the length 10"#.to_owned(),
        ),
        (
            r#"(length: 10, looping: true, bones: {"hips": (rotation: [(3, (inf, 0.0, 0.0))])})"#,
            r#"bone "hips" rotation: the keyframe at tick 3 has a non-finite value"#.to_owned(),
        ),
    ];
    for (src, expected) in cases {
        assert_eq!(error(src), expected, "{src}");
    }
}

#[test]
fn clip_files_deny_unknown_fields() {
    let e =
        error(r#"(length: 10, looping: true, bones: {"hips": (scale: [(0, (1.0, 1.0, 1.0))])})"#);
    assert!(e.contains("scale"), "{e}");
    let e = error("(length: 10, looping: true, bones: {}, speed: 2.0)");
    assert!(e.contains("speed"), "{e}");
    let e = error("(length: 10, bones: {})");
    assert!(e.contains("looping"), "{e}");
}

#[test]
fn clip_file_writes_back_as_one_keyframe_a_line() {
    let src = r#"(length: 30, looping: true, bones: {
        "hips": (translation: [(0, (0.0, -0.05, 0.0)), (30, (0.0, 0.0, 0.0))]),
        "chest": (rotation: [(0, (12.34, 0.0, -5.5))]),
    })"#;
    let file: ClipFile = options().from_str(src).unwrap();
    let ron = file.to_ron();
    assert_eq!(
        ron,
        concat!(
            "(
    length: 30,
    looping: true,
    bones: {
",
            "        \"chest\": (
            rotation: [
",
            "                (0, (12.34, 0.0, -5.5)),
            ],
        ),
",
            "        \"hips\": (
            translation: [
",
            "                (0, (0.0, -0.05, 0.0)),
                (30, (0.0, 0.0, 0.0)),
",
            "            ],
        ),
    },
)
",
        )
    );
    assert_eq!(Clip::parse(&ron).unwrap(), Clip::parse(src).unwrap());
}

#[test]
fn clip_time_between_ticks_interpolates_by_hundredths_of_a_tick() {
    let c = clip(
        40,
        false,
        "translation: [(0, (0.0, 0.0, 0.0)), (10, (1.0, 0.0, 0.0)), (40, (4.0, 0.0, 0.0))]",
    );
    let at = |ctick| c.sample("b", ctick).unwrap().1.x;
    // 2.5 ticks of 10: a quarter of the way, by the formula's f32 quotient.
    let along = |a: f32, b: f32, s: f32| Vec3::new(a, 0.0, 0.0).lerp(Vec3::new(b, 0.0, 0.0), s).x;
    assert_eq!(at(250), along(0.0, 1.0, 250.0 / 1000.0));
    assert_eq!(at(1001), along(1.0, 4.0, 1.0 / 3000.0));
    // Exactly on a keyframe's hundredths: its own value.
    assert_eq!(at(1000), 1.0);
    // Past the end of a one-shot clip: its last value.
    assert_eq!(at(4001), 4.0);
}

#[test]
fn looping_clip_wraps_clip_time_at_its_length_in_hundredths() {
    let c = clip(
        10,
        true,
        "translation: [(0, (0.0, 0.0, 0.0)), (10, (1.0, 0.0, 0.0))]",
    );
    let at = |ctick| c.sample("b", ctick).unwrap().1.x;
    assert_eq!(at(1050), at(50));
    assert_eq!(at(1000), 0.0);
    assert_eq!(at(999), Vec3::ZERO.lerp(Vec3::X, 999.0 / 1000.0).x);
}
