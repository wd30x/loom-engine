//! Custom scenes: loading, scene errors, the `game` field, the reload
//! count.

use std::fs;

use serde_json::Value;

use super::common::{path_str, report, run_loom, test_dir, write_file};

/// A minimal valid souls scene: a character `hero` (line 6) and its orbit
/// camera.
pub(super) const MINIMAL_SCENE: &str = r#"(
    version: 1, game: "souls",
    camera: "cam",
    ambient: (0.3, 0.3, 0.3),
    nodes: [
        (name: "hero", mesh: Capsule(radius: 0.3, height: 1.6), character: (run_speed: 2.0, turn_speed: 360.0)),
        (name: "cam", camera: (fov_y: 50.0, near: 0.1, far: 50.0),
            orbit_camera: (target: "hero", pivot_height: 1.0, distance: 3.0,
                yaw: 0.0, pitch: -10.0, pitch_min: -60.0, pitch_max: 30.0,
                collision_radius: 0.2, return_speed: 4.0)),
    ],
)
"#;

#[test]
fn custom_scene_is_loaded_and_reported() {
    let dir = test_dir("custom_scene_is_loaded_and_reported");
    let scene = write_file(&dir, "min.scene.ron", MINIMAL_SCENE);
    let state = dir.join("s.json");
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "10",
        "--scene",
        path_str(&scene),
        "--dump-state",
        path_str(&state),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["scene"], path_str(&scene));
    let dump: Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("state file was written"))
            .expect("state file is valid JSON");
    let nodes: Vec<&String> = dump["nodes"]
        .as_object()
        .expect("nodes is an object")
        .keys()
        .collect();
    assert_eq!(nodes, ["cam", "hero"]);
}

#[test]
fn scene_without_a_character_is_a_runtime_error() {
    let dir = test_dir("scene_without_a_character_is_a_runtime_error");
    // Without the character, the orbit camera has to go too.
    let start = MINIMAL_SCENE
        .find(",\n            orbit_camera")
        .expect("scene has an orbit camera");
    let end = MINIMAL_SCENE
        .find("return_speed: 4.0)")
        .expect("scene has return_speed")
        + "return_speed: 4.0)".len();
    let src = format!("{}{}", &MINIMAL_SCENE[..start], &MINIMAL_SCENE[end..])
        .replace(", character: (run_speed: 2.0, turn_speed: 360.0)", "");
    let scene = write_file(&dir, "nocharacter.scene.ron", &src);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&scene), "character"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn invalid_scene_is_a_runtime_error_naming_the_node() {
    let dir = test_dir("invalid_scene_is_a_runtime_error_naming_the_node");
    let scene = write_file(
        &dir,
        "bad.scene.ron",
        &MINIMAL_SCENE.replace(
            r#"        (name: "cam","#,
            r#"        (name: "arena", children: [(name: "pillar_2", mesh: Cube, scale: (1.0, 0.0, 1.0))]),
        (name: "cam","#,
        ),
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    let error = r["error"].as_str().expect("error is a string");
    for part in [path_str(&scene), "arena/pillar_2"] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn malformed_scene_reports_line_and_column() {
    let dir = test_dir("malformed_scene_reports_line_and_column");
    let scene = write_file(
        &dir,
        "typo.scene.ron",
        &MINIMAL_SCENE.replace(
            "mesh: Capsule(radius: 0.3, height: 1.6)",
            "mesh: Capsule(radius: 0.3, height: 1.6), colour: (1.0, 0.0, 0.0)",
        ),
    );
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1);
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains(path_str(&scene)), "{error}");
    assert!(error.contains("colour"), "{error}");
    // ron reports the span as line:column-line:column; the typo is on line 6.
    let span = regex_like_span(error);
    assert!(
        span.is_some_and(|(line, _)| line == 6),
        "no line:column span in {error}"
    );
}

/// Finds the first `L:C` (digits, colon, digits) in `text`.
fn regex_like_span(text: &str) -> Option<(u32, u32)> {
    text.split(|c: char| !(c.is_ascii_digit() || c == ':'))
        .filter_map(|word| {
            let (line, col) = word.split_once(':')?;
            Some((line.parse().ok()?, col.parse().ok()?))
        })
        .next()
}

#[test]
fn headless_run_reports_zero_reloads() {
    let dir = test_dir("headless_run_reports_zero_reloads");
    let scene = write_file(&dir, "min.scene.ron", MINIMAL_SCENE);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "10",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["reloads"], 0);
    assert_eq!(r["reload_errors"], 0);
}

#[test]
fn souls_run_reports_its_game() {
    let out = run_loom(&["run", "--headless", "--frames", "1"]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["game"], "souls");
}

/// Runs [`MINIMAL_SCENE`] with its `game: "souls",` replaced by `game`, and
/// returns the error of the (failed) run and the scene path.
fn run_with_game(test: &str, game: &str) -> (String, String) {
    let dir = test_dir(test);
    let src = MINIMAL_SCENE.replacen("game: \"souls\",", game, 1);
    assert_ne!(src, MINIMAL_SCENE);
    let scene = write_file(&dir, "game.scene.ron", &src);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    let error = r["error"].as_str().expect("error is a string").to_owned();
    (error, path_str(&scene).to_owned())
}

#[test]
fn scene_without_a_game_is_a_runtime_error_naming_both_games() {
    let (error, scene) = run_with_game(
        "scene_without_a_game_is_a_runtime_error_naming_both_games",
        "",
    );
    assert_eq!(
        error,
        format!(
            "invalid scene {scene}: the scene does not say which game runs it: \
             add game: \"rogue\" or game: \"souls\" at the top level"
        )
    );
}

#[test]
fn scene_with_an_unknown_game_is_a_runtime_error_listing_the_games() {
    let (error, scene) = run_with_game(
        "scene_with_an_unknown_game_is_a_runtime_error_listing_the_games",
        "game: \"rouge\",",
    );
    assert_eq!(
        error,
        format!(
            "invalid scene {scene}: unknown game \"rouge\": the games are \"rogue\" and \"souls\""
        )
    );
}
