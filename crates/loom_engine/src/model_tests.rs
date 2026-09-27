//! Tests of the glTF import (`docs/architecture.md` section 19) on small
//! `.glb` files built here, and of `Models::load`'s error format.

use glam::Vec3;
use serde_json::{Value, json};

use super::*;

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
    out.extend(u32::try_from(total).unwrap().to_le_bytes());
    out.extend(u32::try_from(text.len()).unwrap().to_le_bytes());
    out.extend(b"JSON");
    out.extend(text);
    out.extend(u32::try_from(bin.len()).unwrap().to_le_bytes());
    out.extend(b"BIN\0");
    out.extend(bin);
    out
}

/// One triangle: (0,0,0), (1,0,0), (0,1,0), normals +X, +Y and (1,1,0)
/// normalized. Byte offsets: positions 0, normals 36, u8 indices 72, u16
/// indices 76, u32 indices 84; 96 in all.
fn triangle_bin() -> Vec<u8> {
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let floats: [f32; 18] = [
        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, // positions
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, h, h, 0.0, // normals
    ];
    let mut bin: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    bin.extend([0u8, 1, 2, 0]); // u8: 0 1 2, padded
    for i in [1u16, 2, 0, 0] {
        bin.extend(i.to_le_bytes()); // u16: 1 2 0, padded
    }
    for i in [2u32, 0, 1] {
        bin.extend(i.to_le_bytes()); // u32: 2 0 1
    }
    bin
}

/// The glTF of [`triangle_bin`]: node 0 (translation (1, 2, 3), 90° about
/// Z, scale (2, 3, 1)) with child node 1 (translation (0, 0, 5)), both
/// showing mesh 0, whose three primitives are the triangle with material
/// 0 and u8 indices, material 1 and u16 indices, no material and u32
/// indices.
fn triangle_gltf() -> Value {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"mesh": 0, "translation": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, s, s],
             "scale": [2.0, 3.0, 1.0], "children": [1]},
            {"mesh": 0, "translation": [0.0, 0.0, 5.0]}
        ],
        "meshes": [{"primitives": [
            {"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 2, "material": 0},
            {"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 3, "material": 1},
            {"attributes": {"POSITION": 0, "NORMAL": 1}, "indices": 4}
        ]}],
        "materials": [
            {"name": "blue", "pbrMetallicRoughness": {"baseColorFactor": [0.1, 0.2, 0.3, 0.5]}},
            {"pbrMetallicRoughness": {"baseColorFactor": [0.9, 0.8, 0.7, 1.0]}}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
             "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5121, "count": 3, "type": "SCALAR"},
            {"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"},
            {"bufferView": 4, "componentType": 5125, "count": 3, "type": "SCALAR"}
        ],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 36},
            {"buffer": 0, "byteOffset": 72, "byteLength": 3},
            {"buffer": 0, "byteOffset": 76, "byteLength": 6},
            {"buffer": 0, "byteOffset": 84, "byteLength": 12}
        ],
        "buffers": [{"byteLength": 96}]
    })
}

fn import_json(json: &Value) -> Result<Imported> {
    import(&glb(json, &triangle_bin()))
}

fn error_of(json: &Value) -> String {
    match import_json(json) {
        Ok(_) => panic!("the model should be rejected"),
        Err(e) => format!("{e:#}"),
    }
}

fn assert_close(got: [f32; 3], want: [f32; 3]) {
    let d = Vec3::from_array(got) - Vec3::from_array(want);
    assert!(d.abs().max_element() < 1e-6, "{got:?} != {want:?}");
}

#[test]
fn nodes_place_their_meshes_by_the_node_tree_and_normals_by_the_inverse_transpose() {
    let m = import_json(&triangle_gltf()).unwrap().mesh;
    assert_eq!((m.vertices.len(), m.indices.len()), (18, 18));
    // Node 0: p -> (1 - 3y, 2 + 2x, 3 + z), worked out by hand from
    // T(1, 2, 3) R(90° about Z) S(2, 3, 1).
    let node0 = [[1.0, 2.0, 3.0], [1.0, 4.0, 3.0], [-2.0, 2.0, 3.0]];
    // Node 1 is 5 further along the parent's Z: (1 - 3y, 2 + 2x, 8 + z).
    let node1 = [[1.0, 2.0, 8.0], [1.0, 4.0, 8.0], [-2.0, 2.0, 8.0]];
    // Normals: R S^-1 n, normalized. +X -> +Y; +Y -> -X; (1, 1, 0) ->
    // (-1/3, 1/2, 0) -> (-2, 3, 0) / sqrt(13).
    let r13 = 13.0_f32.sqrt();
    let normals = [
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [-2.0 / r13, 3.0 / r13, 0.0],
    ];
    for (k, v) in m.vertices.iter().enumerate() {
        let (node, corner) = (k / 9, k % 3);
        let want = if node == 0 { node0 } else { node1 };
        assert_close(v.position, want[corner]);
        assert_close(v.normal, normals[corner]);
    }
}

#[test]
fn each_vertex_takes_its_primitives_base_color_without_alpha_or_white() {
    let m = import_json(&triangle_gltf()).unwrap().mesh;
    let colors = [[0.1, 0.2, 0.3], [0.9, 0.8, 0.7], [1.0, 1.0, 1.0]];
    for (k, v) in m.vertices.iter().enumerate() {
        assert_eq!(v.color, colors[(k / 3) % 3], "vertex {k}");
    }
}

#[test]
fn u8_u16_and_u32_indices_are_read_and_offset_per_primitive() {
    let m = import_json(&triangle_gltf()).unwrap().mesh;
    assert_eq!(
        m.indices,
        [
            0, 1, 2, 4, 5, 3, 8, 6, 7, // node 0: u8, u16, u32
            9, 10, 11, 13, 14, 12, 17, 15, 16, // node 1
        ]
    );
}

#[test]
fn a_mirroring_node_turns_the_winding_around() {
    let mut json = triangle_gltf();
    json["nodes"] = json!([{"mesh": 0, "scale": [-1.0, 1.0, 1.0]}]);
    let m = import_json(&json).unwrap().mesh;
    assert_eq!(&m.indices[..3], [0, 2, 1]);
    assert_close(m.vertices[1].position, [-1.0, 0.0, 0.0]);
    assert_close(m.vertices[0].normal, [-1.0, 0.0, 0.0]);
}

#[test]
fn materials_skins_and_animations_are_described_in_file_order() {
    let mut json = triangle_gltf();
    let imported = import_json(&json).unwrap();
    assert_eq!(
        imported.materials,
        [
            MaterialInfo {
                name: Some("blue".to_owned()),
                color: [0.1, 0.2, 0.3],
            },
            MaterialInfo {
                name: None,
                color: [0.9, 0.8, 0.7],
            },
        ]
    );
    assert!(imported.skins.is_empty() && imported.animations.is_empty());
    // Node 1 is a joint whose parent (node 0) is not; node 2 is a joint
    // under node 1. The skin lists node 2 first.
    json["nodes"][0]["name"] = json!("body");
    json["nodes"][1]["name"] = json!("hips");
    json["nodes"][1]["children"] = json!([2]);
    json["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "spine"}));
    json["skins"] = json!([{"name": "rig", "joints": [2, 1]}]);
    let skins = import_json(&json).unwrap().skins;
    assert_eq!(
        skins,
        [SkinInfo {
            name: Some("rig".to_owned()),
            joints: vec![
                JointInfo {
                    name: Some("spine".to_owned()),
                    parent: Some("hips".to_owned()),
                },
                JointInfo {
                    name: Some("hips".to_owned()),
                    parent: None,
                },
            ],
        }]
    );
}

#[test]
fn a_file_without_a_default_scene_uses_its_first_scene() {
    let mut json = triangle_gltf();
    json.as_object_mut().unwrap().remove("scene");
    assert_eq!(import_json(&json).unwrap().mesh.vertices.len(), 18);
}

// ---------------------------------------------------------------- errors

#[test]
fn a_file_that_is_not_gltf_2_0_is_rejected() {
    let mut json = triangle_gltf();
    json["asset"]["version"] = json!("1.0");
    assert_eq!(
        error_of(&json),
        "not glTF 2.0: the asset version is \"1.0\""
    );
    let e = match import(b"not a glb") {
        Ok(_) => panic!("not a glb"),
        Err(e) => format!("{e:#}"),
    };
    assert!(e.starts_with("not a valid glTF binary: "), "{e}");
}

#[test]
fn a_primitive_that_is_not_triangles_is_rejected() {
    let mut json = triangle_gltf();
    json["meshes"][0]["primitives"][2]["mode"] = json!(1);
    assert_eq!(error_of(&json), "mesh 0 primitive 2 is not triangles");
}

#[test]
fn a_primitive_without_position_or_normal_is_rejected() {
    let mut json = triangle_gltf();
    json["meshes"][0]["primitives"][1]["attributes"]
        .as_object_mut()
        .unwrap()
        .remove("NORMAL");
    assert_eq!(error_of(&json), "mesh 0 primitive 1 has no NORMAL");
    // The gltf crate's own validation rejects a missing POSITION first.
    let mut json = triangle_gltf();
    json["meshes"][0]["primitives"][0]["attributes"] = json!({"NORMAL": 1});
    assert_eq!(
        error_of(&json),
        "not a valid glTF binary: invalid glTF: \
         meshes[0].primitives[0].attributes[\"POSITION\"]: Missing data;"
    );
}

#[test]
fn a_material_with_a_texture_is_rejected() {
    let mut json = triangle_gltf();
    json["images"] = json!([{"uri": "skin.png"}]);
    json["textures"] = json!([{"source": 0}]);
    json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index": 0});
    assert_eq!(error_of(&json), "material 0 (\"blue\") uses a texture");
    let mut json = triangle_gltf();
    json["images"] = json!([{"uri": "n.png"}]);
    json["textures"] = json!([{"source": 0}]);
    json["materials"][1]["normalTexture"] = json!({"index": 0});
    assert_eq!(error_of(&json), "material 1 uses a texture");
}

#[test]
fn a_sparse_accessor_is_rejected() {
    let mut json = triangle_gltf();
    json["accessors"][0]["sparse"] = json!({
        "count": 1,
        "indices": {"bufferView": 2, "componentType": 5121},
        "values": {"bufferView": 0}
    });
    assert_eq!(error_of(&json), "accessor 0 is sparse");
}

#[test]
fn a_default_scene_without_a_mesh_is_rejected() {
    let mut json = triangle_gltf();
    json["nodes"][0].as_object_mut().unwrap().remove("mesh");
    json["nodes"][0]["children"] = json!([]);
    assert_eq!(error_of(&json), "the default scene has no mesh");
}

#[test]
fn a_buffer_outside_the_glb_is_rejected() {
    let mut json = triangle_gltf();
    json["buffers"][0]["uri"] = json!("triangle.bin");
    assert_eq!(error_of(&json), "buffer 0 is not stored in the .glb");
}

#[test]
fn model_errors_name_the_node_and_the_file() {
    let dir = std::env::temp_dir().join(format!("loom_model_tests_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lines.glb");
    let mut json = triangle_gltf();
    json["meshes"][0]["primitives"][0]["mode"] = json!(1);
    std::fs::write(&path, glb(&json, &triangle_bin())).unwrap();
    let file = path.to_str().unwrap().replace('\\', "/");
    let scene = Scene::parse(&format!(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "a", children: [(name: "b", model: "{file}")]),
        ])"#
    ))
    .unwrap();
    let e = format!("{:#}", Models::load(&scene).unwrap_err());
    assert_eq!(
        e,
        format!("node \"a/b\": model {file}: mesh 0 primitive 0 is not triangles")
    );
    let missing = file.replace("lines.glb", "missing.glb");
    let scene = Scene::parse(&format!(
        r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "m", model: "{missing}"),
        ])"#
    ))
    .unwrap();
    let e = format!("{:#}", Models::load(&scene).unwrap_err());
    assert!(
        e.starts_with(&format!("node \"m\": model {missing}: reading the file: ")),
        "{e}"
    );
}
