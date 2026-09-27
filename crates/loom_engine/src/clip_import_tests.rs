//! Tests of `loom clip` on the two-bone model of `anim_tests` with
//! animations added: the round trip against glTF's own rules, thinning,
//! determinism, the report and each error.

use glam::{Mat4, Quat, Vec3};
use loom_scene::Clip;
use serde_json::{Value, json};

use super::*;
use crate::anim::pose;
use crate::anim::tests::{b_bind, two_bone_bin, two_bone_gltf};
use crate::model::tests::glb;

/// `wave`'s rotation keyframes of b (local), at 0, 0.5 and 1 s.
fn wave_rotations() -> [Quat; 3] {
    let rest = b_bind().to_scale_rotation_translation().1;
    let lifted = rest * Quat::from_rotation_x(60f32.to_radians());
    [
        rest,
        lifted,
        lifted * Quat::from_rotation_z(30f32.to_radians()),
    ]
}

/// `wave`'s translation keyframes of a (STEP), at 0, 0.5 and 1 s.
const WAVE_MOVES: [Vec3; 3] = [
    Vec3::ZERO,
    Vec3::new(0.0, 0.5, 0.0),
    Vec3::new(0.2, 0.5, 0.0),
];

/// [`two_bone_bin`] and then, from byte 460: times 0, 0.5, 1 (460);
/// [`wave_rotations`] (472); [`WAVE_MOVES`] (520); times 0, 1 (556); b's
/// rest rotation and that turned 90° about Z (564); a's rest translation
/// twice (596); six zero VEC3s for a CUBICSPLINE sampler (620); scales 1
/// and 2 (692); 716 in all.
fn animated_bin() -> Vec<u8> {
    let mut bin = two_bone_bin();
    let floats =
        |bin: &mut Vec<u8>, fs: &[f32]| bin.extend(fs.iter().flat_map(|f| f.to_le_bytes()));
    floats(&mut bin, &[0.0, 0.5, 1.0]);
    for q in wave_rotations() {
        floats(&mut bin, &q.to_array());
    }
    for v in WAVE_MOVES {
        floats(&mut bin, &v.to_array());
    }
    floats(&mut bin, &[0.0, 1.0]);
    let rest = wave_rotations()[0];
    floats(&mut bin, &rest.to_array());
    floats(
        &mut bin,
        &(rest * Quat::from_rotation_z(90f32.to_radians())).to_array(),
    );
    floats(&mut bin, &[0.0; 6]);
    floats(&mut bin, &[0.0; 18]);
    floats(&mut bin, &[1.0, 1.0, 1.0, 2.0, 2.0, 2.0]);
    assert_eq!(bin.len(), 716);
    bin
}

/// [`two_bone_gltf`] with animations `wave` (b turning LINEAR, a moving
/// STEP) and `turn` (b turning 90° about Z LINEAR, a "moving" to where it
/// is).
fn animated_gltf() -> Value {
    let mut json = two_bone_gltf();
    let accessors = json["accessors"].as_array_mut().unwrap();
    accessors.extend([
        json!({"bufferView": 9, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [1.0]}),
        json!({"bufferView": 10, "componentType": 5126, "count": 3, "type": "VEC4"}),
        json!({"bufferView": 11, "componentType": 5126, "count": 3, "type": "VEC3"}),
        json!({"bufferView": 12, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0]}),
        json!({"bufferView": 13, "componentType": 5126, "count": 2, "type": "VEC4"}),
        json!({"bufferView": 14, "componentType": 5126, "count": 2, "type": "VEC3"}),
        json!({"bufferView": 15, "componentType": 5126, "count": 6, "type": "VEC3"}),
        json!({"bufferView": 16, "componentType": 5126, "count": 2, "type": "VEC3"}),
    ]);
    let views = json["bufferViews"].as_array_mut().unwrap();
    for (offset, length) in [
        (460, 12),
        (472, 48),
        (520, 36),
        (556, 8),
        (564, 32),
        (596, 24),
        (620, 72),
        (692, 24),
    ] {
        views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": length}));
    }
    json["buffers"][0]["byteLength"] = json!(716);
    json["animations"] = json!([
        {"name": "wave",
         "samplers": [{"input": 9, "output": 10}, {"input": 9, "output": 11, "interpolation": "STEP"}],
         "channels": [{"sampler": 0, "target": {"node": 2, "path": "rotation"}},
                      {"sampler": 1, "target": {"node": 1, "path": "translation"}}]},
        {"name": "turn",
         "samplers": [{"input": 12, "output": 13}, {"input": 12, "output": 14}],
         "channels": [{"sampler": 0, "target": {"node": 2, "path": "rotation"}},
                      {"sampler": 1, "target": {"node": 1, "path": "translation"}}]}
    ]);
    json
}

fn convert_json(json: &Value, name: &str) -> Result<ClipFile> {
    to_clip(&glb(json, &animated_bin()), name, false)
}

fn error_of(json: &Value, name: &str) -> String {
    match convert_json(json, name) {
        Ok(_) => panic!("the conversion should fail"),
        Err(e) => format!("{e:#}"),
    }
}

/// The model-space transforms of joints a and b at `t` seconds of `wave`
/// by glTF's rules, worked out here from the keyframes.
fn wave_by_gltf(t: f32) -> [Mat4; 2] {
    let r = wave_rotations();
    let rotation = if t <= 0.5 {
        r[0].slerp(r[1], t / 0.5)
    } else {
        r[1].slerp(r[2], ((t - 0.5) / 0.5).min(1.0))
    };
    let step = WAVE_MOVES[if t < 0.5 {
        0
    } else if t < 1.0 {
        1
    } else {
        2
    }];
    let a = Mat4::from_translation(step);
    let b = a * Mat4::from_rotation_translation(rotation, Vec3::Y);
    [a, b]
}

#[test]
fn converted_clip_poses_the_joints_as_the_gltf_animation_does() {
    let bytes = glb(&animated_gltf(), &animated_bin());
    let file = to_clip(&bytes, "wave", false).unwrap();
    assert_eq!(file.length, 60);
    let clip = Clip::parse(&file.to_ron()).unwrap();
    let skeleton = import(&bytes).unwrap().skinned.unwrap().skeleton;
    let (mut worst_angle, mut worst_move) = (0.0_f32, 0.0_f32);
    for k in 0..=70 {
        let posed = pose(&skeleton, |bone| {
            clip.sample(bone, k).unwrap_or((Quat::IDENTITY, Vec3::ZERO))
        });
        let want = wave_by_gltf(k as f32 / 60.0);
        for (got, want) in posed.iter().zip(want) {
            let (_, gq, gp) = got.to_scale_rotation_translation();
            let (_, wq, wp) = want.to_scale_rotation_translation();
            worst_angle = worst_angle.max(angle(gq, wq));
            worst_move = worst_move.max(gp.distance(wp));
        }
    }
    eprintln!("loom clip round trip: worst rotation {worst_angle}°, worst position {worst_move} m");
    assert!(worst_angle <= 0.15, "{worst_angle}°");
    assert!(worst_move <= 0.001, "{worst_move} m");
    // The STEP move needs keyframes on both sides of each jump.
    let moves: Vec<u64> = file.bones["a"]
        .translation
        .as_ref()
        .unwrap()
        .iter()
        .map(|k| k.0)
        .collect();
    assert_eq!(moves, [0, 29, 30, 59, 60]);
}

#[test]
fn a_steady_turn_keeps_its_two_ends_and_a_still_channel_is_left_out() {
    let file = convert_json(&animated_gltf(), "turn").unwrap();
    assert_eq!(file.length, 60);
    assert_eq!(file.bones.keys().collect::<Vec<_>>(), ["b"]);
    let b = &file.bones["b"];
    assert_eq!(b.translation, None);
    // b's bind pose is its rest pose, so the offset is the turn itself.
    assert_eq!(
        b.rotation,
        Some(vec![(0, (0.0, 0.0, 0.0)), (60, (0.0, 0.0, 90.0))])
    );
}

#[test]
fn converting_twice_writes_the_same_bytes_and_reports_them() {
    let dir = std::env::temp_dir().join(format!("loom_clip_import_tests_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("m.glb");
    std::fs::write(&model, glb(&animated_gltf(), &animated_bin())).unwrap();
    let (one, two) = (dir.join("one.ron"), dir.join("two.ron"));
    let report = convert(&model, "wave", &one, true).unwrap();
    convert(&model, "wave", &two, true).unwrap();
    let text = std::fs::read_to_string(&one).unwrap();
    assert_eq!(text, std::fs::read_to_string(&two).unwrap());
    let head = format!(
        "// Converted by `loom clip` from {}, animation \"wave\".\n(\n    length: 60,\n    looping: true,\n",
        model.display()
    );
    assert!(text.starts_with(&head), "{text}");
    let keyframes: usize = {
        let file = to_clip(&std::fs::read(&model).unwrap(), "wave", true).unwrap();
        file.bones
            .values()
            .map(|b| {
                b.rotation
                    .iter()
                    .chain(&b.translation)
                    .map(Vec::len)
                    .sum::<usize>()
            })
            .sum()
    };
    assert_eq!(
        serde_json::to_string(&report).unwrap(),
        format!(
            r#"{{"status":"ok","path":{},"animation":"wave","out":{},"length":60,"bones":2,"keyframes":{keyframes}}}"#,
            json!(model.display().to_string()),
            json!(one.display().to_string())
        )
    );
}

#[test]
fn an_animation_the_model_does_not_have_is_an_error_listing_the_ones_it_has() {
    assert_eq!(
        error_of(&animated_gltf(), "run"),
        r#"the model has no animation "run": it has "wave" and "turn""#
    );
}

#[test]
fn a_model_that_cannot_be_skinned_cannot_be_converted() {
    let mut json = animated_gltf();
    json.as_object_mut().unwrap().remove("skins");
    json["nodes"][3].as_object_mut().unwrap().remove("skin");
    assert_eq!(error_of(&json, "wave"), "the model has no skin");
}

#[test]
fn a_channel_on_a_node_outside_the_skin_is_an_error() {
    let mut json = animated_gltf();
    json["animations"][0]["channels"][1]["target"]["node"] = json!(3);
    assert_eq!(
        error_of(&json, "wave"),
        r#"animation "wave" channel 1 moves node 3 ("body"), which is not a joint of the skin"#
    );
}

#[test]
fn cubicspline_interpolation_is_an_error() {
    let mut json = animated_gltf();
    json["animations"][1]["samplers"][1] =
        json!({"input": 12, "output": 15, "interpolation": "CUBICSPLINE"});
    assert_eq!(
        error_of(&json, "turn"),
        r#"animation "turn" channel 1 uses CUBICSPLINE interpolation, which loom does not support"#
    );
}

#[test]
fn a_scale_off_the_bind_pose_is_an_error() {
    let mut json = animated_gltf();
    json["animations"][1]["samplers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"input": 12, "output": 16}));
    json["animations"][1]["channels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sampler": 2, "target": {"node": 1, "path": "scale"}}));
    assert_eq!(
        error_of(&json, "turn"),
        r#"animation "turn" scales joint "a" at tick 1, but clips have no scale"#
    );
}
