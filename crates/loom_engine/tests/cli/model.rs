//! `loom model`: the description of a `.glb` (`docs/architecture.md`
//! section 19).

use std::fs;

use serde_json::{Value, json};

use super::common::{report, run_loom, run_loom_in, test_dir};

/// A `.glb` of `json` and the binary chunk `bin`.
fn glb(json: &Value, bin: &[u8]) -> Vec<u8> {
    let mut text = serde_json::to_vec(json).expect("JSON");
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let mut bin = bin.to_vec();
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::new();
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    for (len, kind) in [(total, None), (text.len(), Some(b"JSON"))] {
        out.extend(u32::try_from(len).expect("small").to_le_bytes());
        if let Some(kind) = kind {
            out.extend(kind);
        }
    }
    out.extend(text);
    out.extend(u32::try_from(bin.len()).expect("small").to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// One red triangle, a two-joint skin and a one-channel animation whose
/// last keyframe is at 1.25 s.
fn minimal_glb() -> Vec<u8> {
    let floats: [f32; 18] = [
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, // positions
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, // normals
    ];
    let mut bin: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    for i in [0u16, 1, 2, 0] {
        bin.extend(i.to_le_bytes()); // indices at 72, padded to 80
    }
    let keys: [f32; 12] = [0.0, 0.5, 1.25, 0.0, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0];
    bin.extend(keys.iter().flat_map(|f| f.to_le_bytes())); // times at 80, moves at 92
    let json = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1]}],
        "nodes": [
            {"name": "body", "mesh": 0},
            {"name": "hips", "children": [2]},
            {"name": "spine"}
        ],
        "meshes": [{"primitives": [
            {"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0}
        ]}],
        "materials": [{"name": "red", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}}],
        "skins": [{"name": "rig", "joints": [1, 2]}],
        "animations": [{"name": "wave",
            "samplers": [{"input": 3, "output": 4}],
            "channels": [{"sampler": 0, "target": {"node": 2, "path": "translation"}}]}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
             "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5123, "count": 3, "type": "SCALAR"},
            {"bufferView": 3, "componentType": 5126, "count": 3, "type": "SCALAR",
             "min": [0.0], "max": [1.25]},
            {"bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC3"}
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 36},
            {"buffer": 0, "byteOffset": 72, "byteLength": 6},
            {"buffer": 0, "byteOffset": 80, "byteLength": 12},
            {"buffer": 0, "byteOffset": 92, "byteLength": 36}
        ],
        "buffers": [{"byteLength": 128}]
    });
    glb(&json, &bin)
}

#[test]
fn model_command_describes_a_minimal_glb_byte_for_byte() {
    let dir = test_dir("model_minimal");
    fs::write(dir.join("m.glb"), minimal_glb()).expect("writing the model");
    let out = run_loom_in(&dir, &["model", "m.glb"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    assert_eq!(
        out.stdout,
        concat!(
            r#"{"status":"ok","path":"m.glb","vertices":3,"triangles":1,"#,
            r#""bounds":{"min":[0.0,0.0,0.0],"max":[1.0,1.0,0.0]},"#,
            r#""materials":[{"name":"red","color":[1.0,0.0,0.0]}],"#,
            r#""skins":[{"name":"rig","joints":[{"name":"hips","parent":null},"#,
            r#"{"name":"spine","parent":"hips"}]}],"#,
            r#""animations":[{"name":"wave","seconds":1.25,"channels":1}]}"#,
            "\n"
        )
    );
}

#[test]
fn model_command_rejects_a_bad_file_with_exit_code_1() {
    let dir = test_dir("model_bad");
    fs::write(dir.join("bad.glb"), b"not a glb").expect("writing the file");
    let out = run_loom_in(&dir, &["model", "bad.glb"]);
    assert_eq!(out.code, 1);
    let r = report(&out);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(
        error.starts_with("model bad.glb: not a valid glTF binary: "),
        "{error}"
    );
}

#[test]
fn humanoid_model_has_the_19_named_joints_no_animation_and_few_triangles() {
    let out = run_loom(&["model", "assets/models/humanoid.glb"]);
    assert_eq!(out.code, 0, "{}", out.stdout);
    let r = report(&out);
    assert!(r["triangles"].as_u64().expect("a count") <= 3000);
    assert_eq!(r["animations"], json!([]));
    let joints: Vec<(String, Value)> = r["skins"][0]["joints"]
        .as_array()
        .expect("one skin")
        .iter()
        .map(|j| {
            (
                j["name"].as_str().expect("named").to_owned(),
                j["parent"].clone(),
            )
        })
        .collect();
    let want = [
        ("root", None),
        ("hips", Some("root")),
        ("spine", Some("hips")),
        ("chest", Some("spine")),
        ("neck", Some("chest")),
        ("head", Some("neck")),
        ("upper_arm_l", Some("chest")),
        ("lower_arm_l", Some("upper_arm_l")),
        ("hand_l", Some("lower_arm_l")),
        ("upper_arm_r", Some("chest")),
        ("lower_arm_r", Some("upper_arm_r")),
        ("hand_r", Some("lower_arm_r")),
        ("weapon", Some("hand_r")),
        ("upper_leg_l", Some("hips")),
        ("lower_leg_l", Some("upper_leg_l")),
        ("foot_l", Some("lower_leg_l")),
        ("upper_leg_r", Some("hips")),
        ("lower_leg_r", Some("upper_leg_r")),
        ("foot_r", Some("lower_leg_r")),
    ];
    let want: Vec<(String, Value)> = want
        .iter()
        .map(|(n, p)| ((*n).to_owned(), p.map_or(Value::Null, Value::from)))
        .collect();
    assert_eq!(joints, want);
    // 1.8 m tall standing on y = 0 (the helmet's crest adds 2 cm).
    let bounds = &r["bounds"];
    assert_eq!(bounds["min"][1], json!(0.0));
    let top = bounds["max"][1].as_f64().expect("a number");
    assert!((1.79..1.83).contains(&top), "{top}");
}

#[test]
fn boss_model_has_the_humanoid_joints_no_animation_and_few_triangles() {
    let model = |path: &str| {
        let out = run_loom(&["model", path]);
        assert_eq!(out.code, 0, "{}", out.stdout);
        report(&out)
    };
    let (boss, humanoid) = (
        model("assets/models/boss.glb"),
        model("assets/models/humanoid.glb"),
    );
    assert!(boss["triangles"].as_u64().expect("a count") <= 3000);
    assert_eq!(boss["animations"], json!([]));
    // The same bones, names, parents and order: the clips' axes hold.
    assert_eq!(boss["skins"][0]["joints"], humanoid["skins"][0]["joints"]);
    // Standing on y = 0; the halberd's tip, above the 3 m head, is the top.
    let bounds = &boss["bounds"];
    assert_eq!(bounds["min"][1], json!(0.0));
    let top = bounds["max"][1].as_f64().expect("a number");
    assert!((3.15..3.22).contains(&top), "{top}");
}

#[test]
fn view_of_a_scene_with_a_missing_model_fails_before_rendering() {
    let dir = test_dir("model_missing_view");
    fs::write(
        dir.join("s.ron"),
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "g", children: [(name: "k", model: "missing.glb")]),
        ])"#,
    )
    .expect("writing the scene");
    let out = run_loom_in(&dir, &["view", "--scene", "s.ron", "--screenshot", "v.png"]);
    assert_eq!(out.code, 1);
    let error = report(&out)["error"].as_str().expect("an error").to_owned();
    assert!(
        error.starts_with(
            "invalid scene s.ron: node \"g/k\": model missing.glb: reading the file: "
        ),
        "{error}"
    );
    assert!(!dir.join("v.png").exists());
}
