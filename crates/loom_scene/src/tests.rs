use super::*;

const DEMO: &str = include_str!("../../loom_sim/src/demo.scene.ron");

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
fn demo_scene_parses_into_the_character_scene_tree() {
    let scene = Scene::parse(DEMO).unwrap();
    let paths: Vec<_> = scene.ids().map(|id| scene.path(id)).collect();
    assert_eq!(
        paths,
        [
            "ground",
            "player",
            "player/visor",
            "pillars",
            "pillars/a",
            "pillars/b",
            "pillars/c",
            "sun",
            "main_cam"
        ]
    );
    assert_eq!(scene.camera(), scene.find("main_cam").unwrap());
    assert_eq!(scene.light(), scene.find("sun"));
    assert_eq!(scene.character(), scene.find("player"));
    assert_eq!(scene.orbit_camera(), scene.find("main_cam"));
    assert_eq!(scene.ambient(), Vec3::new(0.2, 0.2, 0.25));
    let player = scene.node(scene.find("player").unwrap());
    assert_eq!(
        player.mesh.map(|m| m.kind),
        Some(MeshKind::Capsule {
            radius: 0.35,
            height: 1.8
        })
    );
    assert_eq!(player.transform.position, Vec3::ZERO);
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
fn scene_rejects_negative_light_intensity() {
    let p = node_problem(
        r#"(name: "sky", children: [(name: "sun", light: (color: (1.0, 1.0, 1.0), intensity: -0.5))])"#,
        "sky/sun",
    );
    assert_eq!(p, Problem::NegativeIntensity);
    // Zero is allowed: a light that is switched off.
    let zero = r#"(name: "sun", light: (color: (1.0, 1.0, 1.0), intensity: 0.0))"#;
    assert!(Scene::parse(&scene_with(zero)).is_ok());
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
    // The camera and sun of the E2 demo scene.
    let scene = Scene::parse(&scene_with(
        "(name: \"eye\", position: (2.5, 2.0, 3.5), look_at: (0.0, 0.5, 0.0)),          (name: \"sun\", position: (4.0, 8.0, 3.0), look_at: (0.0, 0.0, 0.0))",
    ))
    .unwrap();
    let cam = scene.transform(scene.find("eye").unwrap()).rotation;
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

/// A character node `hero` with the given capsule and character fields.
fn hero(capsule: &str, character: &str) -> String {
    format!("(name: \"hero\", mesh: Capsule({capsule}), character: ({character}))")
}

const HERO: &str = "(name: \"hero\", mesh: Capsule(radius: 0.35, height: 1.8), \
                    character: (run_speed: 4.0, turn_speed: 720.0))";

/// A camera node `orbit` with an `orbit_camera` of the given fields.
fn orbit(fields: &str) -> String {
    format!(
        "(name: \"orbit\", camera: (fov_y: 60.0, near: 0.1, far: 100.0), orbit_camera: ({fields}))"
    )
}

const ORBIT_FIELDS: &str = "target: \"hero\", pivot_height: 1.5, distance: 4.0, \
                            yaw: 10.0, pitch: -15.0, pitch_min: -70.0, pitch_max: 20.0";

/// `ORBIT_FIELDS` with `field: old` replaced by `field: new`.
fn orbit_with(old: &str, new: &str) -> String {
    assert!(ORBIT_FIELDS.contains(old), "{old}");
    orbit(&ORBIT_FIELDS.replace(old, new))
}

#[test]
fn character_capsule_and_orbit_camera_parse_with_their_values() {
    // The camera comes before its target, and the target is nested.
    let src = scene_with(&format!(
        "{}, (name: \"arena\", children: [{HERO}])",
        orbit(&ORBIT_FIELDS.replace("\"hero\"", "\"arena/hero\""))
    ));
    let scene = Scene::parse(&src).unwrap();
    let hero = scene.find("arena/hero").unwrap();
    let cam = scene.find("orbit").unwrap();
    assert_eq!(scene.character(), Some(hero));
    assert_eq!(scene.orbit_camera(), Some(cam));
    let node = scene.node(hero);
    assert_eq!(
        node.mesh.map(|m| m.kind),
        Some(MeshKind::Capsule {
            radius: 0.35,
            height: 1.8
        })
    );
    assert_eq!(
        node.character,
        Some(Character {
            run_speed: 4.0,
            turn_speed_degrees: 720.0
        })
    );
    assert_eq!(
        scene.node(cam).orbit_camera,
        Some(OrbitCamera {
            target: hero,
            pivot_height: 1.5,
            distance: 4.0,
            yaw_degrees: 10.0,
            pitch_degrees: -15.0,
            pitch_min_degrees: -70.0,
            pitch_max_degrees: 20.0,
        })
    );
    assert!(scene.node(hero).orbit_camera.is_none());
}

#[test]
fn scene_without_character_or_orbit_camera_reports_none() {
    let scene = Scene::parse(&scene_with("")).unwrap();
    assert_eq!((scene.character(), scene.orbit_camera()), (None, None));
}

#[test]
fn capsule_with_height_equal_to_two_radii_is_accepted() {
    let scene = Scene::parse(&scene_with(
        "(name: \"ball\", mesh: Capsule(radius: 0.5, height: 1.0))",
    ))
    .unwrap();
    let ball = scene.node(scene.find("ball").unwrap());
    assert_eq!(
        ball.mesh.map(|m| m.kind),
        Some(MeshKind::Capsule {
            radius: 0.5,
            height: 1.0
        })
    );
}

#[test]
fn scene_rejects_an_invalid_capsule_shape() {
    for shape in [
        "radius: 0.0, height: 1.0",
        "radius: -0.5, height: 1.0",
        "radius: 0.5, height: 0.99",
    ] {
        let p = node_problem(
            &format!("(name: \"arena\", children: [(name: \"pill\", mesh: Capsule({shape}))])"),
            "arena/pill",
        );
        assert_eq!(p, Problem::CapsuleShape, "{shape}");
        assert!(p.to_string().contains("height >= 2 * radius"), "{p}");
    }
}

#[test]
fn scene_rejects_non_positive_character_speeds() {
    for character in [
        "run_speed: 0.0, turn_speed: 720.0",
        "run_speed: 4.0, turn_speed: -1.0",
    ] {
        let p = node_problem(&hero("radius: 0.35, height: 1.8", character), "hero");
        assert_eq!(p, Problem::CharacterSpeed, "{character}");
    }
}

#[test]
fn scene_rejects_a_second_character() {
    let second = "(name: \"arena\", children: [(name: \"rival\", \
                  character: (run_speed: 1.0, turn_speed: 90.0))])";
    let p = node_problem(&format!("{HERO}, {second}"), "arena/rival");
    assert_eq!(p, Problem::SecondCharacter("hero".to_owned()));
    assert!(p.to_string().contains("\"hero\""), "{p}");
}

#[test]
fn scene_rejects_a_second_orbit_camera() {
    let second = orbit(ORBIT_FIELDS).replace("\"orbit\"", "\"orbit2\"");
    let p = node_problem(
        &format!("{HERO}, {}, {second}", orbit(ORBIT_FIELDS)),
        "orbit2",
    );
    assert_eq!(p, Problem::SecondOrbitCamera("orbit".to_owned()));
}

#[test]
fn orbit_camera_without_a_camera_component_is_rejected() {
    let p = node_problem(
        &format!("{HERO}, (name: \"rig\", orbit_camera: ({ORBIT_FIELDS}))"),
        "rig",
    );
    assert_eq!(p, Problem::OrbitCameraWithoutCamera);
}

#[test]
fn orbit_camera_target_naming_no_node_is_rejected() {
    let p = node_problem(
        &format!("{HERO}, {}", orbit_with("\"hero\"", "\"heroes/hero\"")),
        "orbit",
    );
    assert_eq!(p, Problem::OrbitTargetNotFound("heroes/hero".to_owned()));
}

#[test]
fn orbit_camera_target_without_a_character_is_rejected() {
    let p = node_problem(
        &format!("(name: \"hero\"), {}", orbit(ORBIT_FIELDS)),
        "orbit",
    );
    assert_eq!(p, Problem::OrbitTargetNotCharacter("hero".to_owned()));
    assert!(p.to_string().contains("character component"), "{p}");
}

#[test]
fn orbit_camera_distance_must_be_greater_than_zero() {
    for distance in ["distance: 0.0", "distance: -4.0"] {
        let p = node_problem(
            &format!("{HERO}, {}", orbit_with("distance: 4.0", distance)),
            "orbit",
        );
        assert_eq!(p, Problem::OrbitDistance, "{distance}");
    }
}

#[test]
fn orbit_camera_pitch_limits_must_be_ordered_within_ninety_degrees() {
    for (old, new) in [
        ("pitch_min: -70.0", "pitch_min: 20.0"), // pitch_min == pitch_max
        ("pitch_min: -70.0", "pitch_min: 30.0"),
        ("pitch_min: -70.0", "pitch_min: -90.0"),
        ("pitch_max: 20.0", "pitch_max: 90.0"),
    ] {
        let p = node_problem(&format!("{HERO}, {}", orbit_with(old, new)), "orbit");
        assert_eq!(p, Problem::OrbitPitchRange, "{new}");
    }
}

#[test]
fn orbit_camera_pitch_outside_its_limits_is_rejected() {
    for pitch in ["pitch: -71.0", "pitch: 21.0"] {
        let p = node_problem(
            &format!("{HERO}, {}", orbit_with("pitch: -15.0", pitch)),
            "orbit",
        );
        assert_eq!(p, Problem::OrbitPitchOutOfRange, "{pitch}");
    }
}

#[test]
fn orbit_camera_pitch_on_its_limits_is_accepted() {
    for pitch in ["pitch: -70.0", "pitch: 20.0"] {
        let src = scene_with(&format!("{HERO}, {}", orbit_with("pitch: -15.0", pitch)));
        assert!(Scene::parse(&src).is_ok(), "{pitch}");
    }
}

#[test]
fn character_components_reject_non_finite_numbers() {
    let p = node_problem(
        &hero(
            "radius: inf, height: 1.8",
            "run_speed: 4.0, turn_speed: 720.0",
        ),
        "hero",
    );
    assert_eq!(p, Problem::NotFinite("mesh"));
    let p = node_problem(
        &hero(
            "radius: 0.35, height: 1.8",
            "run_speed: NaN, turn_speed: 720.0",
        ),
        "hero",
    );
    assert_eq!(p, Problem::NotFinite("character"));
    for (old, new) in [
        ("pivot_height: 1.5", "pivot_height: inf"),
        ("distance: 4.0", "distance: NaN"),
        ("yaw: 10.0", "yaw: -inf"),
        ("pitch: -15.0", "pitch: NaN"),
        ("pitch_min: -70.0", "pitch_min: NaN"),
        ("pitch_max: 20.0", "pitch_max: inf"),
    ] {
        let p = node_problem(&format!("{HERO}, {}", orbit_with(old, new)), "orbit");
        assert_eq!(p, Problem::NotFinite("orbit_camera"), "{new}");
    }
}

#[test]
fn misspelled_character_or_orbit_camera_field_is_rejected() {
    for nodes in [
        hero(
            "radius: 0.35, height: 1.8",
            "run_sped: 4.0, turn_speed: 720.0",
        ),
        hero(
            "radius: 0.35, hieght: 1.8",
            "run_speed: 4.0, turn_speed: 720.0",
        ),
        format!("{HERO}, {}", orbit_with("distance: 4.0", "distanse: 4.0")),
    ] {
        let err = Scene::parse(&scene_with(&nodes)).unwrap_err();
        assert!(matches!(err, SceneError::Syntax(_)), "{nodes}: {err}");
    }
}
