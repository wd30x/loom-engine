use super::*;

const DEMO: &str = include_str!("../../../games/souls/src/demo.scene.ron");

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
            "wall",
            "dummy_a",
            "dummy_a/body",
            "dummy_a/block",
            "dummy_a/hp_bar",
            "dummy_b",
            "dummy_b/body",
            "dummy_b/block",
            "dummy_b/hp_bar",
            "dummy_c",
            "dummy_c/body",
            "dummy_c/block",
            "dummy_c/hp_bar",
            "stake",
            "stake/post",
            "stake/block",
            "stake/arm",
            "stake/hp_bar",
            "sun",
            "main_cam"
        ]
    );
    let colliders: Vec<_> = scene.colliders().iter().map(|&id| scene.path(id)).collect();
    assert_eq!(
        colliders,
        [
            "pillars/a",
            "pillars/b",
            "pillars/c",
            "wall",
            "dummy_a/block",
            "dummy_b/block",
            "dummy_c/block",
            "stake/block"
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
                            yaw: 10.0, pitch: -15.0, pitch_min: -70.0, pitch_max: 20.0, \
                            collision_radius: 0.2, return_speed: 4.0";

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
            collision_radius: 0.2,
            return_speed: 4.0,
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
fn orbit_camera_collision_radius_must_be_between_zero_and_the_pivot_height() {
    // 1.5 equals pivot_height.
    for radius in ["0.0", "-0.1", "1.5", "2.0"] {
        let p = node_problem(
            &format!(
                "{HERO}, {}",
                orbit_with(
                    "collision_radius: 0.2",
                    &format!("collision_radius: {radius}")
                )
            ),
            "orbit",
        );
        assert_eq!(p, Problem::OrbitCollisionRadius, "{radius}");
        assert!(
            p.to_string()
                .contains("0 < collision_radius < pivot_height"),
            "{p}"
        );
    }
}

#[test]
fn orbit_camera_return_speed_must_be_greater_than_zero() {
    for speed in ["0.0", "-1.0"] {
        let p = node_problem(
            &format!(
                "{HERO}, {}",
                orbit_with("return_speed: 4.0", &format!("return_speed: {speed}"))
            ),
            "orbit",
        );
        assert_eq!(p, Problem::OrbitReturnSpeed, "{speed}");
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
        ("collision_radius: 0.2", "collision_radius: inf"),
        ("collision_radius: 0.2", "collision_radius: NaN"),
        ("return_speed: 4.0", "return_speed: inf"),
        ("return_speed: 4.0", "return_speed: NaN"),
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

#[test]
fn colliders_parse_and_are_listed_in_pre_order() {
    let scene = Scene::parse(&scene_with(
        "(name: \"z_box\", collider: Box), \
         (name: \"arena\", position: (1.0, 0.0, 0.0), children: [\
             (name: \"wall\", rotation: (0.0, 45.0, 0.0), scale: (4.0, 2.0, 0.5), collider: Box), \
             (name: \"deco\", mesh: Cube)]), \
         (name: \"a_box\", mesh: Cube, collider: Box)",
    ))
    .unwrap();
    let paths: Vec<_> = scene.colliders().iter().map(|&id| scene.path(id)).collect();
    assert_eq!(paths, ["z_box", "arena/wall", "a_box"]);
    let wall = scene.node(scene.find("arena/wall").unwrap());
    assert_eq!(wall.collider, Some(Collider::Box));
    assert!(
        scene
            .node(scene.find("arena/deco").unwrap())
            .collider
            .is_none()
    );
}

#[test]
fn collider_rotated_off_the_y_axis_is_rejected() {
    for rotation in ["rotation: (10.0, 0.0, 0.0)", "rotation: (0.0, 30.0, -5.0)"] {
        let p = node_problem(
            &format!("(name: \"arena\", children: [(name: \"wall\", {rotation}, collider: Box)])"),
            "arena/wall",
        );
        assert_eq!(p, Problem::ColliderRotation, "{rotation}");
    }
}

#[test]
fn collider_with_look_at_is_rejected() {
    let p = node_problem(
        "(name: \"wall\", position: (1.0, 0.0, 0.0), look_at: (0.0, 0.0, 0.0), collider: Box)",
        "wall",
    );
    assert_eq!(p, Problem::ColliderRotation);
}

#[test]
fn collider_under_a_rotating_ancestor_is_rejected() {
    let p = node_problem(
        "(name: \"arena\", rotation: (0.0, 90.0, 0.0), children: [\
             (name: \"group\", children: [(name: \"wall\", collider: Box)])])",
        "arena/group/wall",
    );
    assert_eq!(p, Problem::ColliderAncestor("arena".to_owned()));
    assert!(p.to_string().contains("\"arena\""), "{p}");
}

#[test]
fn collider_under_a_scaling_ancestor_is_rejected() {
    let p = node_problem(
        "(name: \"arena\", children: [\
             (name: \"big\", scale: (2.0, 1.0, 1.0), children: [(name: \"wall\", collider: Box)])])",
        "arena/big/wall",
    );
    assert_eq!(p, Problem::ColliderAncestor("arena/big".to_owned()));
}

#[test]
fn collider_under_an_ancestor_with_look_at_is_rejected() {
    let p = node_problem(
        "(name: \"aim\", position: (1.0, 0.0, 0.0), look_at: (0.0, 0.0, 0.0), \
             children: [(name: \"wall\", collider: Box)])",
        "aim/wall",
    );
    assert_eq!(p, Problem::ColliderAncestor("aim".to_owned()));
}

#[test]
fn collider_and_character_on_one_node_are_rejected() {
    let p = node_problem(
        "(name: \"hero\", mesh: Capsule(radius: 0.35, height: 1.8), \
             character: (run_speed: 4.0, turn_speed: 720.0), collider: Box)",
        "hero",
    );
    assert_eq!(p, Problem::CharacterAndCollider);
}

#[test]
fn collider_accepts_identity_ancestors_yaw_and_its_own_scale() {
    let src = scene_with(
        "(name: \"arena\", position: (5.0, 0.0, 0.0), rotation: (0.0, 0.0, 0.0), scale: (1.0, 1.0, 1.0), \
             children: [(name: \"wall\", rotation: (0.0, 45.0, 0.0), scale: (3.0, 2.0, 0.25), collider: Box)])",
    );
    let scene = Scene::parse(&src).unwrap();
    assert_eq!(scene.colliders(), [scene.find("arena/wall").unwrap()]);
}

#[test]
fn misspelled_collider_kind_is_rejected() {
    let err = Scene::parse(&scene_with("(name: \"wall\", collider: Boxx)")).unwrap_err();
    assert!(matches!(err, SceneError::Syntax(_)), "{err}");
    assert!(err.to_string().contains("Boxx"), "{err}");
}

// ---- 2D: orthographic cameras, sprites and tilemaps (E4.2)

#[test]
fn orthographic_camera_parses_with_its_height() {
    let src = scene_with("").replace(
        "camera: (fov_y: 60.0, near: 0.1, far: 100.0)",
        "camera: (ortho: (height: 11.25), near: 0.1, far: 100.0)",
    );
    let scene = Scene::parse(&src).unwrap();
    let camera = scene.node(scene.camera()).camera.unwrap();
    assert_eq!(
        camera.projection,
        Projection::Orthographic { height: 11.25 }
    );
    assert_eq!((camera.near, camera.far), (0.1, 100.0));
    let perspective = Scene::parse(&scene_with("")).unwrap();
    assert_eq!(
        perspective
            .node(perspective.camera())
            .camera
            .unwrap()
            .projection,
        Projection::Perspective {
            fov_y_degrees: 60.0
        }
    );
}

#[test]
fn camera_needs_exactly_one_projection() {
    for camera in [
        "camera: (near: 0.1, far: 10.0)",
        "camera: (fov_y: 60.0, ortho: (height: 5.0), near: 0.1, far: 10.0)",
    ] {
        let p = node_problem(&format!("(name: \"c\", {camera})"), "c");
        assert_eq!(p, Problem::CameraProjection, "{camera}");
    }
}

#[test]
fn orthographic_height_must_be_positive_and_finite() {
    for height in ["0.0", "-2.0"] {
        let p = node_problem(
            &format!("(name: \"c\", camera: (ortho: (height: {height}), near: 0.1, far: 10.0))"),
            "c",
        );
        assert_eq!(p, Problem::OrthoHeight, "{height}");
    }
    let p = node_problem(
        "(name: \"c\", camera: (ortho: (height: inf), near: 0.1, far: 10.0))",
        "c",
    );
    assert_eq!(p, Problem::NotFinite("camera"));
}

const SPRITE: &str = r#"(name: "hero", position: (3.0, -2.0, 1.0), sprite: (sheet: "art/tiles.json", tag: "player"))"#;
const TILEMAP: &str = r####"(name: "map", tilemap: (sheet: "art/tiles.json", legend: [('#', "wall"), ('.', "floor")], rows: ["###", "#.#"]))"####;

#[test]
fn sprites_and_tilemaps_parse_and_are_listed_in_pre_order() {
    let src = scene_with(&format!(
        "{TILEMAP}, (name: \"units\", children: [{SPRITE}])"
    ));
    let scene = Scene::parse(&src).unwrap();
    let map = scene.find("map").unwrap();
    let hero = scene.find("units/hero").unwrap();
    assert_eq!(scene.sprite_nodes(), [map, hero]);
    let sprite = scene.node(hero).sprite.as_ref().unwrap();
    assert_eq!(sprite.sheet, "art/tiles.json");
    assert_eq!(sprite.tag(), "player");
    assert_eq!(sprite.tags(), ["player"]);
    let tilemap = scene.node(map).tilemap.as_ref().unwrap();
    assert_eq!(
        tilemap.legend,
        [('#', "wall".to_owned()), ('.', "floor".to_owned())]
    );
    assert_eq!(tilemap.rows, ["###", "#.#"]);
    assert_eq!((tilemap.tag('.'), tilemap.tag('x')), (Some("floor"), None));
}

#[test]
fn mesh_sprite_and_tilemap_cannot_share_a_node() {
    let sprite = r#"sprite: (sheet: "s.json", tag: "t")"#;
    let tilemap = r#"tilemap: (sheet: "s.json", legend: [('.', "t")], rows: ["."])"#;
    for components in [
        format!("mesh: Cube, {sprite}"),
        format!("mesh: Cube, {tilemap}"),
        format!("{sprite}, {tilemap}"),
    ] {
        let p = node_problem(&format!("(name: \"n\", {components})"), "n");
        assert_eq!(p, Problem::DrawableConflict, "{components}");
    }
}

/// A node `map` with a tilemap of the given legend and rows.
fn map_with(legend: &str, rows: &str) -> String {
    format!(r#"(name: "map", tilemap: (sheet: "s.json", legend: [{legend}], rows: [{rows}]))"#)
}

#[test]
fn tilemap_needs_at_least_one_row() {
    let p = node_problem(&map_with(r#"('.', "t")"#, ""), "map");
    assert_eq!(p, Problem::TilemapNoRows);
}

#[test]
fn tilemap_rows_must_share_one_non_zero_width() {
    for rows in [r#""..", ".""#, r#""""#, r#""", """#] {
        let p = node_problem(&map_with(r#"('.', "t")"#, rows), "map");
        assert_eq!(p, Problem::TilemapRowWidth, "{rows}");
    }
}

#[test]
fn tilemap_characters_must_be_in_the_legend() {
    let p = node_problem(&map_with(r#"('.', "t")"#, r#""..", ".x""#), "map");
    assert_eq!(
        p,
        Problem::TilemapUnknownChar {
            row: 1,
            column: 1,
            ch: 'x'
        }
    );
    assert!(p.to_string().contains("row 1, column 1"), "{p}");
}

#[test]
fn tilemap_legend_characters_must_be_unique() {
    let p = node_problem(&map_with(r#"('.', "a"), ('.', "b")"#, r#"".""#), "map");
    assert_eq!(p, Problem::TilemapDuplicateLegend('.'));
}

#[test]
fn sheet_and_tag_names_must_not_be_empty() {
    for (node, field) in [
        (
            r#"(name: "n", sprite: (sheet: "", tag: "t"))"#.to_owned(),
            "sprite sheet",
        ),
        (
            r#"(name: "n", sprite: (sheet: "s.json", tag: ""))"#.to_owned(),
            "sprite tag",
        ),
        (
            r#"(name: "n", tilemap: (sheet: "", legend: [('.', "t")], rows: ["."]))"#.to_owned(),
            "tilemap sheet",
        ),
        (
            r#"(name: "n", tilemap: (sheet: "s.json", legend: [('.', "")], rows: ["."]))"#
                .to_owned(),
            "tilemap legend tag",
        ),
    ] {
        let p = node_problem(&node, "n");
        assert_eq!(p, Problem::EmptyField(field), "{node}");
    }
}

#[test]
fn top_level_game_is_optional_and_returned_as_written() {
    let without = Scene::parse(&scene_with(r#"(name: "a")"#)).unwrap();
    assert_eq!(without.game(), None);
    let src =
        scene_with(r#"(name: "a")"#).replacen("(version: 1,", r#"(version: 1, game: "rogue","#, 1);
    let with = Scene::parse(&src).unwrap();
    assert_eq!(with.game(), Some("rogue"));
}

#[test]
fn empty_game_name_is_rejected() {
    let src = scene_with(r#"(name: "a")"#).replacen("(version: 1,", r#"(version: 1, game: "","#, 1);
    let err = Scene::parse(&src).unwrap_err();
    assert_eq!(err.to_string(), "game must not be empty");
    assert!(matches!(err, SceneError::Game(Problem::EmptyField("game"))));
}

/// A sprite node `z` with `alternates` (RON list items) and `props`.
fn actor(alternates: &str, props: &str) -> String {
    format!(
        r#"(name: "z", sprite: (sheet: "s.json", tag: "zombie", alternates: [{alternates}]), props: [{props}])"#
    )
}

#[test]
fn props_parse_in_file_order_and_default_to_none() {
    let scene = Scene::parse(&scene_with(&actor("", r#"("hp", 3), ("speed", 67.5)"#))).unwrap();
    let z = scene.node(scene.find("z").unwrap());
    assert_eq!(z.prop("hp"), Some(3.0));
    assert_eq!(z.prop("speed"), Some(67.5));
    assert_eq!(z.prop("attack"), None);
    let all: Vec<(&str, f64)> = z.props().collect();
    assert_eq!(all, [("hp", 3.0), ("speed", 67.5)]);
    let cam = scene.node(scene.find("cam").unwrap());
    assert_eq!(cam.props().count(), 0);
}

#[test]
fn prop_names_must_be_lowercase_identifiers() {
    for name in ["HP", "1hp", "", "hit-points", "_hp"] {
        let p = node_problem(&actor("", &format!(r#"("{name}", 1)"#)), "z");
        assert_eq!(p, Problem::InvalidPropName(name.to_owned()), "{name:?}");
    }
}

#[test]
fn prop_given_twice_is_rejected() {
    let p = node_problem(&actor("", r#"("hp", 1), ("hp", 2)"#), "z");
    assert_eq!(p, Problem::DuplicateProp("hp".to_owned()));
}

#[test]
fn prop_values_must_be_finite() {
    for value in ["inf", "-inf", "NaN"] {
        let p = node_problem(&actor("", &format!(r#"("hp", {value})"#)), "z");
        assert_eq!(p, Problem::PropNotFinite("hp".to_owned()), "{value}");
    }
}

#[test]
fn sprite_alternates_parse_after_the_tag_and_default_to_none() {
    let scene = Scene::parse(&scene_with(&actor(r#""corpse", "ghost""#, ""))).unwrap();
    let sprite = scene
        .node(scene.find("z").unwrap())
        .sprite
        .as_ref()
        .unwrap();
    assert_eq!(sprite.tag(), "zombie");
    assert_eq!(sprite.tags(), ["zombie", "corpse", "ghost"]);
    let plain = Scene::parse(&scene_with(SPRITE)).unwrap();
    let hero = plain.node(plain.find("hero").unwrap());
    assert_eq!(hero.sprite.as_ref().unwrap().tags(), ["player"]);
}

#[test]
fn empty_sprite_alternate_is_rejected() {
    let p = node_problem(&actor(r#""corpse", """#, ""), "z");
    assert_eq!(p, Problem::EmptyField("sprite alternate"));
}

#[test]
fn sprite_alternate_listed_twice_is_rejected() {
    let p = node_problem(&actor(r#""corpse", "corpse""#, ""), "z");
    assert_eq!(p, Problem::DuplicateAlternate("corpse".to_owned()));
}

#[test]
fn sprite_alternate_equal_to_the_tag_is_rejected() {
    let p = node_problem(&actor(r#""zombie""#, ""), "z");
    assert_eq!(p, Problem::AlternateIsTag("zombie".to_owned()));
}

#[test]
fn set_sprite_tag_switches_between_declared_tags() {
    let mut scene = Scene::parse(&scene_with(&actor(r#""corpse""#, ""))).unwrap();
    let z = scene.find("z").unwrap();
    scene.set_sprite_tag(z, "corpse").unwrap();
    assert_eq!(scene.node(z).sprite.as_ref().unwrap().tag(), "corpse");
    scene.set_sprite_tag(z, "zombie").unwrap();
    assert_eq!(scene.node(z).sprite.as_ref().unwrap().tag(), "zombie");
}

#[test]
fn set_sprite_tag_rejects_an_undeclared_tag() {
    let mut scene = Scene::parse(&scene_with(&actor(r#""corpse""#, ""))).unwrap();
    let z = scene.find("z").unwrap();
    let err = scene.set_sprite_tag(z, "player").unwrap_err();
    assert_eq!(
        err,
        SpriteTagError::Undeclared {
            path: "z".to_owned(),
            tag: "player".to_owned()
        }
    );
    assert_eq!(scene.node(z).sprite.as_ref().unwrap().tag(), "zombie");
}

#[test]
fn set_sprite_tag_on_a_node_without_a_sprite_is_an_error() {
    let mut scene = Scene::parse(&scene_with(r#"(name: "a")"#)).unwrap();
    let a = scene.find("a").unwrap();
    assert_eq!(
        scene.set_sprite_tag(a, "corpse"),
        Err(SpriteTagError::NoSprite("a".to_owned()))
    );
}

/// A scene with a mesh node `g/box` (color 0.2, 0.4, 0.6) under a group `g`
/// that has no mesh.
fn colored_box() -> Scene {
    Scene::parse(&scene_with(
        r#"(name: "g", children: [(name: "box", mesh: Cube, color: (0.2, 0.4, 0.6))])"#,
    ))
    .unwrap()
}

#[test]
fn set_mesh_color_changes_the_color_the_mesh_is_drawn_with() {
    let mut scene = colored_box();
    let b = scene.find("g/box").unwrap();
    scene.set_mesh_color(b, Vec3::new(1.0, 0.5, 0.0)).unwrap();
    assert_eq!(scene.node(b).mesh.unwrap().color, Vec3::new(1.0, 0.5, 0.0));
    // Both ends of the range are allowed.
    scene.set_mesh_color(b, Vec3::ZERO).unwrap();
    scene.set_mesh_color(b, Vec3::ONE).unwrap();
    assert_eq!(scene.node(b).mesh.unwrap().color, Vec3::ONE);
}

#[test]
fn set_mesh_color_on_a_node_without_a_mesh_is_an_error() {
    let mut scene = colored_box();
    let g = scene.find("g").unwrap();
    let err = scene.set_mesh_color(g, Vec3::ONE).unwrap_err();
    assert_eq!(err, MeshColorError::NoMesh("g".to_owned()));
    assert_eq!(err.to_string(), "the node \"g\" has no mesh");
    assert!(scene.node(g).mesh.is_none());
}

#[test]
fn set_mesh_color_outside_zero_to_one_is_an_error_and_changes_nothing() {
    let mut scene = colored_box();
    let b = scene.find("g/box").unwrap();
    let before = scene.node(b).mesh.unwrap().color;
    for (color, text) in [
        (Vec3::new(1.5, 0.0, 0.0), "[1.5, 0, 0]"),
        (Vec3::new(0.0, -0.1, 0.0), "[0, -0.1, 0]"),
        (Vec3::new(0.0, 0.0, f32::NAN), "[0, 0, NaN]"),
    ] {
        let err = scene.set_mesh_color(b, color).unwrap_err();
        assert!(
            matches!(&err, MeshColorError::OutOfRange { path, .. } if path == "g/box"),
            "{err}"
        );
        assert_eq!(
            err.to_string(),
            format!(
                "the node \"g/box\": mesh color components must be between 0 and 1, got {text}"
            )
        );
        assert_eq!(scene.node(b).mesh.unwrap().color, before);
    }
}

#[test]
fn hidden_parent_hides_its_subtree_from_drawing_until_shown_again() {
    let mut scene = colored_box();
    let (g, b) = (scene.find("g").unwrap(), scene.find("g/box").unwrap());
    assert!(scene.is_drawn(g) && scene.is_drawn(b));
    scene.set_visible(g, false);
    assert!(!scene.is_drawn(g) && !scene.is_drawn(b));
    // The child itself is still visible: only its parent is hidden.
    assert!(scene.node(b).visible);
    assert!(!scene.node(g).visible);
    // The camera is not under `g`.
    assert!(scene.is_drawn(scene.camera()));
    scene.set_visible(g, true);
    assert!(scene.is_drawn(g) && scene.is_drawn(b));
    // Hiding the child alone leaves the parent drawn.
    scene.set_visible(b, false);
    assert!(scene.is_drawn(g) && !scene.is_drawn(b));
}
