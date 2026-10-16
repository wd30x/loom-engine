//! Tests of skinning and posing (`docs/architecture.md` section 19,
//! "骨骼和动画片段") on a two-bone model built here and on the humanoid,
//! and of the load errors of animated nodes.

use std::path::PathBuf;

use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Quat, Vec3};
use loom_core::debug::DebugDraw;
use loom_render::SkinnedVertex;
use serde_json::{Value, json};

use super::*;
use crate::model::tests::glb;
use crate::model::{Imported, Models, Skeleton, Skinned, import, load_file};

/// Bone b's bind pose: 1 up from a (at the origin), turned 45° about Z.
pub(crate) fn b_bind() -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_rotation_z(45f32.to_radians()), Vec3::Y)
}

/// Four vertices, two triangles: (0,0,0) and (1,0,0) on bone a, (0,2,0)
/// and (1,2,0) on bone b. The skin lists b first: joint 0 is b, 1 is a.
/// Byte offsets: positions 0, normals 48, joints 96, weights 112, u16
/// indices 176, inverse bind matrices (b, a) 188, joints with an index 5
/// on vertex 0 at 316, weights summing to 0 on vertex 0 at 332, weights
/// (3, 1) on vertex 0 and 2 on the rest at 396; 460 in all.
pub(crate) fn two_bone_bin() -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let floats =
        |bin: &mut Vec<u8>, fs: &[f32]| bin.extend(fs.iter().flat_map(|f| f.to_le_bytes()));
    floats(
        &mut bin,
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.0, 0.0, 1.0, 2.0, 0.0],
    );
    floats(&mut bin, &[0.0, 0.0, 1.0].repeat(4));
    bin.extend([1u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    floats(&mut bin, &[1.0, 0.0, 0.0, 0.0].repeat(4));
    for i in [0u16, 1, 2, 1, 3, 2] {
        bin.extend(i.to_le_bytes());
    }
    floats(&mut bin, &b_bind().inverse().to_cols_array());
    floats(&mut bin, &Mat4::IDENTITY.to_cols_array());
    bin.extend([5u8, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    floats(&mut bin, &[0.0; 4]);
    floats(&mut bin, &[1.0, 0.0, 0.0, 0.0].repeat(3));
    floats(&mut bin, &[3.0, 1.0, 0.0, 0.0]);
    floats(&mut bin, &[2.0, 0.0, 0.0, 0.0].repeat(3));
    assert_eq!(bin.len(), 460);
    bin
}

/// The glTF of [`two_bone_bin`]: root node `rig` with joints `a` → `b`
/// and the skinned mesh node `body`.
pub(crate) fn two_bone_gltf() -> Value {
    let (_, r, t) = b_bind().to_scale_rotation_translation();
    json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "rig", "children": [1, 3]},
            {"name": "a", "children": [2]},
            {"name": "b", "translation": t.to_array(), "rotation": r.to_array()},
            {"name": "body", "mesh": 0, "skin": 0}
        ],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3},
            "indices": 4
        }]}],
        "skins": [{"joints": [2, 1], "inverseBindMatrices": 5}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
             "min": [0.0, 0.0, 0.0], "max": [1.0, 2.0, 0.0]},
            {"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5121, "count": 4, "type": "VEC4"},
            {"bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC4"},
            {"bufferView": 4, "componentType": 5123, "count": 6, "type": "SCALAR"},
            {"bufferView": 5, "componentType": 5126, "count": 2, "type": "MAT4"},
            {"bufferView": 6, "componentType": 5121, "count": 4, "type": "VEC4"},
            {"bufferView": 7, "componentType": 5126, "count": 4, "type": "VEC4"},
            {"bufferView": 8, "componentType": 5126, "count": 4, "type": "VEC4"}
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 48},
            {"buffer": 0, "byteOffset": 48, "byteLength": 48},
            {"buffer": 0, "byteOffset": 96, "byteLength": 16},
            {"buffer": 0, "byteOffset": 112, "byteLength": 64},
            {"buffer": 0, "byteOffset": 176, "byteLength": 12},
            {"buffer": 0, "byteOffset": 188, "byteLength": 128},
            {"buffer": 0, "byteOffset": 316, "byteLength": 16},
            {"buffer": 0, "byteOffset": 332, "byteLength": 64},
            {"buffer": 0, "byteOffset": 396, "byteLength": 64}
        ],
        "buffers": [{"byteLength": 460}]
    })
}

fn import_two_bone(json: &Value) -> Imported {
    import(&glb(json, &two_bone_bin())).expect("the static mesh imports")
}

fn skinned_of(json: &Value) -> Skinned {
    import_two_bone(json)
        .skinned
        .expect("the model can be skinned")
}

fn skin_error(json: &Value) -> String {
    match import_two_bone(json).skinned {
        Ok(_) => panic!("the model should not be skinnable"),
        Err(e) => e,
    }
}

fn clip(src: &str) -> Clip {
    Clip::parse(src).expect("a valid clip")
}

/// Where the joint matrices `m` put vertex `v`.
fn skin(v: &SkinnedVertex, m: &[Mat4]) -> Vec3 {
    let p = Vec3::from_array(v.position);
    v.joints
        .iter()
        .zip(v.weights)
        .map(|(&j, w)| m[usize::from(j)].transform_point3(p) * w)
        .sum()
}

/// The model-space pose of `skeleton` by `clip` alone, `t` ticks in.
fn pose_by(skeleton: &Skeleton, clip: &Clip, t: u64) -> Vec<Mat4> {
    pose(skeleton, |bone| {
        clip.sample(bone, t * loom_scene::CTICKS)
            .unwrap_or((Quat::IDENTITY, Vec3::ZERO))
    })
}

/// The joint matrices of `skeleton` posed by `clip` at tick `t`.
fn matrices(skeleton: &Skeleton, clip: &Clip, t: u64) -> Vec<Mat4> {
    joint_matrices(skeleton, &pose_by(skeleton, clip, t))
}

/// The skinned positions of `skinned` posed by `clip` at tick `t`.
fn posed(skinned: &Skinned, clip: &Clip, t: u64) -> Vec<Vec3> {
    let m = matrices(&skinned.skeleton, clip, t);
    skinned.vertices.iter().map(|v| skin(v, &m)).collect()
}

fn assert_near(got: &[Vec3], want: &[Vec3]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!((*g - *w).abs().max_element() < 1e-5, "{got:?} != {want:?}");
    }
}

#[test]
fn skeleton_puts_parents_first_and_vertices_index_it() {
    let s = skinned_of(&two_bone_gltf());
    let names: Vec<_> = s.skeleton.joints.iter().map(|j| j.name.clone()).collect();
    assert_eq!(names, [Some("a".to_owned()), Some("b".to_owned())]);
    assert_eq!(s.skeleton.joints[1].parent, Some(0));
    assert_eq!(s.skeleton.joints[0].parent, None);
    let joints: Vec<_> = s.vertices.iter().map(|v| v.joints[0]).collect();
    assert_eq!(joints, [0, 0, 1, 1]);
    assert_eq!(s.indices, [0, 1, 2, 1, 3, 2]);
}

#[test]
fn empty_clip_gives_identity_joint_matrices_and_the_static_mesh() {
    let imported = import_two_bone(&two_bone_gltf());
    let skinned = imported.skinned.expect("the model can be skinned");
    let empty = clip("(length: 1, looping: true, bones: {})");
    for m in matrices(&skinned.skeleton, &empty, 0) {
        assert!(m.abs_diff_eq(Mat4::IDENTITY, 1e-5), "{m}");
    }
    let statics: Vec<Vec3> = imported
        .mesh
        .vertices
        .iter()
        .map(|v| Vec3::from_array(v.position))
        .collect();
    assert_near(&posed(&skinned, &empty, 0), &statics);
}

#[test]
fn rotation_offset_turns_the_bone_about_its_position_along_the_model_axes() {
    let s = skinned_of(&two_bone_gltf());
    // b is at (0, 1, 0), turned 45°: the offset is still about model Z.
    let c =
        clip(r#"(length: 10, looping: false, bones: {"b": (rotation: [(0, (0.0, 0.0, 90.0))])})"#);
    let want = [
        Vec3::ZERO,
        Vec3::X,
        // (0, 2, 0) is (0, 1, 0) from b: turned 90° it is (-1, 0, 0).
        Vec3::new(-1.0, 1.0, 0.0),
        // (1, 2, 0) is (1, 1, 0) from b: turned it is (-1, 1, 0).
        Vec3::new(-1.0, 2.0, 0.0),
    ];
    assert_near(&posed(&s, &c, 3), &want);
}

#[test]
fn translation_offset_of_the_parent_moves_its_whole_subtree() {
    let s = skinned_of(&two_bone_gltf());
    let c = clip(
        r#"(length: 10, looping: false, bones: {"a": (translation: [(0, (0.0, 1.0, 0.0))])})"#,
    );
    let want = [
        Vec3::Y,
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 3.0, 0.0),
        Vec3::new(1.0, 3.0, 0.0),
    ];
    assert_near(&posed(&s, &c, 0), &want);
}

#[test]
fn translation_offset_of_a_turned_bone_moves_along_the_model_axes() {
    let s = skinned_of(&two_bone_gltf());
    let c = clip(
        r#"(length: 10, looping: false, bones: {"b": (translation: [(0, (0.5, 0.0, 0.0))])})"#,
    );
    let want = [
        Vec3::ZERO,
        Vec3::X,
        Vec3::new(0.5, 2.0, 0.0),
        Vec3::new(1.5, 2.0, 0.0),
    ];
    assert_near(&posed(&s, &c, 0), &want);
}

#[test]
fn skeleton_draw_is_a_cross_at_each_joint_and_a_segment_to_its_parent() {
    let s = skinned_of(&two_bone_gltf());
    // a moved 1 up (b with it), the model 2 along X.
    let c = clip(
        r#"(length: 10, looping: false, bones: {"a": (translation: [(0, (0.0, 1.0, 0.0))])})"#,
    );
    let mut draw = DebugDraw::new();
    let world = Mat4::from_translation(Vec3::new(2.0, 0.0, 0.0));
    draw_skeleton(&s.skeleton, &pose_by(&s.skeleton, &c, 0), world, &mut draw);
    let lines: Vec<(Vec3, Vec3)> = draw.lines().iter().map(|l| (l.a, l.b)).collect();
    let (a, b) = (Vec3::new(2.0, 1.0, 0.0), Vec3::new(2.0, 2.0, 0.0));
    let cross =
        |p: Vec3| [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| (p - axis * 0.015, p + axis * 0.015));
    let want: Vec<(Vec3, Vec3)> = cross(a)
        .into_iter()
        .chain(cross(b))
        .chain([(a, b)])
        .collect();
    assert_eq!(lines.len(), want.len());
    for (got, want) in lines.iter().zip(&want) {
        assert!(
            got.0.abs_diff_eq(want.0, 1e-5) && got.1.abs_diff_eq(want.1, 1e-5),
            "{got:?} != {want:?}"
        );
    }
    assert!(
        draw.lines()
            .iter()
            .all(|l| l.color == Vec3::new(1.0, 0.8, 0.0))
    );
}

#[test]
fn skinned_weights_are_normalized() {
    let mut json = two_bone_gltf();
    json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(8);
    let s = skinned_of(&json);
    // Vertex 0: joints (a, b) weighing 3 and 1 in the file.
    assert_eq!(s.vertices[0].joints[..2], [0, 1]);
    assert_eq!(s.vertices[0].weights, [0.75, 0.25, 0.0, 0.0]);
    assert_eq!(s.vertices[1].weights, [1.0, 0.0, 0.0, 0.0]);
}

#[test]
fn humanoid_skinned_in_its_bind_pose_is_its_static_mesh() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/humanoid.glb");
    let imported = load_file(&path).expect("the humanoid imports");
    let skinned = imported.skinned.expect("the humanoid can be skinned");
    assert_eq!(skinned.skeleton.joints.len(), 19);
    let empty = clip("(length: 1, looping: true, bones: {})");
    let posed = posed(&skinned, &empty, 0);
    assert_eq!(posed.len(), imported.mesh.vertices.len());
    let error = posed
        .iter()
        .zip(&imported.mesh.vertices)
        .map(|(p, v)| (*p - Vec3::from_array(v.position)).abs().max_element())
        .fold(0.0_f32, f32::max);
    // The mesh node is not transformed, so the static mesh is in model
    // space as the skinned one is.
    let bytes = std::fs::read(&path).expect("the humanoid is readable");
    let gltf = gltf::Gltf::from_slice(&bytes).expect("a glTF binary");
    let mesh_node = gltf
        .nodes()
        .find(|n| n.mesh().is_some())
        .expect("a mesh node");
    let transform = Mat4::from_cols_array_2d(&mesh_node.transform().matrix());
    eprintln!(
        "humanoid bind pose: skinned vs static max error {error:e}; mesh node {:?} transform identity: {}",
        mesh_node.name(),
        transform == Mat4::IDENTITY
    );
    assert_eq!(transform, Mat4::IDENTITY);
    assert!(error < 1e-4, "{error}");
    assert!(skinned.indices == imported.mesh.indices);
}

#[test]
fn a_model_without_exactly_one_skin_cannot_be_skinned() {
    let mut none = two_bone_gltf();
    none.as_object_mut().unwrap().remove("skins");
    none["nodes"][3].as_object_mut().unwrap().remove("skin");
    assert_eq!(skin_error(&none), "the model has no skin");
    let mut two = two_bone_gltf();
    two["skins"] = json!([{"joints": [2, 1], "inverseBindMatrices": 5}, {"joints": [1]}]);
    assert_eq!(skin_error(&two), "the model has 2 skins, not one");
}

#[test]
fn a_primitive_without_joints_0_or_weights_0_cannot_be_skinned() {
    for (attribute, message) in [
        ("JOINTS_0", "mesh 0 primitive 0 has no JOINTS_0"),
        ("WEIGHTS_0", "mesh 0 primitive 0 has no WEIGHTS_0"),
    ] {
        let mut json = two_bone_gltf();
        json["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove(attribute);
        assert_eq!(skin_error(&json), message);
    }
}

#[test]
fn a_primitive_with_joints_1_cannot_be_skinned() {
    let mut json = two_bone_gltf();
    json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_1"] = json!(2);
    json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_1"] = json!(3);
    assert_eq!(
        skin_error(&json),
        "mesh 0 primitive 0 has JOINTS_1: at most 4 joints a vertex"
    );
}

#[test]
fn a_joint_index_past_the_skin_cannot_be_skinned() {
    let mut json = two_bone_gltf();
    json["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"] = json!(6);
    assert_eq!(
        skin_error(&json),
        "mesh 0 primitive 0 has joint index 5, but the skin has 2 joints"
    );
}

#[test]
fn weights_summing_to_zero_cannot_be_skinned() {
    let mut json = two_bone_gltf();
    json["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = json!(7);
    assert_eq!(
        skin_error(&json),
        "mesh 0 primitive 0 vertex 0 has weights that sum to 0"
    );
}

/// A fresh directory for one test's files.
fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom_anim_tests_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp directory");
    dir
}

/// A scene with node `g/k` showing `model` with `animation`.
fn scene_of(model: &str, animation: &str) -> Scene {
    Scene::parse(&format!(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "g", children: [(name: "k", model: {model:?}, animation: {animation})]),
        ])"#
    ))
    .expect("a valid scene")
}

fn forward(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

#[test]
fn animated_node_errors_name_the_node_the_clip_and_the_file() {
    let dir = test_dir("errors");
    let model = dir.join("m.glb");
    std::fs::write(&model, glb(&two_bone_gltf(), &two_bone_bin())).unwrap();
    let mut unskinned = two_bone_gltf();
    unskinned["nodes"][3]
        .as_object_mut()
        .unwrap()
        .remove("skin");
    unskinned.as_object_mut().unwrap().remove("skins");
    let plain = dir.join("plain.glb");
    std::fs::write(&plain, glb(&unskinned, &two_bone_bin())).unwrap();
    let good = dir.join("good.ron");
    std::fs::write(
        &good,
        r#"(length: 10, looping: true, bones: {"b": (rotation: [(0, (0.0, 0.0, 0.0))])})"#,
    )
    .unwrap();
    let tail = dir.join("tail.ron");
    std::fs::write(
        &tail,
        r#"(length: 10, looping: true, bones: {"tail": (rotation: [(0, (0.0, 0.0, 0.0))])})"#,
    )
    .unwrap();
    let broken = dir.join("broken.ron");
    std::fs::write(&broken, "(length: 0, looping: true, bones: {})").unwrap();
    let (model, plain) = (forward(&model), forward(&plain));
    let clips = |path: &std::path::Path| {
        format!(
            r#"(clips: [("idle", {:?}), ("wave", {:?})], play: "idle")"#,
            forward(&good),
            forward(path)
        )
    };
    let error = |scene: &Scene| format!("{:#}", Models::load(scene).unwrap_err());

    Models::load(&scene_of(&model, &clips(&good))).expect("a good animated node");
    assert_eq!(
        error(&scene_of(&plain, &clips(&good))),
        format!("node \"g/k\": model {plain}: the model has no skin")
    );
    assert_eq!(
        error(&scene_of(&model, &clips(&tail))),
        format!(
            "node \"g/k\": animation clip \"wave\" {}: bone \"tail\" is not a joint of the model's skin",
            forward(&tail)
        )
    );
    assert_eq!(
        error(&scene_of(&model, &clips(&broken))),
        format!(
            "node \"g/k\": animation clip \"wave\" {}: length must be greater than 0",
            forward(&broken)
        )
    );
    let missing = dir.join("missing.ron");
    let e = error(&scene_of(&model, &clips(&missing)));
    let head = format!(
        "node \"g/k\": animation clip \"wave\" {}: reading the file: ",
        forward(&missing)
    );
    assert!(e.starts_with(&head), "{e}");
    // Without animation, the unskinned model draws as before.
    let scene = Scene::parse(&format!(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "k", model: {plain:?}),
        ])"#
    ))
    .unwrap();
    Models::load(&scene).expect("a static model needs no skin");
}

/// The index of joint `name` of `skeleton`.
fn joint(skeleton: &Skeleton, name: &str) -> usize {
    skeleton
        .joints
        .iter()
        .position(|j| j.name.as_deref() == Some(name))
        .unwrap()
}

/// The skeleton of [`two_bone_gltf`].
fn two_bone_skeleton() -> Skeleton {
    import(&glb(&two_bone_gltf(), &two_bone_bin()))
        .unwrap()
        .skinned
        .unwrap()
        .skeleton
}

#[test]
fn anchor_in_the_bind_pose_only_moves_to_the_joint() {
    let skeleton = two_bone_skeleton();
    // b's bind pose is at (0, 1, 0), turned 45° about Z; the anchor keeps
    // the model's axes.
    let b = joint(&skeleton, "b");
    let bind = anchor(&skeleton, None, b);
    assert_eq!(bind, Mat4::from_translation(Vec3::Y));
    let still = pose(&skeleton, |_| (Quat::IDENTITY, Vec3::ZERO));
    let posed = anchor(&skeleton, Some(&still), b);
    assert!(posed.abs_diff_eq(bind, 1e-6), "{posed}");
}

#[test]
fn anchor_turns_with_its_bone_along_the_model_axes() {
    let skeleton = two_bone_skeleton();
    // b turned 90° about the model's Z: an offset of 0.1 m up points to
    // -X.
    let turned = pose(&skeleton, |bone| match bone {
        "b" => (Quat::from_rotation_z(FRAC_PI_2), Vec3::ZERO),
        _ => (Quat::IDENTITY, Vec3::ZERO),
    });
    let b = joint(&skeleton, "b");
    let a = anchor(&skeleton, Some(&turned), b);
    let at = a.transform_point3(Vec3::new(0.0, 0.1, 0.0));
    assert!(at.abs_diff_eq(Vec3::new(-0.1, 1.0, 0.0), 1e-6), "{at}");
    // a moved 0.5 m along X carries b and the anchor with it.
    let moved = pose(&skeleton, |bone| match bone {
        "a" => (Quat::IDENTITY, Vec3::new(0.5, 0.0, 0.0)),
        _ => (Quat::IDENTITY, Vec3::ZERO),
    });
    let at = anchor(&skeleton, Some(&moved), b).transform_point3(Vec3::ZERO);
    assert!(at.abs_diff_eq(Vec3::new(0.5, 1.0, 0.0), 1e-6), "{at}");
}

#[test]
fn bone_errors_name_the_node_the_bone_and_the_model() {
    let dir = test_dir("bones");
    let model = dir.join("m.glb");
    std::fs::write(&model, glb(&two_bone_gltf(), &two_bone_bin())).unwrap();
    let mut unskinned = two_bone_gltf();
    unskinned["nodes"][3]
        .as_object_mut()
        .unwrap()
        .remove("skin");
    unskinned.as_object_mut().unwrap().remove("skins");
    let plain = dir.join("plain.glb");
    std::fs::write(&plain, glb(&unskinned, &two_bone_bin())).unwrap();
    let (model, plain) = (forward(&model), forward(&plain));
    let scene = |model: &str, bone: &str| {
        Scene::parse(&format!(
            r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
                (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
                (name: "k", model: {model:?}, children: [(name: "f", bone: {bone:?}, mesh: Cube)]),
            ])"#
        ))
        .expect("a valid scene")
    };
    let error = |scene: &Scene| format!("{:#}", Models::load(scene).unwrap_err());
    // A static model with a skin takes bones: its bind pose.
    Models::load(&scene(&model, "b")).expect("a bone of the skin");
    assert_eq!(
        error(&scene(&plain, "b")),
        format!("node \"k/f\": bone \"b\": model {plain}: the model has no skin")
    );
    assert_eq!(
        error(&scene(&model, "tail")),
        format!("node \"k/f\": bone \"tail\": model {model}: not a joint of the model's skin")
    );
}
