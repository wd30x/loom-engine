use super::*;

const DEMO: &str = include_str!("../../loom_engine/src/demo.scene.ron");

/// A scene file with a camera node `cam` followed by `nodes`.
fn scene_with(nodes: &str) -> String {
    format!(
        "(version: 1, camera: \"cam\", ambient: (0.2, 0.2, 0.2), nodes: [\
         (name: \"cam\", camera: (fov_y: 60.0, near: 0.1, far: 100.0)), {nodes}])"
    )
}

/// Parses `nodes`, expects a node error, checks that the message names
/// `path`, and returns the problem.
fn node_problem(nodes: &str, path: &str) -> Problem {
    let err = Scene::parse(&scene_with(nodes)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains(&format!("{path:?}")), "{msg}");
    match err {
        SceneError::Node { problem, .. } => problem,
        other => panic!("expected a node error, got {other}"),
    }
}

fn assert_close(a: Vec3, b: Vec3) {
    assert!(a.abs_diff_eq(b, 1e-5), "{a} != {b}");
}

fn quat_bits(q: Quat) -> [u32; 4] {
    q.to_array().map(f32::to_bits)
}

#[test]
fn demo_scene_parses_into_four_nodes() {
    let scene = Scene::parse(DEMO).unwrap();
    let paths: Vec<_> = scene.ids().map(|id| scene.path(id)).collect();
    assert_eq!(paths, ["ground", "cube", "sun", "main_cam"]);
    assert_eq!(scene.camera(), scene.find("main_cam").unwrap());
    assert_eq!(scene.light(), scene.find("sun"));
    assert_eq!(scene.ambient(), Vec3::new(0.2, 0.2, 0.25));
    let cube = scene.node(scene.find("cube").unwrap());
    assert_eq!(cube.mesh.map(|m| m.kind), Some(MeshKind::Cube));
    assert_eq!(cube.transform.position, Vec3::new(0.0, 0.5, 0.0));
}

#[test]
fn scene_rejects_unsupported_version() {
    let src = scene_with("").replace("version: 1", "version: 2");
    let err = Scene::parse(&src).unwrap_err();
    assert!(matches!(err, SceneError::UnsupportedVersion(2)), "{err}");
}

#[test]
fn scene_rejects_an_invalid_name() {
    for name in ["Cube", "1a", "", "a-b", "_x"] {
        let p = node_problem(&format!("(name: \"{name}\")"), name);
        assert_eq!(p, Problem::InvalidName, "{name:?}");
    }
}

#[test]
fn scene_rejects_duplicate_sibling_names() {
    let p = node_problem(
        r#"(name: "arena", children: [(name: "pillar"), (name: "pillar")])"#,
        "arena/pillar",
    );
    assert_eq!(p, Problem::DuplicateName);
}

#[test]
fn scene_rejects_a_camera_path_to_a_missing_node() {
    let src = scene_with("").replace("camera: \"cam\"", "camera: \"nope/cam\"");
    let err = Scene::parse(&src).unwrap_err();
    assert!(
        matches!(err, SceneError::CameraNotFound(ref p) if p == "nope/cam"),
        "{err}"
    );
}

#[test]
fn scene_rejects_a_camera_path_to_a_node_without_camera() {
    let src =
        scene_with(r#"(name: "box", mesh: Cube)"#).replace("camera: \"cam\"", "camera: \"box\"");
    let err = Scene::parse(&src).unwrap_err();
    assert!(
        matches!(err, SceneError::NotACamera(ref p) if p == "box"),
        "{err}"
    );
}

#[test]
fn scene_rejects_a_second_light() {
    let p = node_problem(
        r#"(name: "sun", light: (color: (1.0, 1.0, 1.0), intensity: 1.0)),
           (name: "sky", children: [(name: "moon", light: (color: (1.0, 1.0, 1.0), intensity: 0.5))])"#,
        "sky/moon",
    );
    assert_eq!(p, Problem::SecondLight("sun".into()));
}

#[test]
fn scene_rejects_rotation_together_with_look_at() {
    let p = node_problem(
        r#"(name: "a", rotation: (0.0, 10.0, 0.0), look_at: (1.0, 0.0, 0.0))"#,
        "a",
    );
    assert_eq!(p, Problem::RotationAndLookAt);
}

#[test]
fn scene_rejects_look_at_its_own_position() {
    let p = node_problem(
        r#"(name: "a", position: (1.0, 2.0, 3.0), look_at: (1.0, 2.0, 3.0))"#,
        "a",
    );
    assert_eq!(p, Problem::LookAtSelf);
}

#[test]
fn scene_rejects_look_at_parallel_to_y() {
    let p = node_problem(
        r#"(name: "top", children: [(name: "down", position: (0.0, 5.0, 0.0), look_at: (0.0, 0.0, 0.0))])"#,
        "top/down",
    );
    assert_eq!(p, Problem::LookAtVertical);
}

#[test]
fn scene_rejects_non_finite_numbers() {
    for (fields, what) in [
        ("position: (inf, 0.0, 0.0)", "position"),
        ("rotation: (0.0, NaN, 0.0)", "rotation"),
        ("scale: (1.0, 1.0, -inf)", "scale"),
        ("look_at: (NaN, 0.0, 1.0)", "look_at"),
        ("mesh: Cube, color: (NaN, 0.5, 0.5)", "color"),
        ("camera: (fov_y: NaN, near: 0.1, far: 10.0)", "camera"),
        (
            "light: (color: (1.0, 1.0, 1.0), intensity: inf)",
            "light intensity",
        ),
    ] {
        let p = node_problem(&format!("(name: \"a\", {fields})"), "a");
        assert_eq!(p, Problem::NotFinite(what), "{fields}");
    }
    let src = scene_with("").replace("ambient: (0.2, 0.2, 0.2)", "ambient: (0.2, inf, 0.2)");
    let err = Scene::parse(&src).unwrap_err();
    assert!(
        matches!(err, SceneError::Ambient(Problem::NotFinite("ambient"))),
        "{err}"
    );
}

#[test]
fn scene_rejects_non_positive_scale() {
    for scale in ["(0.0, 1.0, 1.0)", "(1.0, -2.0, 1.0)"] {
        let p = node_problem(&format!("(name: \"a\", scale: {scale})"), "a");
        assert_eq!(p, Problem::ScaleNotPositive, "{scale}");
    }
}

#[test]
fn scene_rejects_colors_outside_zero_to_one() {
    let p = node_problem(r#"(name: "a", mesh: Cube, color: (1.5, 0.0, 0.0))"#, "a");
    assert_eq!(p, Problem::ColorOutOfRange("color"));
    let p = node_problem(
        r#"(name: "a", light: (color: (1.0, -0.1, 1.0), intensity: 1.0))"#,
        "a",
    );
    assert_eq!(p, Problem::ColorOutOfRange("light color"));
    let src = scene_with("").replace("ambient: (0.2, 0.2, 0.2)", "ambient: (0.2, 0.2, 2.0)");
    let err = Scene::parse(&src).unwrap_err();
    assert!(
        matches!(err, SceneError::Ambient(Problem::ColorOutOfRange(_))),
        "{err}"
    );
}

#[test]
fn scene_rejects_fov_out_of_range() {
    for fov in ["0.0", "180.0", "-5.0"] {
        let p = node_problem(
            &format!("(name: \"c\", camera: (fov_y: {fov}, near: 0.1, far: 10.0))"),
            "c",
        );
        assert_eq!(p, Problem::FovOutOfRange, "{fov}");
    }
}

#[test]
fn scene_rejects_near_not_below_far() {
    for (near, far) in [
        ("1.0", "1.0"),
        ("2.0", "1.0"),
        ("0.0", "1.0"),
        ("-1.0", "1.0"),
    ] {
        let p = node_problem(
            &format!("(name: \"c\", camera: (fov_y: 60.0, near: {near}, far: {far}))"),
            "c",
        );
        assert_eq!(p, Problem::NearFar, "{near} {far}");
    }
}

#[test]
fn scene_rejects_an_unknown_field() {
    let err = Scene::parse(&scene_with(r#"(name: "a", postion: (1.0, 0.0, 0.0))"#)).unwrap_err();
    assert!(matches!(err, SceneError::Syntax(_)), "{err}");
    let msg = err.to_string();
    assert!(msg.contains("postion"), "{msg}");
    // Line:column span, reported once (not repeated through a source chain).
    assert!(msg.starts_with("1:"), "{msg}");
}

#[test]
fn nested_paths_resolve_both_ways() {
    let scene = Scene::parse(&scene_with(
        r#"(name: "a", children: [(name: "b", children: [(name: "c")])])"#,
    ))
    .unwrap();
    let c = scene.find("a/b/c").unwrap();
    assert_eq!(scene.path(c), "a/b/c");
    assert_eq!(scene.node(c).name, "c");
    assert_eq!(scene.find("a/c"), None);
    assert_eq!(scene.find("a/b/c/d"), None);
}

#[test]
fn same_names_under_different_parents_coexist() {
    let scene = Scene::parse(&scene_with(
        r#"(name: "left", children: [(name: "post")]), (name: "right", children: [(name: "post")])"#,
    ))
    .unwrap();
    let (l, r) = (
        scene.find("left/post").unwrap(),
        scene.find("right/post").unwrap(),
    );
    assert_ne!(l, r);
    assert_eq!(scene.path(r), "right/post");
}

#[test]
fn child_world_position_adds_parent_translation() {
    let scene = Scene::parse(&scene_with(
        r#"(name: "p", position: (1.0, 0.0, 0.0), children: [(name: "c", position: (0.0, 1.0, 0.0))])"#,
    ))
    .unwrap();
    let world = scene.world_matrix(scene.find("p/c").unwrap());
    assert_close(world.transform_point3(Vec3::ZERO), Vec3::new(1.0, 1.0, 0.0));
}

#[test]
fn child_world_position_follows_parent_rotation() {
    let scene = Scene::parse(&scene_with(
        r#"(name: "p", rotation: (0.0, 90.0, 0.0), children: [(name: "c", position: (1.0, 0.0, 0.0))])"#,
    ))
    .unwrap();
    let world = scene.world_matrix(scene.find("p/c").unwrap());
    assert_close(
        world.transform_point3(Vec3::ZERO),
        Vec3::new(0.0, 0.0, -1.0),
    );
}

#[test]
fn euler_yaw_of_ninety_degrees_turns_forward_to_left() {
    // Integer literals are accepted for angles, as in hand-written files.
    let scene = Scene::parse(&scene_with(r#"(name: "a", rotation: (0, 90, 0))"#)).unwrap();
    let rotation = scene.transform(scene.find("a").unwrap()).rotation;
    assert_close(rotation * Vec3::NEG_Z, Vec3::NEG_X);
}

#[test]
fn euler_angles_combine_yaw_then_pitch() {
    // YXZ intrinsic: yaw 90 turns -Z to -X, then pitch 90 lifts it to +Y.
    let scene = Scene::parse(&scene_with(r#"(name: "a", rotation: (90.0, 90.0, 0.0))"#)).unwrap();
    let rotation = scene.transform(scene.find("a").unwrap()).rotation;
    assert_close(rotation * Vec3::NEG_Z, Vec3::Y);
}

#[test]
fn look_at_rotations_match_the_reference_bits() {
    let scene = Scene::parse(DEMO).unwrap();
    let cam = scene.transform(scene.find("main_cam").unwrap()).rotation;
    let sun = scene.transform(scene.find("sun").unwrap()).rotation;
    assert_eq!(
        quat_bits(cam),
        [-0.15902498f32, 0.30089226, 0.05096195, 0.9389238].map(f32::to_bits)
    );
    assert_eq!(
        quat_bits(sun),
        [-0.43359014f32, 0.39115202, 0.21679507, 0.78230405].map(f32::to_bits)
    );
    // And the camera really looks at its target.
    let dir = cam * Vec3::NEG_Z;
    assert_close(
        dir,
        (Vec3::new(0.0, 0.5, 0.0) - Vec3::new(2.5, 2.0, 3.5)).normalize(),
    );
}
